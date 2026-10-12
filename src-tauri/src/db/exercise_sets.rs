use rusqlite::{params, Connection, Result};

use crate::models::exercise_set::ExerciseSet;

pub fn insert_exercise_sets(conn: &Connection, sets: &[ExerciseSet]) -> Result<()> {
    let mut stmt = conn.prepare(
        "INSERT INTO exercise_set (activity_id, set_number, start_time,
         category, category_subtype, set_type, duration_s, repetitions,
         weight_kg, wkt_step_index)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
    )?;

    for set in sets {
        stmt.execute(params![
            set.activity_id,
            set.set_number,
            set.start_time,
            set.category,
            set.category_subtype,
            set.set_type,
            set.duration_s,
            set.repetitions,
            set.weight_kg,
            set.wkt_step_index,
        ])?;
    }
    Ok(())
}

pub fn get_exercise_sets(conn: &Connection, activity_id: &str) -> Result<Vec<ExerciseSet>> {
    let mut stmt = conn.prepare(
        "SELECT id, activity_id, set_number, start_time,
         category, category_subtype, set_type, duration_s,
         repetitions, weight_kg, wkt_step_index
         FROM exercise_set WHERE activity_id = ?1 ORDER BY set_number ASC",
    )?;

    let rows = stmt.query_map(params![activity_id], |row| {
        Ok(ExerciseSet {
            id: row.get(0)?,
            activity_id: row.get(1)?,
            set_number: row.get(2)?,
            start_time: row.get(3)?,
            category: row.get(4)?,
            category_subtype: row.get(5)?,
            set_type: row.get(6)?,
            duration_s: row.get(7)?,
            repetitions: row.get(8)?,
            weight_kg: row.get(9)?,
            wkt_step_index: row.get(10)?,
        })
    })?;

    let mut sets = Vec::new();
    for row in rows {
        sets.push(row?);
    }
    Ok(sets)
}

/// Activities with a set that says anything about its exercise — the ones a
/// change in how exercises are named can touch.
pub fn activities_naming_exercises(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT activity_id FROM exercise_set
         WHERE category IS NOT NULL OR category_subtype IS NOT NULL
         ORDER BY activity_id",
    )?;
    let rows = stmt.query_map([], |row| row.get(0))?;
    rows.collect()
}

/// Activities holding a set whose exercise is still the debug dump of a FIT
/// array (`[UInt16(14), UInt16(14), UInt16(65534)]`) — what the parser
/// stored before #213. A decoded name never starts with a bracket.
pub fn activities_with_undecoded_exercises(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT activity_id FROM exercise_set
         WHERE category LIKE '[%' OR category_subtype LIKE '[%'
         ORDER BY activity_id",
    )?;
    let rows = stmt.query_map([], |row| row.get(0))?;
    rows.collect()
}

/// Rename the exercise of one stored set. Everything else about the set —
/// its time, repetitions, weight — is left as imported.
pub fn set_exercise(
    conn: &Connection,
    activity_id: &str,
    set_number: i32,
    category: Option<&str>,
    category_subtype: Option<&str>,
) -> Result<()> {
    conn.execute(
        "UPDATE exercise_set SET category = ?3, category_subtype = ?4
         WHERE activity_id = ?1 AND set_number = ?2",
        params![activity_id, set_number, category, category_subtype],
    )?;
    Ok(())
}

// Symmetric per-table delete. Activity removal cascades via ON DELETE CASCADE,
// so this targeted helper is currently exercised only by tests.
#[allow(dead_code)]
pub fn delete_exercise_sets(conn: &Connection, activity_id: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM exercise_set WHERE activity_id = ?1",
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
            sport_type: "strength".to_string(),
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

    fn sample_exercise_set(activity_id: &str, num: i32) -> ExerciseSet {
        ExerciseSet {
            id: None,
            activity_id: activity_id.to_string(),
            set_number: num,
            start_time: Some("2025-06-01T08:00:00Z".to_string()),
            category: Some("bench_press".to_string()),
            category_subtype: Some("barbell".to_string()),
            set_type: Some("active".to_string()),
            duration_s: Some(30.0),
            repetitions: Some(10),
            weight_kg: Some(60.0),
            wkt_step_index: Some(0),
        }
    }

    #[test]
    fn insert_and_get_exercise_sets() {
        let conn = db::test_db();
        insert_test_activity(&conn, "es-act");

        let sets = vec![
            sample_exercise_set("es-act", 1),
            sample_exercise_set("es-act", 2),
        ];
        insert_exercise_sets(&conn, &sets).unwrap();

        let loaded = get_exercise_sets(&conn, "es-act").unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].set_number, 1);
        assert_eq!(loaded[1].set_number, 2);
        assert_eq!(loaded[0].category, Some("bench_press".to_string()));
        assert_eq!(loaded[0].category_subtype, Some("barbell".to_string()));
        assert_eq!(loaded[0].set_type, Some("active".to_string()));
        assert_eq!(loaded[0].duration_s, Some(30.0));
        assert_eq!(loaded[0].repetitions, Some(10));
        assert_eq!(loaded[0].weight_kg, Some(60.0));
        assert_eq!(loaded[0].wkt_step_index, Some(0));
        assert!(loaded[0].id.is_some());
    }

    #[test]
    fn get_exercise_sets_empty() {
        let conn = db::test_db();
        insert_test_activity(&conn, "es-empty");

        let sets = get_exercise_sets(&conn, "es-empty").unwrap();
        assert!(sets.is_empty());
    }

    /// #213: only activities with a set still holding the dump are listed,
    /// whichever of the two columns holds it; renaming a set touches that
    /// set alone and nothing but its exercise.
    #[test]
    fn undecoded_exercises_are_listed_and_renamed_per_set() {
        let conn = db::test_db();
        for id in ["es-dump", "es-sub-dump", "es-fine", "es-none"] {
            insert_test_activity(&conn, id);
        }
        let with = |id: &str, num: i32, category: Option<&str>, subtype: Option<&str>| ExerciseSet {
            category: category.map(str::to_string),
            category_subtype: subtype.map(str::to_string),
            ..sample_exercise_set(id, num)
        };
        let dump = "[UInt16(14), UInt16(14), UInt16(65534)]";
        insert_exercise_sets(
            &conn,
            &[
                with("es-dump", 1, Some(dump), None),
                with("es-dump", 2, Some(dump), None),
                with("es-sub-dump", 1, Some("curl"), Some("[UInt16(3), UInt16(65534)]")),
                with("es-fine", 1, Some("bench_press"), Some("barbell_bench_press")),
                with("es-none", 1, None, None),
            ],
        )
        .unwrap();

        assert_eq!(
            activities_with_undecoded_exercises(&conn).unwrap(),
            vec!["es-dump".to_string(), "es-sub-dump".to_string()]
        );
        // Every activity whose sets say anything about an exercise, in
        // whichever column and whatever form; not the one that says nothing.
        assert_eq!(
            activities_naming_exercises(&conn).unwrap(),
            vec!["es-dump".to_string(), "es-fine".to_string(), "es-sub-dump".to_string()]
        );

        set_exercise(&conn, "es-dump", 1, Some("lateral_raise"), Some("seated_lateral_raise")).unwrap();
        let sets = get_exercise_sets(&conn, "es-dump").unwrap();
        assert_eq!(sets[0].category.as_deref(), Some("lateral_raise"));
        assert_eq!(sets[0].category_subtype.as_deref(), Some("seated_lateral_raise"));
        assert_eq!(sets[0].repetitions, Some(10));
        assert_eq!(sets[0].weight_kg, Some(60.0));
        assert_eq!(sets[1].category.as_deref(), Some(dump), "the other set is not touched");
        // Still listed while one set holds the dump; gone once none does.
        assert!(activities_with_undecoded_exercises(&conn).unwrap().contains(&"es-dump".to_string()));
        set_exercise(&conn, "es-dump", 2, None, None).unwrap();
        assert_eq!(
            activities_with_undecoded_exercises(&conn).unwrap(),
            vec!["es-sub-dump".to_string()]
        );
        // A set number the activity does not have is a no-op, not an error.
        set_exercise(&conn, "es-fine", 9, Some("row"), None).unwrap();
        assert_eq!(get_exercise_sets(&conn, "es-fine").unwrap()[0].category.as_deref(), Some("bench_press"));
    }

    #[test]
    fn delete_exercise_sets_removes_all() {
        let conn = db::test_db();
        insert_test_activity(&conn, "es-del");

        let sets = vec![sample_exercise_set("es-del", 1), sample_exercise_set("es-del", 2)];
        insert_exercise_sets(&conn, &sets).unwrap();

        delete_exercise_sets(&conn, "es-del").unwrap();
        let loaded = get_exercise_sets(&conn, "es-del").unwrap();
        assert!(loaded.is_empty());
    }
}
