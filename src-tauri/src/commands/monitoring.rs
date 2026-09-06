use tauri::State;

use crate::db;
use crate::models::monitoring::{MonitoringDeleted, MonitoringSummary};
use crate::state::AppState;

/// Settings → Vault: how much Garmin monitoring the vault holds.
#[tauri::command]
pub fn get_monitoring_summary(state: State<AppState>) -> Result<MonitoringSummary, String> {
    // While locked the connection is a schema-less stand-in — say "locked",
    // not "no such table".
    super::import::ensure_vault_unlocked(&state)?;
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::monitoring::summary(&conn).map_err(|e| e.to_string())
}

/// Delete the monitoring data of an inclusive local-date range (ADR 0002,
/// privacy): the readings and day rows, and every Monitor file that no
/// remaining day still overlaps — file, crash-drift sibling and raw_file
/// row, so its hash stops blocking a re-import.
#[tauri::command]
pub fn delete_monitoring_range(
    from: String,
    to: String,
    state: State<AppState>,
) -> Result<MonitoringDeleted, String> {
    delete_monitoring_range_core(&state, &from, &to)
}

fn is_date(s: &str) -> bool {
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok()
}

/// The State-free part, testable against a bare [`AppState`].
pub(crate) fn delete_monitoring_range_core(
    state: &AppState,
    from: &str,
    to: &str,
) -> Result<MonitoringDeleted, String> {
    if !is_date(from) || !is_date(to) {
        return Err("Dates must be YYYY-MM-DD".to_string());
    }
    if to < from {
        return Err("The end date is before the start date".to_string());
    }
    super::import::ensure_vault_unlocked(state)?;
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut out = MonitoringDeleted { days: 0, files: 0, failed: 0, error: None };
    // Each file is tried once per call: one the OS refuses would otherwise
    // be counted again by the closing sweep.
    let mut tried = std::collections::BTreeSet::new();
    // Whatever an earlier delete left behind (its rows committed, the
    // process gone before the files were) goes first — the operation is
    // idempotent, a retry of any range finishes the previous one.
    let orphans = db::raw_files::orphan_monitoring_files(&conn).map_err(|e| e.to_string())?;
    drop_files(state, &conn, &orphans, &mut out, &mut tried)?;
    out.days = db::monitoring::count_days(&conn, from, to).map_err(|e| e.to_string())?;
    let released = db::monitoring::delete_range(&conn, from, to).map_err(|e| e.to_string())?;
    let paths =
        db::raw_files::monitoring_paths_by_ids(&conn, &released).map_err(|e| e.to_string())?;
    drop_files(state, &conn, &paths, &mut out, &mut tried)?;
    // Files delete_range cannot name: a corrupt timestamp kept `store`
    // from writing their span, yet their readings sat in ordinary days —
    // once those days are gone nothing references the file any more.
    let orphans = db::raw_files::orphan_monitoring_files(&conn).map_err(|e| e.to_string())?;
    drop_files(state, &conn, &orphans, &mut out, &mut tried)?;
    Ok(out)
}

/// Remove `(id, path)` files from the vault and, only once a file is really
/// gone, its raw_file row — a row without a file would keep the hash
/// blocking a re-import, a file without a row would sit in raw/ unseen.
fn drop_files(
    state: &AppState,
    conn: &rusqlite::Connection,
    files: &[(String, String)],
    out: &mut MonitoringDeleted,
    tried: &mut std::collections::BTreeSet<String>,
) -> Result<(), String> {
    for (id, path) in files {
        if !tried.insert(id.clone()) {
            continue;
        }
        // Same as delete_activity: the file and its x ↔ x.enc sibling (the
        // DB path can lag the disk name after an interrupted encryption
        // toggle). Already-missing is fine — the row is the truth.
        let sibling = match path.strip_suffix(".enc") {
            Some(plain) => plain.to_string(),
            None => format!("{path}.enc"),
        };
        let result = remove_if_present(&state.vault_path.join(path))
            .and_then(|_| remove_if_present(&state.vault_path.join(&sibling)));
        match result {
            Ok(()) => {
                db::raw_files::delete_by_id(conn, id).map_err(|e| e.to_string())?;
                out.files += 1;
            }
            Err(e) => {
                out.failed += 1;
                out.error.get_or_insert_with(|| format!("{path}: {e}"));
            }
        }
    }
    Ok(())
}

fn remove_if_present(path: &std::path::Path) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::db;
    use crate::parser::monitoring::{ParsedMonitoring, Sample};

    // 2026-09-04T21:00:00Z = local midnight of 2026-09-05 at +03:00.
    const MIDNIGHT: i64 = 1_788_555_600;
    const DAY: i64 = 86_400;

    fn test_state(vault: &Path) -> AppState {
        AppState {
            db: Arc::new(Mutex::new(db::test_db())),
            vault_path: vault.to_path_buf(),
            encryption_key: Mutex::new(None),
            watcher_handle: Mutex::new(None),
            db_locked: Mutex::new(false),
            vault_error: Mutex::new(None),
            services_started: Mutex::new(false),
            geocoding_flight: crate::state::SingleFlight::default(),
            vault_flight: crate::state::SingleFlight::default(),
        }
    }

    /// One Monitor file: heart rate every 10 min over one local day.
    fn one_day(start: i64) -> ParsedMonitoring {
        ParsedMonitoring {
            device_serial: Some("dev1".into()),
            device_product: Some("fenix6x".into()),
            tz_offset_s: 3 * 3600,
            tz_confirmed: true,
            first_ts: Some(start),
            last_ts: Some(start + DAY),
            hr: (0..144)
                .map(|i| Sample { ts: start + i * 600, value: 55.0, confidence: None })
                .collect(),
            stress: vec![],
            respiration: vec![],
            spo2: vec![],
            rhr: vec![],
            totals: vec![],
            intensity: vec![],
            active_minutes: vec![],
        }
    }

    fn seed_file(state: &AppState, id: &str, parsed: &ParsedMonitoring, enc: bool) {
        let path = if enc { format!("raw/{id}.fit.enc") } else { format!("raw/{id}.fit") };
        std::fs::write(state.vault_path.join(&path), b"fit").unwrap();
        let conn = state.db.lock().unwrap();
        conn.execute(
            "INSERT INTO raw_file (id, path_in_vault, format, hash_sha256, kind)
             VALUES (?1, ?2, 'fit', ?3, 'monitoring')",
            rusqlite::params![id, path, format!("hash-{id}")],
        )
        .unwrap();
        db::monitoring::store(&conn, parsed, Some(id)).unwrap();
    }

    fn count(state: &AppState, sql: &str) -> i64 {
        state.db.lock().unwrap().query_row(sql, [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn deleting_a_day_removes_its_file_row_and_hash_but_keeps_the_other_day() {
        let vault = std::env::temp_dir().join(format!("syz_mon_del_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(vault.join("raw")).unwrap();
        let state = test_state(&vault);
        seed_file(&state, "rf-a", &one_day(MIDNIGHT), false);
        seed_file(&state, "rf-b", &one_day(MIDNIGHT + DAY), true);
        // A crash-drift sibling of the plaintext file.
        std::fs::write(vault.join("raw/rf-a.fit.enc"), b"stale").unwrap();
        {
            let conn = state.db.lock().unwrap();
            let s = db::monitoring::summary(&conn).unwrap();
            assert_eq!((s.days, s.files), (2, 2));
            assert_eq!(s.first_date.as_deref(), Some("2026-09-05"));
            assert_eq!(s.last_date.as_deref(), Some("2026-09-06"));
            assert_eq!(db::monitoring::count_days(&conn, "2026-09-05", "2026-09-05").unwrap(), 1);
        }

        let out = delete_monitoring_range_core(&state, "2026-09-05", "2026-09-05").unwrap();
        assert_eq!(out, MonitoringDeleted { days: 1, files: 1, failed: 0, error: None });
        assert!(!vault.join("raw/rf-a.fit").exists(), "file removed");
        assert!(!vault.join("raw/rf-a.fit.enc").exists(), "drift sibling removed");
        assert!(vault.join("raw/rf-b.fit.enc").exists(), "the other day's file stays");
        {
            let conn = state.db.lock().unwrap();
            assert!(!db::raw_files::hash_exists(&conn, "hash-rf-a").unwrap(), "hash freed");
            assert!(db::raw_files::hash_exists(&conn, "hash-rf-b").unwrap());
        }
        assert_eq!(count(&state, "SELECT COUNT(*) FROM monitoring_raw_file"), 1);
        assert_eq!(count(&state, "SELECT COUNT(*) FROM monitoring_day"), 1);
        assert_eq!(count(&state, "SELECT COUNT(*) FROM raw_file WHERE kind = 'monitoring'"), 1);

        // Nothing left in the range: a no-op that still answers.
        let again = delete_monitoring_range_core(&state, "2026-09-05", "2026-09-05").unwrap();
        assert_eq!(again, MonitoringDeleted { days: 0, files: 0, failed: 0, error: None });
        let _ = std::fs::remove_dir_all(&vault);
    }

    #[test]
    fn a_file_stays_while_one_of_its_days_remains() {
        // One file spanning two days: deleting one day keeps the file (and
        // its hash) until the other day goes too.
        let vault = std::env::temp_dir().join(format!("syz_mon_del_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(vault.join("raw")).unwrap();
        let state = test_state(&vault);
        let mut two = one_day(MIDNIGHT);
        two.last_ts = Some(MIDNIGHT + 2 * DAY);
        two.hr.extend((0..144).map(|i| Sample {
            ts: MIDNIGHT + DAY + i * 600,
            value: 56.0,
            confidence: None,
        }));
        seed_file(&state, "rf-ab", &two, false);

        let first = delete_monitoring_range_core(&state, "2026-09-05", "2026-09-05").unwrap();
        assert_eq!(first, MonitoringDeleted { days: 1, files: 0, failed: 0, error: None });
        assert!(vault.join("raw/rf-ab.fit").exists());
        // The sweep must not take it either: its span still overlaps 06.09.
        let noop = delete_monitoring_range_core(&state, "2030-01-01", "2030-01-01").unwrap();
        assert_eq!(noop.files, 0);
        assert!(vault.join("raw/rf-ab.fit").exists());
        let second = delete_monitoring_range_core(&state, "2026-09-06", "2026-09-06").unwrap();
        assert_eq!(second, MonitoringDeleted { days: 1, files: 1, failed: 0, error: None });
        assert!(!vault.join("raw/rf-ab.fit").exists());
        assert_eq!(count(&state, "SELECT COUNT(*) FROM raw_file"), 0);
        let _ = std::fs::remove_dir_all(&vault);
    }

    #[test]
    fn bad_ranges_are_refused_before_touching_anything() {
        let vault = std::env::temp_dir().join(format!("syz_mon_del_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(vault.join("raw")).unwrap();
        let state = test_state(&vault);
        seed_file(&state, "rf-a", &one_day(MIDNIGHT), false);
        assert!(delete_monitoring_range_core(&state, "2026-9-5", "2026-09-05").is_err());
        assert!(delete_monitoring_range_core(&state, "2026-09-05", "yesterday").is_err());
        assert!(delete_monitoring_range_core(&state, "2026-09-06", "2026-09-05").is_err());
        assert_eq!(count(&state, "SELECT COUNT(*) FROM monitoring_day"), 1);
        assert!(vault.join("raw/rf-a.fit").exists());
        let _ = std::fs::remove_dir_all(&vault);
    }

    /// A corrupt timestamp keeps `store` from writing the file's span, so
    /// delete_range can never name it — the orphan sweep must.
    #[test]
    fn a_file_without_a_span_row_goes_once_its_readings_are_gone() {
        let vault = std::env::temp_dir().join(format!("syz_mon_del_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(vault.join("raw")).unwrap();
        let state = test_state(&vault);
        let mut bad = one_day(MIDNIGHT);
        bad.first_ts = Some(12_345); // 1970 — rejected, no span row
        seed_file(&state, "rf-bad", &bad, false);
        assert_eq!(count(&state, "SELECT COUNT(*) FROM monitoring_raw_file"), 0);
        assert_eq!(count(&state, "SELECT COUNT(*) FROM monitoring_day"), 1);

        // Its day is still stored: the file is referenced and stays.
        let none = delete_monitoring_range_core(&state, "2020-01-01", "2020-01-02").unwrap();
        assert_eq!(none.files, 0);
        assert!(vault.join("raw/rf-bad.fit").exists());

        let out = delete_monitoring_range_core(&state, "2026-09-05", "2026-09-05").unwrap();
        assert_eq!(out, MonitoringDeleted { days: 1, files: 1, failed: 0, error: None });
        assert!(!vault.join("raw/rf-bad.fit").exists());
        assert_eq!(count(&state, "SELECT COUNT(*) FROM raw_file"), 0);
        let _ = std::fs::remove_dir_all(&vault);
    }

    /// Rows committed, process gone before the files were removed: the
    /// next delete of ANY range finishes the job.
    #[test]
    fn an_interrupted_delete_is_finished_by_the_next_one() {
        let vault = std::env::temp_dir().join(format!("syz_mon_del_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(vault.join("raw")).unwrap();
        let state = test_state(&vault);
        std::fs::write(vault.join("raw/rf-left.fit"), b"fit").unwrap();
        state
            .db
            .lock()
            .unwrap()
            .execute(
                "INSERT INTO raw_file (id, path_in_vault, format, hash_sha256, kind)
                 VALUES ('rf-left', 'raw/rf-left.fit', 'fit', 'hash-left', 'monitoring')",
                [],
            )
            .unwrap();
        let out = delete_monitoring_range_core(&state, "2030-01-01", "2030-01-01").unwrap();
        assert_eq!(out, MonitoringDeleted { days: 0, files: 1, failed: 0, error: None });
        assert!(!vault.join("raw/rf-left.fit").exists());
        assert!(!db::raw_files::hash_exists(&state.db.lock().unwrap(), "hash-left").unwrap());
        let _ = std::fs::remove_dir_all(&vault);
    }

    /// The OS refusing a removal keeps the row (and the hash): the file is
    /// still there, so the truth is "not deleted", and the answer says so.
    #[test]
    fn a_file_the_os_will_not_remove_is_reported_and_its_row_kept() {
        let vault = std::env::temp_dir().join(format!("syz_mon_del_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(vault.join("raw")).unwrap();
        let state = test_state(&vault);
        seed_file(&state, "rf-a", &one_day(MIDNIGHT), false);
        // A directory where the file should be: remove_file fails on it.
        std::fs::remove_file(vault.join("raw/rf-a.fit")).unwrap();
        std::fs::create_dir_all(vault.join("raw/rf-a.fit")).unwrap();
        let out = delete_monitoring_range_core(&state, "2026-09-05", "2026-09-05").unwrap();
        assert_eq!((out.days, out.files, out.failed), (1, 0, 1));
        assert!(out.error.as_deref().unwrap().starts_with("raw/rf-a.fit: "));
        assert!(db::raw_files::hash_exists(&state.db.lock().unwrap(), "hash-rf-a").unwrap());
        assert_eq!(count(&state, "SELECT COUNT(*) FROM monitoring_day"), 0);
        assert_eq!(count(&state, "SELECT COUNT(*) FROM monitoring_raw_file"), 1, "span kept");

        // Once the OS lets go, a delete of ANY range picks the file up: its
        // span overlaps no remaining day, so the sweep names it even though
        // delete_range no longer can (there are no days left to delete).
        std::fs::remove_dir_all(vault.join("raw/rf-a.fit")).unwrap();
        std::fs::write(vault.join("raw/rf-a.fit"), b"fit").unwrap();
        let retry = delete_monitoring_range_core(&state, "2030-01-01", "2030-01-01").unwrap();
        assert_eq!(retry, MonitoringDeleted { days: 0, files: 1, failed: 0, error: None });
        assert!(!vault.join("raw/rf-a.fit").exists());
        assert!(!db::raw_files::hash_exists(&state.db.lock().unwrap(), "hash-rf-a").unwrap());
        assert_eq!(count(&state, "SELECT COUNT(*) FROM raw_file"), 0);
        let _ = std::fs::remove_dir_all(&vault);
    }

    #[test]
    fn a_locked_vault_refuses_the_delete_before_touching_anything() {
        let vault = std::env::temp_dir().join(format!("syz_mon_del_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(vault.join("raw")).unwrap();
        let state = test_state(&vault);
        seed_file(&state, "rf-a", &one_day(MIDNIGHT), false);
        let lock = crate::crypto::VaultLock {
            salt: String::new(),
            verifier: String::new(),
            nonce: String::new(),
            created_at: String::new(),
            scopes: crate::crypto::EncryptionScopes {
                activities: true,
                database: false,
                photos: false,
            },
        };
        crate::crypto::write_vault_lock(&vault, &lock).unwrap();
        let err = delete_monitoring_range_core(&state, "2026-09-05", "2026-09-05").unwrap_err();
        assert!(err.contains("locked"), "{err}");
        assert_eq!(count(&state, "SELECT COUNT(*) FROM monitoring_day"), 1);
        assert!(vault.join("raw/rf-a.fit").exists());
        let _ = std::fs::remove_dir_all(&vault);
    }

    #[test]
    fn summary_of_an_empty_vault_has_no_dates() {
        let conn = db::test_db();
        let s = db::monitoring::summary(&conn).unwrap();
        assert_eq!(
            s,
            MonitoringSummary { days: 0, first_date: None, last_date: None, files: 0 }
        );
    }
}
