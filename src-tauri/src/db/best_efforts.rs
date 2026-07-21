use rusqlite::{params, Connection, Result};

/// A best-effort hit: (activity_id, title, date, duration_s).
type BestEffortHit = (String, Option<String>, String, f64);

/// Replace the stored best-effort splits for one activity.
pub fn set_best_efforts(conn: &Connection, activity_id: &str, efforts: &[(f64, f64)]) -> Result<()> {
    conn.execute(
        "DELETE FROM best_effort WHERE activity_id = ?1",
        params![activity_id],
    )?;
    let mut stmt = conn.prepare(
        "INSERT INTO best_effort (activity_id, distance_m, duration_s) VALUES (?1, ?2, ?3)",
    )?;
    for (distance_m, duration_s) in efforts {
        stmt.execute(params![activity_id, distance_m, duration_s])?;
    }
    Ok(())
}

/// Fastest best effort for a given distance within a sport, with its activity.
/// Returns (activity_id, title, date, duration_s).
pub fn fastest_for_distance(
    conn: &Connection,
    sport: &str,
    distance_m: f64,
) -> Result<Option<BestEffortHit>> {
    let mut stmt = conn.prepare(
        "SELECT a.id, a.title, date(a.start_time), be.duration_s
         FROM best_effort be JOIN activity a ON a.id = be.activity_id
         WHERE a.sport_type = ?1 AND be.distance_m = ?2
         ORDER BY be.duration_s ASC LIMIT 1",
    )?;
    let mut rows = stmt.query(params![sport, distance_m])?;
    match rows.next()? {
        Some(row) => Ok(Some((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))),
        None => Ok(None),
    }
}

/// Sports that get running-style best-effort splits.
pub const RUNNING_SPORTS: [&str; 3] = ["run", "trail_run", "treadmill"];

/// Recompute best-effort splits for every running activity from its stored
/// trackpoints. Returns the number of activities that produced any efforts.
/// Used both by the manual recompute command and the one-time startup backfill.
pub fn recompute_running(conn: &Connection) -> Result<usize> {
    let ids = activity_ids_for_sports(conn, &RUNNING_SPORTS)?;
    let mut n = 0usize;
    for id in ids {
        if let Ok(cols) = crate::db::trackpoints::get_trackpoints_columnar(conn, &id) {
            let recorded: Option<f64> = conn
                .query_row(
                    "SELECT distance_m FROM activity WHERE id = ?1",
                    [&id],
                    |row| row.get::<_, Option<f64>>(0),
                )
                .ok()
                .flatten();
            let efforts = crate::import::best_effort::compute_best_efforts(
                &cols.distance_m,
                &cols.t,
                recorded,
            );
            set_best_efforts(conn, &id, &efforts)?;
            if !efforts.is_empty() {
                n += 1;
            }
        }
    }
    Ok(n)
}

/// Recompute (or clear) best-effort splits for a SINGLE activity from its
/// current sport and stored track. A running activity gets fresh splits; any
/// other sport has its rows cleared — so correcting a mis-imported sport in
/// the edit modal makes the run earn / lose its distance PBs immediately.
pub fn recompute_for_activity(conn: &Connection, activity_id: &str) -> Result<()> {
    let sport: Option<String> = conn
        .query_row(
            "SELECT sport_type FROM activity WHERE id = ?1",
            [activity_id],
            |row| row.get(0),
        )
        .ok();
    let is_running = sport.as_deref().is_some_and(|s| RUNNING_SPORTS.contains(&s));
    if !is_running {
        conn.execute(
            "DELETE FROM best_effort WHERE activity_id = ?1",
            params![activity_id],
        )?;
        return Ok(());
    }
    let Ok(cols) = crate::db::trackpoints::get_trackpoints_columnar(conn, activity_id) else {
        return Ok(());
    };
    let recorded: Option<f64> = conn
        .query_row(
            "SELECT distance_m FROM activity WHERE id = ?1",
            [activity_id],
            |row| row.get::<_, Option<f64>>(0),
        )
        .ok()
        .flatten();
    let efforts =
        crate::import::best_effort::compute_best_efforts(&cols.distance_m, &cols.t, recorded);
    set_best_efforts(conn, activity_id, &efforts)
}

/// All activity ids for the given sports (used to backfill best efforts).
pub fn activity_ids_for_sports(conn: &Connection, sports: &[&str]) -> Result<Vec<String>> {
    if sports.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = sports.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
    let sql = format!(
        "SELECT id FROM activity WHERE sport_type IN ({})",
        placeholders
    );
    let mut stmt = conn.prepare(&sql)?;
    let params: Vec<&dyn rusqlite::ToSql> = sports.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
    let rows = stmt.query_map(params.as_slice(), |row| row.get::<_, String>(0))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::models::activity::Activity;
    use crate::models::trackpoint::TrackPoint;

    fn insert_run(conn: &Connection, id: &str) {
        let mut a = Activity {
            id: id.to_string(),
            start_time: "2025-06-01T08:00:00+00:00".to_string(),
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
            created_at: String::new(), updated_at: String::new(), parent_id: None,
        };
        a.created_at = "x".into();
        db::activities::insert_activity(conn, &a).unwrap();
    }

    fn tp(id: &str, t: &str, lat: f64) -> TrackPoint {
        TrackPoint {
            activity_id: id.to_string(),
            t: Some(t.to_string()),
            lat: Some(lat), lon: Some(37.62),
            altitude_m: None, speed_mps: None, hr: None, cadence: None, power_w: None,
            temperature_c: None, vertical_oscillation_mm: None, stance_time_ms: None,
            stance_time_percent: None, step_length_mm: None, grade_percent: None,
            left_right_balance: None, left_torque_effectiveness: None, right_torque_effectiveness: None,
            left_pedal_smoothness: None, right_pedal_smoothness: None,
        }
    }

    /// End-to-end: a run stored with ISO-8601 trackpoint timestamps should yield
    /// best-effort splits. This is the real-data path that was silently empty
    /// before timestamps were parsed to seconds.
    #[test]
    fn recompute_running_from_iso_trackpoints() {
        let conn = db::test_db();
        insert_run(&conn, "r1");
        // ~1 km per 0.009° latitude; 7 points → ~6 km, 5 min apart (5:00/km).
        let mut tps = Vec::new();
        for k in 0..7 {
            let lat = 55.0 + 0.009 * k as f64;
            let min = k * 5;
            let t = format!("2025-06-01T08:{:02}:00+00:00", min);
            tps.push(tp("r1", &t, lat));
        }
        db::trackpoints::insert_trackpoints(&conn, &tps).unwrap();

        let n = recompute_running(&conn).unwrap();
        assert_eq!(n, 1, "one running activity produced efforts");

        // The 5 km split exists and is roughly 5 × 300 s (allow window slack).
        let five = fastest_for_distance(&conn, "run", 5000.0).unwrap();
        assert!(five.is_some(), "5 km best effort stored");
        let secs = five.unwrap().3;
        assert!((1400.0..=1600.0).contains(&secs), "5 km ~1500 s, got {secs}");
    }

    /// Correcting a mis-imported sport: a run first stored as another sport
    /// gains splits when set to "run", and loses them when set back.
    #[test]
    fn recompute_for_activity_tracks_the_sport() {
        let conn = db::test_db();
        insert_run(&conn, "r1"); // starts as "run"
        let mut tps = Vec::new();
        for k in 0..7 {
            let t = format!("2025-06-01T08:{:02}:00+00:00", k * 5);
            tps.push(tp("r1", &t, 55.0 + 0.009 * k as f64));
        }
        db::trackpoints::insert_trackpoints(&conn, &tps).unwrap();

        // Not a running sport → no splits.
        conn.execute("UPDATE activity SET sport_type='hike' WHERE id='r1'", []).unwrap();
        recompute_for_activity(&conn, "r1").unwrap();
        assert!(fastest_for_distance(&conn, "run", 5000.0).unwrap().is_none());

        // Corrected to a running sport → splits appear.
        conn.execute("UPDATE activity SET sport_type='run' WHERE id='r1'", []).unwrap();
        recompute_for_activity(&conn, "r1").unwrap();
        assert!(fastest_for_distance(&conn, "run", 5000.0).unwrap().is_some());

        // Changed away again → splits cleared.
        conn.execute("UPDATE activity SET sport_type='ride' WHERE id='r1'", []).unwrap();
        recompute_for_activity(&conn, "r1").unwrap();
        let cnt: i64 = conn
            .query_row("SELECT COUNT(*) FROM best_effort WHERE activity_id='r1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(cnt, 0, "non-running activity keeps no best-effort rows");
    }

    #[test]
    fn recompute_is_idempotent() {
        let conn = db::test_db();
        insert_run(&conn, "r1");
        let mut tps = Vec::new();
        for k in 0..7 {
            let t = format!("2025-06-01T08:{:02}:00+00:00", k * 5);
            tps.push(tp("r1", &t, 55.0 + 0.009 * k as f64));
        }
        db::trackpoints::insert_trackpoints(&conn, &tps).unwrap();

        recompute_running(&conn).unwrap();
        recompute_running(&conn).unwrap();
        // No duplicate rows for the same (activity, distance).
        let cnt: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM best_effort WHERE activity_id='r1' AND distance_m=5000.0",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cnt, 1);
    }
}
