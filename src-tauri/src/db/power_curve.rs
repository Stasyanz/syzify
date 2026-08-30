use rusqlite::{params, Connection, Result};

use crate::models::power_curve::{PowerCurveData, PowerCurveEnvelopePoint, PowerCurvePoint};

/// Sports whose power streams are comparable. The envelope never mixes
/// running power (Stryd & friends) into a ride's chart — a 900 W running
/// 5-second peak would sit on top of every cycling curve forever.
fn power_sport_group(sport: &str) -> &'static [&'static str] {
    match sport {
        "ride" | "mountain_bike" => &["ride", "mountain_bike"],
        "run" | "trail_run" | "treadmill" => &["run", "trail_run", "treadmill"],
        _ => &[], // caller falls back to the exact sport
    }
}

/// Replace an activity's stored mean-max curve (idempotent for re-imports
/// and the startup backfill). Transactional: a failure mid-insert must not
/// leave a truncated curve that nothing will ever recompute.
pub fn set_power_curve(
    conn: &Connection,
    activity_id: &str,
    points: &[PowerCurvePoint],
) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "DELETE FROM power_curve WHERE activity_id = ?1",
        params![activity_id],
    )?;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO power_curve (activity_id, window_s, watts) VALUES (?1, ?2, ?3)",
        )?;
        for p in points {
            stmt.execute(params![activity_id, p.window_s, p.watts])?;
        }
    }
    tx.commit()
}

pub fn get_power_curve(conn: &Connection, activity_id: &str) -> Result<Vec<PowerCurvePoint>> {
    let mut stmt = conn.prepare(
        "SELECT window_s, watts FROM power_curve
         WHERE activity_id = ?1 ORDER BY window_s ASC",
    )?;
    let rows = stmt.query_map(params![activity_id], |row| {
        Ok(PowerCurvePoint {
            window_s: row.get(0)?,
            watts: row.get(1)?,
        })
    })?;
    rows.collect()
}

/// The activity's curve plus the envelope of its sport group, in one fetch —
/// the Power Curve panel always draws them together.
pub fn get_data(conn: &Connection, activity_id: &str) -> Result<PowerCurveData> {
    let sport: String = conn.query_row(
        "SELECT sport_type FROM activity WHERE id = ?1",
        params![activity_id],
        |row| row.get(0),
    )?;
    Ok(PowerCurveData {
        points: get_power_curve(conn, activity_id)?,
        envelope: get_envelope_for_sport(conn, &sport)?,
    })
}

/// The all-time envelope over one sport group: per window, the best stored
/// watts and the activity that set it. Ties break on activity id so the
/// attribution is stable. Plain NOT EXISTS instead of a window function —
/// the table holds a couple dozen rows per activity, nothing to optimize.
pub fn get_envelope_for_sport(
    conn: &Connection,
    sport: &str,
) -> Result<Vec<PowerCurveEnvelopePoint>> {
    let group = power_sport_group(sport);
    let sports: Vec<&str> = if group.is_empty() { vec![sport] } else { group.to_vec() };
    let placeholders = (1..=sports.len())
        .map(|i| format!("?{}", i))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT p.window_s, p.watts, p.activity_id, a.title, a.start_time
         FROM power_curve p
         JOIN activity a ON a.id = p.activity_id
         WHERE a.sport_type IN ({ph})
           AND NOT EXISTS (
             SELECT 1 FROM power_curve q
             JOIN activity b ON b.id = q.activity_id
             WHERE q.window_s = p.window_s
               AND b.sport_type IN ({ph})
               AND (q.watts > p.watts
                    OR (q.watts = p.watts AND q.activity_id < p.activity_id))
         )
         ORDER BY p.window_s ASC",
        ph = placeholders
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(sports.iter()), |row| {
        Ok(PowerCurveEnvelopePoint {
            window_s: row.get(0)?,
            watts: row.get(1)?,
            activity_id: row.get(2)?,
            title: row.get(3)?,
            start_time: row.get(4)?,
        })
    })?;
    rows.collect()
}

/// Activities whose track carries power (backfill work list). Merged
/// triathlon containers hold no trackpoints of their own, so the DISTINCT
/// scan naturally covers legs only.
pub fn powered_activity_ids(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt =
        conn.prepare("SELECT DISTINCT activity_id FROM trackpoint WHERE power_w > 0")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    rows.collect()
}

/// Compute + store curves for the given activities. Reads only (t, power_w)
/// — the full columnar loader computes haversine distances the curve never
/// uses, and the startup backfill walks the whole powered library.
pub fn recompute_for(conn: &Connection, ids: &[String]) -> Result<usize> {
    let mut n = 0usize;
    for id in ids {
        let mut stmt = conn.prepare(
            "SELECT t, power_w FROM trackpoint WHERE activity_id = ?1 ORDER BY id ASC",
        )?;
        let rows = stmt.query_map(params![id], |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<i32>>(1)?,
            ))
        })?;
        let mut t: Vec<Option<f64>> = Vec::new();
        let mut power: Vec<Option<i32>> = Vec::new();
        for row in rows {
            let (ts, p) = row?;
            t.push(ts.as_deref().and_then(crate::db::trackpoints::parse_time_seconds));
            power.push(p);
        }
        let curve = crate::import::power_curve::compute_power_curve(&t, &power);
        if !curve.is_empty() {
            set_power_curve(conn, id, &curve)?;
            n += 1;
        }
    }
    Ok(n)
}

/// One-shot recompute over the whole library (tests and small vaults; the
/// startup backfill chunks the id list itself to keep the DB lock polite).
pub fn recompute_all(conn: &Connection) -> Result<usize> {
    let ids = powered_activity_ids(conn)?;
    recompute_for(conn, &ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::models::activity::Activity;
    use crate::models::trackpoint::TrackPoint;

    fn insert_activity(conn: &Connection, id: &str, title: &str, start: &str) {
        insert_sport_activity(conn, id, title, start, "ride");
    }

    fn insert_sport_activity(conn: &Connection, id: &str, title: &str, start: &str, sport: &str) {
        let a = Activity {
            id: id.to_string(),
            start_time: start.to_string(),
            sport_type: sport.to_string(),
            title: Some(title.to_string()),
            ..Default::default()
        };
        db::activities::insert_activity(conn, &a).unwrap();
    }

    fn point(window_s: i64, watts: f64) -> PowerCurvePoint {
        PowerCurvePoint { window_s, watts }
    }

    fn tp(activity_id: &str, t: &str, power_w: Option<i32>) -> TrackPoint {
        TrackPoint {
            activity_id: activity_id.to_string(),
            t: Some(t.to_string()),
            lat: None, lon: None, altitude_m: None, speed_mps: None,
            hr: None, cadence: None, power_w,
            temperature_c: None, vertical_oscillation_mm: None,
            stance_time_ms: None, stance_time_percent: None,
            step_length_mm: None, grade_percent: None,
            left_right_balance: None,
            left_torque_effectiveness: None, right_torque_effectiveness: None,
            left_pedal_smoothness: None, right_pedal_smoothness: None,
        }
    }

    #[test]
    fn set_get_roundtrip_and_replace() {
        let conn = db::test_db();
        insert_activity(&conn, "a1", "Ride", "2026-08-29T08:00:00+03:00");

        set_power_curve(&conn, "a1", &[point(5, 600.0), point(60, 300.0)]).unwrap();
        let got = get_power_curve(&conn, "a1").unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].window_s, 5);
        assert_eq!(got[0].watts, 600.0);

        // Re-set replaces, never accumulates.
        set_power_curve(&conn, "a1", &[point(5, 650.0)]).unwrap();
        let got = get_power_curve(&conn, "a1").unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].watts, 650.0);
    }

    #[test]
    fn envelope_picks_the_best_per_window_with_attribution() {
        let conn = db::test_db();
        insert_activity(&conn, "a1", "Older", "2026-08-29T08:00:00+03:00");
        insert_activity(&conn, "a2", "Newer", "2026-08-30T09:00:00+03:00");
        set_power_curve(&conn, "a1", &[point(5, 646.0), point(60, 346.0)]).unwrap();
        set_power_curve(&conn, "a2", &[point(5, 615.0), point(60, 339.0), point(300, 280.0)]).unwrap();

        let env = get_envelope_for_sport(&conn, "ride").unwrap();
        assert_eq!(env.len(), 3);
        assert_eq!(env[0].window_s, 5);
        assert_eq!(env[0].activity_id, "a1");
        assert_eq!(env[0].watts, 646.0);
        assert_eq!(env[0].title.as_deref(), Some("Older"));
        // 300 s exists only in a2 — the envelope still covers it.
        assert_eq!(env[2].window_s, 300);
        assert_eq!(env[2].activity_id, "a2");
    }

    #[test]
    fn envelope_stays_inside_the_sport_group() {
        let conn = db::test_db();
        insert_sport_activity(&conn, "r1", "Ride", "2026-08-29T08:00:00+03:00", "ride");
        insert_sport_activity(&conn, "m1", "MTB", "2026-08-29T10:00:00+03:00", "mountain_bike");
        insert_sport_activity(&conn, "s1", "Stryd Run", "2026-08-30T08:00:00+03:00", "run");
        set_power_curve(&conn, "r1", &[point(5, 646.0)]).unwrap();
        set_power_curve(&conn, "m1", &[point(5, 660.0)]).unwrap();
        set_power_curve(&conn, "s1", &[point(5, 900.0)]).unwrap();

        // Cycling group: MTB counts, the running 900 W peak does not.
        let env = get_envelope_for_sport(&conn, "ride").unwrap();
        assert_eq!(env.len(), 1);
        assert_eq!(env[0].activity_id, "m1");
        assert_eq!(env[0].watts, 660.0);

        // Running sees only itself.
        let env = get_envelope_for_sport(&conn, "run").unwrap();
        assert_eq!(env[0].activity_id, "s1");

        // A sport outside both groups matches only its exact kind.
        assert!(get_envelope_for_sport(&conn, "ski_xc").unwrap().is_empty());
    }

    #[test]
    fn envelope_tie_breaks_deterministically() {
        let conn = db::test_db();
        insert_activity(&conn, "b1", "One", "2026-08-29T08:00:00+03:00");
        insert_activity(&conn, "b2", "Two", "2026-08-30T08:00:00+03:00");
        set_power_curve(&conn, "b1", &[point(60, 300.0)]).unwrap();
        set_power_curve(&conn, "b2", &[point(60, 300.0)]).unwrap();

        let env = get_envelope_for_sport(&conn, "ride").unwrap();
        assert_eq!(env.len(), 1);
        assert_eq!(env[0].activity_id, "b1"); // smaller id wins the tie
    }

    #[test]
    fn get_data_bundles_curve_and_group_envelope() {
        let conn = db::test_db();
        insert_activity(&conn, "a1", "Ride", "2026-08-29T08:00:00+03:00");
        insert_sport_activity(&conn, "s1", "Run", "2026-08-30T08:00:00+03:00", "run");
        set_power_curve(&conn, "a1", &[point(5, 646.0)]).unwrap();
        set_power_curve(&conn, "s1", &[point(5, 900.0)]).unwrap();

        let data = get_data(&conn, "a1").unwrap();
        assert_eq!(data.points.len(), 1);
        assert_eq!(data.envelope.len(), 1);
        assert_eq!(data.envelope[0].activity_id, "a1"); // run stayed out
    }

    #[test]
    fn deleting_the_activity_cascades() {
        let conn = db::test_db();
        insert_activity(&conn, "c1", "Gone", "2026-08-29T08:00:00+03:00");
        set_power_curve(&conn, "c1", &[point(5, 500.0)]).unwrap();

        db::activities::delete_activity(&conn, "c1").unwrap();
        assert!(get_envelope_for_sport(&conn, "ride").unwrap().is_empty());
    }

    #[test]
    fn recompute_all_builds_curves_from_trackpoints() {
        let conn = db::test_db();
        insert_activity(&conn, "d1", "Powered", "2026-08-29T08:00:00+03:00");
        insert_activity(&conn, "d2", "No power", "2026-08-30T08:00:00+03:00");

        let mut tps = Vec::new();
        for i in 0..90 {
            let hhmmss = format!("08:{:02}:{:02}+03:00", i / 60, i % 60);
            tps.push(tp("d1", &format!("2026-08-29T{}", hhmmss), Some(250)));
            tps.push(tp("d2", &format!("2026-08-30T{}", hhmmss), None));
        }
        db::trackpoints::insert_trackpoints(&conn, &tps).unwrap();

        let n = recompute_all(&conn).unwrap();
        assert_eq!(n, 1, "only the powered activity produced a curve");
        let curve = get_power_curve(&conn, "d1").unwrap();
        assert!(!curve.is_empty());
        assert!(curve.iter().all(|p| (p.watts - 250.0).abs() < 1e-9));
        assert!(get_power_curve(&conn, "d2").unwrap().is_empty());
    }

    #[test]
    fn powered_ids_feed_chunked_recompute() {
        let conn = db::test_db();
        insert_activity(&conn, "e1", "P1", "2026-08-29T08:00:00+03:00");
        insert_activity(&conn, "e2", "P2", "2026-08-30T08:00:00+03:00");
        let mut tps = Vec::new();
        for i in 0..30 {
            let t1 = format!("2026-08-29T08:00:{:02}+03:00", i);
            let t2 = format!("2026-08-30T08:00:{:02}+03:00", i);
            tps.push(tp("e1", &t1, Some(200)));
            tps.push(tp("e2", &t2, Some(210)));
        }
        db::trackpoints::insert_trackpoints(&conn, &tps).unwrap();

        let ids = powered_activity_ids(&conn).unwrap();
        assert_eq!(ids.len(), 2);
        // Chunk of one at a time — what the startup backfill does.
        for chunk in ids.chunks(1) {
            recompute_for(&conn, chunk).unwrap();
        }
        assert!(!get_power_curve(&conn, "e1").unwrap().is_empty());
        assert!(!get_power_curve(&conn, "e2").unwrap().is_empty());
    }
}
