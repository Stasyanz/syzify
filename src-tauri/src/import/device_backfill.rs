//! One-time correction of `activity.source_device` (#144).
//!
//! Activities imported before the resolver preferred `file_id` may name a
//! paired sensor as their recording device. Every stored FIT file is decoded
//! again with `parser::fit::source_device_of_bytes` and the activity updated
//! where the answer changed. Files are read and decoded WITHOUT the DB lock;
//! only the writes take it, per chunk, so UI commands interleave. Best-effort
//! like the other startup backfills: any failure leaves the flag unset to
//! retry next launch — including a vault whose activities scope is locked,
//! which cannot be read yet.

use std::collections::HashSet;

use rusqlite::Connection;

use crate::db;
use crate::models::raw_file::ActivityFitFile;
use crate::state::AppState;

pub const FLAG: &str = "source_device_backfilled_v1";
const CHUNK: usize = 50;

/// One activity → the device it should name from now on.
pub type DeviceUpdate = (String, String);

/// Keep the earliest-imported FIT file of each activity: that is the file
/// the activity was parsed from, and the listing is ordered that way.
pub fn first_file_per_activity(files: Vec<ActivityFitFile>) -> Vec<ActivityFitFile> {
    let mut seen = HashSet::new();
    files
        .into_iter()
        .filter(|f| seen.insert(f.activity_id.clone()))
        .collect()
}

/// Decide the updates: read each file, derive its device, keep the ones
/// that differ from what the activity names. A file that cannot be read or
/// decoded, or that yields no device at all, changes nothing — the backfill
/// corrects, it never erases.
pub fn plan_updates(
    files: &[ActivityFitFile],
    read: impl Fn(&str) -> Option<Vec<u8>>,
    derive: impl Fn(&[u8]) -> Option<String>,
) -> Vec<DeviceUpdate> {
    files
        .iter()
        .filter_map(|f| {
            let device = derive(&read(&f.path_in_vault)?)?;
            (f.source_device.as_deref() != Some(device.as_str()))
                .then(|| (f.activity_id.clone(), device))
        })
        .collect()
}

/// Write one chunk of updates atomically.
pub fn apply(conn: &Connection, updates: &[DeviceUpdate]) -> rusqlite::Result<()> {
    let tx = conn.unchecked_transaction()?;
    for (id, device) in updates {
        db::activities::set_source_device(&tx, id, Some(device))?;
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
                eprintln!("Failed to read settings for source-device backfill: {}", e);
                return;
            }
        }
        match db::raw_files::activity_fit_files(&conn) {
            Ok(files) => first_file_per_activity(files),
            Err(e) => {
                eprintln!("Source-device backfill failed to list files: {}", e);
                return;
            }
        }
    };

    let key = match state.encryption_key_for(|s| s.activities) {
        Ok(key) => key,
        Err(e) => {
            eprintln!("Source-device backfill could not read the vault lock: {}", e);
            return;
        }
    };
    if key.is_none() && files.iter().any(|f| f.path_in_vault.ends_with(".enc")) {
        // Encrypted activities without their key: nothing to read yet.
        eprintln!("Source-device backfill deferred: activities are locked");
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
    let updates = plan_updates(&files, read, crate::parser::fit::source_device_of_bytes);

    for chunk in updates.chunks(CHUNK) {
        let conn = match state.db.lock() {
            Ok(c) => c,
            Err(_) => return,
        };
        if let Err(e) = apply(&conn, chunk) {
            eprintln!("Source-device backfill failed (will retry next launch): {}", e);
            return;
        }
    }

    if let Ok(conn) = state.db.lock() {
        if let Err(e) = db::settings::set_setting(&conn, FLAG, "1") {
            eprintln!("Failed to mark source-device backfill done: {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(id: &str, path: &str, device: Option<&str>) -> ActivityFitFile {
        ActivityFitFile {
            activity_id: id.into(),
            path_in_vault: path.into(),
            source_device: device.map(str::to_string),
        }
    }

    #[test]
    fn first_file_per_activity_keeps_the_listing_order_and_drops_later_files() {
        let files = vec![
            file("a", "raw/a-early.fit", None),
            file("a", "raw/a-late.fit", None),
            file("b", "raw/b.fit", None),
        ];
        let kept = first_file_per_activity(files);
        assert_eq!(
            kept.iter().map(|f| f.path_in_vault.as_str()).collect::<Vec<_>>(),
            vec!["raw/a-early.fit", "raw/b.fit"]
        );
    }

    /// The planner corrects and never erases: unreadable or undecodable
    /// files and files that name no device leave the activity alone, and an
    /// activity already naming the right device is not rewritten.
    #[test]
    fn plan_updates_only_changed_and_readable() {
        let files = vec![
            file("sensor-first", "raw/1.fit", Some("Garmin 1620")),
            file("already-right", "raw/2.fit", Some("Garmin fenix6x")),
            file("missing", "raw/3.fit.enc", Some("Garmin 1620")),
            file("undecodable", "raw/4.fit", Some("Garmin 1620")),
            file("no-device", "raw/5.fit", Some("Garmin 1620")),
            file("was-empty", "raw/6.fit", None),
        ];
        let read = |path: &str| -> Option<Vec<u8>> {
            match path {
                "raw/3.fit.enc" => None,
                p => Some(p.as_bytes().to_vec()),
            }
        };
        let derive = |bytes: &[u8]| -> Option<String> {
            match std::str::from_utf8(bytes).unwrap() {
                "raw/1.fit" | "raw/2.fit" | "raw/6.fit" => Some("Garmin fenix6x".into()),
                "raw/4.fit" => None,
                "raw/5.fit" => None,
                other => panic!("unexpected read {other}"),
            }
        };
        assert_eq!(
            plan_updates(&files, read, derive),
            vec![
                ("sensor-first".to_string(), "Garmin fenix6x".to_string()),
                ("was-empty".to_string(), "Garmin fenix6x".to_string()),
            ]
        );
    }

    #[test]
    fn apply_writes_the_devices_in_one_transaction() {
        let conn = db::test_db();
        for id in ["x", "y"] {
            db::activities::insert_activity(
                &conn,
                &crate::models::activity::Activity {
                    id: id.into(),
                    start_time: "2025-06-01T08:00:00+00:00".into(),
                    sport_type: "ride".into(),
                    source_device: Some("Garmin 1620".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        }
        apply(&conn, &[("x".into(), "Garmin fenix6x".into())]).unwrap();
        let dev = |id: &str| -> Option<String> {
            conn.query_row("SELECT source_device FROM activity WHERE id = ?1", [id], |r| r.get(0)).unwrap()
        };
        assert_eq!(dev("x").as_deref(), Some("Garmin fenix6x"));
        assert_eq!(dev("y").as_deref(), Some("Garmin 1620"));
        // An unknown id is a no-op UPDATE, not an error.
        apply(&conn, &[("nope".into(), "Garmin edge_840".into())]).unwrap();
    }
}
