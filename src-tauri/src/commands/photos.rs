use std::fs;
use std::path::Path;

use image::{ImageDecoder, ImageReader};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::State;
use uuid::Uuid;

use crate::db;
use crate::models::photo::Photo;
use crate::state::AppState;

/// Read a vault-relative photo file, decrypting when it is a `.enc` (photos
/// scope encrypted). Shared by the protocol handler and the data-URL command
/// so both agree on encryption; a locked vault (no key) fails rather than
/// returning ciphertext.
fn read_photo_file(
    vault_path: &Path,
    key: Option<&[u8; 32]>,
    rel: &str,
) -> Result<Vec<u8>, String> {
    let full = vault_path.join(rel);
    if rel.ends_with(".enc") {
        let key = key.ok_or("Vault is locked")?;
        crate::crypto::decrypt_file_to_memory(key, &full)
    } else {
        fs::read(&full).map_err(|e| format!("read photo file: {}", e))
    }
}

const THUMB_MAX_DIM: u32 = 512;
/// Reject source files larger than this before reading them into memory.
const MAX_PHOTO_BYTES: u64 = 50 * 1024 * 1024;
/// Cap per-image allocation during decode to defend against decode bombs
/// (a small file that claims enormous dimensions).
const MAX_DECODE_ALLOC: u64 = 256 * 1024 * 1024;
const MAX_DECODE_DIM: u32 = 20_000;

#[derive(Serialize)]
pub struct AttachPhotosResult {
    pub attached: Vec<Photo>,
    pub skipped: Vec<String>,
    pub failed: Vec<FailedPhoto>,
}

#[derive(Serialize)]
pub struct FailedPhoto {
    pub path: String,
    pub reason: String,
}

#[tauri::command]
pub fn attach_photos(
    activity_id: String,
    paths: Vec<String>,
    state: State<AppState>,
) -> Result<AttachPhotosResult, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;

    db::activities::get_activity_by_id(&conn, &activity_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Activity not found: {}", activity_id))?;

    let activity_dir = state.vault_path.join("photos").join(&activity_id);
    fs::create_dir_all(&activity_dir)
        .map_err(|e| format!("Failed to create photos dir: {}", e))?;

    // Key only when the `photos` scope is on — see AppState::encryption_key_for.
    let key = state.encryption_key_for(|s| s.photos)?;

    let mut result = AttachPhotosResult {
        attached: Vec::new(),
        skipped: Vec::new(),
        failed: Vec::new(),
    };

    for path_str in paths {
        match attach_one(&conn, &state.vault_path, &activity_id, &path_str, key.as_ref()) {
            Ok(Some(photo)) => result.attached.push(photo),
            Ok(None) => result.skipped.push(path_str),
            Err(reason) => result.failed.push(FailedPhoto {
                path: path_str,
                reason,
            }),
        }
    }

    Ok(result)
}

/// Encrypt a freshly written vault file in place and return its new
/// vault-relative `.enc` path (same pattern as import/pipeline.rs).
fn encrypt_to_rel(key: &[u8; 32], vault_path: &Path, abs: &Path) -> Result<String, String> {
    let enc = crate::crypto::encrypt_file(key, abs)?;
    Ok(enc
        .strip_prefix(vault_path)
        .unwrap_or(&enc)
        .to_string_lossy()
        .to_string())
}

/// Convert a HEIC/HEIF file to JPEG bytes with the system `sips` tool —
/// present on every macOS install, so no HEVC-decoder dependency is needed.
#[cfg(target_os = "macos")]
fn convert_heic_to_jpeg(src: &Path) -> Result<Vec<u8>, String> {
    let tmp = std::env::temp_dir().join(format!("syzify_heic_{}.jpg", Uuid::new_v4()));
    let out = std::process::Command::new("/usr/bin/sips")
        .arg("-s")
        .arg("format")
        .arg("jpeg")
        .arg(src)
        .arg("--out")
        .arg(&tmp)
        .output()
        .map_err(|e| format!("run sips: {}", e))?;
    if !out.status.success() {
        let _ = fs::remove_file(&tmp);
        return Err(format!(
            "HEIC conversion failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let bytes = fs::read(&tmp).map_err(|e| format!("read converted HEIC: {}", e));
    let _ = fs::remove_file(&tmp);
    bytes
}

#[cfg(not(target_os = "macos"))]
fn convert_heic_to_jpeg(_src: &Path) -> Result<Vec<u8>, String> {
    Err("HEIC photos are supported on macOS only".to_string())
}

/// Open a decoder with the decode-bomb limits applied (see MAX_DECODE_*).
fn limited_decoder<'a, R: std::io::BufRead + std::io::Seek + 'a>(
    reader: ImageReader<R>,
) -> Result<impl ImageDecoder + 'a, String> {
    let mut reader = reader
        .with_guessed_format()
        .map_err(|e| format!("guess format: {}", e))?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DECODE_DIM);
    limits.max_image_height = Some(MAX_DECODE_DIM);
    limits.max_alloc = Some(MAX_DECODE_ALLOC);
    reader.limits(limits);
    reader
        .into_decoder()
        .map_err(|e| format!("decode image: {}", e))
}

/// One-time backfill: photos attached before EXIF-orientation support have
/// sideways thumbnails and pre-rotation dimensions. Regenerate both for every
/// photo whose EXIF orientation is a real transform. Any failure returns Err
/// so the caller leaves the run-once flag unset and retries next launch
/// (e.g. encrypted photos while the key is not loaded).
pub fn orient_existing_photos(
    conn: &rusqlite::Connection,
    vault_path: &Path,
    key: Option<&[u8; 32]>,
) -> Result<(), String> {
    for photo in db::photos::get_all_photos(conn).map_err(|e| e.to_string())? {
        let bytes = read_photo_file(vault_path, key, &photo.path_in_vault)
            .map_err(|e| format!("photo {}: {}", photo.id, e))?;
        let mut decoder = limited_decoder(ImageReader::new(std::io::Cursor::new(&bytes)))
            .map_err(|e| format!("photo {}: {}", photo.id, e))?;
        let orientation = decoder
            .orientation()
            .unwrap_or(image::metadata::Orientation::NoTransforms);
        // The common case: no rotation tag — thumbnail and dims are fine, and
        // the pixel decode can be skipped entirely.
        if matches!(orientation, image::metadata::Orientation::NoTransforms) {
            continue;
        }
        let mut img = image::DynamicImage::from_decoder(decoder)
            .map_err(|e| format!("photo {}: decode: {}", photo.id, e))?;
        img.apply_orientation(orientation);

        if let Some(thumb_rel) = &photo.thumbnail_path {
            let plain_abs = vault_path.join(thumb_rel.trim_end_matches(".enc"));
            img.thumbnail(THUMB_MAX_DIM, THUMB_MAX_DIM)
                .to_rgb8()
                .save_with_format(&plain_abs, image::ImageFormat::Jpeg)
                .map_err(|e| format!("photo {}: save thumbnail: {}", photo.id, e))?;
            if thumb_rel.ends_with(".enc") {
                let key = key.ok_or_else(|| format!("photo {}: vault is locked", photo.id))?;
                crate::crypto::encrypt_file(key, &plain_abs)
                    .map_err(|e| format!("photo {}: {}", photo.id, e))?;
            }
        }

        db::photos::update_dimensions(conn, &photo.id, img.width() as i64, img.height() as i64)
            .map_err(|e| format!("photo {}: {}", photo.id, e))?;
    }
    Ok(())
}

fn attach_one(
    conn: &rusqlite::Connection,
    vault_path: &Path,
    activity_id: &str,
    src_path: &str,
    key: Option<&[u8; 32]>,
) -> Result<Option<Photo>, String> {
    let src = Path::new(src_path);

    let meta = fs::metadata(src).map_err(|e| format!("stat failed: {}", e))?;
    if meta.len() > MAX_PHOTO_BYTES {
        return Err(format!(
            "Image too large: {} bytes (max {})",
            meta.len(),
            MAX_PHOTO_BYTES
        ));
    }

    let bytes = fs::read(src).map_err(|e| format!("read failed: {}", e))?;

    let hash = sha256_hex(&bytes);

    if db::photos::hash_exists_for_activity(conn, activity_id, &hash)
        .map_err(|e| e.to_string())?
    {
        return Ok(None);
    }

    let ext = src
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();

    // taken_at comes from the ORIGINAL bytes — kamadak-exif reads HEIF
    // containers too, and conversion below could drop metadata.
    let taken_at = extract_taken_at(&bytes);

    // What actually lands in the vault. HEIC (iPhone) is converted to JPEG so
    // the vault stays viewable on every platform; everything else is stored
    // byte-for-byte. The dedup hash above is of the original file either way.
    let (stored_bytes, stored_ext, mime_type) = match ext.as_str() {
        "jpg" | "jpeg" => (bytes, "jpg", "image/jpeg"),
        "png" => (bytes, "png", "image/png"),
        "webp" => (bytes, "webp", "image/webp"),
        "heic" | "heif" => (convert_heic_to_jpeg(src)?, "jpg", "image/jpeg"),
        _ => return Err(format!("Unsupported image format: {}", ext)),
    };
    let mime_type = mime_type.to_string();

    let mut decoder = limited_decoder(ImageReader::new(std::io::Cursor::new(&stored_bytes)))?;
    // Phone JPEGs store sensor-orientation pixels plus an EXIF rotation tag.
    // Bake the rotation into the decoded image so the thumbnail and the stored
    // dimensions match what the browser shows for the untouched original.
    let orientation = decoder
        .orientation()
        .unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut img = image::DynamicImage::from_decoder(decoder)
        .map_err(|e| format!("decode image: {}", e))?;
    img.apply_orientation(orientation);

    let width = img.width();
    let height = img.height();

    let photo_id = Uuid::new_v4().to_string();
    let photos_dir = vault_path.join("photos").join(activity_id);

    let stored_filename = format!("{}.{}", photo_id, stored_ext);
    let stored_path = photos_dir.join(&stored_filename);
    fs::write(&stored_path, &stored_bytes)
        .map_err(|e| format!("write copy: {}", e))?;

    let thumb_filename = format!("{}.thumb.jpg", photo_id);
    let thumb_path = photos_dir.join(&thumb_filename);
    let thumb = img.thumbnail(THUMB_MAX_DIM, THUMB_MAX_DIM);
    thumb
        .to_rgb8()
        .save_with_format(&thumb_path, image::ImageFormat::Jpeg)
        .map_err(|e| format!("save thumbnail: {}", e))?;

    let mut rel_path = format!("photos/{}/{}", activity_id, stored_filename);
    let mut rel_thumb = format!("photos/{}/{}", activity_id, thumb_filename);

    // Photos scope on and vault unlocked: encrypt the copy and thumbnail now.
    // Deferring to the next unlock's resume_file_encryption pass would leave
    // this photo in plaintext on disk for the rest of the session.
    if let Some(key) = key {
        rel_path = encrypt_to_rel(key, vault_path, &stored_path)?;
        rel_thumb = encrypt_to_rel(key, vault_path, &thumb_path)?;
    }

    let sort_order = db::photos::next_sort_order(conn, activity_id)
        .map_err(|e| e.to_string())?;

    let photo = Photo {
        id: photo_id,
        activity_id: activity_id.to_string(),
        path_in_vault: rel_path,
        thumbnail_path: Some(rel_thumb),
        original_path: Some(src_path.to_string()),
        mime_type,
        width: Some(width as i64),
        height: Some(height as i64),
        size_bytes: stored_bytes.len() as i64,
        hash_sha256: hash,
        taken_at,
        caption: None,
        sort_order,
        created_at: String::new(),
    };

    db::photos::insert_photo(conn, &photo).map_err(|e| e.to_string())?;

    let inserted = db::photos::get_photo_by_id(conn, &photo.id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Just-inserted photo not found".to_string())?;

    Ok(Some(inserted))
}

#[tauri::command]
pub fn get_photos(
    activity_id: String,
    state: State<AppState>,
) -> Result<Vec<Photo>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::photos::get_photos_for_activity(&conn, &activity_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_photo(
    photo_id: String,
    state: State<AppState>,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;

    let photo = db::photos::get_photo_by_id(&conn, &photo_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Photo not found: {}", photo_id))?;

    let full_path = state.vault_path.join(&photo.path_in_vault);
    let _ = fs::remove_file(&full_path);
    if let Some(thumb) = &photo.thumbnail_path {
        let _ = fs::remove_file(state.vault_path.join(thumb));
    }

    db::photos::delete_photo(&conn, &photo_id).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn update_photo_caption(
    photo_id: String,
    caption: Option<String>,
    state: State<AppState>,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::photos::update_caption(&conn, &photo_id, caption.as_deref())
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Save a PNG produced on the frontend (e.g. share-image export) to an
/// arbitrary destination path the user picked via the file dialog.
#[tauri::command]
pub fn save_share_image(dest_path: String, png_base64: String) -> Result<(), String> {
    use base64::{engine::general_purpose, Engine as _};

    // This command writes to a path supplied by the frontend. Restrict it to
    // a PNG destination so it can't be abused (e.g. via XSS) to overwrite
    // arbitrary files — the share export is always a PNG.
    let is_png = Path::new(&dest_path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("png"))
        .unwrap_or(false);
    if !is_png {
        return Err("Share image must be saved with a .png extension".to_string());
    }

    let bytes = general_purpose::STANDARD
        .decode(png_base64.as_bytes())
        .map_err(|e| format!("base64 decode: {}", e))?;
    fs::write(&dest_path, &bytes).map_err(|e| format!("write file: {}", e))?;
    Ok(())
}

/// Return a photo as a base64 data URL so it can be inlined into an SVG
/// (used by html-to-image which can't fetch from custom URI schemes).
#[tauri::command]
pub fn get_photo_data_url(
    photo_id: String,
    size: Option<String>,
    state: State<AppState>,
) -> Result<String, String> {
    use base64::{engine::general_purpose, Engine as _};
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let photo = db::photos::get_photo_by_id(&conn, &photo_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Photo not found: {}", photo_id))?;

    let (rel, mime): (String, String) = if size.as_deref() == Some("thumb") {
        match &photo.thumbnail_path {
            Some(t) => (t.clone(), "image/jpeg".to_string()),
            None => (photo.path_in_vault.clone(), photo.mime_type.clone()),
        }
    } else {
        (photo.path_in_vault.clone(), photo.mime_type.clone())
    };

    // Decrypt .enc photos with the in-memory key — otherwise this base64s
    // ciphertext and breaks the share image (matches resolve_photo_request).
    let key = *state.encryption_key.lock().map_err(|e| e.to_string())?;
    let bytes = read_photo_file(&state.vault_path, key.as_ref(), &rel)?;
    let encoded = general_purpose::STANDARD.encode(&bytes);
    Ok(format!("data:{};base64,{}", mime, encoded))
}

#[tauri::command]
pub fn reorder_photos(
    photo_ids: Vec<String>,
    state: State<AppState>,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    for (idx, id) in photo_ids.iter().enumerate() {
        db::photos::update_sort_order(&conn, id, idx as i64)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// Extract the capture timestamp from EXIF and normalize it to
/// `YYYY-MM-DDTHH:MM:SS`. Returns `None` when there is no usable EXIF date
/// (e.g. PNG/WebP screenshots, stripped metadata).
fn extract_taken_at(bytes: &[u8]) -> Option<String> {
    let exif = exif::Reader::new()
        .read_from_container(&mut std::io::Cursor::new(bytes))
        .ok()?;

    // Prefer the original capture time, fall back to the digitized time.
    let field = exif
        .get_field(exif::Tag::DateTimeOriginal, exif::In::PRIMARY)
        .or_else(|| exif.get_field(exif::Tag::DateTimeDigitized, exif::In::PRIMARY))?;

    // The DateTime fields are stored as ASCII "YYYY:MM:DD HH:MM:SS".
    match &field.value {
        exif::Value::Ascii(parts) => {
            let raw = String::from_utf8_lossy(parts.first()?);
            normalize_exif_datetime(raw.trim())
        }
        _ => None,
    }
}

fn normalize_exif_datetime(raw: &str) -> Option<String> {
    let (date, time) = raw.split_once(' ')?;
    let mut parts = date.splitn(3, ':');
    let y = parts.next()?;
    let mo = parts.next()?;
    let d = parts.next()?;
    let valid = |p: &str, len: usize| p.len() == len && p.chars().all(|c| c.is_ascii_digit());
    if !valid(y, 4) || !valid(mo, 2) || !valid(d, 2) {
        return None;
    }
    let time_parts: Vec<&str> = time.split(':').collect();
    if time_parts.len() != 3 || !time_parts.iter().all(|p| valid(p, 2)) {
        return None;
    }
    // Cameras write all-zero dates ("0000:00:00") when no real date is set.
    if y == "0000" || mo == "00" || d == "00" {
        return None;
    }
    Some(format!("{}-{}-{}T{}", y, mo, d, time))
}

/// Resolve a photo:// URI to (mime_type, file bytes).
/// URI: photo://localhost/{photo_id}?size=thumb|full
pub fn resolve_photo_request(
    vault_path: &Path,
    db: &rusqlite::Connection,
    key: Option<&[u8; 32]>,
    uri: &str,
) -> Result<(String, Vec<u8>), String> {
    let path_part = uri
        .strip_prefix("photo://localhost/")
        .or_else(|| uri.strip_prefix("photo://localhost\\"))
        .ok_or_else(|| format!("Invalid photo URI: {}", uri))?;

    let (id_part, size) = match path_part.split_once('?') {
        Some((id, q)) => {
            let mut s = "full";
            for kv in q.split('&') {
                if let Some(v) = kv.strip_prefix("size=") {
                    s = v;
                }
            }
            (id, s)
        }
        None => (path_part, "full"),
    };

    let photo_id = id_part.trim_end_matches('/');

    let photo = db::photos::get_photo_by_id(db, photo_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Photo not found: {}", photo_id))?;

    let (rel, mime): (String, String) = if size == "thumb" {
        match &photo.thumbnail_path {
            Some(t) => (t.clone(), "image/jpeg".to_string()),
            None => (photo.path_in_vault.clone(), photo.mime_type.clone()),
        }
    } else {
        (photo.path_in_vault.clone(), photo.mime_type.clone())
    };

    let bytes = read_photo_file(vault_path, key, &rel)?;
    Ok((mime, bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::models::activity::Activity;
    use std::path::PathBuf;
    use std::io::Cursor;

    fn unique_dir(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("tv_photos_{}_{}", tag, Uuid::new_v4()));
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn make_activity(conn: &rusqlite::Connection, id: &str) {
        let a = Activity {
            id: id.to_string(),
            start_time: "2026-05-01T08:00:00+00:00".to_string(),
            timezone_offset: None,
            sport_type: "run".to_string(),
            title: None, notes: None,
            distance_m: None, duration_s: None,
            elev_gain_m: None, elev_loss_m: None,
            avg_speed_mps: None, max_speed_mps: None,
            avg_hr: None, max_hr: None, avg_cadence: None,
            calories: None,
            avg_temperature_c: None, max_temperature_c: None,
            source_device: None, location_name: None,
            start_lat: None, start_lon: None,
            avg_power_w: None, max_power_w: None, normalized_power_w: None,
            total_work_kj: None, threshold_power_w: None,
            training_stress_score: None, intensity_factor: None,
            training_effect_aerobic: None, training_effect_anaerobic: None, training_load_peak: None,
            avg_vertical_oscillation_mm: None, avg_stance_time_ms: None, avg_stance_time_percent: None,
            avg_step_length_mm: None, total_strides: None,
            min_hr: None, moving_time_s: None, sub_sport: None,
            avg_respiration_rate: None, max_respiration_rate: None,
            hrv_rmssd: None, hrv_sdrr: None, end_lat: None, end_lon: None,
            avg_left_torque_effectiveness: None, avg_right_torque_effectiveness: None,
            avg_left_pedal_smoothness: None, avg_right_pedal_smoothness: None,
            avg_left_right_balance: None,
            ..Default::default()
        };
        db::activities::insert_activity(conn, &a).unwrap();
    }

    fn jpeg_bytes(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbImage::from_pixel(w, h, image::Rgb([120, 200, 80]));
        let mut buf = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut buf, image::ImageFormat::Jpeg)
            .unwrap();
        buf.into_inner()
    }

    fn png_bytes(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbImage::from_pixel(w, h, image::Rgb([10, 20, 30]));
        let mut buf = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut buf, image::ImageFormat::Png)
            .unwrap();
        buf.into_inner()
    }

    /// The same JPEG with a minimal EXIF APP1 segment (TIFF header + one IFD
    /// entry) declaring the given Orientation value, spliced in after SOI.
    fn jpeg_bytes_with_orientation(w: u32, h: u32, orientation: u16) -> Vec<u8> {
        let jpeg = jpeg_bytes(w, h);
        let mut tiff: Vec<u8> = Vec::new();
        tiff.extend(b"II\x2a\x00"); // little-endian TIFF magic
        tiff.extend(8u32.to_le_bytes()); // IFD0 offset
        tiff.extend(1u16.to_le_bytes()); // one entry
        tiff.extend(0x0112u16.to_le_bytes()); // Orientation tag
        tiff.extend(3u16.to_le_bytes()); // SHORT
        tiff.extend(1u32.to_le_bytes()); // count
        tiff.extend(orientation.to_le_bytes());
        tiff.extend([0u8, 0]); // value padding
        tiff.extend(0u32.to_le_bytes()); // no next IFD

        let mut app1: Vec<u8> = vec![0xFF, 0xE1];
        app1.extend(((2 + 6 + tiff.len()) as u16).to_be_bytes());
        app1.extend(b"Exif\x00\x00");
        app1.extend(&tiff);

        let mut out = jpeg[..2].to_vec(); // SOI
        out.extend(&app1);
        out.extend(&jpeg[2..]);
        out
    }

    fn decoded_dims(bytes: &[u8]) -> (u32, u32) {
        let img = image::load_from_memory(bytes).unwrap();
        (img.width(), img.height())
    }

    /// EXIF orientation 6 (90° CW to display): stored dims and the thumbnail
    /// must be the rotated portrait ones, matching what the browser renders.
    #[test]
    fn attach_one_applies_exif_orientation() {
        let conn = db::test_db();
        make_activity(&conn, "act-o");
        let vault = unique_dir("orient");
        fs::create_dir_all(vault.join("photos").join("act-o")).unwrap();

        let src = unique_dir("src").join("rotated.jpg");
        fs::write(&src, jpeg_bytes_with_orientation(40, 30, 6)).unwrap();

        let photo = attach_one(&conn, &vault, "act-o", src.to_str().unwrap(), None)
            .unwrap()
            .expect("attach should store the photo");

        assert_eq!(photo.width, Some(30), "width must be post-rotation");
        assert_eq!(photo.height, Some(40), "height must be post-rotation");
        let thumb = fs::read(vault.join(photo.thumbnail_path.unwrap())).unwrap();
        assert_eq!(decoded_dims(&thumb), (384, 512), "thumbnail must be rotated");

        let _ = fs::remove_dir_all(&vault);
    }

    /// The startup backfill rotates thumbnails/dims of photos attached before
    /// orientation support, and leaves tag-free photos byte-identical.
    #[test]
    fn orient_existing_photos_fixes_old_thumbnails() {
        let conn = db::test_db();
        make_activity(&conn, "act-b");
        let vault = unique_dir("backfill");
        let dir = vault.join("photos").join("act-b");
        fs::create_dir_all(&dir).unwrap();

        // Old-style import of a rotated photo: original bytes kept, thumbnail
        // generated without applying the EXIF tag, sensor dims in the DB.
        fs::write(dir.join("old.jpg"), jpeg_bytes_with_orientation(40, 30, 6)).unwrap();
        fs::write(dir.join("old.thumb.jpg"), jpeg_bytes(40, 30)).unwrap();
        // A tag-free photo that must not be touched.
        fs::write(dir.join("plain.jpg"), jpeg_bytes(20, 10)).unwrap();
        fs::write(dir.join("plain.thumb.jpg"), jpeg_bytes(20, 10)).unwrap();

        let base = Photo {
            id: String::new(),
            activity_id: "act-b".into(),
            path_in_vault: String::new(),
            thumbnail_path: None,
            original_path: None,
            mime_type: "image/jpeg".into(),
            width: None,
            height: None,
            size_bytes: 1,
            hash_sha256: "h1".into(),
            taken_at: None,
            caption: None,
            sort_order: 0,
            created_at: String::new(),
        };
        db::photos::insert_photo(
            &conn,
            &Photo {
                id: "ph-old".into(),
                path_in_vault: "photos/act-b/old.jpg".into(),
                thumbnail_path: Some("photos/act-b/old.thumb.jpg".into()),
                width: Some(40),
                height: Some(30),
                ..base.clone()
            },
        )
        .unwrap();
        db::photos::insert_photo(
            &conn,
            &Photo {
                id: "ph-plain".into(),
                path_in_vault: "photos/act-b/plain.jpg".into(),
                thumbnail_path: Some("photos/act-b/plain.thumb.jpg".into()),
                width: Some(20),
                height: Some(10),
                hash_sha256: "h2".into(),
                ..base.clone()
            },
        )
        .unwrap();
        let plain_thumb_before = fs::read(dir.join("plain.thumb.jpg")).unwrap();

        orient_existing_photos(&conn, &vault, None).unwrap();

        let old = db::photos::get_photo_by_id(&conn, "ph-old").unwrap().unwrap();
        assert_eq!((old.width, old.height), (Some(30), Some(40)));
        let thumb = fs::read(dir.join("old.thumb.jpg")).unwrap();
        assert_eq!(decoded_dims(&thumb), (384, 512), "thumbnail must be regenerated rotated");

        let plain = db::photos::get_photo_by_id(&conn, "ph-plain").unwrap().unwrap();
        assert_eq!((plain.width, plain.height), (Some(20), Some(10)));
        assert_eq!(
            fs::read(dir.join("plain.thumb.jpg")).unwrap(),
            plain_thumb_before,
            "tag-free photo must not be rewritten"
        );

        let _ = fs::remove_dir_all(&vault);
    }

    /// Encrypted vault: the backfill decrypts the original, re-encrypts the
    /// regenerated thumbnail, and fails (for retry) when the key is missing.
    #[test]
    fn orient_existing_photos_handles_encrypted_and_locked() {
        let conn = db::test_db();
        make_activity(&conn, "act-e2");
        let vault = unique_dir("backfill_enc");
        let dir = vault.join("photos").join("act-e2");
        fs::create_dir_all(&dir).unwrap();
        let key = [7u8; 32];

        fs::write(dir.join("p.jpg"), jpeg_bytes_with_orientation(40, 30, 6)).unwrap();
        fs::write(dir.join("p.thumb.jpg"), jpeg_bytes(40, 30)).unwrap();
        crate::crypto::encrypt_file(&key, &dir.join("p.jpg")).unwrap();
        crate::crypto::encrypt_file(&key, &dir.join("p.thumb.jpg")).unwrap();

        db::photos::insert_photo(
            &conn,
            &Photo {
                id: "ph-enc".into(),
                activity_id: "act-e2".into(),
                path_in_vault: "photos/act-e2/p.jpg.enc".into(),
                thumbnail_path: Some("photos/act-e2/p.thumb.jpg.enc".into()),
                original_path: None,
                mime_type: "image/jpeg".into(),
                width: Some(40),
                height: Some(30),
                size_bytes: 1,
                hash_sha256: "h".into(),
                taken_at: None,
                caption: None,
                sort_order: 0,
                created_at: String::new(),
            },
        )
        .unwrap();

        // Locked: must fail so the flag stays unset and it retries later.
        assert!(orient_existing_photos(&conn, &vault, None).is_err());

        orient_existing_photos(&conn, &vault, Some(&key)).unwrap();
        let p = db::photos::get_photo_by_id(&conn, "ph-enc").unwrap().unwrap();
        assert_eq!((p.width, p.height), (Some(30), Some(40)));
        let thumb =
            crate::crypto::decrypt_file_to_memory(&key, &dir.join("p.thumb.jpg.enc")).unwrap();
        assert_eq!(decoded_dims(&thumb), (384, 512));
        assert!(!dir.join("p.thumb.jpg").exists(), "no plaintext thumbnail left");

        let _ = fs::remove_dir_all(&vault);
    }

    /// HEIC attach (macOS only — uses the system sips both to build the
    /// fixture and inside attach_one): stored copy becomes a JPEG, dims come
    /// from the converted image, and dedup keys on the ORIGINAL heic bytes.
    #[cfg(target_os = "macos")]
    #[test]
    fn attach_one_converts_heic_to_jpeg() {
        let conn = db::test_db();
        make_activity(&conn, "act-h");
        let vault = unique_dir("heic");
        fs::create_dir_all(vault.join("photos").join("act-h")).unwrap();

        // Build a real HEIC fixture from a JPEG via sips.
        let dir = unique_dir("src");
        let jpg = dir.join("src.jpg");
        fs::write(&jpg, jpeg_bytes(40, 30)).unwrap();
        let heic = dir.join("photo.heic");
        let st = std::process::Command::new("/usr/bin/sips")
            .args(["-s", "format", "heic"])
            .arg(&jpg)
            .arg("--out")
            .arg(&heic)
            .output()
            .unwrap();
        assert!(st.status.success(), "sips jpeg->heic failed: {}",
            String::from_utf8_lossy(&st.stderr));

        let photo = attach_one(&conn, &vault, "act-h", heic.to_str().unwrap(), None)
            .unwrap()
            .expect("heic attach should store the photo");

        assert_eq!(photo.mime_type, "image/jpeg");
        assert!(photo.path_in_vault.ends_with(".jpg"), "got: {}", photo.path_in_vault);
        assert_eq!((photo.width, photo.height), (Some(40), Some(30)));
        let stored = fs::read(vault.join(&photo.path_in_vault)).unwrap();
        assert_eq!(decoded_dims(&stored), (40, 30), "stored copy must decode as JPEG");

        // Re-attaching the same heic dedups via the original-bytes hash.
        let dup = attach_one(&conn, &vault, "act-h", heic.to_str().unwrap(), None).unwrap();
        assert!(dup.is_none(), "identical heic must be deduplicated");

        let _ = fs::remove_dir_all(&vault);
    }

    /// Off-macOS there is no HEVC decoder — the per-file error must say so
    /// (this is what lands in AttachPhotosResult.failed on Windows/Linux).
    #[cfg(not(target_os = "macos"))]
    #[test]
    fn attach_one_rejects_heic_off_macos() {
        let conn = db::test_db();
        make_activity(&conn, "act-h");
        let vault = unique_dir("heic_reject");
        fs::create_dir_all(vault.join("photos").join("act-h")).unwrap();
        let src = unique_dir("src").join("photo.heic");
        fs::write(&src, b"heic bytes irrelevant, rejected before decode").unwrap();

        let err = attach_one(&conn, &vault, "act-h", src.to_str().unwrap(), None).unwrap_err();
        assert!(err.contains("macOS"), "got: {}", err);

        let _ = fs::remove_dir_all(&vault);
    }

    #[test]
    fn sha256_hex_known_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn normalize_exif_datetime_cases() {
        assert_eq!(
            normalize_exif_datetime("2026:05:01 14:30:15"),
            Some("2026-05-01T14:30:15".to_string())
        );
        // All-zero date (camera default when unset) is rejected.
        assert_eq!(normalize_exif_datetime("0000:00:00 00:00:00"), None);
        // Malformed inputs.
        assert_eq!(normalize_exif_datetime("not a date"), None);
        assert_eq!(normalize_exif_datetime("2026-05-01 14:30:15"), None);
        assert_eq!(normalize_exif_datetime("2026:5:1 14:30:15"), None);
        assert_eq!(normalize_exif_datetime("2026:05:01 14:30"), None);
    }

    #[test]
    fn extract_taken_at_none_for_non_exif_image() {
        // A freshly-encoded PNG carries no EXIF DateTimeOriginal.
        assert_eq!(extract_taken_at(&png_bytes(8, 8)), None);
    }

    #[test]
    fn attach_one_stores_thumbnail_and_dedups() {
        let conn = db::test_db();
        make_activity(&conn, "act-1");
        let vault = unique_dir("attach");
        fs::create_dir_all(vault.join("photos").join("act-1")).unwrap();

        let src = unique_dir("src").join("pic.jpg");
        fs::write(&src, jpeg_bytes(40, 30)).unwrap();

        let photo = attach_one(&conn, &vault, "act-1", src.to_str().unwrap(), None)
            .unwrap()
            .expect("first attach should store the photo");

        assert_eq!(photo.width, Some(40));
        assert_eq!(photo.height, Some(30));
        assert!(vault.join(&photo.path_in_vault).exists(), "stored copy exists");
        let thumb = photo.thumbnail_path.clone().unwrap();
        assert!(vault.join(&thumb).exists(), "thumbnail exists");

        // Same bytes again -> deduped (returns None, nothing new stored).
        let dup = attach_one(&conn, &vault, "act-1", src.to_str().unwrap(), None).unwrap();
        assert!(dup.is_none(), "identical photo must be deduplicated");
        assert_eq!(db::photos::get_photos_for_activity(&conn, "act-1").unwrap().len(), 1);

        let _ = fs::remove_dir_all(&vault);
    }

    #[test]
    fn save_share_image_requires_png_extension() {
        use base64::{engine::general_purpose, Engine as _};
        let b64 = general_purpose::STANDARD.encode(b"\x89PNG fake");

        // Non-PNG destinations are rejected before any write.
        let evil = unique_dir("share").join("passwd");
        assert!(save_share_image(evil.to_str().unwrap().into(), b64.clone()).is_err());
        assert!(!evil.exists());

        // A .png destination is accepted.
        let ok = unique_dir("share").join("out.png");
        save_share_image(ok.to_str().unwrap().into(), b64).unwrap();
        assert!(ok.exists());
    }

    #[test]
    fn attach_one_rejects_unsupported_format() {
        let conn = db::test_db();
        let vault = unique_dir("reject");
        let src = unique_dir("src").join("note.txt");
        fs::write(&src, b"not an image").unwrap();

        let err = attach_one(&conn, &vault, "act-x", src.to_str().unwrap(), None).unwrap_err();
        assert!(err.contains("Unsupported image format"), "got: {}", err);
    }

    #[test]
    fn resolve_photo_request_full_thumb_and_bad_uri() {
        let conn = db::test_db();
        make_activity(&conn, "act-r");
        let vault = unique_dir("resolve");
        fs::create_dir_all(vault.join("photos").join("act-r")).unwrap();
        fs::write(vault.join("photos/act-r/full.jpg"), b"FULLDATA").unwrap();
        fs::write(vault.join("photos/act-r/thumb.jpg"), b"THUMBDATA").unwrap();

        let photo = Photo {
            id: "ph-1".into(),
            activity_id: "act-r".into(),
            path_in_vault: "photos/act-r/full.jpg".into(),
            thumbnail_path: Some("photos/act-r/thumb.jpg".into()),
            original_path: None,
            mime_type: "image/jpeg".into(),
            width: Some(1),
            height: Some(1),
            size_bytes: 8,
            hash_sha256: "h".into(),
            taken_at: None,
            caption: None,
            sort_order: 0,
            created_at: String::new(),
        };
        db::photos::insert_photo(&conn, &photo).unwrap();

        let (mime, bytes) =
            resolve_photo_request(&vault, &conn, None, "photo://localhost/ph-1").unwrap();
        assert_eq!(mime, "image/jpeg");
        assert_eq!(bytes, b"FULLDATA");

        let (_, thumb) =
            resolve_photo_request(&vault, &conn, None, "photo://localhost/ph-1?size=thumb").unwrap();
        assert_eq!(thumb, b"THUMBDATA");

        assert!(resolve_photo_request(&vault, &conn, None, "https://evil/etc/passwd").is_err());
        assert!(resolve_photo_request(&vault, &conn, None, "photo://localhost/missing").is_err());

        let _ = fs::remove_dir_all(&vault);
    }

    /// Encrypted photos: the protocol decrypts .enc files with the key and
    /// fails cleanly when the vault is locked (no key).
    #[test]
    fn resolve_photo_request_decrypts_encrypted_photo() {
        let vault = unique_dir("enc");
        let conn = db::test_db();
        make_activity(&conn, "act-e");
        fs::create_dir_all(vault.join("photos/act-e")).unwrap();

        // Write a plaintext photo, then encrypt it and point the DB row at .enc.
        let key = crate::crypto::derive_key("pw", &[1u8; 32]);
        fs::write(vault.join("photos/act-e/full.jpg"), b"REALJPEGBYTES").unwrap();
        crate::crypto::encrypt_file(&key, &vault.join("photos/act-e/full.jpg")).unwrap();

        let photo = Photo {
            id: "ph-e".into(),
            activity_id: "act-e".into(),
            path_in_vault: "photos/act-e/full.jpg.enc".into(),
            thumbnail_path: None,
            original_path: None,
            mime_type: "image/jpeg".into(),
            width: None, height: None, size_bytes: 13, hash_sha256: "h".into(),
            taken_at: None, caption: None, sort_order: 0, created_at: String::new(),
        };
        db::photos::insert_photo(&conn, &photo).unwrap();

        // With the key: decrypts back to the original bytes.
        let (_, bytes) =
            resolve_photo_request(&vault, &conn, Some(&key), "photo://localhost/ph-e").unwrap();
        assert_eq!(bytes, b"REALJPEGBYTES");

        // Locked (no key): fails rather than serving ciphertext.
        assert!(resolve_photo_request(&vault, &conn, None, "photo://localhost/ph-e").is_err());

        let _ = fs::remove_dir_all(&vault);
    }

    /// The shared read helper (used by both the protocol and get_photo_data_url)
    /// decrypts .enc with the key, reads plaintext directly, and refuses to
    /// serve ciphertext when locked.
    #[test]
    fn read_photo_file_handles_plain_encrypted_and_locked() {
        let vault = unique_dir("readhelper");
        fs::create_dir_all(vault.join("photos/act")).unwrap();
        let key = crate::crypto::derive_key("pw", &[2u8; 32]);

        // Plaintext file: read straight through.
        fs::write(vault.join("photos/act/plain.jpg"), b"PLAINDATA").unwrap();
        assert_eq!(
            read_photo_file(&vault, None, "photos/act/plain.jpg").unwrap(),
            b"PLAINDATA"
        );

        // Encrypted file: decrypts with the key, errors without it.
        fs::write(vault.join("photos/act/enc.jpg"), b"SECRETPIXELS").unwrap();
        crate::crypto::encrypt_file(&key, &vault.join("photos/act/enc.jpg")).unwrap();
        assert_eq!(
            read_photo_file(&vault, Some(&key), "photos/act/enc.jpg.enc").unwrap(),
            b"SECRETPIXELS"
        );
        assert!(read_photo_file(&vault, None, "photos/act/enc.jpg.enc").is_err());

        let _ = fs::remove_dir_all(&vault);
    }

    /// Photos scope active: attach_one encrypts the stored copy and thumbnail
    /// immediately — a photo attached while unlocked must not sit in plaintext
    /// until the next restart's resume_file_encryption pass.
    #[test]
    fn attach_one_encrypts_copy_and_thumbnail_when_key_present() {
        let conn = db::test_db();
        make_activity(&conn, "act-k");
        let vault = unique_dir("attach_enc");
        fs::create_dir_all(vault.join("photos").join("act-k")).unwrap();
        let key = [5u8; 32];

        let src = unique_dir("src").join("pic.jpg");
        let original = jpeg_bytes(64, 48);
        fs::write(&src, &original).unwrap();

        let photo = attach_one(&conn, &vault, "act-k", src.to_str().unwrap(), Some(&key))
            .unwrap()
            .expect("attach should store the photo");

        // DB rows point at .enc files that exist on disk...
        assert!(photo.path_in_vault.ends_with(".enc"), "got: {}", photo.path_in_vault);
        let thumb_rel = photo.thumbnail_path.clone().unwrap();
        assert!(thumb_rel.ends_with(".enc"), "got: {}", thumb_rel);
        assert!(vault.join(&photo.path_in_vault).exists());
        assert!(vault.join(&thumb_rel).exists());

        // ...no plaintext copy is left behind...
        assert!(!vault.join(photo.path_in_vault.trim_end_matches(".enc")).exists());
        assert!(!vault.join(thumb_rel.trim_end_matches(".enc")).exists());

        // ...and the stored copy decrypts back to the original bytes.
        let bytes =
            crate::crypto::decrypt_file_to_memory(&key, &vault.join(&photo.path_in_vault))
                .unwrap();
        assert_eq!(bytes, original);

        // Dedup still recognizes the same source (hash is of the plaintext).
        let dup = attach_one(&conn, &vault, "act-k", src.to_str().unwrap(), Some(&key)).unwrap();
        assert!(dup.is_none(), "identical photo must be deduplicated");

        let _ = fs::remove_dir_all(&vault);
    }

}
