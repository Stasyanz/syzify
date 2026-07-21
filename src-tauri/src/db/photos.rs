use rusqlite::{params, Connection, OptionalExtension, Result};

use crate::models::photo::Photo;

pub fn insert_photo(conn: &Connection, p: &Photo) -> Result<()> {
    conn.execute(
        "INSERT INTO photo (id, activity_id, path_in_vault, thumbnail_path, original_path,
         mime_type, width, height, size_bytes, hash_sha256, taken_at, caption, sort_order)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            p.id,
            p.activity_id,
            p.path_in_vault,
            p.thumbnail_path,
            p.original_path,
            p.mime_type,
            p.width,
            p.height,
            p.size_bytes,
            p.hash_sha256,
            p.taken_at,
            p.caption,
            p.sort_order,
        ],
    )?;
    Ok(())
}

pub fn get_photos_for_activity(conn: &Connection, activity_id: &str) -> Result<Vec<Photo>> {
    let mut stmt = conn.prepare(
        "SELECT id, activity_id, path_in_vault, thumbnail_path, original_path,
         mime_type, width, height, size_bytes, hash_sha256, taken_at, caption, sort_order, created_at
         FROM photo WHERE activity_id = ?1 ORDER BY sort_order ASC, created_at ASC",
    )?;
    let rows = stmt.query_map(params![activity_id], row_to_photo)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

pub fn get_photo_by_id(conn: &Connection, id: &str) -> Result<Option<Photo>> {
    conn.query_row(
        "SELECT id, activity_id, path_in_vault, thumbnail_path, original_path,
         mime_type, width, height, size_bytes, hash_sha256, taken_at, caption, sort_order, created_at
         FROM photo WHERE id = ?1",
        params![id],
        row_to_photo,
    )
    .optional()
}

pub fn delete_photo(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM photo WHERE id = ?1", params![id])?;
    Ok(())
}

/// Every stored photo path (full images + thumbnails) — used to update rows
/// after encrypt/decrypt rewrites files under new names, and for crash-drift
/// reconciliation.
pub fn all_paths(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT path_in_vault FROM photo
         UNION ALL
         SELECT thumbnail_path FROM photo WHERE thumbnail_path IS NOT NULL",
    )?;
    let rows = stmt.query_map([], |row| row.get(0))?;
    rows.collect()
}

/// Repoint whichever photo path column (full or thumbnail) currently equals
/// `old_path`. Photos carry two paths per row, so both columns are checked.
pub fn update_path(conn: &Connection, old_path: &str, new_path: &str) -> Result<()> {
    conn.execute(
        "UPDATE photo SET path_in_vault = ?1 WHERE path_in_vault = ?2",
        params![new_path, old_path],
    )?;
    conn.execute(
        "UPDATE photo SET thumbnail_path = ?1 WHERE thumbnail_path = ?2",
        params![new_path, old_path],
    )?;
    Ok(())
}

pub fn update_caption(conn: &Connection, id: &str, caption: Option<&str>) -> Result<()> {
    conn.execute(
        "UPDATE photo SET caption = ?1 WHERE id = ?2",
        params![caption, id],
    )?;
    Ok(())
}

pub fn update_sort_order(conn: &Connection, id: &str, sort_order: i64) -> Result<()> {
    conn.execute(
        "UPDATE photo SET sort_order = ?1 WHERE id = ?2",
        params![sort_order, id],
    )?;
    Ok(())
}

pub fn hash_exists_for_activity(
    conn: &Connection,
    activity_id: &str,
    hash: &str,
) -> Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM photo WHERE activity_id = ?1 AND hash_sha256 = ?2",
        params![activity_id, hash],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

pub fn next_sort_order(conn: &Connection, activity_id: &str) -> Result<i64> {
    let max: Option<i64> = conn.query_row(
        "SELECT MAX(sort_order) FROM photo WHERE activity_id = ?1",
        params![activity_id],
        |row| row.get(0),
    )?;
    Ok(max.map(|m| m + 1).unwrap_or(0))
}

fn row_to_photo(row: &rusqlite::Row) -> Result<Photo> {
    Ok(Photo {
        id: row.get(0)?,
        activity_id: row.get(1)?,
        path_in_vault: row.get(2)?,
        thumbnail_path: row.get(3)?,
        original_path: row.get(4)?,
        mime_type: row.get(5)?,
        width: row.get(6)?,
        height: row.get(7)?,
        size_bytes: row.get(8)?,
        hash_sha256: row.get(9)?,
        taken_at: row.get(10)?,
        caption: row.get(11)?,
        sort_order: row.get(12)?,
        created_at: row.get(13)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::models::activity::Activity;

    fn make_activity(conn: &Connection, id: &str) {
        let a = Activity {
            id: id.to_string(),
            start_time: "2026-05-01T08:00:00+00:00".to_string(),
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

    fn sample_photo(id: &str, activity_id: &str, hash: &str) -> Photo {
        Photo {
            id: id.to_string(),
            activity_id: activity_id.to_string(),
            path_in_vault: format!("photos/{}/{}.jpg", activity_id, id),
            thumbnail_path: Some(format!("photos/{}/{}.thumb.jpg", activity_id, id)),
            original_path: Some("/tmp/photo.jpg".to_string()),
            mime_type: "image/jpeg".to_string(),
            width: Some(1920),
            height: Some(1080),
            size_bytes: 12345,
            hash_sha256: hash.to_string(),
            taken_at: None,
            caption: None,
            sort_order: 0,
            created_at: String::new(),
        }
    }

    #[test]
    fn insert_and_list_photos() {
        let conn = db::test_db();
        make_activity(&conn, "act-1");
        let p = sample_photo("p-1", "act-1", "hash-1");
        insert_photo(&conn, &p).unwrap();

        let photos = get_photos_for_activity(&conn, "act-1").unwrap();
        assert_eq!(photos.len(), 1);
        assert_eq!(photos[0].id, "p-1");
    }

    #[test]
    fn cascade_on_activity_delete() {
        let conn = db::test_db();
        make_activity(&conn, "act-cascade");
        insert_photo(&conn, &sample_photo("p-c", "act-cascade", "h")).unwrap();

        conn.execute("DELETE FROM activity WHERE id = ?1", params!["act-cascade"]).unwrap();

        let photos = get_photos_for_activity(&conn, "act-cascade").unwrap();
        assert!(photos.is_empty());
    }

    #[test]
    fn hash_dedup_within_activity() {
        let conn = db::test_db();
        make_activity(&conn, "act-h");
        insert_photo(&conn, &sample_photo("p-h1", "act-h", "dup")).unwrap();
        assert!(hash_exists_for_activity(&conn, "act-h", "dup").unwrap());
        assert!(!hash_exists_for_activity(&conn, "act-h", "other").unwrap());
    }

    #[test]
    fn caption_and_sort_updates() {
        let conn = db::test_db();
        make_activity(&conn, "act-u");
        insert_photo(&conn, &sample_photo("p-u", "act-u", "h")).unwrap();
        update_caption(&conn, "p-u", Some("hello")).unwrap();
        update_sort_order(&conn, "p-u", 5).unwrap();

        let photo = get_photo_by_id(&conn, "p-u").unwrap().unwrap();
        assert_eq!(photo.caption, Some("hello".to_string()));
        assert_eq!(photo.sort_order, 5);
    }

    #[test]
    fn next_sort_order_increments() {
        let conn = db::test_db();
        make_activity(&conn, "act-s");
        assert_eq!(next_sort_order(&conn, "act-s").unwrap(), 0);

        let mut p = sample_photo("p-s1", "act-s", "h1");
        p.sort_order = 0;
        insert_photo(&conn, &p).unwrap();
        assert_eq!(next_sort_order(&conn, "act-s").unwrap(), 1);

        let mut p2 = sample_photo("p-s2", "act-s", "h2");
        p2.sort_order = 1;
        insert_photo(&conn, &p2).unwrap();
        assert_eq!(next_sort_order(&conn, "act-s").unwrap(), 2);
    }
}
