use rusqlite::{params, Connection, Result};

use crate::models::swim_length::SwimLength;

pub fn insert_swim_lengths(conn: &Connection, lengths: &[SwimLength]) -> Result<()> {
    let mut stmt = conn.prepare(
        "INSERT INTO swim_length (activity_id, length_number, start_time,
         total_elapsed_time_s, total_timer_time_s, avg_speed_mps,
         avg_swimming_cadence, swim_stroke, total_strokes, total_calories, length_type)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
    )?;

    for length in lengths {
        stmt.execute(params![
            length.activity_id,
            length.length_number,
            length.start_time,
            length.total_elapsed_time_s,
            length.total_timer_time_s,
            length.avg_speed_mps,
            length.avg_swimming_cadence,
            length.swim_stroke,
            length.total_strokes,
            length.total_calories,
            length.length_type,
        ])?;
    }
    Ok(())
}

pub fn get_swim_lengths(conn: &Connection, activity_id: &str) -> Result<Vec<SwimLength>> {
    let mut stmt = conn.prepare(
        "SELECT id, activity_id, length_number, start_time,
         total_elapsed_time_s, total_timer_time_s, avg_speed_mps,
         avg_swimming_cadence, swim_stroke, total_strokes, total_calories, length_type
         FROM swim_length WHERE activity_id = ?1 ORDER BY length_number ASC",
    )?;

    let rows = stmt.query_map(params![activity_id], |row| {
        Ok(SwimLength {
            id: row.get(0)?,
            activity_id: row.get(1)?,
            length_number: row.get(2)?,
            start_time: row.get(3)?,
            total_elapsed_time_s: row.get(4)?,
            total_timer_time_s: row.get(5)?,
            avg_speed_mps: row.get(6)?,
            avg_swimming_cadence: row.get(7)?,
            swim_stroke: row.get(8)?,
            total_strokes: row.get(9)?,
            total_calories: row.get(10)?,
            length_type: row.get(11)?,
        })
    })?;

    let mut lengths = Vec::new();
    for row in rows {
        lengths.push(row?);
    }
    Ok(lengths)
}

// Symmetric per-table delete. Activity removal cascades via ON DELETE CASCADE,
// so this targeted helper is currently exercised only by tests.
#[allow(dead_code)]
pub fn delete_swim_lengths(conn: &Connection, activity_id: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM swim_length WHERE activity_id = ?1",
        params![activity_id],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::models::activity::Activity;

    fn insert_test_activity(conn: &Connection, id: &str) {
        let a = Activity {
            id: id.to_string(),
            start_time: "2025-06-01T08:00:00+00:00".to_string(),
            timezone_offset: None,
            sport_type: "swim".to_string(),
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

    fn sample_swim_length(activity_id: &str, num: i32) -> SwimLength {
        SwimLength {
            id: None,
            activity_id: activity_id.to_string(),
            length_number: num,
            start_time: Some("2025-06-01T08:00:00Z".to_string()),
            total_elapsed_time_s: Some(45.0),
            total_timer_time_s: Some(42.0),
            avg_speed_mps: Some(1.2),
            avg_swimming_cadence: Some(30.0),
            swim_stroke: Some("freestyle".to_string()),
            total_strokes: Some(18),
            total_calories: Some(8.0),
            length_type: Some("active".to_string()),
        }
    }

    #[test]
    fn insert_and_get_swim_lengths() {
        let conn = db::test_db();
        insert_test_activity(&conn, "sl-act");

        let lengths = vec![
            sample_swim_length("sl-act", 1),
            sample_swim_length("sl-act", 2),
        ];
        insert_swim_lengths(&conn, &lengths).unwrap();

        let loaded = get_swim_lengths(&conn, "sl-act").unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].length_number, 1);
        assert_eq!(loaded[1].length_number, 2);
        assert_eq!(loaded[0].total_elapsed_time_s, Some(45.0));
        assert_eq!(loaded[0].avg_swimming_cadence, Some(30.0));
        assert_eq!(loaded[0].swim_stroke, Some("freestyle".to_string()));
        assert_eq!(loaded[0].total_strokes, Some(18));
        assert_eq!(loaded[0].total_calories, Some(8.0));
        assert_eq!(loaded[0].length_type, Some("active".to_string()));
        assert!(loaded[0].id.is_some());
    }

    #[test]
    fn get_swim_lengths_empty() {
        let conn = db::test_db();
        insert_test_activity(&conn, "sl-empty");

        let lengths = get_swim_lengths(&conn, "sl-empty").unwrap();
        assert!(lengths.is_empty());
    }

    #[test]
    fn delete_swim_lengths_removes_all() {
        let conn = db::test_db();
        insert_test_activity(&conn, "sl-del");

        let lengths = vec![sample_swim_length("sl-del", 1), sample_swim_length("sl-del", 2)];
        insert_swim_lengths(&conn, &lengths).unwrap();

        delete_swim_lengths(&conn, "sl-del").unwrap();
        let loaded = get_swim_lengths(&conn, "sl-del").unwrap();
        assert!(loaded.is_empty());
    }
}
