use rusqlite::{Connection, OptionalExtension, Result};

/// Check if an activity with similar start_time and metrics already exists.
///
/// Two strategies:
/// 1. With distance: start_time ±5min AND distance ±5% (and sport_type if
///    `require_sport` — used for CSV rows where two different sports can share a
///    distance/time and must not be collapsed; the GPX pipeline passes false so
///    a GPX+CSV pair of the same activity can still dedup across sport labels).
/// 2. Without distance (indoor/strength): start_time ±5min AND sport_type match AND duration ±10%
///
/// A sport_type match is a match within the sport's family (`SportType::family`):
/// a ride that came in as plain `ride` from a TCX and again as `indoor_ride`
/// from the FIT that names the trainer is one workout (#189).
pub fn is_content_duplicate(
    conn: &Connection,
    start_time: &str,
    sport_type: &str,
    distance_m: Option<f64>,
    duration_s: Option<f64>,
    require_sport: bool,
) -> Result<bool> {
    Ok(find_content_duplicate(conn, start_time, sport_type, distance_m, duration_s, require_sport)?
        .is_some())
}

/// Like [`is_content_duplicate`], but returns the matched activity's id — the
/// import pipeline links the duplicate's raw file to it, so deleting the
/// activity takes every source file (and its dedup hash) along.
pub fn find_content_duplicate(
    conn: &Connection,
    start_time: &str,
    sport_type: &str,
    distance_m: Option<f64>,
    duration_s: Option<f64>,
    require_sport: bool,
) -> Result<Option<String>> {
    const TIME_TOLERANCE_S: i64 = 300; // 5 minutes

    // The sport's family, or the sport alone, as `sport_type IN (?N, …)`
    // starting at placeholder `first` — the params follow the fixed ones.
    let family = crate::models::activity::SportType::family(sport_type);
    let sports: Vec<&str> = if family.is_empty() { vec![sport_type] } else { family.to_vec() };
    let sport_in = |first: usize| -> String {
        let marks: Vec<String> = (0..sports.len()).map(|i| format!("?{}", first + i)).collect();
        format!("sport_type IN ({})", marks.join(", "))
    };
    let with_sports = |fixed: Vec<Box<dyn rusqlite::ToSql>>| -> Vec<Box<dyn rusqlite::ToSql>> {
        let mut all = fixed;
        all.extend(sports.iter().map(|s| Box::new(s.to_string()) as Box<dyn rusqlite::ToSql>));
        all
    };

    // Strategy 1: dedup by distance (outdoor activities)
    if let Some(d) = distance_m {
        if d > 0.0 {
            let dist_min = d * 0.95;
            let dist_max = d * 1.05;

            let sport_clause = if require_sport { format!("AND {}", sport_in(5)) } else { String::new() };
            let sql = format!(
                "SELECT id FROM activity
                 WHERE distance_m BETWEEN ?1 AND ?2
                 AND abs(julianday(start_time) - julianday(?3)) * 86400 < ?4 {sport_clause}
                 LIMIT 1"
            );
            let fixed: Vec<Box<dyn rusqlite::ToSql>> = vec![
                Box::new(dist_min),
                Box::new(dist_max),
                Box::new(start_time.to_string()),
                Box::new(TIME_TOLERANCE_S),
            ];
            let params = if require_sport { with_sports(fixed) } else { fixed };
            let id: Option<String> = conn
                .query_row(&sql, rusqlite::params_from_iter(params.iter()), |row| row.get(0))
                .optional()?;

            return Ok(id);
        }
    }

    // Strategy 2: dedup by sport_type + duration (indoor/strength/zero-distance)
    if let Some(dur) = duration_s {
        if dur > 0.0 {
            let dur_min = dur * 0.90;
            let dur_max = dur * 1.10;

            let sql = format!(
                "SELECT id FROM activity
                 WHERE duration_s BETWEEN ?1 AND ?2
                 AND abs(julianday(start_time) - julianday(?3)) * 86400 < ?4
                 AND {}
                 LIMIT 1",
                sport_in(5)
            );
            let params = with_sports(vec![
                Box::new(dur_min),
                Box::new(dur_max),
                Box::new(start_time.to_string()),
                Box::new(TIME_TOLERANCE_S),
            ]);
            let id: Option<String> = conn
                .query_row(&sql, rusqlite::params_from_iter(params.iter()), |row| row.get(0))
                .optional()?;

            return Ok(id);
        }
    }

    // Strategy 3: no distance AND no duration — match by sport_type + time only
    let sql = format!(
        "SELECT id FROM activity
         WHERE COALESCE(distance_m, 0) = 0
         AND COALESCE(duration_s, 0) = 0
         AND abs(julianday(start_time) - julianday(?1)) * 86400 < ?2
         AND {}
         LIMIT 1",
        sport_in(3)
    );
    let params = with_sports(vec![Box::new(start_time.to_string()), Box::new(TIME_TOLERANCE_S)]);
    let id: Option<String> = conn
        .query_row(&sql, rusqlite::params_from_iter(params.iter()), |row| row.get(0))
        .optional()?;

    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::models::activity::Activity;

    fn insert_activity(conn: &Connection, id: &str, start: &str, sport: &str, dist: Option<f64>, dur: Option<f64>) {
        let a = Activity {
            id: id.to_string(),
            start_time: start.to_string(),
            timezone_offset: None,
            sport_type: sport.to_string(),
            title: None, notes: None,
            distance_m: dist, duration_s: dur,
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

    /// The pipeline links a duplicate's raw file to the matched activity —
    /// the finder must hand back the id, not just a yes/no.
    #[test]
    fn find_returns_the_matched_activity_id() {
        let conn = db::test_db();
        insert_activity(&conn, "a1", "2025-06-01T08:00:00", "run", Some(5000.0), Some(1800.0));

        assert_eq!(
            find_content_duplicate(&conn, "2025-06-01T08:02:00", "run", Some(5000.0), None, false)
                .unwrap()
                .as_deref(),
            Some("a1")
        );
        assert_eq!(
            find_content_duplicate(&conn, "2025-06-01T12:00:00", "run", Some(5000.0), None, false)
                .unwrap(),
            None
        );
    }

    #[test]
    fn no_duplicate_when_db_empty() {
        let conn = db::test_db();
        assert!(!is_content_duplicate(&conn, "2025-06-01T08:00:00", "run", Some(5000.0), Some(1800.0), false).unwrap());
    }

    #[test]
    fn detects_duplicate_by_distance() {
        let conn = db::test_db();
        insert_activity(&conn, "a1", "2025-06-01T08:00:00", "run", Some(5000.0), Some(1800.0));

        // Same time, same distance
        assert!(is_content_duplicate(&conn, "2025-06-01T08:00:00", "run", Some(5000.0), Some(1800.0), false).unwrap());

        // 2 min offset (within 5min tolerance)
        assert!(is_content_duplicate(&conn, "2025-06-01T08:02:00", "run", Some(5000.0), Some(1800.0), false).unwrap());

        // Distance within 5%
        assert!(is_content_duplicate(&conn, "2025-06-01T08:00:00", "run", Some(5200.0), Some(1800.0), false).unwrap());
    }

    #[test]
    fn no_duplicate_when_too_different() {
        let conn = db::test_db();
        insert_activity(&conn, "a1", "2025-06-01T08:00:00", "run", Some(5000.0), Some(1800.0));

        // Time too far apart (>5min)
        assert!(!is_content_duplicate(&conn, "2025-06-01T10:00:00", "run", Some(5000.0), Some(1800.0), false).unwrap());

        // Distance too different (>5%)
        assert!(!is_content_duplicate(&conn, "2025-06-01T08:00:00", "run", Some(10000.0), Some(1800.0), false).unwrap());
    }

    #[test]
    fn strict_sport_does_not_collapse_different_sports() {
        let conn = db::test_db();
        insert_activity(&conn, "a1", "2025-06-01T08:00:00", "swim", Some(1000.0), Some(1800.0));

        // require_sport=false (GPX pipeline) — distance+time match regardless of sport.
        assert!(is_content_duplicate(&conn, "2025-06-01T08:03:00", "run", Some(1010.0), Some(1800.0), false).unwrap());

        // require_sport=true (CSV path) — a different sport at the same distance/time
        // is NOT a duplicate (no silent data loss).
        assert!(!is_content_duplicate(&conn, "2025-06-01T08:03:00", "run", Some(1010.0), Some(1800.0), true).unwrap());

        // Same sport still dedups under strict mode.
        assert!(is_content_duplicate(&conn, "2025-06-01T08:03:00", "swim", Some(1010.0), Some(1800.0), true).unwrap());
    }

    #[test]
    fn dedup_indoor_by_duration_and_sport() {
        let conn = db::test_db();
        insert_activity(&conn, "a1", "2025-06-01T08:00:00", "strength", None, Some(3600.0));

        // Same sport, similar time and duration
        assert!(is_content_duplicate(&conn, "2025-06-01T08:01:00", "strength", None, Some(3600.0), false).unwrap());

        // Duration within 10%
        assert!(is_content_duplicate(&conn, "2025-06-01T08:00:00", "strength", None, Some(3500.0), false).unwrap());

        // Different sport — not a duplicate
        assert!(!is_content_duplicate(&conn, "2025-06-01T08:00:00", "swim", None, Some(3600.0), false).unwrap());

        // Duration too different
        assert!(!is_content_duplicate(&conn, "2025-06-01T08:00:00", "strength", None, Some(7200.0), false).unwrap());
    }

    /// A ride that came in as plain `ride` (a TCX, a CSV row) and again from
    /// the FIT that names the trainer is one workout (#189): the family
    /// matches, on every strategy.
    #[test]
    fn dedup_matches_a_trainer_ride_with_its_plain_ride_copy() {
        let conn = db::test_db();
        insert_activity(&conn, "no-sensor", "2025-06-01T08:00:00", "ride", None, Some(3600.0));
        insert_activity(&conn, "zwift", "2025-06-02T08:00:00", "virtual_ride", Some(20000.0), Some(2400.0));
        insert_activity(&conn, "bare", "2025-06-03T08:00:00", "ride", None, None);

        // Strategy 2: duration only.
        assert_eq!(
            find_content_duplicate(&conn, "2025-06-01T08:01:00", "indoor_ride", None, Some(3500.0), false).unwrap().as_deref(),
            Some("no-sensor")
        );
        assert!(!is_content_duplicate(&conn, "2025-06-01T08:01:00", "treadmill", None, Some(3600.0), false).unwrap());
        // Strategy 1 with the sport required (the CSV path).
        assert_eq!(
            find_content_duplicate(&conn, "2025-06-02T08:02:00", "ride", Some(20100.0), Some(2400.0), true).unwrap().as_deref(),
            Some("zwift")
        );
        assert!(!is_content_duplicate(&conn, "2025-06-02T08:02:00", "run", Some(20100.0), Some(2400.0), true).unwrap());
        // Strategy 3: nothing but the time.
        assert_eq!(
            find_content_duplicate(&conn, "2025-06-03T08:03:00", "virtual_ride", None, None, false).unwrap().as_deref(),
            Some("bare")
        );
        // The running family is one family too: a treadmill session that a
        // CSV row filed as a run.
        insert_activity(&conn, "belt", "2025-06-04T07:00:00", "run", None, Some(1800.0));
        assert_eq!(
            find_content_duplicate(&conn, "2025-06-04T07:01:00", "treadmill", None, Some(1800.0), false).unwrap().as_deref(),
            Some("belt")
        );
    }

    #[test]
    fn dedup_zero_distance_zero_duration() {
        let conn = db::test_db();
        insert_activity(&conn, "a1", "2025-06-01T08:00:00", "other", Some(0.0), Some(0.0));

        // Same sport, similar time, both zero
        assert!(is_content_duplicate(&conn, "2025-06-01T08:01:00", "other", Some(0.0), Some(0.0), false).unwrap());
        assert!(is_content_duplicate(&conn, "2025-06-01T08:01:00", "other", None, None, false).unwrap());

        // Different sport
        assert!(!is_content_duplicate(&conn, "2025-06-01T08:01:00", "run", None, None, false).unwrap());
    }
}
