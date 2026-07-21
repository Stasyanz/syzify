use rusqlite::{params, Connection, Result};

use crate::models::tag::Tag;

pub fn get_all_tags(conn: &Connection) -> Result<Vec<Tag>> {
    let mut stmt = conn.prepare("SELECT id, name FROM tag ORDER BY name")?;
    let rows = stmt.query_map([], |row| {
        Ok(Tag {
            id: row.get(0)?,
            name: row.get(1)?,
        })
    })?;

    let mut tags = Vec::new();
    for row in rows {
        tags.push(row?);
    }
    Ok(tags)
}

pub fn create_tag(conn: &Connection, name: &str) -> Result<Tag> {
    conn.execute("INSERT INTO tag (name) VALUES (?1)", params![name])?;
    let id = conn.last_insert_rowid();
    Ok(Tag {
        id,
        name: name.to_string(),
    })
}

pub fn get_tags_for_activity(conn: &Connection, activity_id: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT t.name FROM tag t
         JOIN activity_tag at ON t.id = at.tag_id
         WHERE at.activity_id = ?1
         ORDER BY t.name",
    )?;

    let rows = stmt.query_map(params![activity_id], |row| row.get::<_, String>(0))?;
    let mut tags = Vec::new();
    for row in rows {
        tags.push(row?);
    }
    Ok(tags)
}

pub fn set_activity_tags(conn: &Connection, activity_id: &str, tag_ids: &[i64]) -> Result<()> {
    conn.execute(
        "DELETE FROM activity_tag WHERE activity_id = ?1",
        params![activity_id],
    )?;

    let mut stmt = conn.prepare(
        "INSERT INTO activity_tag (activity_id, tag_id) VALUES (?1, ?2)",
    )?;
    for tag_id in tag_ids {
        stmt.execute(params![activity_id, tag_id])?;
    }
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
            title: None,
            notes: None,
            distance_m: None,
            duration_s: None,
            elev_gain_m: None,
            elev_loss_m: None,
            avg_speed_mps: None,
            max_speed_mps: None,
            avg_hr: None,
            max_hr: None,
            avg_cadence: None,
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
            created_at: String::new(),
            updated_at: String::new(),
            parent_id: None,
        };
        db::activities::insert_activity(conn, &a).unwrap();
    }

    #[test]
    fn create_and_list_tags() {
        let conn = db::test_db();
        let t1 = create_tag(&conn, "interval").unwrap();
        let t2 = create_tag(&conn, "easy").unwrap();

        assert!(t1.id > 0);
        assert_eq!(t1.name, "interval");

        let all = get_all_tags(&conn).unwrap();
        assert_eq!(all.len(), 2);
        // sorted by name
        assert_eq!(all[0].name, "easy");
        assert_eq!(all[1].name, "interval");
        let _ = t2;
    }

    #[test]
    fn assign_and_get_tags_for_activity() {
        let conn = db::test_db();
        insert_test_activity(&conn, "act-1");

        let t1 = create_tag(&conn, "long").unwrap();
        let t2 = create_tag(&conn, "tempo").unwrap();

        set_activity_tags(&conn, "act-1", &[t1.id, t2.id]).unwrap();

        let tags = get_tags_for_activity(&conn, "act-1").unwrap();
        assert_eq!(tags.len(), 2);
        assert!(tags.contains(&"long".to_string()));
        assert!(tags.contains(&"tempo".to_string()));
    }

    #[test]
    fn replace_tags_clears_old_ones() {
        let conn = db::test_db();
        insert_test_activity(&conn, "act-2");

        let t1 = create_tag(&conn, "a").unwrap();
        let t2 = create_tag(&conn, "b").unwrap();

        set_activity_tags(&conn, "act-2", &[t1.id, t2.id]).unwrap();
        set_activity_tags(&conn, "act-2", &[t2.id]).unwrap();

        let tags = get_tags_for_activity(&conn, "act-2").unwrap();
        assert_eq!(tags, vec!["b".to_string()]);
    }
}
