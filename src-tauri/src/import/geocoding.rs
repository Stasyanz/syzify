use std::thread;
use std::time::Duration;

use rusqlite::Connection;
use tauri::{AppHandle, Emitter, Manager};

use crate::db;
use crate::geocoding;
use crate::state::AppState;

/// Nominatim asks for at most 1 request per second; we space calls slightly
/// above that to stay within the usage policy.
const NOMINATIM_INTERVAL: Duration = Duration::from_millis(1100);

/// The settings key behind the "Location names" toggle in Settings.
pub const GEOCODING_SETTING: &str = "geocoding_enabled";

/// Whether the user has opted into online reverse geocoding. Defaults to OFF:
/// looking up a location sends the activity's start coordinates to
/// nominatim.openstreetmap.org, and this app promises no network calls the
/// user didn't ask for (PRD §16) — so the lookup runs only after the user
/// flips the disclosed toggle in Settings.
pub fn geocoding_enabled(conn: &Connection) -> bool {
    matches!(
        db::settings::get_setting(conn, GEOCODING_SETTING),
        Ok(Some(v)) if v == "true"
    )
}

/// Reverse-geocode every activity that doesn't have a `location_name` yet.
///
/// Runs as a background job both at startup (`lib.rs`) and after an import
/// (`commands::import`). Locks the DB only for the brief read/write around each
/// network call so it never blocks the UI for the whole batch.
pub fn run_background_geocoding<R: tauri::Runtime>(app: &AppHandle<R>) {
    let state = app.state::<AppState>();

    // Single-flight: boot, post-import and the Settings toggle can each start
    // a pass. Overlapping passes would multiply the request rate at Nominatim
    // (their policy is 1 req/s) and re-send the same coordinates. The slot
    // frees when this run returns (guard drop).
    let Some(_flight) = state.geocoding_flight.try_begin() else {
        return;
    };

    let snapshot = || {
        let Ok(conn) = state.db.lock() else { return Vec::new() };
        if !geocoding_enabled(&conn) {
            return Vec::new();
        }
        db::activities::get_activities_without_location(&conn).unwrap_or_default()
    }; // Lock released per snapshot

    // Snapshot after snapshot: activities that land while a batch runs (a
    // sync's next rounds) are picked up by the next one. The UI hears about
    // every batch that named something, not only the first.
    drain(snapshot, |missing| {
        let mut progressed = false;
        let mut named_any = false;
        for (id, lat, lon) in missing {
            // Re-check per item, BEFORE the network call: flipping the toggle
            // off mid-batch stops the remaining lookups, not just the writes.
            match state.db.lock() {
                Ok(conn) if geocoding_enabled(&conn) => {}
                _ => break,
            }
            // A definitive "Nominatim knows no name here" (open water,
            // wilderness) is recorded as an empty string so the same
            // coordinates aren't re-sent on every launch; a transient network
            // error stays NULL and retries next run.
            let name = match geocoding::reverse_geocode(*lat, *lon) {
                Ok(Some(name)) => name,
                Ok(None) => String::new(),
                Err(_) => {
                    thread::sleep(NOMINATIM_INTERVAL);
                    continue;
                }
            };
            // Progress is a row that actually changed: a write that keeps
            // failing (a busy or swapped DB) would otherwise hand the next
            // snapshot the same list forever.
            if let Ok(conn) = state.db.lock() {
                if db::activities::set_location_name(&conn, id, &name).is_ok() {
                    progressed = true;
                    named_any = named_any || !name.is_empty();
                }
            }
            thread::sleep(NOMINATIM_INTERVAL);
        }
        if named_any {
            let _ = app.emit("activities:updated", ());
        }
        progressed
    });
}

/// Run `process` on batch after batch of `snapshot` until a snapshot is
/// empty or a batch wrote nothing (every lookup failed, or the toggle went
/// off mid-batch): the next snapshot would be the same list, and a dead
/// network must not be hammered in a loop.
fn drain<T>(mut snapshot: impl FnMut() -> Vec<T>, mut process: impl FnMut(&[T]) -> bool) {
    loop {
        let batch = snapshot();
        if batch.is_empty() || !process(&batch) {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Batches follow one another until nothing is left, and a batch that
    /// wrote nothing ends the run even with work still listed.
    #[test]
    fn drain_takes_snapshot_after_snapshot_and_stops_on_a_dead_batch() {
        let mut snapshots = vec![vec![1, 2, 3], vec![4], vec![]].into_iter();
        let mut seen = Vec::new();
        drain(
            || snapshots.next().unwrap_or_default(),
            |batch| {
                seen.push(batch.to_vec());
                true
            },
        );
        assert_eq!(seen, vec![vec![1, 2, 3], vec![4]]);

        let mut calls = 0;
        drain(
            || vec![1],
            |_| {
                calls += 1;
                false
            },
        );
        assert_eq!(calls, 1, "a batch that made no progress ends the run");
    }

    /// Off by default (no setting row), on only for the explicit "true".
    #[test]
    fn geocoding_is_opt_in() {
        let conn = crate::db::test_db();
        assert!(!geocoding_enabled(&conn));

        db::settings::set_setting(&conn, GEOCODING_SETTING, "false").unwrap();
        assert!(!geocoding_enabled(&conn));

        db::settings::set_setting(&conn, GEOCODING_SETTING, "true").unwrap();
        assert!(geocoding_enabled(&conn));
    }
}
