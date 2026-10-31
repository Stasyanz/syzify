//! What the GPS cleaning (#256) took off a track, kept so a later version
//! of the filter can put it back and judge again (migration 035).
//! Trackpoints are addressed by index in the `ORDER BY id ASC` space every
//! track read shares.

use rusqlite::{params, Connection, Result};

use crate::models::trackpoint::{DroppedGps, ReplacedFigures};

/// The activity's trackpoint ids in track order — refused unless there are
/// exactly `track_len` of them, so an index can't land on another point.
fn track_ids(conn: &Connection, activity_id: &str, track_len: usize) -> Result<Vec<i64>> {
    let mut stmt = conn.prepare("SELECT id FROM trackpoint WHERE activity_id = ?1 ORDER BY id ASC")?;
    let ids: Vec<i64> = stmt.query_map(params![activity_id], |r| r.get(0))?.collect::<Result<_>>()?;
    if ids.len() != track_len {
        return Err(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_MISMATCH),
            Some(format!("track of {activity_id} has {} points, the cleaning saw {track_len}", ids.len())),
        ));
    }
    Ok(ids)
}

/// Keep what a cleaning dropped from the stored track of `track_len` points
/// and, when the activity's figures were replaced, what they were. A value
/// already kept stays: the first record is the original.
pub fn record(
    conn: &Connection,
    activity_id: &str,
    track_len: usize,
    dropped: &[DroppedGps],
    replaced: Option<&ReplacedFigures>,
) -> Result<()> {
    let ids = track_ids(conn, activity_id, track_len)?;
    let mut keep = conn.prepare(
        "INSERT INTO gps_dropped (trackpoint_id, activity_id, lat, lon, speed_mps) VALUES (?1, ?2, ?3, ?4, ?5) \
         ON CONFLICT(trackpoint_id) DO UPDATE SET lat = COALESCE(lat, excluded.lat), \
         lon = COALESCE(lon, excluded.lon), speed_mps = COALESCE(speed_mps, excluded.speed_mps)",
    )?;
    for d in dropped {
        if let Some(id) = ids.get(d.index) {
            keep.execute(params![id, activity_id, d.lat, d.lon, d.speed_mps])?;
        }
    }
    if let Some(f) = replaced {
        conn.execute(
            "INSERT OR IGNORE INTO gps_replaced_figures (activity_id, distance_m, avg_speed_mps, max_speed_mps) \
             VALUES (?1, ?2, ?3, ?4)",
            params![activity_id, f.distance_m, f.avg_speed_mps, f.max_speed_mps],
        )?;
    }
    Ok(())
}

/// Drop the values in `dropped` from the stored track of `track_len` points
/// and keep them ([`record`]). The rows stay, so segment efforts keep their
/// indices.
pub fn clear(
    conn: &Connection,
    activity_id: &str,
    track_len: usize,
    dropped: &[DroppedGps],
) -> Result<()> {
    let ids = track_ids(conn, activity_id, track_len)?;
    let mut clear_fix = conn.prepare("UPDATE trackpoint SET lat = NULL, lon = NULL WHERE id = ?1")?;
    let mut clear_speed = conn.prepare("UPDATE trackpoint SET speed_mps = NULL WHERE id = ?1")?;
    for d in dropped {
        let Some(id) = ids.get(d.index) else { continue };
        if d.lat.is_some() || d.lon.is_some() {
            clear_fix.execute(params![id])?;
        }
        if d.speed_mps.is_some() {
            clear_speed.execute(params![id])?;
        }
    }
    record(conn, activity_id, track_len, dropped, None)
}

/// Put back everything the cleaning took off the activity: the dropped
/// values and the figures it replaced. `false` when it had taken nothing.
pub fn restore(conn: &Connection, activity_id: &str) -> Result<bool> {
    let points = conn.execute(
        "UPDATE trackpoint SET \
         lat = COALESCE((SELECT d.lat FROM gps_dropped d WHERE d.trackpoint_id = trackpoint.id), lat), \
         lon = COALESCE((SELECT d.lon FROM gps_dropped d WHERE d.trackpoint_id = trackpoint.id), lon), \
         speed_mps = COALESCE((SELECT d.speed_mps FROM gps_dropped d WHERE d.trackpoint_id = trackpoint.id), speed_mps) \
         WHERE id IN (SELECT trackpoint_id FROM gps_dropped WHERE activity_id = ?1)",
        params![activity_id],
    )?;
    let figures = conn.execute(
        "UPDATE activity SET \
         distance_m = (SELECT distance_m FROM gps_replaced_figures WHERE activity_id = ?1), \
         avg_speed_mps = (SELECT avg_speed_mps FROM gps_replaced_figures WHERE activity_id = ?1), \
         max_speed_mps = (SELECT max_speed_mps FROM gps_replaced_figures WHERE activity_id = ?1) \
         WHERE id = ?1 AND EXISTS (SELECT 1 FROM gps_replaced_figures WHERE activity_id = ?1)",
        params![activity_id],
    )?;
    conn.execute("DELETE FROM gps_dropped WHERE activity_id = ?1", params![activity_id])?;
    conn.execute("DELETE FROM gps_replaced_figures WHERE activity_id = ?1", params![activity_id])?;
    Ok(points > 0 || figures > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    /// An activity of three trackpoints at (55.0 + k, 37.0), speed 8.
    fn stored(conn: &Connection) {
        conn.execute(
            "INSERT INTO activity (id, start_time, sport_type, distance_m, avg_speed_mps, max_speed_mps) \
             VALUES ('a1', '2026-01-01T08:00:00+03:00', 'ride', 30000.0, 19.2, 65.0)",
            [],
        )
        .unwrap();
        for k in 0..3 {
            conn.execute(
                "INSERT INTO trackpoint (activity_id, lat, lon, speed_mps) VALUES ('a1', ?1, 37.0, 8.0)",
                params![55.0 + k as f64],
            )
            .unwrap();
        }
    }

    fn track(conn: &Connection) -> Vec<(Option<f64>, Option<f64>, Option<f64>)> {
        let mut stmt = conn.prepare("SELECT lat, lon, speed_mps FROM trackpoint ORDER BY id").unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap().map(|r| r.unwrap()).collect()
    }

    #[test]
    fn a_cleaning_is_undone_value_by_value() {
        let conn = db::test_db();
        stored(&conn);
        let before = track(&conn);
        let dropped = [
            DroppedGps { index: 1, lat: Some(56.0), lon: Some(37.0), speed_mps: Some(8.0) },
            DroppedGps { index: 2, lat: None, lon: None, speed_mps: Some(8.0) },
        ];
        clear(&conn, "a1", 3, &dropped).unwrap();
        record(
            &conn,
            "a1",
            3,
            &[],
            Some(&ReplacedFigures { distance_m: Some(30000.0), avg_speed_mps: Some(19.2), max_speed_mps: Some(65.0) }),
        )
        .unwrap();
        db::activities::set_gps_figures(&conn, "a1", Some(12000.0), Some(7.7), Some(Some(8.0))).unwrap();
        assert_eq!(track(&conn), vec![before[0], (None, None, None), (Some(57.0), Some(37.0), None)]);

        assert!(restore(&conn, "a1").unwrap());
        assert_eq!(track(&conn), before);
        let figures: (f64, f64, f64) = conn
            .query_row("SELECT distance_m, avg_speed_mps, max_speed_mps FROM activity WHERE id = 'a1'", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .unwrap();
        assert_eq!(figures, (30000.0, 19.2, 65.0));
        // Nothing left to undo.
        assert!(!restore(&conn, "a1").unwrap());
    }

    #[test]
    fn the_first_record_is_the_original() {
        let conn = db::test_db();
        stored(&conn);
        let first = [DroppedGps { index: 0, lat: Some(55.0), lon: Some(37.0), speed_mps: None }];
        let later = [DroppedGps { index: 0, lat: None, lon: None, speed_mps: Some(8.0) }];
        clear(&conn, "a1", 3, &first).unwrap();
        clear(&conn, "a1", 3, &later).unwrap();
        let kept: (Option<f64>, Option<f64>) =
            conn.query_row("SELECT lat, speed_mps FROM gps_dropped", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!(kept, (Some(55.0), Some(8.0)));
    }

    #[test]
    fn a_track_of_another_length_is_refused() {
        let conn = db::test_db();
        stored(&conn);
        let dropped = [DroppedGps { index: 1, lat: Some(56.0), lon: Some(37.0), speed_mps: None }];
        assert!(clear(&conn, "a1", 4, &dropped).is_err());
        assert_eq!(track(&conn)[1].0, Some(56.0));
    }

    #[test]
    fn the_record_goes_with_the_activity() {
        let conn = db::test_db();
        stored(&conn);
        clear(&conn, "a1", 3, &[DroppedGps { index: 1, lat: Some(56.0), lon: Some(37.0), speed_mps: None }]).unwrap();
        conn.execute("DELETE FROM activity WHERE id = 'a1'", []).unwrap();
        let left: i64 = conn.query_row("SELECT COUNT(*) FROM gps_dropped", [], |r| r.get(0)).unwrap();
        assert_eq!(left, 0);
    }
}
