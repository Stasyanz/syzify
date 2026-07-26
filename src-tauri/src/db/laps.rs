use rusqlite::{params, Connection, Result};

use crate::models::lap::Lap;

pub fn insert_laps(conn: &Connection, laps: &[Lap]) -> Result<()> {
    let mut stmt = conn.prepare(
        "INSERT INTO lap (activity_id, lap_number, start_time,
         total_elapsed_time_s, total_timer_time_s, total_distance_m,
         avg_speed_mps, max_speed_mps, avg_hr, max_hr,
         avg_cadence, max_cadence, total_ascent_m, total_descent_m, total_calories,
         avg_power_w, max_power_w, normalized_power_w,
         avg_vertical_oscillation_mm, avg_stance_time_ms, avg_step_length_mm)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)",
    )?;

    for lap in laps {
        stmt.execute(params![
            lap.activity_id,
            lap.lap_number,
            lap.start_time,
            lap.total_elapsed_time_s,
            lap.total_timer_time_s,
            lap.total_distance_m,
            lap.avg_speed_mps,
            lap.max_speed_mps,
            lap.avg_hr,
            lap.max_hr,
            lap.avg_cadence,
            lap.max_cadence,
            lap.total_ascent_m,
            lap.total_descent_m,
            lap.total_calories,
            lap.avg_power_w,
            lap.max_power_w,
            lap.normalized_power_w,
            lap.avg_vertical_oscillation_mm,
            lap.avg_stance_time_ms,
            lap.avg_step_length_mm,
        ])?;
    }
    Ok(())
}

pub fn get_laps(conn: &Connection, activity_id: &str) -> Result<Vec<Lap>> {
    let mut stmt = conn.prepare(
        "SELECT id, activity_id, lap_number, start_time,
         total_elapsed_time_s, total_timer_time_s, total_distance_m,
         avg_speed_mps, max_speed_mps, avg_hr, max_hr,
         avg_cadence, max_cadence, total_ascent_m, total_descent_m, total_calories,
         avg_power_w, max_power_w, normalized_power_w,
         avg_vertical_oscillation_mm, avg_stance_time_ms, avg_step_length_mm
         FROM lap WHERE activity_id = ?1 ORDER BY lap_number ASC",
    )?;

    let rows = stmt.query_map(params![activity_id], |row| {
        Ok(Lap {
            id: row.get(0)?,
            activity_id: row.get(1)?,
            lap_number: row.get(2)?,
            start_time: row.get(3)?,
            total_elapsed_time_s: row.get(4)?,
            total_timer_time_s: row.get(5)?,
            total_distance_m: row.get(6)?,
            avg_speed_mps: row.get(7)?,
            max_speed_mps: row.get(8)?,
            avg_hr: row.get(9)?,
            max_hr: row.get(10)?,
            avg_cadence: row.get(11)?,
            max_cadence: row.get(12)?,
            total_ascent_m: row.get(13)?,
            total_descent_m: row.get(14)?,
            total_calories: row.get(15)?,
            avg_power_w: row.get(16)?,
            max_power_w: row.get(17)?,
            normalized_power_w: row.get(18)?,
            avg_vertical_oscillation_mm: row.get(19)?,
            avg_stance_time_ms: row.get(20)?,
            avg_step_length_mm: row.get(21)?,
        })
    })?;

    let mut laps = Vec::new();
    for row in rows {
        laps.push(row?);
    }
    Ok(laps)
}

// Symmetric per-table delete. Activity removal cascades via ON DELETE CASCADE,
// so this targeted helper is currently exercised only by tests.
#[allow(dead_code)]
pub fn delete_laps(conn: &Connection, activity_id: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM lap WHERE activity_id = ?1",
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

    fn sample_lap(activity_id: &str, num: i32) -> Lap {
        Lap {
            id: None,
            activity_id: activity_id.to_string(),
            lap_number: num,
            start_time: Some("2025-06-01T08:00:00Z".to_string()),
            total_elapsed_time_s: Some(600.0),
            total_timer_time_s: Some(590.0),
            total_distance_m: Some(2000.0),
            avg_speed_mps: Some(3.33),
            max_speed_mps: Some(4.0),
            avg_hr: Some(155.0),
            max_hr: Some(170.0),
            avg_cadence: Some(85.0),
            max_cadence: Some(90.0),
            total_ascent_m: Some(20.0),
            total_descent_m: Some(15.0),
            total_calories: Some(120.0),
            avg_power_w: Some(250.0),
            max_power_w: Some(400.0),
            normalized_power_w: Some(260.0),
            avg_vertical_oscillation_mm: Some(95.0),
            avg_stance_time_ms: Some(240.0),
            avg_step_length_mm: Some(1100.0),
        }
    }

    #[test]
    fn insert_and_get_laps() {
        let conn = db::test_db();
        insert_test_activity(&conn, "lap-act");

        let laps = vec![
            sample_lap("lap-act", 1),
            sample_lap("lap-act", 2),
        ];
        insert_laps(&conn, &laps).unwrap();

        let loaded = get_laps(&conn, "lap-act").unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].lap_number, 1);
        assert_eq!(loaded[1].lap_number, 2);
        assert_eq!(loaded[0].total_distance_m, Some(2000.0));
        assert_eq!(loaded[0].avg_hr, Some(155.0));
        assert_eq!(loaded[0].total_calories, Some(120.0));
        assert!(loaded[0].id.is_some());
    }

    #[test]
    fn get_laps_empty() {
        let conn = db::test_db();
        insert_test_activity(&conn, "lap-empty");

        let laps = get_laps(&conn, "lap-empty").unwrap();
        assert!(laps.is_empty());
    }

    #[test]
    fn delete_laps_removes_all() {
        let conn = db::test_db();
        insert_test_activity(&conn, "lap-del");

        let laps = vec![sample_lap("lap-del", 1), sample_lap("lap-del", 2)];
        insert_laps(&conn, &laps).unwrap();

        delete_laps(&conn, "lap-del").unwrap();
        let loaded = get_laps(&conn, "lap-del").unwrap();
        assert!(loaded.is_empty());
    }
}
