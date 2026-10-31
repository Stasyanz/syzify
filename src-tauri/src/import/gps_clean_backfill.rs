//! One-off pass over the activities imported before the GPS sanity filter
//! (#256): clean each stored track the way an import now does, put the
//! cleaned distance and speeds on the activity, keep what it dropped
//! (`db::gps_dropped`, so a later filter can undo it), and redo what read
//! the old track: best efforts, segment efforts, a merged event's sums.
//!
//! NOT WIRED: 0.13.0 cleans new imports only, so a mistake of the filter
//! can't reach anyone's whole history at once. Before calling [`run`] from
//! `start_background_services`: recount stored laps as an import does (it
//! leaves them), give the flag a retry for an activity that failed, and
//! check the filter against the tracks people import until then.
//!
//! It works on the stored trackpoints, not the raw files — they still hold
//! every fix an old import wrote, and the filter only ever drops one. The
//! speed sensor comes from the stored sensor list, so the pass waits for
//! the gear-keys backfill that filled it for old activities.

use rusqlite::Connection;

use crate::db;
use crate::import::gps_clean::{clean_trackpoints, is_speed_sensor, rejected_fixes, sport_limits};
use crate::import::pipeline::{compute_metrics, gps_override, GpsOverride};
use crate::models::activity::SportType;
use crate::models::trackpoint::ReplacedFigures;
use crate::state::AppState;

pub const FLAG: &str = "gps_cleaned_v1";
const CHUNK: usize = 25;

/// Clean one stored activity's track like an import; `true` when it changed.
pub(crate) fn clean_stored(conn: &Connection, activity_id: &str) -> rusqlite::Result<bool> {
    let Some(activity) = db::activities::get_activity_by_id(conn, activity_id)? else {
        return Ok(false);
    };
    let sport = SportType::from_str(&activity.sport_type);
    let Some(limits) = sport_limits(&sport) else {
        return Ok(false);
    };
    // The geometry alone answers for almost every track: nothing to drop.
    let geo = db::trackpoints::get_track_geometry(conn, activity_id)?;
    if rejected_fixes(&geo.t, &geo.lat, &geo.lon, &geo.altitude_m, limits).is_empty() {
        return Ok(false);
    }

    let tx = conn.unchecked_transaction()?;
    let mut tps = db::trackpoints::get_trackpoints(&tx, activity_id)?;
    let raw_distance = compute_metrics(&tps).distance_m;
    let speed_from_sensor = db::activity_sensors::device_types_of(&tx, activity_id)?
        .iter()
        .any(|t| is_speed_sensor(t.as_deref()));
    let cleaning = clean_trackpoints(&mut tps, &sport, speed_from_sensor);
    if cleaning.is_empty() {
        return Ok(false);
    }
    let multisport = db::multisport_legs::is_multisport(&tx, activity_id)?;
    let gps = gps_override(
        &cleaning,
        speed_from_sensor,
        multisport,
        activity.distance_m,
        raw_distance,
        &compute_metrics(&tps),
        activity.duration_s,
    );
    db::gps_dropped::clear(&tx, activity_id, tps.len(), &cleaning.dropped)?;
    if gps != GpsOverride::default() {
        let replaced = ReplacedFigures {
            distance_m: activity.distance_m,
            avg_speed_mps: activity.avg_speed_mps,
            max_speed_mps: activity.max_speed_mps,
        };
        db::gps_dropped::record(&tx, activity_id, tps.len(), &[], Some(&replaced))?;
        db::activities::set_gps_figures(&tx, activity_id, gps.distance_m, gps.avg_speed_mps, gps.max_speed_mps)?;
        db::multisport_legs::refresh_merged_leg(&tx, activity_id)?;
    }
    tx.commit()?;

    // What read the old track, non-fatal like an import's analysis — and
    // outside the transaction: segment matching opens its own.
    let _ = db::best_efforts::recompute_for_activity(conn, activity_id);
    let _ = db::segment_efforts::match_activity(conn, activity_id);
    Ok(true)
}

/// The startup entry point: list under the lock, clean per chunk, mark done.
/// An activity that fails is logged and skipped — it keeps its track as it
/// was, and the rest of the history still gets cleaned.
pub fn run(state: &AppState) {
    let ids = {
        let conn = match state.db.lock() {
            Ok(c) => c,
            Err(_) => return,
        };
        let flags = (
            db::settings::get_setting(&conn, FLAG),
            db::settings::get_setting(&conn, crate::import::gear_keys_backfill::FLAG),
        );
        match flags {
            (Ok(None), Ok(Some(_))) => {}
            // Done, or the sensor lists of old activities are not in yet
            // (a locked vault): a wheel sensor would go unseen, so wait.
            (Ok(_), Ok(_)) => return,
            (Err(e), _) | (_, Err(e)) => {
                eprintln!("Failed to read settings for the GPS cleaning backfill: {}", e);
                return;
            }
        }
        match db::trackpoints::activities_with_fixes(&conn) {
            Ok(ids) => ids,
            Err(e) => {
                eprintln!("GPS cleaning backfill failed to list activities: {}", e);
                return;
            }
        }
    };

    let (mut cleaned, mut failed) = (0, 0);
    for chunk in ids.chunks(CHUNK) {
        let conn = match state.db.lock() {
            Ok(c) => c,
            Err(_) => return,
        };
        for id in chunk {
            match clean_stored(&conn, id) {
                Ok(changed) => cleaned += changed as usize,
                Err(e) => {
                    failed += 1;
                    eprintln!("GPS cleaning backfill skipped activity {}: {}", id, e);
                }
            }
        }
    }
    if cleaned > 0 || failed > 0 {
        eprintln!("GPS cleaning backfill: {} activities cleaned, {} skipped", cleaned, failed);
    }

    let conn = match state.db.lock() {
        Ok(c) => c,
        Err(_) => return,
    };
    if let Err(e) = db::settings::set_setting(&conn, FLAG, "1") {
        eprintln!("Failed to mark the GPS cleaning backfill done: {}", e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::fit_builder::jammed_ride_fixture;

    /// The jammed ride as an import before #256 stored it: every fix, the
    /// session's 30 km and 65 m/s, and the sensors given.
    fn stored_jammed_ride(conn: &Connection, id: &str, sport: &str, sensors: &[&str]) {
        let parsed = crate::parser::fit::parse_fit_bytes(&jammed_ride_fixture(&[]), id).unwrap();
        conn.execute(
            "INSERT INTO activity (id, start_time, sport_type, distance_m, duration_s, avg_speed_mps, max_speed_mps) \
             VALUES (?1, '2026-09-04T08:00:00+03:00', ?2, 30000.0, 1559.0, 19.2, 65.0)",
            rusqlite::params![id, sport],
        )
        .unwrap();
        db::trackpoints::insert_trackpoints(conn, &parsed.trackpoints).unwrap();
        let sensors: Vec<crate::parser::SensorInfo> = sensors
            .iter()
            .enumerate()
            .map(|(k, kind)| crate::parser::SensorInfo {
                serial: k.to_string(),
                device_type: Some(kind.to_string()),
                manufacturer: None,
                product: None,
            })
            .collect();
        db::activity_sensors::replace(conn, id, &sensors).unwrap();
    }

    fn figures(conn: &Connection, id: &str) -> (Option<f64>, Option<f64>, Option<f64>, i64) {
        conn.query_row(
            "SELECT distance_m, avg_speed_mps, max_speed_mps, \
             (SELECT COUNT(lat) FROM trackpoint WHERE activity_id = ?1) FROM activity WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap()
    }

    #[test]
    fn an_old_jammed_ride_is_cleaned_once_and_can_be_undone() {
        let conn = db::test_db();
        stored_jammed_ride(&conn, "a1", "ride", &["bike_power"]);
        assert!(clean_stored(&conn, "a1").unwrap());
        let (d, avg, max, fixes) = figures(&conn, "a1");
        let d = d.unwrap();
        assert!((d - 12_472.0).abs() < 15.0, "distance {d}");
        assert!((avg.unwrap() - d / 1559.0).abs() < 1e-6);
        assert_eq!((max, fixes), (Some(8.0), 1200));
        // What it leaves is clean: a second pass finds nothing.
        assert!(!clean_stored(&conn, "a1").unwrap());
        // And what it took is all there to put back.
        assert!(db::gps_dropped::restore(&conn, "a1").unwrap());
        assert_eq!(figures(&conn, "a1"), (Some(30_000.0), Some(19.2), Some(65.0), 1260));
    }

    #[test]
    fn a_stored_speed_sensor_keeps_the_device_figures() {
        let conn = db::test_db();
        stored_jammed_ride(&conn, "a1", "ride", &["bike_speed"]);
        assert!(clean_stored(&conn, "a1").unwrap());
        assert_eq!(figures(&conn, "a1"), (Some(30_000.0), Some(19.2), Some(65.0), 1200));
    }

    #[test]
    fn a_sport_the_filter_leaves_alone_is_not_touched() {
        let conn = db::test_db();
        stored_jammed_ride(&conn, "a1", "ski", &[]);
        assert!(!clean_stored(&conn, "a1").unwrap());
        // Nor is an activity deleted since the list was taken.
        assert!(!clean_stored(&conn, "gone").unwrap());
        assert_eq!(figures(&conn, "a1"), (Some(30_000.0), Some(19.2), Some(65.0), 1260));
    }

    /// A ride segment along `from..to` metres north of the fixture's origin.
    fn segment(conn: &Connection, id: &str, from: f64, to: f64) {
        let north = |m: f64| 55.0 + m / 111_195.0;
        conn.execute(
            "INSERT INTO segment (id, name, sport, distance_m, start_lat, start_lon, end_lat, end_lon, \
             min_lat, max_lat, min_lon, max_lon, created_at) \
             VALUES (?1, 'Segment', 'ride', ?4, ?2, 37.0, ?3, 37.0, ?2, ?3, 37.0, 37.0, '2026-09-01')",
            rusqlite::params![id, north(from), north(to), to - from],
        )
        .unwrap();
        for (k, m) in [from, (from + to) / 2.0, to].iter().enumerate() {
            conn.execute(
                "INSERT INTO segment_point (segment_id, seq, lat, lon, distance_m) VALUES (?1, ?2, ?3, 37.0, ?4)",
                rusqlite::params![id, k as i64, north(*m), m - from],
            )
            .unwrap();
        }
    }

    fn efforts(conn: &Connection, segment_id: &str) -> i64 {
        conn.query_row("SELECT COUNT(*) FROM segment_effort WHERE segment_id = ?1", [segment_id], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn a_cleaned_ride_sheds_the_teleport_s_effort_and_keeps_the_road_s() {
        let conn = db::test_db();
        stored_jammed_ride(&conn, "a1", "ride", &[]);
        segment(&conn, "fake", 20_100.0, 21_100.0); // only the teleport rode it
        segment(&conn, "road", 1_000.0, 3_000.0); // the real ride did
        db::segment_efforts::match_activity(&conn, "a1").unwrap();
        assert_eq!((efforts(&conn, "fake"), efforts(&conn, "road")), (1, 1));
        conn.execute("UPDATE segment_effort SET elapsed_s = -1", []).unwrap();
        assert!(clean_stored(&conn, "a1").unwrap());
        assert_eq!((efforts(&conn, "fake"), efforts(&conn, "road")), (0, 1));
        // Matched again on the cleaned track, not left over.
        let elapsed: f64 =
            conn.query_row("SELECT elapsed_s FROM segment_effort WHERE segment_id = 'road'", [], |r| r.get(0)).unwrap();
        assert!(elapsed > 0.0);
    }

    #[test]
    fn a_merged_leg_s_container_follows_it() {
        let conn = db::test_db();
        stored_jammed_ride(&conn, "ride", "ride", &[]);
        conn.execute(
            "INSERT INTO activity (id, start_time, sport_type, distance_m) VALUES ('tri', '2026-09-04T07:00:00+03:00', 'triathlon', 31500.0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO activity (id, start_time, sport_type, distance_m, parent_id) VALUES ('swim', '2026-09-04T07:00:00+03:00', 'swim', 1500.0, 'tri')",
            [],
        )
        .unwrap();
        conn.execute("UPDATE activity SET parent_id = 'tri' WHERE id = 'ride'", []).unwrap();
        assert!(clean_stored(&conn, "ride").unwrap());
        let (container,): (f64,) =
            conn.query_row("SELECT distance_m FROM activity WHERE id = 'tri'", [], |r| Ok((r.get(0)?,))).unwrap();
        assert!((container - (1500.0 + 12_472.0)).abs() < 15.0, "container {container}");
    }

    fn test_state() -> AppState {
        use std::sync::{Arc, Mutex};
        AppState {
            db: Arc::new(Mutex::new(db::test_db())),
            vault_path: std::path::PathBuf::from("/nonexistent"),
            encryption_key: Mutex::new(None),
            watcher_handle: Mutex::new(None),
            db_locked: Mutex::new(false),
            vault_error: Mutex::new(None),
            services_started: Mutex::new(false),
            geocoding_flight: crate::state::SingleFlight::default(),
            vault_flight: crate::state::SingleFlight::default(),
        }
    }

    #[test]
    fn the_pass_waits_for_the_sensor_lists_then_runs_once() {
        let state = test_state();
        {
            let conn = state.db.lock().unwrap();
            stored_jammed_ride(&conn, "a1", "ride", &[]);
        }
        let flag = |s: &AppState| db::settings::get_setting(&s.db.lock().unwrap(), FLAG).unwrap();

        run(&state);
        assert_eq!(flag(&state), None, "no sensor lists yet");
        assert_eq!(figures(&state.db.lock().unwrap(), "a1").3, 1260);

        db::settings::set_setting(&state.db.lock().unwrap(), crate::import::gear_keys_backfill::FLAG, "1")
            .unwrap();
        run(&state);
        assert_eq!(flag(&state).as_deref(), Some("1"));
        assert_eq!(figures(&state.db.lock().unwrap(), "a1").3, 1200);
    }

    #[test]
    fn an_activity_that_fails_is_skipped_and_the_rest_cleaned() {
        let state = test_state();
        {
            let conn = state.db.lock().unwrap();
            stored_jammed_ride(&conn, "a1", "ride", &[]);
            stored_jammed_ride(&conn, "a2", "ride", &[]);
            db::settings::set_setting(&conn, crate::import::gear_keys_backfill::FLAG, "1").unwrap();
            // a1's record table refuses its rows: the cleaning of a1 fails.
            conn.execute_batch(
                "CREATE TRIGGER refuse_a1 BEFORE INSERT ON gps_dropped WHEN NEW.activity_id = 'a1' \
                 BEGIN SELECT RAISE(ABORT, 'refused'); END;",
            )
            .unwrap();
        }
        run(&state);
        let conn = state.db.lock().unwrap();
        assert_eq!(figures(&conn, "a1").3, 1260, "a1 rolled back whole");
        assert_eq!(figures(&conn, "a2").3, 1200);
        assert_eq!(db::settings::get_setting(&conn, FLAG).unwrap().as_deref(), Some("1"));
    }

    #[test]
    fn activities_with_fixes_lists_only_tracks_with_a_position() {
        let conn = db::test_db();
        stored_jammed_ride(&conn, "a1", "ride", &[]);
        conn.execute(
            "INSERT INTO activity (id, start_time, sport_type) VALUES ('indoor', '2026-09-03T08:00:00+03:00', 'indoor_ride')",
            [],
        )
        .unwrap();
        conn.execute("INSERT INTO trackpoint (activity_id, hr) VALUES ('indoor', 120)", []).unwrap();
        assert_eq!(db::trackpoints::activities_with_fixes(&conn).unwrap(), vec!["a1".to_string()]);
    }
}
