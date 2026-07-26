use rusqlite::{params, Connection, Result};

use crate::models::time_in_zone::TimeInZone;

pub fn insert_time_in_zones(conn: &Connection, zones: &[TimeInZone]) -> Result<()> {
    let mut stmt = conn.prepare(
        "INSERT INTO time_in_zone (activity_id, zone_type, zone_index, time_s, zone_high_boundary)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )?;

    for z in zones {
        stmt.execute(params![
            z.activity_id,
            z.zone_type,
            z.zone_index,
            z.time_s,
            z.zone_high_boundary
        ])?;
    }
    Ok(())
}

pub fn get_time_in_zones(conn: &Connection, activity_id: &str) -> Result<Vec<TimeInZone>> {
    let mut stmt = conn.prepare(
        "SELECT id, activity_id, zone_type, zone_index, time_s, zone_high_boundary
         FROM time_in_zone WHERE activity_id = ?1 ORDER BY zone_type, zone_index ASC",
    )?;

    let rows = stmt.query_map(params![activity_id], |row| {
        Ok(TimeInZone {
            id: row.get(0)?,
            activity_id: row.get(1)?,
            zone_type: row.get(2)?,
            zone_index: row.get(3)?,
            time_s: row.get(4)?,
            zone_high_boundary: row.get(5)?,
        })
    })?;

    let mut zones = Vec::new();
    for row in rows {
        zones.push(row?);
    }
    Ok(zones)
}

// Symmetric per-table delete. Activity removal cascades via ON DELETE CASCADE,
// so this targeted helper is currently exercised only by tests.
#[allow(dead_code)]
pub fn delete_time_in_zones(conn: &Connection, activity_id: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM time_in_zone WHERE activity_id = ?1",
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

    fn sample_time_in_zone(activity_id: &str, zone_type: &str, index: i32) -> TimeInZone {
        TimeInZone {
            id: None,
            activity_id: activity_id.to_string(),
            zone_type: zone_type.to_string(),
            zone_index: index,
            time_s: 120.0 + index as f64 * 60.0,
            zone_high_boundary: Some(100.0 + index as f64 * 20.0),
        }
    }

    #[test]
    fn insert_and_get_time_in_zones() {
        let conn = db::test_db();
        insert_test_activity(&conn, "tz-act");

        let zones = vec![
            sample_time_in_zone("tz-act", "hr", 0),
            sample_time_in_zone("tz-act", "hr", 1),
            sample_time_in_zone("tz-act", "hr", 2),
            sample_time_in_zone("tz-act", "power", 0),
            sample_time_in_zone("tz-act", "power", 1),
        ];
        insert_time_in_zones(&conn, &zones).unwrap();

        let loaded = get_time_in_zones(&conn, "tz-act").unwrap();
        assert_eq!(loaded.len(), 5);
        // Ordered by zone_type, zone_index: hr 0,1,2, power 0,1
        assert_eq!(loaded[0].zone_type, "hr");
        assert_eq!(loaded[0].zone_index, 0);
        assert_eq!(loaded[0].time_s, 120.0);
        assert_eq!(loaded[0].zone_high_boundary, Some(100.0));
        assert_eq!(loaded[2].zone_type, "hr");
        assert_eq!(loaded[2].zone_index, 2);
        assert_eq!(loaded[3].zone_type, "power");
        assert_eq!(loaded[3].zone_index, 0);
        assert!(loaded[0].id.is_some());
    }

    #[test]
    fn get_time_in_zones_empty() {
        let conn = db::test_db();
        insert_test_activity(&conn, "tz-empty");

        let zones = get_time_in_zones(&conn, "tz-empty").unwrap();
        assert!(zones.is_empty());
    }

    #[test]
    fn delete_time_in_zones_removes_all() {
        let conn = db::test_db();
        insert_test_activity(&conn, "tz-del");

        let zones = vec![
            sample_time_in_zone("tz-del", "hr", 0),
            sample_time_in_zone("tz-del", "power", 0),
        ];
        insert_time_in_zones(&conn, &zones).unwrap();

        delete_time_in_zones(&conn, "tz-del").unwrap();
        let loaded = get_time_in_zones(&conn, "tz-del").unwrap();
        assert!(loaded.is_empty());
    }

    #[test]
    fn insert_time_in_zone_without_boundary() {
        let conn = db::test_db();
        insert_test_activity(&conn, "tz-nobound");

        let zone = TimeInZone {
            id: None,
            activity_id: "tz-nobound".to_string(),
            zone_type: "hr".to_string(),
            zone_index: 0,
            time_s: 300.0,
            zone_high_boundary: None,
        };
        insert_time_in_zones(&conn, &[zone]).unwrap();

        let loaded = get_time_in_zones(&conn, "tz-nobound").unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].time_s, 300.0);
        assert_eq!(loaded[0].zone_high_boundary, None);
    }
}
