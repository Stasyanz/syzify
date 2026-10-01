//! One-time fill of what the gear rules match on (#167): the activity
//! profile (`profile_name`) and the paired sensors of every activity
//! imported before the pipeline kept them. Every stored FIT file is decoded
//! again with `parser::fit::gear_keys_of_bytes` and the keys written. Same
//! shape as the source-device backfill (#144): files are read and decoded
//! WITHOUT the DB lock, only the writes take it, per chunk; best-effort, a
//! failure leaves the flag unset to retry next launch, a locked activities
//! scope defers it.

use rusqlite::Connection;

use crate::db;
use crate::import::device_backfill::first_file_per_activity;
use crate::models::raw_file::ActivityFitFile;
use crate::parser::SensorInfo;
use crate::state::AppState;

pub const FLAG: &str = "gear_keys_backfilled_v1";
const CHUNK: usize = 50;

/// One activity → the keys its file carries.
pub type KeysUpdate = (String, Option<String>, Vec<SensorInfo>);

/// Decide the updates: read each file, derive its keys, keep the ones that
/// carry anything. A file that cannot be read or decoded, or that names
/// neither a profile nor a sensor, changes nothing.
pub fn plan_updates(
    files: &[ActivityFitFile],
    read: impl Fn(&str) -> Option<Vec<u8>>,
    derive: impl Fn(&[u8]) -> Option<(Option<String>, Vec<SensorInfo>)>,
) -> Vec<KeysUpdate> {
    files
        .iter()
        .filter_map(|f| {
            let (profile, sensors) = derive(&read(&f.path_in_vault)?)?;
            (profile.is_some() || !sensors.is_empty()).then(|| (f.activity_id.clone(), profile, sensors))
        })
        .collect()
}

/// Write one chunk of updates atomically. An activity deleted while its
/// file was being decoded (the decode runs without the lock) is skipped:
/// a sensor row for it would fail the FK and take the chunk down.
pub fn apply(conn: &Connection, updates: &[KeysUpdate]) -> rusqlite::Result<()> {
    let tx = conn.unchecked_transaction()?;
    for (id, profile, sensors) in updates {
        if db::activities::get_activity_by_id(&tx, id)?.is_none() {
            continue;
        }
        if profile.is_some() {
            db::activities::set_profile_name(&tx, id, profile.as_deref())?;
        }
        if !sensors.is_empty() {
            db::activity_sensors::replace(&tx, id, sensors)?;
        }
    }
    tx.commit()
}

/// The startup entry point: list under the lock, read and decode without it,
/// write per chunk, mark done.
pub fn run(state: &AppState) {
    let files = {
        let conn = match state.db.lock() {
            Ok(c) => c,
            Err(_) => return,
        };
        match db::settings::get_setting(&conn, FLAG) {
            Ok(None) => {}
            Ok(Some(_)) => return,
            Err(e) => {
                eprintln!("Failed to read settings for gear-keys backfill: {}", e);
                return;
            }
        }
        match db::raw_files::activity_fit_files(&conn) {
            Ok(files) => first_file_per_activity(files),
            Err(e) => {
                eprintln!("Gear-keys backfill failed to list files: {}", e);
                return;
            }
        }
    };

    let key = match state.encryption_key_for(|s| s.activities) {
        Ok(key) => key,
        Err(e) => {
            eprintln!("Gear-keys backfill could not read the vault lock: {}", e);
            return;
        }
    };
    if key.is_none() && files.iter().any(|f| f.path_in_vault.ends_with(".enc")) {
        eprintln!("Gear-keys backfill deferred: activities are locked");
        return;
    }
    let read = |path: &str| -> Option<Vec<u8>> {
        let full = state.vault_path.join(path);
        if path.ends_with(".enc") {
            key.as_ref()
                .and_then(|k| crate::crypto::decrypt_file_to_memory(k, &full).ok())
        } else {
            std::fs::read(&full).ok()
        }
    };
    let updates = plan_updates(&files, read, crate::parser::fit::gear_keys_of_bytes);

    for chunk in updates.chunks(CHUNK) {
        let conn = match state.db.lock() {
            Ok(c) => c,
            Err(_) => return,
        };
        if let Err(e) = apply(&conn, chunk) {
            eprintln!("Gear-keys backfill failed (will retry next launch): {}", e);
            return;
        }
    }

    if let Ok(conn) = state.db.lock() {
        if let Err(e) = db::settings::set_setting(&conn, FLAG, "1") {
            eprintln!("Failed to mark gear-keys backfill done: {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(id: &str, path: &str) -> ActivityFitFile {
        ActivityFitFile { activity_id: id.into(), path_in_vault: path.into(), source_device: None }
    }
    fn sensor(serial: &str) -> SensorInfo {
        SensorInfo { serial: serial.into(), device_type: Some("bike_power".into()), manufacturer: None, product: None }
    }

    #[test]
    fn plans_only_what_a_readable_file_carries() {
        let files = vec![file("a", "raw/a.fit"), file("b", "raw/b.fit"), file("c", "raw/c.fit"), file("d", "raw/d.fit")];
        let read = |p: &str| (p != "raw/c.fit").then(|| p.as_bytes().to_vec());
        let derive = |bytes: &[u8]| match bytes {
            b"raw/a.fit" => Some((Some("ROAD".to_string()), vec![sensor("3632674300")])),
            b"raw/b.fit" => Some((None, vec![])),
            b"raw/d.fit" => None,
            _ => unreachable!(),
        };
        let plan = plan_updates(&files, read, derive);
        assert_eq!(plan, vec![("a".to_string(), Some("ROAD".to_string()), vec![sensor("3632674300")])]);
    }

    #[test]
    fn apply_skips_an_activity_that_is_gone() {
        let conn = crate::db::test_db();
        conn.execute("INSERT INTO activity (id, start_time, sport_type) VALUES ('a', '2026-01-01T08:00:00+03:00', 'ride')", []).unwrap();
        apply(&conn, &[("gone".into(), Some("ROAD".into()), vec![sensor("1")]), ("a".into(), Some("Bike".into()), vec![])]).unwrap();
        let profile: Option<String> = conn.query_row("SELECT profile_name FROM activity WHERE id = 'a'", [], |r| r.get(0)).unwrap();
        assert_eq!(profile.as_deref(), Some("Bike"), "the chunk went through");
    }

    fn test_state(vault: &std::path::Path) -> AppState {
        use std::sync::{Arc, Mutex};
        AppState {
            db: Arc::new(Mutex::new(crate::db::test_db())),
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

    fn flag_of(state: &AppState) -> Option<String> {
        let conn = state.db.lock().unwrap();
        db::settings::get_setting(&conn, FLAG).unwrap()
    }

    fn stored_file(state: &AppState, id: &str, path: &str, bytes: &[u8]) {
        let full = state.vault_path.join(path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(&full, bytes).unwrap();
        let conn = state.db.lock().unwrap();
        conn.execute(
            "INSERT INTO activity (id, start_time, sport_type) VALUES (?1, '2026-01-01T08:00:00+03:00', 'ride')",
            [id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO raw_file (id, activity_id, path_in_vault, original_path, format, hash_sha256) \
             VALUES (?1, ?2, ?3, ?3, 'fit', ?1)",
            rusqlite::params![format!("raw-{id}"), id, path],
        )
        .unwrap();
    }

    /// `run` on a vault: done once (the flag), deferred while the
    /// activities are locked, and otherwise a pass over every file that
    /// marks itself done — an undecodable file changes nothing.
    #[test]
    fn run_is_once_and_waits_for_the_key() {
        let vault = std::env::temp_dir().join(format!("syz_gearkeys_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&vault).unwrap();
        let state = test_state(&vault);
        stored_file(&state, "a", "raw/a.fit", b"not a fit file");
        run(&state);
        assert_eq!(flag_of(&state).as_deref(), Some("1"), "a pass over unreadable files still completes");
        {
            let conn = state.db.lock().unwrap();
            let profile: Option<String> = conn.query_row("SELECT profile_name FROM activity WHERE id = 'a'", [], |r| r.get(0)).unwrap();
            assert_eq!(profile, None);
        }
        run(&state); // idempotent: the flag stops it

        // Encrypted activities without the key: deferred, no flag.
        let locked = test_state(&vault);
        stored_file(&locked, "e", "raw/e.fit.enc", b"ciphertext");
        run(&locked);
        assert_eq!(flag_of(&locked), None, "deferred until unlocked");
        let _ = std::fs::remove_dir_all(&vault);
    }

    #[test]
    fn apply_writes_the_keys() {
        let conn = crate::db::test_db();
        conn.execute("INSERT INTO activity (id, start_time, sport_type) VALUES ('a', '2026-01-01T08:00:00+03:00', 'ride')", []).unwrap();
        conn.execute("INSERT INTO activity (id, start_time, sport_type) VALUES ('b', '2026-01-02T08:00:00+03:00', 'ride')", []).unwrap();
        apply(
            &conn,
            &[
                ("a".into(), Some("ROAD".into()), vec![sensor("3632674300")]),
                ("b".into(), None, vec![sensor("111")]),
            ],
        )
        .unwrap();
        let profile: Option<String> = conn.query_row("SELECT profile_name FROM activity WHERE id = 'a'", [], |r| r.get(0)).unwrap();
        assert_eq!(profile.as_deref(), Some("ROAD"));
        let none: Option<String> = conn.query_row("SELECT profile_name FROM activity WHERE id = 'b'", [], |r| r.get(0)).unwrap();
        assert_eq!(none, None);
        assert_eq!(crate::db::activity_sensors::serials_of(&conn, "a").unwrap(), vec!["3632674300"]);
        assert_eq!(crate::db::activity_sensors::serials_of(&conn, "b").unwrap(), vec!["111"]);
    }
}
