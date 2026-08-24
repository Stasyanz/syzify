use rusqlite::{params, Connection, Result};

use crate::models::trackpoint::{TrackPoint, TrackPointColumns};

pub fn insert_trackpoints(conn: &Connection, trackpoints: &[TrackPoint]) -> Result<()> {
    let mut stmt = conn.prepare(
        "INSERT INTO trackpoint (activity_id, t, lat, lon, altitude_m, speed_mps, hr, cadence, power_w, temperature_c,
         vertical_oscillation_mm, stance_time_ms, stance_time_percent, step_length_mm, grade_percent,
         left_right_balance, left_torque_effectiveness, right_torque_effectiveness, left_pedal_smoothness, right_pedal_smoothness)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
    )?;

    for tp in trackpoints {
        stmt.execute(params![
            tp.activity_id,
            tp.t,
            tp.lat,
            tp.lon,
            tp.altitude_m,
            tp.speed_mps,
            tp.hr,
            tp.cadence,
            tp.power_w,
            tp.temperature_c,
            tp.vertical_oscillation_mm,
            tp.stance_time_ms,
            tp.stance_time_percent,
            tp.step_length_mm,
            tp.grade_percent,
            tp.left_right_balance,
            tp.left_torque_effectiveness,
            tp.right_torque_effectiveness,
            tp.left_pedal_smoothness,
            tp.right_pedal_smoothness,
        ])?;
    }
    Ok(())
}

/// Returns trackpoints as rows (for export).
pub fn get_trackpoints(conn: &Connection, activity_id: &str) -> Result<Vec<TrackPoint>> {
    let mut stmt = conn.prepare(
        "SELECT activity_id, t, lat, lon, altitude_m, speed_mps, hr, cadence, power_w, temperature_c,
         vertical_oscillation_mm, stance_time_ms, stance_time_percent, step_length_mm, grade_percent,
         left_right_balance, left_torque_effectiveness, right_torque_effectiveness, left_pedal_smoothness, right_pedal_smoothness
         FROM trackpoint WHERE activity_id = ?1 ORDER BY id ASC",
    )?;

    let rows = stmt.query_map(params![activity_id], |row| {
        Ok(TrackPoint {
            activity_id: row.get(0)?,
            t: row.get(1)?,
            lat: row.get(2)?,
            lon: row.get(3)?,
            altitude_m: row.get(4)?,
            speed_mps: row.get(5)?,
            hr: row.get(6)?,
            cadence: row.get(7)?,
            power_w: row.get(8)?,
            temperature_c: row.get(9)?,
            vertical_oscillation_mm: row.get(10)?,
            stance_time_ms: row.get(11)?,
            stance_time_percent: row.get(12)?,
            step_length_mm: row.get(13)?,
            grade_percent: row.get(14)?,
            left_right_balance: row.get(15)?,
            left_torque_effectiveness: row.get(16)?,
            right_torque_effectiveness: row.get(17)?,
            left_pedal_smoothness: row.get(18)?,
            right_pedal_smoothness: row.get(19)?,
        })
    })?;

    let mut tps = Vec::new();
    for row in rows {
        tps.push(row?);
    }
    Ok(tps)
}

/// Returns trackpoints in columnar format for efficient frontend transfer.
pub fn get_trackpoints_columnar(conn: &Connection, activity_id: &str) -> Result<TrackPointColumns> {
    let mut stmt = conn.prepare(
        "SELECT t, lat, lon, altitude_m, speed_mps, hr, cadence, power_w, temperature_c,
         vertical_oscillation_mm, stance_time_ms, stance_time_percent, step_length_mm, grade_percent,
         left_right_balance, left_torque_effectiveness, right_torque_effectiveness, left_pedal_smoothness, right_pedal_smoothness
         FROM trackpoint WHERE activity_id = ?1 ORDER BY id ASC",
    )?;

    let mut cols = TrackPointColumns {
        t: Vec::new(),
        lat: Vec::new(),
        lon: Vec::new(),
        altitude_m: Vec::new(),
        speed_mps: Vec::new(),
        hr: Vec::new(),
        cadence: Vec::new(),
        power_w: Vec::new(),
        temperature_c: Vec::new(),
        vertical_oscillation_mm: Vec::new(),
        stance_time_ms: Vec::new(),
        stance_time_percent: Vec::new(),
        step_length_mm: Vec::new(),
        grade_percent: Vec::new(),
        distance_m: Vec::new(),
        left_right_balance: Vec::new(),
        left_torque_effectiveness: Vec::new(),
        right_torque_effectiveness: Vec::new(),
        left_pedal_smoothness: Vec::new(),
        right_pedal_smoothness: Vec::new(),
    };

    let rows = stmt.query_map(params![activity_id], |row| {
        Ok((
            (
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<f64>>(1)?,
                row.get::<_, Option<f64>>(2)?,
                row.get::<_, Option<f64>>(3)?,
                row.get::<_, Option<f64>>(4)?,
                row.get::<_, Option<i32>>(5)?,
                row.get::<_, Option<i32>>(6)?,
                row.get::<_, Option<i32>>(7)?,
                row.get::<_, Option<f64>>(8)?,
                row.get::<_, Option<f64>>(9)?,
            ),
            (
                row.get::<_, Option<f64>>(10)?,
                row.get::<_, Option<f64>>(11)?,
                row.get::<_, Option<f64>>(12)?,
                row.get::<_, Option<f64>>(13)?,
                row.get::<_, Option<f64>>(14)?,
                row.get::<_, Option<f64>>(15)?,
                row.get::<_, Option<f64>>(16)?,
                row.get::<_, Option<f64>>(17)?,
                row.get::<_, Option<f64>>(18)?,
            ),
        ))
    })?;

    let mut prev_lat: Option<f64> = None;
    let mut prev_lon: Option<f64> = None;
    let mut cumulative_dist = 0.0;

    for row in rows {
        let ((t_str, lat, lon, alt, speed, hr, cad, power, temp, vert_osc),
             (stance, stance_pct, step_len, grade, lr_balance, l_torque, r_torque, l_smooth, r_smooth)) = row?;

        // Time as seconds (f64). Trackpoints store an ISO-8601 timestamp, so
        // parse that to epoch seconds; fall back to a bare numeric offset. Both
        // the charts (time axis) and best-effort splits consume this as seconds.
        let t_val: Option<f64> = t_str.and_then(|s| parse_time_seconds(&s));
        cols.t.push(t_val);
        cols.lat.push(lat);
        cols.lon.push(lon);
        cols.altitude_m.push(alt);
        cols.speed_mps.push(speed);
        cols.hr.push(hr);
        cols.cadence.push(cad);
        cols.power_w.push(power);
        cols.temperature_c.push(temp);
        cols.vertical_oscillation_mm.push(vert_osc);
        cols.stance_time_ms.push(stance);
        cols.stance_time_percent.push(stance_pct);
        cols.step_length_mm.push(step_len);
        cols.grade_percent.push(grade);
        cols.left_right_balance.push(lr_balance);
        cols.left_torque_effectiveness.push(l_torque);
        cols.right_torque_effectiveness.push(r_torque);
        cols.left_pedal_smoothness.push(l_smooth);
        cols.right_pedal_smoothness.push(r_smooth);

        // Compute cumulative distance using haversine
        if let (Some(lat2), Some(lon2)) = (lat, lon) {
            if let (Some(lat1), Some(lon1)) = (prev_lat, prev_lon) {
                cumulative_dist += haversine_m(lat1, lon1, lat2, lon2);
            }
            prev_lat = Some(lat2);
            prev_lon = Some(lon2);
        }
        cols.distance_m.push(Some(cumulative_dist));
    }

    Ok(cols)
}

/// Just the geometry columns, in the SAME `ORDER BY id ASC` as
/// [`get_trackpoints_columnar`] — the segment feature addresses trackpoints
/// by index across both reads, so their row order must never diverge.
pub fn get_track_geometry(
    conn: &Connection,
    activity_id: &str,
) -> Result<crate::models::trackpoint::TrackGeometry> {
    let mut stmt = conn.prepare(
        "SELECT t, lat, lon, altitude_m FROM trackpoint WHERE activity_id = ?1 ORDER BY id ASC",
    )?;
    let mut geo = crate::models::trackpoint::TrackGeometry {
        t: Vec::new(),
        lat: Vec::new(),
        lon: Vec::new(),
        altitude_m: Vec::new(),
    };
    let mut rows = stmt.query(params![activity_id])?;
    while let Some(row) = rows.next()? {
        let t_str: Option<String> = row.get(0)?;
        geo.t.push(t_str.and_then(|s| parse_time_seconds(&s)));
        geo.lat.push(row.get(1)?);
        geo.lon.push(row.get(2)?);
        geo.altitude_m.push(row.get(3)?);
    }
    Ok(geo)
}

/// Trackpoint time → seconds. Accepts an ISO-8601 timestamp (what parsers
/// store, e.g. "2025-02-07T18:55:09+03:00") returned as epoch seconds, or a
/// bare numeric offset. Returns `None` for anything unparseable.
fn parse_time_seconds(s: &str) -> Option<f64> {
    if let Ok(v) = s.parse::<f64>() {
        return Some(v);
    }
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.timestamp() as f64)
}

/// Haversine distance in meters between two lat/lon points.
pub(crate) fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6_371_000.0; // Earth radius in meters
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
    use crate::db;
    use crate::models::trackpoint::TrackPoint;

    fn insert_test_activity(conn: &rusqlite::Connection, id: &str) {
        let a = crate::models::activity::Activity {
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
            ..Default::default()
        };
        db::activities::insert_activity(conn, &a).unwrap();
    }

    #[test]
    fn insert_and_retrieve_trackpoints() {
        let conn = db::test_db();
        insert_test_activity(&conn, "tp-act");

        let tps = vec![
            TrackPoint {
                activity_id: "tp-act".to_string(),
                t: Some("2025-06-01T08:00:00+00:00".to_string()),
                lat: Some(55.75), lon: Some(37.62),
                altitude_m: Some(150.0), speed_mps: Some(3.0),
                hr: Some(140), cadence: Some(85), power_w: None, temperature_c: None,
                vertical_oscillation_mm: None, stance_time_ms: None, stance_time_percent: None, step_length_mm: None, grade_percent: None,
                left_right_balance: None, left_torque_effectiveness: None, right_torque_effectiveness: None,
                left_pedal_smoothness: None, right_pedal_smoothness: None,
            },
            TrackPoint {
                activity_id: "tp-act".to_string(),
                t: Some("2025-06-01T08:00:05+00:00".to_string()),
                lat: Some(55.7501), lon: Some(37.6201),
                altitude_m: Some(151.0), speed_mps: Some(3.1),
                hr: Some(142), cadence: Some(86), power_w: None, temperature_c: None,
                vertical_oscillation_mm: None, stance_time_ms: None, stance_time_percent: None, step_length_mm: None, grade_percent: None,
                left_right_balance: None, left_torque_effectiveness: None, right_torque_effectiveness: None,
                left_pedal_smoothness: None, right_pedal_smoothness: None,
            },
        ];

        insert_trackpoints(&conn, &tps).unwrap();

        let cols = get_trackpoints_columnar(&conn, "tp-act").unwrap();
        assert_eq!(cols.lat.len(), 2);
        assert_eq!(cols.hr, vec![Some(140), Some(142)]);
        assert!(cols.distance_m[0].unwrap() < 0.01); // first point ~ 0
        assert!(cols.distance_m[1].unwrap() > 0.0);  // second point > 0
    }

    #[test]
    fn parse_time_seconds_iso_and_numeric() {
        // ISO-8601 (what parsers store) → epoch seconds.
        let a = parse_time_seconds("2025-02-07T18:55:09+03:00").unwrap();
        let b = parse_time_seconds("2025-02-07T18:55:19+03:00").unwrap();
        assert!((b - a - 10.0).abs() < 0.001, "10 s apart");
        // Bare numeric offset still works.
        assert_eq!(parse_time_seconds("42"), Some(42.0));
        // Garbage → None.
        assert_eq!(parse_time_seconds("not-a-time"), None);
    }

    #[test]
    fn columnar_t_is_seconds_from_iso() {
        let conn = db::test_db();
        insert_test_activity(&conn, "tp-time");
        let tps = vec![
            TrackPoint {
                activity_id: "tp-time".to_string(),
                t: Some("2025-06-01T08:00:00+00:00".to_string()),
                lat: Some(55.75), lon: Some(37.62),
                altitude_m: None, speed_mps: None, hr: None, cadence: None, power_w: None, temperature_c: None,
                vertical_oscillation_mm: None, stance_time_ms: None, stance_time_percent: None, step_length_mm: None, grade_percent: None,
                left_right_balance: None, left_torque_effectiveness: None, right_torque_effectiveness: None,
                left_pedal_smoothness: None, right_pedal_smoothness: None,
            },
            TrackPoint {
                activity_id: "tp-time".to_string(),
                t: Some("2025-06-01T08:00:30+00:00".to_string()),
                lat: Some(55.7501), lon: Some(37.6201),
                altitude_m: None, speed_mps: None, hr: None, cadence: None, power_w: None, temperature_c: None,
                vertical_oscillation_mm: None, stance_time_ms: None, stance_time_percent: None, step_length_mm: None, grade_percent: None,
                left_right_balance: None, left_torque_effectiveness: None, right_torque_effectiveness: None,
                left_pedal_smoothness: None, right_pedal_smoothness: None,
            },
        ];
        insert_trackpoints(&conn, &tps).unwrap();
        let cols = get_trackpoints_columnar(&conn, "tp-time").unwrap();
        // Both parse to numbers (previously ISO failed f64 parse → None).
        let t0 = cols.t[0].unwrap();
        let t1 = cols.t[1].unwrap();
        assert!((t1 - t0 - 30.0).abs() < 0.001);
    }

    #[test]
    fn haversine_known_distance() {
        // Moscow (55.75, 37.62) to approx 1 degree north ≈ 111km
        let d = haversine_m(55.75, 37.62, 56.75, 37.62);
        assert!((d - 111_195.0).abs() < 500.0); // within 500m tolerance
    }

    #[test]
    fn get_trackpoints_row_format() {
        let conn = db::test_db();
        insert_test_activity(&conn, "tp-row");

        let tps = vec![
            TrackPoint {
                activity_id: "tp-row".to_string(),
                t: Some("2025-06-01T08:00:00+00:00".to_string()),
                lat: Some(55.75), lon: Some(37.62),
                altitude_m: Some(150.0), speed_mps: Some(3.0),
                hr: Some(140), cadence: Some(85), power_w: Some(200), temperature_c: None,
                vertical_oscillation_mm: None, stance_time_ms: None, stance_time_percent: None, step_length_mm: None, grade_percent: None,
                left_right_balance: None, left_torque_effectiveness: None, right_torque_effectiveness: None,
                left_pedal_smoothness: None, right_pedal_smoothness: None,
            },
            TrackPoint {
                activity_id: "tp-row".to_string(),
                t: Some("2025-06-01T08:00:05+00:00".to_string()),
                lat: Some(55.7501), lon: Some(37.6201),
                altitude_m: Some(151.0), speed_mps: Some(3.1),
                hr: Some(142), cadence: Some(86), power_w: None, temperature_c: None,
                vertical_oscillation_mm: None, stance_time_ms: None, stance_time_percent: None, step_length_mm: None, grade_percent: None,
                left_right_balance: None, left_torque_effectiveness: None, right_torque_effectiveness: None,
                left_pedal_smoothness: None, right_pedal_smoothness: None,
            },
        ];

        insert_trackpoints(&conn, &tps).unwrap();

        let rows = get_trackpoints(&conn, "tp-row").unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].lat, Some(55.75));
        assert_eq!(rows[0].power_w, Some(200));
        assert_eq!(rows[1].power_w, None);
        assert_eq!(rows[1].cadence, Some(86));
    }

    #[test]
    fn get_trackpoints_empty_activity() {
        let conn = db::test_db();
        insert_test_activity(&conn, "tp-empty");

        let rows = get_trackpoints(&conn, "tp-empty").unwrap();
        assert!(rows.is_empty());

        let cols = get_trackpoints_columnar(&conn, "tp-empty").unwrap();
        assert!(cols.lat.is_empty());
    }

    #[test]
    fn columnar_cumulative_distance_indoor_points() {
        let conn = db::test_db();
        insert_test_activity(&conn, "tp-indoor");

        let tps = vec![
            TrackPoint {
                activity_id: "tp-indoor".to_string(),
                t: None,
                lat: None, lon: None, // indoor
                altitude_m: None, speed_mps: None,
                hr: Some(150), cadence: None, power_w: None, temperature_c: None,
                vertical_oscillation_mm: None, stance_time_ms: None, stance_time_percent: None, step_length_mm: None, grade_percent: None,
                left_right_balance: None, left_torque_effectiveness: None, right_torque_effectiveness: None,
                left_pedal_smoothness: None, right_pedal_smoothness: None,
            },
            TrackPoint {
                activity_id: "tp-indoor".to_string(),
                t: None,
                lat: None, lon: None,
                altitude_m: None, speed_mps: None,
                hr: Some(155), cadence: None, power_w: None, temperature_c: None,
                vertical_oscillation_mm: None, stance_time_ms: None, stance_time_percent: None, step_length_mm: None, grade_percent: None,
                left_right_balance: None, left_torque_effectiveness: None, right_torque_effectiveness: None,
                left_pedal_smoothness: None, right_pedal_smoothness: None,
            },
        ];

        insert_trackpoints(&conn, &tps).unwrap();

        let cols = get_trackpoints_columnar(&conn, "tp-indoor").unwrap();
        assert_eq!(cols.hr, vec![Some(150), Some(155)]);
        // Distance should still be 0 for indoor points
        assert_eq!(cols.distance_m, vec![Some(0.0), Some(0.0)]);
    }

    #[test]
    fn geometry_read_matches_columnar_row_order() {
        // The segment feature addresses trackpoints by index across BOTH
        // reads — if their ORDER BY ever diverges, every saved segment
        // silently shifts. Lock the contract down element-by-element.
        let conn = db::test_db();
        insert_test_activity(&conn, "tp-geo");

        let mk = |lat: f64, alt: Option<f64>| TrackPoint {
            activity_id: "tp-geo".to_string(),
            t: None,
            lat: Some(lat), lon: Some(37.62),
            altitude_m: alt, speed_mps: None,
            hr: None, cadence: None, power_w: None, temperature_c: None,
            vertical_oscillation_mm: None, stance_time_ms: None, stance_time_percent: None, step_length_mm: None, grade_percent: None,
            left_right_balance: None, left_torque_effectiveness: None, right_torque_effectiveness: None,
            left_pedal_smoothness: None, right_pedal_smoothness: None,
        };
        insert_trackpoints(&conn, &[mk(55.75, Some(100.0)), mk(55.76, None), mk(55.77, Some(120.0))])
            .unwrap();

        let cols = get_trackpoints_columnar(&conn, "tp-geo").unwrap();
        let geo = get_track_geometry(&conn, "tp-geo").unwrap();
        assert_eq!(geo.t, cols.t);
        assert_eq!(geo.lat, cols.lat);
        assert_eq!(geo.lon, cols.lon);
        assert_eq!(geo.altitude_m, cols.altitude_m);
    }
}
