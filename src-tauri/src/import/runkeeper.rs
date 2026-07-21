//! Runkeeper data export (`.zip`) import data source.
//!
//! A Runkeeper export is a flat zip of per-workout `*.gpx` plus a
//! `cardioActivities.csv` listing every activity. We import the GPX files
//! through the normal pipeline (real tracks) and the CSV rows that have no GPX
//! (swimming, manual, indoor) as GPS-less activities — so nothing is lost.
//! Dedup makes re-importing safe.

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use uuid::Uuid;
use zip::ZipArchive;

use crate::import::pipeline::{self, FailedFile, ImportResult};
use crate::import::runkeeper_csv;

// Bounds on a (possibly malicious) export so it can't OOM or flood the disk.
const MAX_GPX_BYTES: u64 = 64 * 1024 * 1024; // 64 MiB per GPX
const MAX_CSV_BYTES: u64 = 256 * 1024 * 1024; // 256 MiB for the CSV
const MAX_TOTAL_BYTES: u64 = 2 * 1024 * 1024 * 1024; // 2 GiB uncompressed total
const MAX_FILES: usize = 5000;

/// Remove a temp directory on drop.
struct TempDir(PathBuf);
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub fn import_zip(
    conn: &Connection,
    vault_path: &Path,
    zip_path: &str,
    encryption_key: Option<&[u8; 32]>,
) -> Result<ImportResult, String> {
    let file = fs::File::open(zip_path).map_err(|e| format!("Failed to open export: {e}"))?;
    let mut zip = ZipArchive::new(file).map_err(|e| format!("Not a valid .zip export: {e}"))?;

    let tmp = std::env::temp_dir().join(format!("rk_import_{}", Uuid::new_v4()));
    fs::create_dir_all(&tmp).map_err(|e| format!("Failed to create temp dir: {e}"))?;
    let _guard = TempDir(tmp.clone());

    let mut gpx_entries: Vec<(String, String)> = Vec::new(); // (temp path, original name)
    let mut csv_path: Option<PathBuf> = None;
    let mut total_bytes: u64 = 0;
    let mut file_count: usize = 0;

    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| format!("Corrupt zip entry: {e}"))?;
        // Only regular files: this deliberately rejects directory and symlink
        // entries (a zip symlink could otherwise be a write-outside vector).
        if !entry.is_file() {
            continue;
        }
        // Flatten to the bare filename — neutralizes any path traversal in names
        // (`../x`, `/abs/x`, UNC). See the traversal test below.
        let Some(fname) = Path::new(entry.name()).file_name().map(|n| n.to_string_lossy().into_owned())
        else {
            continue;
        };
        let lower = fname.to_ascii_lowercase();
        let is_gpx = lower.ends_with(".gpx");
        let is_csv = lower == "cardioactivities.csv";
        if !is_gpx && !is_csv {
            continue;
        }

        file_count += 1;
        if file_count > MAX_FILES {
            return Err(format!("export has too many files (limit {MAX_FILES})"));
        }

        let cap = if is_csv { MAX_CSV_BYTES } else { MAX_GPX_BYTES };
        let bytes = crate::util::read_capped(&mut entry, cap, &fname)?;
        total_bytes += bytes.len() as u64;
        if total_bytes > MAX_TOTAL_BYTES {
            return Err("export is too large (uncompressed) — refusing to import".to_string());
        }

        // Prefix with the entry index so same-named entries in different folders
        // don't overwrite each other.
        let dest = tmp.join(format!("{i}_{fname}"));
        fs::write(&dest, &bytes).map_err(|e| format!("Failed to extract {fname}: {e}"))?;

        if is_gpx {
            gpx_entries.push((dest.to_string_lossy().into_owned(), fname));
        } else {
            csv_path = Some(dest);
        }
    }

    gpx_entries.sort();
    let gpx_paths: Vec<String> = gpx_entries.iter().map(|(p, _)| p.clone()).collect();

    // 1) GPX first (real tracks). 2) CSV adds only the GPS-less rows (it skips
    //    rows that reference a GPX file). Dedup guards any overlap.
    let mut result = pipeline::import_files(conn, vault_path, &gpx_paths, encryption_key, |_, _, _| {});
    // Provenance: store the real export filename, not the (soon-deleted) temp path.
    for (temp, name) in &gpx_entries {
        let _ = conn.execute(
            "UPDATE raw_file SET original_path = ?1 WHERE original_path = ?2",
            rusqlite::params![name, temp],
        );
    }
    if let Some(csv) = csv_path {
        // A broken CSV must not discard the already-imported GPX — record it as a
        // failure instead of aborting the whole import.
        match runkeeper_csv::import_runkeeper_csv(conn, &csv.to_string_lossy()) {
            Ok(csv_res) => {
                result.imported += csv_res.imported;
                result.skipped += csv_res.skipped;
                result.failed.extend(csv_res.failed);
            }
            Err(e) => result.failed.push(FailedFile {
                path: "cardioActivities.csv".to_string(),
                reason: e,
            }),
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    const GPX: &str = r#"<?xml version="1.0"?>
<gpx version="1.1" creator="RunKeeper" xmlns="http://www.topografix.com/GPX/1/1">
<trk><name>Running 1/1/15</name><time>2015-01-01T08:00:00Z</time><trkseg>
<trkpt lat="55.75" lon="37.62"><ele>150</ele><time>2015-01-01T08:00:00Z</time></trkpt>
<trkpt lat="55.751" lon="37.621"><ele>151</ele><time>2015-01-01T08:01:00Z</time></trkpt>
</trkseg></trk></gpx>"#;

    const CSV: &str = "Date,Type,Route Name,Distance (km),Duration,Average Pace,Average Speed (km/h),Calories Burned,Climb (m),Average Heart Rate (bpm),Notes,GPX File\n\
2015-01-01 08:00:00,Running,,2.0,10:00,,12.0,150,5,,,2015-01-01-0800.gpx\n\
2015-01-02 07:00:00,Swimming,,1.0,30:00,,2.0,300,0,,,\n";

    #[test]
    fn imports_gpx_and_gpsless_from_zip() {
        let conn = db::test_db();
        let dir = std::env::temp_dir().join(format!("rk_zip_test_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let zip_path = dir.join("export.zip");
        {
            let f = fs::File::create(&zip_path).unwrap();
            let mut zw = zip::ZipWriter::new(f);
            let o = SimpleFileOptions::default();
            zw.start_file("2015-01-01-0800.gpx", o).unwrap();
            zw.write_all(GPX.as_bytes()).unwrap();
            zw.start_file("cardioActivities.csv", o).unwrap();
            zw.write_all(CSV.as_bytes()).unwrap();
            zw.finish().unwrap();
        }

        let r = import_zip(&conn, &dir, zip_path.to_str().unwrap(), None).unwrap();
        // 1 GPX (running) + 1 GPS-less CSV row (swimming); the CSV running row is
        // skipped because it references a GPX file.
        assert_eq!(r.imported, 2, "{r:?}");

        let count: i64 = conn.query_row("SELECT COUNT(*) FROM activity", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 2);
        let swim: i64 = conn
            .query_row("SELECT COUNT(*) FROM activity WHERE sport_type='swim'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(swim, 1);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn runkeeper_in_datasource_list() {
        let sources = crate::import::datasource::list();
        assert!(sources.iter().any(|d| d.id == "runkeeper" && d.extensions == ["zip"]));
    }

    fn gpx_at(date: &str, lat: &str) -> String {
        format!(
            "<?xml version=\"1.0\"?>\n<gpx version=\"1.1\" creator=\"RunKeeper\" xmlns=\"http://www.topografix.com/GPX/1/1\">\n\
<trk><name>Running</name><time>{date}T08:00:00Z</time><trkseg>\n\
<trkpt lat=\"{lat}\" lon=\"37.62\"><ele>150</ele><time>{date}T08:00:00Z</time></trkpt>\n\
<trkpt lat=\"{lat}\" lon=\"37.63\"><ele>151</ele><time>{date}T08:05:00Z</time></trkpt>\n\
</trkseg></trk></gpx>"
        )
    }

    fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
        use std::io::Write;
        use zip::write::SimpleFileOptions;
        let f = fs::File::create(path).unwrap();
        let mut zw = zip::ZipWriter::new(f);
        for (name, bytes) in entries {
            zw.start_file(*name, SimpleFileOptions::default()).unwrap();
            zw.write_all(bytes).unwrap();
        }
        zw.finish().unwrap();
    }

    #[test]
    fn duplicate_gpx_names_do_not_collapse() {
        let conn = db::test_db();
        let dir = std::env::temp_dir().join(format!("rk_dup_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let zip_path = dir.join("export.zip");
        // Same base name in two folders, different content (different dates/coords).
        let a = gpx_at("2015-01-01", "55.10");
        let b = gpx_at("2015-02-02", "55.20");
        write_zip(&zip_path, &[("a/run.gpx", a.as_bytes()), ("b/run.gpx", b.as_bytes())]);

        let r = import_zip(&conn, &dir, zip_path.to_str().unwrap(), None).unwrap();
        assert_eq!(r.imported, 2, "both GPX must import, not collapse: {r:?}");
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM activity", [], |x| x.get(0)).unwrap();
        assert_eq!(count, 2);
        // N1: original_path is the real export filename, not a temp path.
        let orig: String = conn
            .query_row("SELECT DISTINCT original_path FROM raw_file", [], |x| x.get(0))
            .unwrap();
        assert_eq!(orig, "run.gpx", "provenance is the export filename");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn traversal_named_entry_stays_inside_temp() {
        let conn = db::test_db();
        let dir = std::env::temp_dir().join(format!("rk_slip_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let zip_path = dir.join("export.zip");
        // A malicious entry name; file_name() must flatten it to evil.gpx in temp.
        write_zip(&zip_path, &[("../../../evil.gpx", gpx_at("2015-03-03", "55.40").as_bytes())]);

        let r = import_zip(&conn, &dir, zip_path.to_str().unwrap(), None).unwrap();
        assert_eq!(r.imported, 1, "imported as a normal flattened file");
        // Nothing escaped to the parent of the temp dir.
        assert!(!dir.parent().unwrap().join("evil.gpx").exists());
        fs::remove_dir_all(&dir).ok();
    }

    /// With the (scope-gated) vault key, the GPX raw file lands in the vault
    /// already encrypted — a datasource import must not open a plaintext
    /// window any more than the manual-import pipeline does.
    #[test]
    fn import_zip_encrypts_raw_files_with_key() {
        let conn = db::test_db();
        let dir = std::env::temp_dir().join(format!("rk_enc_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let zip_path = dir.join("export.zip");
        write_zip(&zip_path, &[("run.gpx", gpx_at("2015-04-04", "55.50").as_bytes())]);

        let key = [8u8; 32];
        let r = import_zip(&conn, &dir, zip_path.to_str().unwrap(), Some(&key)).unwrap();
        assert_eq!(r.imported, 1, "{r:?}");

        // The DB row points at .enc, the ciphertext exists, no plaintext copy.
        let stored: String = conn
            .query_row("SELECT path_in_vault FROM raw_file", [], |x| x.get(0))
            .unwrap();
        assert!(stored.ends_with(".enc"), "raw file stored encrypted: {stored}");
        assert!(dir.join(&stored).exists());
        assert!(!dir.join(stored.trim_end_matches(".enc")).exists());

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn broken_csv_does_not_abort_gpx_import() {
        let conn = db::test_db();
        let dir = std::env::temp_dir().join(format!("rk_badcsv_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let zip_path = dir.join("export.zip");
        // A valid GPX + a CSV that isn't a Runkeeper export (missing Date/Type).
        write_zip(
            &zip_path,
            &[
                ("2015-01-01-0800.gpx", gpx_at("2015-01-01", "55.30").as_bytes()),
                ("cardioActivities.csv", b"garbage,not,runkeeper\n1,2,3\n"),
            ],
        );

        let r = import_zip(&conn, &dir, zip_path.to_str().unwrap(), None).unwrap();
        assert_eq!(r.imported, 1, "GPX still imported despite the bad CSV");
        assert!(
            r.failed.iter().any(|f| f.path.contains("cardioActivities")),
            "the CSV error is reported, not silently dropped: {r:?}"
        );
        fs::remove_dir_all(&dir).ok();
    }
}
