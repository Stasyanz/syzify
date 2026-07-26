use std::fs;
use std::io::Read;
use std::path::Path;

use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::db;
use crate::import::dedup;
use crate::models::activity::Activity;
use crate::models::raw_file::{FileFormat, ParseStatus, RawFile};
use crate::parser;
use rusqlite::Connection;

/// Cap on the decompressed size of a .gz import. Watch-folder scans feed
/// arbitrary files here, so an unbounded decompress would let a small crafted
/// archive (decompression bomb) exhaust memory. Real workout files decompress
/// to a few MB.
const MAX_GZ_DECOMPRESSED: u64 = 100 * 1024 * 1024;

/// Cap on the on-disk size of any imported file — it is read into memory
/// whole (hashing + parsing), so without a bound a mispicked multi-GB file
/// exhausts RAM before the parser ever rejects it. Real workout files are a
/// few MB; the largest multi-day FIT logs stay well under this.
const MAX_IMPORT_BYTES: u64 = 512 * 1024 * 1024;

/// Pre-read size gate for [`MAX_IMPORT_BYTES`], split out for testing.
fn ensure_import_size(size: u64) -> Result<(), String> {
    if size > MAX_IMPORT_BYTES {
        return Err(format!(
            "File is {} MB — larger than the {} MB import limit",
            size / (1024 * 1024),
            MAX_IMPORT_BYTES / (1024 * 1024)
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ImportResult {
    pub imported: usize,
    pub skipped: usize,
    pub failed: Vec<FailedFile>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct FailedFile {
    pub path: String,
    pub reason: String,
}

pub fn import_files<F>(
    conn: &Connection,
    vault_path: &Path,
    paths: &[String],
    encryption_key: Option<&[u8; 32]>,
    on_progress: F,
) -> ImportResult
where
    F: Fn(usize, usize, &str),
{
    let mut result = ImportResult {
        imported: 0,
        skipped: 0,
        failed: Vec::new(),
    };

    let total = paths.len();
    for (i, path_str) in paths.iter().enumerate() {
        let filename = Path::new(path_str)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(path_str);
        on_progress(i + 1, total, filename);

        match import_single_file(conn, vault_path, path_str, encryption_key) {
            Ok(ImportOutcome::Imported) => result.imported += 1,
            Ok(ImportOutcome::Skipped) => result.skipped += 1,
            Err(reason) => {
                result.failed.push(FailedFile {
                    path: path_str.clone(),
                    reason,
                });
            }
        }
    }

    result
}

pub enum ImportOutcome {
    Imported,
    Skipped,
}

/// Inner extension of a `.gz` path: "activity.fit.gz" → "fit".
/// None if the path isn't .gz at all.
fn gz_inner_ext(source_path: &Path) -> Result<Option<String>, String> {
    let ext = source_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    if !ext.eq_ignore_ascii_case("gz") {
        return Ok(None);
    }
    let stem = source_path.file_stem().ok_or("No file stem for .gz")?;
    let inner_ext = Path::new(stem)
        .extension()
        .and_then(|e| e.to_str())
        .ok_or_else(|| format!("Cannot determine format inside .gz: {}", source_path.display()))?
        .to_lowercase();
    Ok(Some(inner_ext))
}

/// Decompress gzip bytes fully in memory — never through a plaintext temp
/// file (the import may belong to a vault with file encryption on).
/// `max_size` bounds the output; exceeding it is an error, not a truncation.
fn decompress_gz(compressed: &[u8], max_size: u64) -> Result<Vec<u8>, String> {
    let mut decoder = GzDecoder::new(compressed).take(max_size + 1);
    let mut decompressed = Vec::new();
    decoder
        .read_to_end(&mut decompressed)
        .map_err(|e| format!("Failed to decompress .gz: {}", e))?;
    if decompressed.len() as u64 > max_size {
        return Err(format!(
            "Decompressed .gz exceeds the {} MB import limit",
            max_size / (1024 * 1024)
        ));
    }
    Ok(decompressed)
}

pub fn import_single_file(
    conn: &Connection,
    vault_path: &Path,
    path_str: &str,
    encryption_key: Option<&[u8; 32]>,
) -> Result<ImportOutcome, String> {
    let source_path = Path::new(path_str);

    // 1. Detect format from the extension (inner extension for .gz) —
    // unsupported files are rejected before any bytes are read.
    let gz_inner = gz_inner_ext(source_path)?;
    let effective_ext = match &gz_inner {
        Some(inner_ext) => inner_ext.as_str(),
        None => source_path
            .extension()
            .and_then(|e| e.to_str())
            .ok_or_else(|| "No file extension".to_string())?,
    };
    let format = FileFormat::from_extension(effective_ext)
        .ok_or_else(|| format!("Unsupported format: {}", effective_ext))?;

    // 2. Read the original once: hashed for dedup, and fed to the gz decoder.
    // Size-gated first — the whole file lands in memory.
    let size = fs::metadata(source_path)
        .map_err(|e| format!("Failed to read file: {}", e))?
        .len();
    ensure_import_size(size)?;
    let file_bytes =
        fs::read(source_path).map_err(|e| format!("Failed to read file: {}", e))?;
    let hash = hex::encode(Sha256::digest(&file_bytes));

    // 3. Check hash dedup — before decompression, so known files cost nothing
    if db::raw_files::hash_exists(conn, &hash).map_err(|e| e.to_string())? {
        return Ok(ImportOutcome::Skipped);
    }

    // 4. Decompress .gz in memory (size-capped); no plaintext temp file
    let decompressed = match gz_inner {
        Some(_) => Some(decompress_gz(&file_bytes, MAX_GZ_DECOMPRESSED)?),
        None => None,
    };
    let effective_bytes: &[u8] = decompressed.as_deref().unwrap_or(&file_bytes);

    // 5. Generate IDs
    let activity_id = Uuid::new_v4().to_string();
    let raw_file_id = Uuid::new_v4().to_string();

    // 6. Parse (the decompressed bytes if .gz)
    let parsed = match format {
        FileFormat::Gpx => parser::gpx::parse_gpx_bytes(effective_bytes, &activity_id)?,
        FileFormat::Fit => parser::fit::parse_fit_bytes(effective_bytes, &activity_id)?,
        FileFormat::Tcx => parser::tcx::parse_tcx_bytes(effective_bytes, &activity_id)?,
    };

    // 6. Resolve start_time: file header → first trackpoint timestamp → reject
    let start_time = if let Some(ref st) = parsed.start_time {
        st.clone()
    } else {
        // Try to extract from first trackpoint with a timestamp
        let tp_time = parsed.trackpoints.iter().find_map(|tp| tp.t.clone());
        match tp_time {
            Some(t) => t,
            None => {
                return Err(
                    "Not an activity file (no timestamp in file or trackpoints)".to_string(),
                );
            }
        }
    };

    let metrics = compute_metrics(&parsed.trackpoints);

    let sub_sport = parsed
        .session_metrics
        .as_ref()
        .and_then(|s| s.sub_sport.as_deref());
    let sport =
        crate::models::activity::SportType::resolve(parsed.sport_type.as_deref(), sub_sport);

    // Session metrics from FIT file take priority over computed metrics.
    // Duration prefers the session's timer time — what the device showed,
    // pauses excluded — over wall-clock elapsed time; a paused ride would
    // otherwise report hours it never recorded.
    let sm = &parsed.session_metrics;
    let dedup_distance = sm.as_ref().and_then(|s| s.total_distance_m).or(metrics.distance_m);
    let session_duration_s = preferred_duration_s(sm.as_ref(), metrics.duration_s);

    if let Some(existing_id) =
        dedup::find_content_duplicate(conn, &start_time, sport.as_str(), dedup_distance, session_duration_s, false)
            .map_err(|e| e.to_string())?
    {
        // Still store the raw file, LINKED to the existing activity — an
        // unlinked (NULL) row would survive delete_activity forever: its GPS
        // bytes stay in raw/ and its hash blocks any reimport.
        let mut dest = store_raw_file(vault_path, effective_bytes, &raw_file_id, effective_ext)?;
        if let Some(key) = encryption_key {
            let abs_path = vault_path.join(&dest);
            let enc_path = crate::crypto::encrypt_file(key, &abs_path)?;
            dest = enc_path
                .strip_prefix(vault_path)
                .unwrap_or(&enc_path)
                .to_string_lossy()
                .to_string();
        }
        let raw = RawFile {
            id: raw_file_id,
            activity_id: Some(existing_id),
            path_in_vault: dest,
            original_path: Some(path_str.to_string()),
            format: format.as_str().to_string(),
            hash_sha256: hash,
            imported_at: String::new(),
            parse_status: ParseStatus::Ok.as_str().to_string(),
            failure_reason: None,
        };
        db::raw_files::insert_raw_file(conn, &raw).map_err(|e| e.to_string())?;
        return Ok(ImportOutcome::Skipped);
    }

    // 7. Store raw file in vault (encrypt if encryption is active)
    let mut dest = store_raw_file(vault_path, effective_bytes, &raw_file_id, effective_ext)?;
    if let Some(key) = encryption_key {
        let abs_path = vault_path.join(&dest);
        let enc_path = crate::crypto::encrypt_file(key, &abs_path)?;
        dest = enc_path
            .strip_prefix(vault_path)
            .unwrap_or(&enc_path)
            .to_string_lossy()
            .to_string();
    }

    // 8. Location name — deferred to background geocoding thread (avoids blocking import)
    let location_name: Option<String> = None;

    // 9. Create activity — generate a default title when the file carries none.
    let title = parsed.title.clone().or_else(|| {
        Some(crate::models::activity::default_activity_title(&sport, &start_time))
    });
    let activity = Activity {
        id: activity_id.clone(),
        start_time: start_time.clone(),
        timezone_offset: None,
        sport_type: sport.as_str().to_string(),
        title,
        notes: None,
        distance_m: sm.as_ref().and_then(|s| s.total_distance_m).or(metrics.distance_m),
        duration_s: session_duration_s,
        elev_gain_m: sm.as_ref().and_then(|s| s.total_ascent_m).or(metrics.elev_gain_m),
        elev_loss_m: sm.as_ref().and_then(|s| s.total_descent_m).or(metrics.elev_loss_m),
        avg_speed_mps: sm
            .as_ref()
            .and_then(|s| s.avg_speed_mps)
            .or(metrics.avg_speed_mps)
            // Fall back to overall average when the source omits avg speed.
            .or_else(|| {
                let d = sm.as_ref().and_then(|s| s.total_distance_m).or(metrics.distance_m);
                match (d, session_duration_s) {
                    (Some(d), Some(t)) if d > 0.0 && t > 0.0 => Some(d / t),
                    _ => None,
                }
            }),
        max_speed_mps: sm.as_ref().and_then(|s| s.max_speed_mps).or(metrics.max_speed_mps),
        avg_hr: sm.as_ref().and_then(|s| s.avg_hr).or(metrics.avg_hr),
        max_hr: sm.as_ref().and_then(|s| s.max_hr).or(metrics.max_hr),
        avg_cadence: sm.as_ref().and_then(|s| s.avg_cadence).or(metrics.avg_cadence),
        calories: sm.as_ref().and_then(|s| s.total_calories),
        avg_temperature_c: sm.as_ref().and_then(|s| s.avg_temperature_c),
        max_temperature_c: sm.as_ref().and_then(|s| s.max_temperature_c),
        source_device: parsed.source_device,
        location_name,
        start_lat: None,
        start_lon: None,
        avg_power_w: sm.as_ref().and_then(|s| s.avg_power_w),
        max_power_w: sm.as_ref().and_then(|s| s.max_power_w),
        normalized_power_w: sm.as_ref().and_then(|s| s.normalized_power_w),
        total_work_kj: sm.as_ref().and_then(|s| s.total_work_kj),
        threshold_power_w: sm.as_ref().and_then(|s| s.threshold_power_w),
        training_stress_score: sm.as_ref().and_then(|s| s.training_stress_score),
        intensity_factor: sm.as_ref().and_then(|s| s.intensity_factor),
        training_effect_aerobic: sm.as_ref().and_then(|s| s.training_effect_aerobic),
        training_effect_anaerobic: sm.as_ref().and_then(|s| s.training_effect_anaerobic),
        training_load_peak: sm.as_ref().and_then(|s| s.training_load_peak),
        avg_vertical_oscillation_mm: sm.as_ref().and_then(|s| s.avg_vertical_oscillation_mm),
        avg_stance_time_ms: sm.as_ref().and_then(|s| s.avg_stance_time_ms),
        avg_stance_time_percent: sm.as_ref().and_then(|s| s.avg_stance_time_percent),
        avg_step_length_mm: sm.as_ref().and_then(|s| s.avg_step_length_mm),
        total_strides: sm.as_ref().and_then(|s| s.total_strides),
        min_hr: sm.as_ref().and_then(|s| s.min_hr),
        moving_time_s: sm.as_ref().and_then(|s| s.moving_time_s),
        sub_sport: sm.as_ref().and_then(|s| s.sub_sport.clone()),
        avg_respiration_rate: sm.as_ref().and_then(|s| s.avg_respiration_rate),
        max_respiration_rate: sm.as_ref().and_then(|s| s.max_respiration_rate),
        hrv_rmssd: sm.as_ref().and_then(|s| s.hrv_rmssd),
        hrv_sdrr: sm.as_ref().and_then(|s| s.hrv_sdrr),
        end_lat: sm.as_ref().and_then(|s| s.end_lat),
        end_lon: sm.as_ref().and_then(|s| s.end_lon),
        avg_left_torque_effectiveness: sm.as_ref().and_then(|s| s.avg_left_torque_effectiveness),
        avg_right_torque_effectiveness: sm.as_ref().and_then(|s| s.avg_right_torque_effectiveness),
        avg_left_pedal_smoothness: sm.as_ref().and_then(|s| s.avg_left_pedal_smoothness),
        avg_right_pedal_smoothness: sm.as_ref().and_then(|s| s.avg_right_pedal_smoothness),
        avg_left_right_balance: sm.as_ref().and_then(|s| s.avg_left_right_balance),
        avg_left_pco_mm: sm.as_ref().and_then(|s| s.avg_left_pco_mm),
        avg_right_pco_mm: sm.as_ref().and_then(|s| s.avg_right_pco_mm),
        avg_left_power_phase_start_deg: sm.as_ref().and_then(|s| s.avg_left_power_phase_start_deg),
        avg_left_power_phase_end_deg: sm.as_ref().and_then(|s| s.avg_left_power_phase_end_deg),
        avg_left_power_phase_peak_start_deg: sm.as_ref().and_then(|s| s.avg_left_power_phase_peak_start_deg),
        avg_left_power_phase_peak_end_deg: sm.as_ref().and_then(|s| s.avg_left_power_phase_peak_end_deg),
        avg_right_power_phase_start_deg: sm.as_ref().and_then(|s| s.avg_right_power_phase_start_deg),
        avg_right_power_phase_end_deg: sm.as_ref().and_then(|s| s.avg_right_power_phase_end_deg),
        avg_right_power_phase_peak_start_deg: sm.as_ref().and_then(|s| s.avg_right_power_phase_peak_start_deg),
        avg_right_power_phase_peak_end_deg: sm.as_ref().and_then(|s| s.avg_right_power_phase_peak_end_deg),
        avg_power_seated_w: sm.as_ref().and_then(|s| s.avg_power_seated_w),
        avg_power_standing_w: sm.as_ref().and_then(|s| s.avg_power_standing_w),
        max_power_seated_w: sm.as_ref().and_then(|s| s.max_power_seated_w),
        max_power_standing_w: sm.as_ref().and_then(|s| s.max_power_standing_w),
        avg_cadence_seated: sm.as_ref().and_then(|s| s.avg_cadence_seated),
        avg_cadence_standing: sm.as_ref().and_then(|s| s.avg_cadence_standing),
        max_cadence_seated: sm.as_ref().and_then(|s| s.max_cadence_seated),
        max_cadence_standing: sm.as_ref().and_then(|s| s.max_cadence_standing),
        time_standing_s: sm.as_ref().and_then(|s| s.time_standing_s),
        stand_count: sm.as_ref().and_then(|s| s.stand_count),
        created_at: String::new(),
        updated_at: String::new(),
        parent_id: None,
    };

    db::activities::insert_activity(conn, &activity).map_err(|e| e.to_string())?;

    // 9. Insert trackpoints in batches
    db::trackpoints::insert_trackpoints(conn, &parsed.trackpoints).map_err(|e| e.to_string())?;

    // 9a. Best-effort splits (running only) — computed from the stored track so
    // cumulative distance / elapsed time match the rest of the app.
    if matches!(sport.as_str(), "run" | "trail_run" | "treadmill") {
        if let Ok(cols) = db::trackpoints::get_trackpoints_columnar(conn, &activity_id) {
            let efforts = crate::import::best_effort::compute_best_efforts(
                &cols.distance_m,
                &cols.t,
                activity.distance_m,
            );
            if !efforts.is_empty() {
                let _ = db::best_efforts::set_best_efforts(conn, &activity_id, &efforts);
            }
        }
    }

    // 9b. Insert laps if present
    if !parsed.laps.is_empty() {
        db::laps::insert_laps(conn, &parsed.laps).map_err(|e| e.to_string())?;
    }

    // 9b². Insert multisport legs if present (triathlon per-leg breakdown)
    if !parsed.legs.is_empty() {
        db::multisport_legs::insert_legs(conn, &parsed.legs).map_err(|e| e.to_string())?;
    }

    // 9c. Insert swim lengths if present
    if !parsed.lengths.is_empty() {
        db::swim_lengths::insert_swim_lengths(conn, &parsed.lengths).map_err(|e| e.to_string())?;
    }

    // 9d. Insert exercise sets if present
    if !parsed.sets.is_empty() {
        db::exercise_sets::insert_exercise_sets(conn, &parsed.sets).map_err(|e| e.to_string())?;
    }

    // 9e. Insert time in zones if present
    if !parsed.time_in_zones.is_empty() {
        db::time_in_zones::insert_time_in_zones(conn, &parsed.time_in_zones).map_err(|e| e.to_string())?;
    }

    // 9f. Insert HRV samples if present
    if !parsed.hrv_samples.is_empty() {
        db::hrv_samples::insert_hrv_samples(conn, &parsed.hrv_samples).map_err(|e| e.to_string())?;
    }

    // 10. Insert raw file record
    let raw = RawFile {
        id: raw_file_id,
        activity_id: Some(activity_id),
        path_in_vault: dest,
        original_path: Some(path_str.to_string()),
        format: format.as_str().to_string(),
        hash_sha256: hash,
        imported_at: String::new(),
        parse_status: ParseStatus::Ok.as_str().to_string(),
        failure_reason: None,
    };
    db::raw_files::insert_raw_file(conn, &raw).map_err(|e| e.to_string())?;

    Ok(ImportOutcome::Imported)
}

fn store_raw_file(
    vault_path: &Path,
    contents: &[u8],
    file_id: &str,
    ext: &str,
) -> Result<String, String> {
    let raw_dir = vault_path.join("raw");
    fs::create_dir_all(&raw_dir).map_err(|e| format!("Failed to create raw dir: {}", e))?;

    let dest_name = format!("{}.{}", file_id, ext);
    fs::write(raw_dir.join(&dest_name), contents)
        .map_err(|e| format!("Failed to copy file: {}", e))?;

    Ok(format!("raw/{}", dest_name))
}

struct ComputedMetrics {
    distance_m: Option<f64>,
    duration_s: Option<f64>,
    elev_gain_m: Option<f64>,
    elev_loss_m: Option<f64>,
    avg_speed_mps: Option<f64>,
    max_speed_mps: Option<f64>,
    avg_hr: Option<f64>,
    max_hr: Option<f64>,
    avg_cadence: Option<f64>,
}

fn compute_metrics(trackpoints: &[crate::models::trackpoint::TrackPoint]) -> ComputedMetrics {
    if trackpoints.is_empty() {
        return ComputedMetrics {
            distance_m: None,
            duration_s: None,
            elev_gain_m: None,
            elev_loss_m: None,
            avg_speed_mps: None,
            max_speed_mps: None,
            avg_hr: None,
            max_hr: None,
            avg_cadence: None,
        };
    }

    // Distance (haversine)
    let mut total_distance = 0.0;
    for i in 1..trackpoints.len() {
        if let (Some(lat1), Some(lon1), Some(lat2), Some(lon2)) = (
            trackpoints[i - 1].lat,
            trackpoints[i - 1].lon,
            trackpoints[i].lat,
            trackpoints[i].lon,
        ) {
            total_distance += haversine_m(lat1, lon1, lat2, lon2);
        }
    }

    // Elevation gain/loss with smoothing to reduce GPS noise
    let mut elev_gain = 0.0;
    let mut elev_loss = 0.0;
    let alts: Vec<f64> = trackpoints.iter().filter_map(|tp| tp.altitude_m).collect();
    if alts.len() > 2 {
        // Simple moving average smoothing (window of 5)
        let smoothed: Vec<f64> = if alts.len() >= 5 {
            (0..alts.len())
                .map(|i| {
                    let start = i.saturating_sub(2);
                    let end = if i + 2 < alts.len() { i + 3 } else { alts.len() };
                    let sum: f64 = alts[start..end].iter().sum();
                    sum / (end - start) as f64
                })
                .collect()
        } else {
            alts.clone()
        };

        for i in 1..smoothed.len() {
            let diff = smoothed[i] - smoothed[i - 1];
            if diff > 0.5 {
                elev_gain += diff;
            } else if diff < -0.5 {
                elev_loss += diff.abs();
            }
        }
    }

    // Speed
    let speeds: Vec<f64> = trackpoints.iter().filter_map(|tp| tp.speed_mps).collect();
    let max_speed = speeds.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let avg_speed = if !speeds.is_empty() {
        Some(speeds.iter().sum::<f64>() / speeds.len() as f64)
    } else if total_distance > 0.0 {
        // Fallback: compute from distance and duration
        None
    } else {
        None
    };

    // HR
    let hrs: Vec<f64> = trackpoints
        .iter()
        .filter_map(|tp| tp.hr.map(|v| v as f64))
        .collect();
    let avg_hr = if !hrs.is_empty() {
        Some(hrs.iter().sum::<f64>() / hrs.len() as f64)
    } else {
        None
    };
    let max_hr = hrs.iter().cloned().fold(None, |max, v| {
        Some(max.map_or(v, |m: f64| m.max(v)))
    });

    // Cadence
    let cads: Vec<f64> = trackpoints
        .iter()
        .filter_map(|tp| tp.cadence.map(|v| v as f64))
        .collect();
    let avg_cadence = if !cads.is_empty() {
        Some(cads.iter().sum::<f64>() / cads.len() as f64)
    } else {
        None
    };

    // Duration from first to last timestamp
    let duration_s = compute_duration(trackpoints);

    ComputedMetrics {
        distance_m: Some(total_distance),
        duration_s,
        elev_gain_m: if alts.len() > 1 {
            Some(elev_gain)
        } else {
            None
        },
        elev_loss_m: if alts.len() > 1 {
            Some(elev_loss)
        } else {
            None
        },
        avg_speed_mps: avg_speed,
        max_speed_mps: if max_speed.is_finite() {
            Some(max_speed)
        } else {
            None
        },
        avg_hr,
        max_hr,
        avg_cadence,
    }
}

/// The duration stored on an activity: the session's timer time (what the
/// device counted, pauses excluded), else the session's elapsed time (TCX
/// carries only that, and its TotalTimeSeconds is already active time), else
/// the trackpoint span.
fn preferred_duration_s(
    sm: Option<&crate::parser::SessionMetrics>,
    computed: Option<f64>,
) -> Option<f64> {
    sm.and_then(|s| s.total_timer_time_s.or(s.total_elapsed_time_s)).or(computed)
}

fn compute_duration(trackpoints: &[crate::models::trackpoint::TrackPoint]) -> Option<f64> {
    // Find first and last valid timestamps
    let first_ts = trackpoints.iter().find_map(|tp| tp.t.as_ref())?;
    let last_ts = trackpoints.iter().rev().find_map(|tp| tp.t.as_ref())?;

    // Try to parse as ISO 8601 / RFC 3339
    let first = chrono::DateTime::parse_from_rfc3339(first_ts).ok()?;
    let last = chrono::DateTime::parse_from_rfc3339(last_ts).ok()?;

    let duration = (last - first).num_seconds() as f64;
    if duration > 0.0 {
        Some(duration)
    } else {
        None
    }
}

pub(crate) fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6_371_000.0;
    let d_lat = (lat2 - lat1).to_radians();
    let d_lon = (lon2 - lon1).to_radians();
    let a = (d_lat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (d_lon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().asin();
    r * c
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::trackpoint::TrackPoint;

    /// A content-duplicate import (same workout from a second source, so a
    /// different hash) stores its raw file LINKED to the matched activity.
    /// An unlinked (NULL) row would survive delete_activity forever: GPS
    /// bytes left in raw/, reimport eternally Skipped.
    #[test]
    fn content_duplicate_import_links_its_raw_file() {
        let conn = crate::db::test_db();
        let tmp = std::env::temp_dir().join(format!("syz_dup_link_{}", uuid::Uuid::new_v4()));
        let vault = tmp.join("vault");
        fs::create_dir_all(vault.join("raw")).unwrap();

        let gpx = |extra: &str| {
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<gpx version="1.1" creator="test" xmlns="http://www.topografix.com/GPX/1/1">
  <trk><name>Dup run</name>{extra}<trkseg>
    <trkpt lat="55.7500" lon="37.6200"><time>2025-06-01T08:00:00Z</time></trkpt>
    <trkpt lat="55.7600" lon="37.6200"><time>2025-06-01T08:10:00Z</time></trkpt>
  </trkseg></trk>
</gpx>"#
            )
        };
        let first = tmp.join("garmin.gpx");
        let second = tmp.join("strava.gpx");
        fs::write(&first, gpx("")).unwrap();
        // Same workout, different bytes (different hash) → content-dup path.
        fs::write(&second, gpx("<desc>exported elsewhere</desc>")).unwrap();

        let r1 = import_single_file(&conn, &vault, first.to_str().unwrap(), None).unwrap();
        assert!(matches!(r1, ImportOutcome::Imported));
        let r2 = import_single_file(&conn, &vault, second.to_str().unwrap(), None).unwrap();
        assert!(matches!(r2, ImportOutcome::Skipped));

        // Both raw rows point at the one activity — none is orphaned.
        let mut stmt = conn
            .prepare("SELECT activity_id FROM raw_file ORDER BY imported_at")
            .unwrap();
        let owners: Vec<Option<String>> = stmt
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(owners.len(), 2);
        let activity_id: String = conn
            .query_row("SELECT id FROM activity", [], |r| r.get(0))
            .unwrap();
        assert!(
            owners.iter().all(|o| o.as_deref() == Some(activity_id.as_str())),
            "every raw row must link to the matched activity, got {:?}",
            owners
        );

        let _ = fs::remove_dir_all(&tmp);
    }

    /// Imports are read whole into memory — the size gate must reject
    /// over-limit files before the read, and pass everything under it.
    #[test]
    fn import_size_gate() {
        assert!(ensure_import_size(0).is_ok());
        assert!(ensure_import_size(MAX_IMPORT_BYTES).is_ok());
        let err = ensure_import_size(MAX_IMPORT_BYTES + 1).unwrap_err();
        assert!(err.contains("import limit"), "got: {}", err);
    }

    fn make_tp(activity_id: &str, t: Option<&str>, lat: Option<f64>, lon: Option<f64>, alt: Option<f64>, speed: Option<f64>, hr: Option<i32>) -> TrackPoint {
        TrackPoint {
            activity_id: activity_id.to_string(),
            t: t.map(|s| s.to_string()),
            lat, lon,
            altitude_m: alt,
            speed_mps: speed,
            hr, cadence: None, power_w: None, temperature_c: None,
            vertical_oscillation_mm: None, stance_time_ms: None, stance_time_percent: None,
            step_length_mm: None, grade_percent: None,
            left_right_balance: None, left_torque_effectiveness: None, right_torque_effectiveness: None,
            left_pedal_smoothness: None, right_pedal_smoothness: None,
        }
    }

    #[test]
    fn haversine_zero_distance() {
        let d = haversine_m(55.75, 37.62, 55.75, 37.62);
        assert!(d.abs() < 0.001);
    }

    #[test]
    fn haversine_known_distance() {
        // ~1 degree latitude ≈ 111 km
        let d = haversine_m(0.0, 0.0, 1.0, 0.0);
        assert!((d - 111_195.0).abs() < 500.0);
    }

    #[test]
    fn compute_metrics_empty() {
        let m = compute_metrics(&[]);
        assert!(m.distance_m.is_none());
        assert!(m.duration_s.is_none());
        assert!(m.avg_hr.is_none());
    }

    #[test]
    fn compute_metrics_basic() {
        let tps = vec![
            make_tp("a", Some("2025-06-01T08:00:00+00:00"), Some(55.75), Some(37.62), Some(100.0), Some(3.0), Some(140)),
            make_tp("a", Some("2025-06-01T08:00:10+00:00"), Some(55.7501), Some(37.6201), Some(105.0), Some(3.2), Some(145)),
            make_tp("a", Some("2025-06-01T08:00:20+00:00"), Some(55.7502), Some(37.6202), Some(103.0), Some(2.8), Some(150)),
        ];

        let m = compute_metrics(&tps);
        assert!(m.distance_m.unwrap() > 0.0);
        assert_eq!(m.duration_s, Some(20.0));
        assert!((m.avg_hr.unwrap() - 145.0).abs() < 0.01);
        assert_eq!(m.max_hr, Some(150.0));
        assert!((m.max_speed_mps.unwrap() - 3.2).abs() < 0.01);
    }

    /// A paused ride: timer time (4h18m) must win over elapsed (6h38m) — the
    /// stored duration is what the device counted, not wall-clock time.
    #[test]
    fn preferred_duration_timer_beats_elapsed() {
        let sm = crate::parser::SessionMetrics {
            total_timer_time_s: Some(15474.0),
            total_elapsed_time_s: Some(23862.0),
            ..Default::default()
        };
        assert_eq!(preferred_duration_s(Some(&sm), Some(23900.0)), Some(15474.0));
    }

    /// TCX sessions carry only elapsed (its TotalTimeSeconds is active time) —
    /// it must be used when timer time is absent, and the trackpoint span is
    /// the last resort.
    #[test]
    fn preferred_duration_fallbacks() {
        let sm = crate::parser::SessionMetrics {
            total_elapsed_time_s: Some(1800.0),
            ..Default::default()
        };
        assert_eq!(preferred_duration_s(Some(&sm), Some(2000.0)), Some(1800.0));
        let empty = crate::parser::SessionMetrics::default();
        assert_eq!(preferred_duration_s(Some(&empty), Some(2000.0)), Some(2000.0));
        assert_eq!(preferred_duration_s(None, Some(2000.0)), Some(2000.0));
        assert_eq!(preferred_duration_s(None, None), None);
    }

    #[test]
    fn compute_duration_valid() {
        let tps = vec![
            make_tp("a", Some("2025-06-01T08:00:00+00:00"), None, None, None, None, None),
            make_tp("a", Some("2025-06-01T08:30:00+00:00"), None, None, None, None, None),
        ];
        assert_eq!(compute_duration(&tps), Some(1800.0));
    }

    #[test]
    fn compute_duration_no_timestamps() {
        let tps = vec![
            make_tp("a", None, None, None, None, None, None),
        ];
        assert_eq!(compute_duration(&tps), None);
    }

    #[test]
    fn elevation_gain_with_smoothing() {
        // Create trackpoints going steadily up 100m
        let tps: Vec<TrackPoint> = (0..20).map(|i| {
            make_tp("a", None, Some(55.75), Some(37.62), Some(100.0 + i as f64 * 5.0), None, None)
        }).collect();

        let m = compute_metrics(&tps);
        assert!(m.elev_gain_m.unwrap() > 50.0); // should have significant gain
        assert!(m.elev_loss_m.unwrap() < 5.0);  // minimal loss
    }

    #[test]
    fn elevation_loss_computed() {
        // Steadily descending
        let tps: Vec<TrackPoint> = (0..20).map(|i| {
            make_tp("a", None, Some(55.75), Some(37.62), Some(200.0 - i as f64 * 5.0), None, None)
        }).collect();

        let m = compute_metrics(&tps);
        assert!(m.elev_loss_m.unwrap() > 50.0);
        assert!(m.elev_gain_m.unwrap() < 5.0);
    }

    #[test]
    fn compute_metrics_no_speeds_no_hr() {
        let tps = vec![
            make_tp("a", Some("2025-06-01T08:00:00+00:00"), Some(55.75), Some(37.62), None, None, None),
            make_tp("a", Some("2025-06-01T08:00:10+00:00"), Some(55.7501), Some(37.6201), None, None, None),
        ];

        let m = compute_metrics(&tps);
        assert!(m.distance_m.unwrap() > 0.0);
        assert!(m.avg_speed_mps.is_none());
        assert!(m.max_speed_mps.is_none());
        assert!(m.avg_hr.is_none());
        assert!(m.max_hr.is_none());
        assert!(m.elev_gain_m.is_none());
    }

    #[test]
    fn compute_metrics_with_cadence() {
        let mut tp1 = make_tp("a", None, None, None, None, None, None);
        tp1.cadence = Some(170);
        let mut tp2 = make_tp("a", None, None, None, None, None, None);
        tp2.cadence = Some(180);

        let m = compute_metrics(&[tp1, tp2]);
        assert_eq!(m.avg_cadence, Some(175.0));
    }

    #[test]
    fn import_gpx_file_integration() {
        let conn = crate::db::test_db();
        let vault_dir = std::env::temp_dir().join("tv_test_vault_import");
        let _ = std::fs::remove_dir_all(&vault_dir);
        std::fs::create_dir_all(&vault_dir).unwrap();

        // Write a sample GPX file
        let gpx_path = vault_dir.join("test_run.gpx");
        std::fs::write(&gpx_path, r#"<?xml version="1.0" encoding="UTF-8"?>
<gpx version="1.1" creator="TestDevice" xmlns="http://www.topografix.com/GPX/1/1">
  <trk>
    <name>Test Run</name>
    <type>Running</type>
    <trkseg>
      <trkpt lat="55.75" lon="37.62">
        <ele>150.0</ele>
        <time>2025-06-01T08:00:00Z</time>
      </trkpt>
      <trkpt lat="55.7501" lon="37.6201">
        <ele>155.0</ele>
        <time>2025-06-01T08:00:10Z</time>
      </trkpt>
    </trkseg>
  </trk>
</gpx>"#).unwrap();

        let result = import_files(&conn, &vault_dir, &[gpx_path.to_str().unwrap().to_string()], None, |_, _, _| {});
        assert_eq!(result.imported, 1);
        assert_eq!(result.skipped, 0);
        assert!(result.failed.is_empty());

        // Verify activity was created
        let activities = crate::db::activities::get_activities(&conn, &crate::models::activity::ActivityFilters::default()).unwrap();
        assert_eq!(activities.len(), 1);
        assert_eq!(activities[0].sport_type, "run");

        // Verify trackpoints
        let tps = crate::db::trackpoints::get_trackpoints_columnar(&conn, &activities[0].id).unwrap();
        assert_eq!(tps.lat.len(), 2);

        // Verify raw file
        let raws = crate::db::raw_files::get_raw_files_for_activity(&conn, &activities[0].id).unwrap();
        assert_eq!(raws.len(), 1);
        assert_eq!(raws[0].format, "gpx");

        std::fs::remove_dir_all(&vault_dir).ok();
    }

    #[test]
    fn import_duplicate_file_skipped_by_hash() {
        let conn = crate::db::test_db();
        let vault_dir = std::env::temp_dir().join("tv_test_vault_dedup");
        let _ = std::fs::remove_dir_all(&vault_dir);
        std::fs::create_dir_all(&vault_dir).unwrap();

        let gpx_path = vault_dir.join("run.gpx");
        std::fs::write(&gpx_path, r#"<?xml version="1.0" encoding="UTF-8"?>
<gpx version="1.1" xmlns="http://www.topografix.com/GPX/1/1">
  <trk><trkseg>
    <trkpt lat="55.75" lon="37.62"><time>2025-06-01T08:00:00Z</time></trkpt>
  </trkseg></trk>
</gpx>"#).unwrap();

        let paths = vec![gpx_path.to_str().unwrap().to_string()];

        // First import
        let r1 = import_files(&conn, &vault_dir, &paths, None, |_, _, _| {});
        assert_eq!(r1.imported, 1);

        // Second import (same file) — should skip by hash
        let r2 = import_files(&conn, &vault_dir, &paths, None, |_, _, _| {});
        assert_eq!(r2.skipped, 1);
        assert_eq!(r2.imported, 0);

        std::fs::remove_dir_all(&vault_dir).ok();
    }

    #[test]
    fn import_tcx_file_integration() {
        let conn = crate::db::test_db();
        let vault_dir = std::env::temp_dir().join("tv_test_vault_tcx");
        let _ = std::fs::remove_dir_all(&vault_dir);
        std::fs::create_dir_all(&vault_dir).unwrap();

        let tcx_path = vault_dir.join("test_run.tcx");
        std::fs::write(&tcx_path, r#"<?xml version="1.0" encoding="UTF-8"?>
<TrainingCenterDatabase xmlns="http://www.garmin.com/xmlschemas/TrainingCenterDatabase/v2">
  <Activities>
    <Activity Sport="Running">
      <Id>2025-06-01T08:00:00Z</Id>
      <Lap StartTime="2025-06-01T08:00:00Z">
        <TotalTimeSeconds>600</TotalTimeSeconds>
        <Trackpoint>
          <Time>2025-06-01T08:00:00Z</Time>
          <Position><LatitudeDegrees>55.75</LatitudeDegrees><LongitudeDegrees>37.62</LongitudeDegrees></Position>
          <AltitudeMeters>150.0</AltitudeMeters>
          <DistanceMeters>0</DistanceMeters>
          <HeartRateBpm><Value>140</Value></HeartRateBpm>
        </Trackpoint>
        <Trackpoint>
          <Time>2025-06-01T08:00:10Z</Time>
          <Position><LatitudeDegrees>55.7501</LatitudeDegrees><LongitudeDegrees>37.6201</LongitudeDegrees></Position>
          <AltitudeMeters>155.0</AltitudeMeters>
          <DistanceMeters>100</DistanceMeters>
          <HeartRateBpm><Value>150</Value></HeartRateBpm>
        </Trackpoint>
      </Lap>
    </Activity>
  </Activities>
</TrainingCenterDatabase>"#).unwrap();

        let result = import_files(&conn, &vault_dir, &[tcx_path.to_str().unwrap().to_string()], None, |_, _, _| {});
        assert_eq!(result.imported, 1);
        assert!(result.failed.is_empty());

        let activities = crate::db::activities::get_activities(&conn, &crate::models::activity::ActivityFilters::default()).unwrap();
        assert_eq!(activities.len(), 1);
        assert_eq!(activities[0].sport_type, "run");
        // Session metrics from TCX should be populated
        assert!(activities[0].distance_m.is_some());

        std::fs::remove_dir_all(&vault_dir).ok();
    }

    #[test]
    fn import_unsupported_extension_fails() {
        let conn = crate::db::test_db();
        let vault_dir = std::env::temp_dir().join("tv_test_vault_bad_ext");
        let _ = std::fs::remove_dir_all(&vault_dir);
        std::fs::create_dir_all(&vault_dir).unwrap();

        let bad_path = vault_dir.join("data.csv");
        std::fs::write(&bad_path, "not a workout").unwrap();

        let result = import_files(&conn, &vault_dir, &[bad_path.to_str().unwrap().to_string()], None, |_, _, _| {});
        assert_eq!(result.imported, 0);
        assert_eq!(result.failed.len(), 1);
        assert!(result.failed[0].reason.contains("Unsupported format"));

        std::fs::remove_dir_all(&vault_dir).ok();
    }

    #[test]
    fn import_nonexistent_file_fails() {
        let conn = crate::db::test_db();
        let vault_dir = std::env::temp_dir().join("tv_test_vault_nofile");
        let _ = std::fs::remove_dir_all(&vault_dir);
        std::fs::create_dir_all(&vault_dir).unwrap();

        let result = import_files(&conn, &vault_dir, &["/nonexistent/path.gpx".to_string()], None, |_, _, _| {});
        assert_eq!(result.failed.len(), 1);

        std::fs::remove_dir_all(&vault_dir).ok();
    }

    #[test]
    fn store_raw_file_creates_dir_and_writes() {
        let vault_dir = std::env::temp_dir().join("tv_test_vault_copy");
        let _ = std::fs::remove_dir_all(&vault_dir);
        std::fs::create_dir_all(&vault_dir).unwrap();

        let dest = store_raw_file(&vault_dir, b"gpx content", "file-123", "gpx").unwrap();
        assert_eq!(dest, "raw/file-123.gpx");
        assert_eq!(
            std::fs::read(vault_dir.join("raw/file-123.gpx")).unwrap(),
            b"gpx content"
        );

        std::fs::remove_dir_all(&vault_dir).ok();
    }

    #[test]
    fn import_with_encryption_creates_enc_file() {
        let conn = crate::db::test_db();
        let vault_dir = std::env::temp_dir().join("tv_test_vault_enc_import");
        let _ = std::fs::remove_dir_all(&vault_dir);
        std::fs::create_dir_all(&vault_dir).unwrap();

        let gpx_path = vault_dir.join("enc_test.gpx");
        std::fs::write(&gpx_path, r#"<?xml version="1.0" encoding="UTF-8"?>
<gpx version="1.1" xmlns="http://www.topografix.com/GPX/1/1">
  <trk><trkseg>
    <trkpt lat="55.75" lon="37.62"><time>2025-06-01T08:00:00Z</time></trkpt>
    <trkpt lat="55.7501" lon="37.6201"><time>2025-06-01T08:00:10Z</time></trkpt>
  </trkseg></trk>
</gpx>"#).unwrap();

        let key = crate::crypto::derive_key("test-import-enc", &[42u8; 32]);
        let result = import_files(
            &conn,
            &vault_dir,
            &[gpx_path.to_str().unwrap().to_string()],
            Some(&key),
            |_, _, _| {},
        );
        assert_eq!(result.imported, 1);

        // Verify raw_file path ends with .enc
        let activities = crate::db::activities::get_activities(
            &conn,
            &crate::models::activity::ActivityFilters::default(),
        ).unwrap();
        assert_eq!(activities.len(), 1);

        let raws = crate::db::raw_files::get_raw_files_for_activity(&conn, &activities[0].id).unwrap();
        assert_eq!(raws.len(), 1);
        assert!(raws[0].path_in_vault.ends_with(".enc"), "Expected .enc path, got: {}", raws[0].path_in_vault);

        // Verify the file on disk is actually encrypted (not plaintext GPX)
        let enc_file = vault_dir.join(&raws[0].path_in_vault);
        assert!(enc_file.exists());
        let enc_data = std::fs::read(&enc_file).unwrap();
        // Encrypted files start with 12-byte nonce, not XML
        assert!(enc_data.len() > 12);
        assert_ne!(&enc_data[..5], b"<?xml");

        std::fs::remove_dir_all(&vault_dir).ok();
    }

    #[test]
    fn enable_disable_encryption_roundtrip() {
        let conn = crate::db::test_db();
        let vault_dir = std::env::temp_dir().join("tv_test_vault_enc_roundtrip");
        let _ = std::fs::remove_dir_all(&vault_dir);
        std::fs::create_dir_all(vault_dir.join("raw")).unwrap();

        // Import a file first (no encryption)
        let gpx_path = vault_dir.join("roundtrip.gpx");
        let gpx_content = r#"<?xml version="1.0" encoding="UTF-8"?>
<gpx version="1.1" xmlns="http://www.topografix.com/GPX/1/1">
  <trk><trkseg>
    <trkpt lat="55.75" lon="37.62"><time>2025-07-01T10:00:00Z</time></trkpt>
  </trkseg></trk>
</gpx>"#;
        std::fs::write(&gpx_path, gpx_content).unwrap();
        let r = import_files(&conn, &vault_dir, &[gpx_path.to_str().unwrap().to_string()], None, |_, _, _| {});
        assert_eq!(r.imported, 1);

        // Get the raw file path
        let activities = crate::db::activities::get_activities(
            &conn,
            &crate::models::activity::ActivityFilters::default(),
        ).unwrap();
        let raws = crate::db::raw_files::get_raw_files_for_activity(&conn, &activities[0].id).unwrap();
        let original_path = raws[0].path_in_vault.clone();
        assert!(!original_path.ends_with(".enc"));
        assert!(vault_dir.join(&original_path).exists());

        // Enable encryption — the per-file callback pairs each rename with
        // its DB row update, exactly like the enable_encryption command.
        let salt = crate::crypto::generate_salt();
        let key = crate::crypto::derive_key("roundtrip-pw", &salt);
        let count = crate::crypto::encrypt_all_raw_files(&key, &vault_dir, &mut |old_p, new_p| {
            crate::db::raw_files::update_path(&conn, old_p, new_p).map_err(|e| e.to_string())
        })
        .unwrap();
        assert_eq!(count, 1);

        // Verify .enc files
        let raws = crate::db::raw_files::get_raw_files_for_activity(&conn, &activities[0].id).unwrap();
        assert!(raws[0].path_in_vault.ends_with(".enc"));
        assert!(vault_dir.join(&raws[0].path_in_vault).exists());
        assert!(!vault_dir.join(&original_path).exists());

        // Disable encryption (decrypt all)
        let dec_count = crate::crypto::decrypt_all_raw_files(&key, &vault_dir, &mut |old_p, new_p| {
            crate::db::raw_files::update_path(&conn, old_p, new_p).map_err(|e| e.to_string())
        })
        .unwrap();
        assert_eq!(dec_count, 1);

        // Verify restored
        let raws = crate::db::raw_files::get_raw_files_for_activity(&conn, &activities[0].id).unwrap();
        assert!(!raws[0].path_in_vault.ends_with(".enc"));
        assert!(vault_dir.join(&raws[0].path_in_vault).exists());

        std::fs::remove_dir_all(&vault_dir).ok();
    }

    #[test]
    fn import_gz_file_integration() {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use std::io::Write;

        let conn = crate::db::test_db();
        let vault_dir = std::env::temp_dir().join("tv_test_vault_gz");
        let _ = std::fs::remove_dir_all(&vault_dir);
        std::fs::create_dir_all(&vault_dir).unwrap();

        let gpx_content = br#"<?xml version="1.0" encoding="UTF-8"?>
<gpx version="1.1" xmlns="http://www.topografix.com/GPX/1/1">
  <trk>
    <name>GZ Test Run</name>
    <type>Running</type>
    <trkseg>
      <trkpt lat="55.75" lon="37.62">
        <ele>150.0</ele>
        <time>2025-09-01T08:00:00Z</time>
      </trkpt>
      <trkpt lat="55.7501" lon="37.6201">
        <ele>155.0</ele>
        <time>2025-09-01T08:00:10Z</time>
      </trkpt>
    </trkseg>
  </trk>
</gpx>"#;

        // Compress to .gpx.gz
        let gz_path = vault_dir.join("test_run.gpx.gz");
        let file = std::fs::File::create(&gz_path).unwrap();
        let mut encoder = GzEncoder::new(file, Compression::default());
        encoder.write_all(gpx_content).unwrap();
        encoder.finish().unwrap();

        let result = import_files(
            &conn,
            &vault_dir,
            &[gz_path.to_str().unwrap().to_string()],
            None,
            |_, _, _| {},
        );
        assert_eq!(result.imported, 1);
        assert_eq!(result.skipped, 0);
        assert!(result.failed.is_empty());

        let activities = crate::db::activities::get_activities(
            &conn,
            &crate::models::activity::ActivityFilters::default(),
        )
        .unwrap();
        assert_eq!(activities.len(), 1);
        assert_eq!(activities[0].sport_type, "run");

        // Raw file in vault should be stored as .gpx (decompressed)
        let raws =
            crate::db::raw_files::get_raw_files_for_activity(&conn, &activities[0].id).unwrap();
        assert_eq!(raws.len(), 1);
        assert!(
            raws[0].path_in_vault.ends_with(".gpx"),
            "Expected .gpx, got: {}",
            raws[0].path_in_vault
        );

        std::fs::remove_dir_all(&vault_dir).ok();
    }

    #[test]
    fn gz_inner_ext_returns_none_for_regular_file() {
        let result = gz_inner_ext(Path::new("/tmp/test.fit")).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn gz_inner_ext_extracts_inner_extension() {
        let result = gz_inner_ext(Path::new("/tmp/activity.FIT.gz")).unwrap();
        assert_eq!(result.as_deref(), Some("fit"));
    }

    fn gz_bytes(payload: &[u8]) -> Vec<u8> {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use std::io::Write;
        let mut enc = GzEncoder::new(Vec::new(), Compression::default());
        enc.write_all(payload).unwrap();
        enc.finish().unwrap()
    }

    #[test]
    fn decompress_gz_roundtrip_within_limit() {
        let payload = b"<gpx>small workout</gpx>";
        let out = decompress_gz(&gz_bytes(payload), 1024).unwrap();
        assert_eq!(out, payload);
    }

    #[test]
    fn decompress_gz_rejects_oversized_output() {
        // A decompression bomb in miniature: 1 KiB of zeros against a 100-byte
        // cap. Must error out, not truncate or allocate unboundedly.
        let bomb = gz_bytes(&[0u8; 1024]);
        let err = decompress_gz(&bomb, 100).unwrap_err();
        assert!(err.contains("exceeds"), "unexpected error: {}", err);
    }

    #[test]
    fn import_oversized_gz_fails_cleanly() {
        // End-to-end: an over-limit .gpx.gz is reported as a failed file and
        // creates no activity.
        let conn = crate::db::test_db();
        let vault_dir = std::env::temp_dir().join("tv_test_vault_gz_bomb");
        let _ = std::fs::remove_dir_all(&vault_dir);
        std::fs::create_dir_all(&vault_dir).unwrap();

        // 128 MiB of zeros compresses to ~130 KB but blows past the cap.
        let gz_path = vault_dir.join("bomb.gpx.gz");
        std::fs::write(&gz_path, gz_bytes(&vec![0u8; 128 * 1024 * 1024])).unwrap();

        let result = import_files(
            &conn,
            &vault_dir,
            &[gz_path.to_str().unwrap().to_string()],
            None,
            |_, _, _| {},
        );
        assert_eq!(result.imported, 0);
        assert_eq!(result.failed.len(), 1);
        assert!(result.failed[0].reason.contains("exceeds"));

        let activities = crate::db::activities::get_activities(
            &conn,
            &crate::models::activity::ActivityFilters::default(),
        )
        .unwrap();
        assert!(activities.is_empty());

        std::fs::remove_dir_all(&vault_dir).ok();
    }

    #[test]
    fn maybe_decompress_gz_rejects_unknown_inner_ext() {
        let tmp = std::env::temp_dir().join("test.csv.gz");
        // Write a minimal gzip
        {
            use flate2::write::GzEncoder;
            use flate2::Compression;
            use std::io::Write;
            let f = std::fs::File::create(&tmp).unwrap();
            let mut enc = GzEncoder::new(f, Compression::default());
            enc.write_all(b"not a workout").unwrap();
            enc.finish().unwrap();
        }
        let result = import_files(
            &crate::db::test_db(),
            &std::env::temp_dir(),
            &[tmp.to_str().unwrap().to_string()],
            None,
            |_, _, _| {},
        );
        assert_eq!(result.imported, 0);
        assert_eq!(result.failed.len(), 1);
        assert!(result.failed[0].reason.contains("Unsupported format"));
        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn import_empty_fit_file_skipped() {
        let conn = crate::db::test_db();
        let vault_dir = std::env::temp_dir().join("tv_test_vault_empty_fit");
        let _ = std::fs::remove_dir_all(&vault_dir);
        std::fs::create_dir_all(&vault_dir).unwrap();

        // Minimal valid FIT file header (14 bytes) with no data records
        // This simulates a non-activity FIT file (e.g. Strava settings/segments)
        let fit_path = vault_dir.join("settings.fit");
        let header: [u8; 14] = [14, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, b'.', b'F', b'I', b'T', 0x00, 0x00];
        std::fs::write(&fit_path, header).unwrap();

        let result = import_files(
            &conn,
            &vault_dir,
            &[fit_path.to_str().unwrap().to_string()],
            None,
            |_, _, _| {},
        );
        // Should fail (not a real activity), not create an empty activity
        assert_eq!(result.imported, 0);

        let activities = crate::db::activities::get_activities(
            &conn,
            &crate::models::activity::ActivityFilters::default(),
        ).unwrap();
        assert_eq!(activities.len(), 0);

        std::fs::remove_dir_all(&vault_dir).ok();
    }
}
