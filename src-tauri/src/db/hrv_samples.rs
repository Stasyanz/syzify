use rusqlite::{params, Connection, Result};

use crate::models::hrv_sample::HrvSample;

pub fn insert_hrv_samples(conn: &Connection, samples: &[HrvSample]) -> Result<()> {
    let mut stmt = conn.prepare(
        "INSERT INTO hrv_sample (activity_id, sample_index, rr_interval_ms)
         VALUES (?1, ?2, ?3)",
    )?;
    for s in samples {
        stmt.execute(params![s.activity_id, s.sample_index, s.rr_interval_ms])?;
    }
    Ok(())
}

pub fn get_hrv_samples(conn: &Connection, activity_id: &str) -> Result<Vec<HrvSample>> {
    let mut stmt = conn.prepare(
        "SELECT id, activity_id, sample_index, rr_interval_ms
         FROM hrv_sample WHERE activity_id = ?1 ORDER BY sample_index ASC",
    )?;

    let rows = stmt.query_map(params![activity_id], |row| {
        Ok(HrvSample {
            id: row.get(0)?,
            activity_id: row.get(1)?,
            sample_index: row.get(2)?,
            rr_interval_ms: row.get(3)?,
        })
    })?;

    let mut samples = Vec::new();
    for row in rows {
        samples.push(row?);
    }
    Ok(samples)
}

// Symmetric per-table delete. Activity removal cascades via ON DELETE CASCADE,
// so this targeted helper is currently exercised only by tests.
#[allow(dead_code)]
pub fn delete_hrv_samples(conn: &Connection, activity_id: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM hrv_sample WHERE activity_id = ?1",
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
            created_at: String::new(), updated_at: String::new(), parent_id: None,
        };
        db::activities::insert_activity(conn, &a).unwrap();
    }

    fn sample_hrv(activity_id: &str, index: i32, rr_ms: f64) -> HrvSample {
        HrvSample {
            id: None,
            activity_id: activity_id.to_string(),
            sample_index: index,
            rr_interval_ms: rr_ms,
        }
    }

    #[test]
    fn insert_and_get_hrv_samples() {
        let conn = db::test_db();
        insert_test_activity(&conn, "hrv-act");

        let samples = vec![
            sample_hrv("hrv-act", 0, 832.0),
            sample_hrv("hrv-act", 1, 845.5),
            sample_hrv("hrv-act", 2, 810.0),
        ];
        insert_hrv_samples(&conn, &samples).unwrap();

        let loaded = get_hrv_samples(&conn, "hrv-act").unwrap();
        assert_eq!(loaded.len(), 3);
        assert_eq!(loaded[0].sample_index, 0);
        assert_eq!(loaded[0].rr_interval_ms, 832.0);
        assert_eq!(loaded[1].sample_index, 1);
        assert_eq!(loaded[1].rr_interval_ms, 845.5);
        assert_eq!(loaded[2].sample_index, 2);
        assert_eq!(loaded[2].rr_interval_ms, 810.0);
        assert!(loaded[0].id.is_some());
    }

    #[test]
    fn get_hrv_samples_empty() {
        let conn = db::test_db();
        insert_test_activity(&conn, "hrv-empty");

        let samples = get_hrv_samples(&conn, "hrv-empty").unwrap();
        assert!(samples.is_empty());
    }

    #[test]
    fn delete_hrv_samples_removes_all() {
        let conn = db::test_db();
        insert_test_activity(&conn, "hrv-del");

        let samples = vec![
            sample_hrv("hrv-del", 0, 800.0),
            sample_hrv("hrv-del", 1, 810.0),
        ];
        insert_hrv_samples(&conn, &samples).unwrap();

        delete_hrv_samples(&conn, "hrv-del").unwrap();
        let loaded = get_hrv_samples(&conn, "hrv-del").unwrap();
        assert!(loaded.is_empty());
    }
}
