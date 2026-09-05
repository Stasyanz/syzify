use rusqlite::{params, Connection, Result};

use crate::models::raw_file::{RawFile, RawFileKind};

pub fn insert_raw_file(conn: &Connection, raw: &RawFile) -> Result<()> {
    insert_raw_file_of_kind(conn, raw, RawFileKind::Activity)
}

/// Monitor files share the table (hash dedup, encryption, backups) with
/// `activity_id` NULL and `kind = 'monitoring'`.
pub fn insert_raw_file_of_kind(conn: &Connection, raw: &RawFile, kind: RawFileKind) -> Result<()> {
    conn.execute(
        "INSERT INTO raw_file (id, activity_id, path_in_vault, original_path, format,
         hash_sha256, parse_status, failure_reason, kind)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            raw.id,
            raw.activity_id,
            raw.path_in_vault,
            raw.original_path,
            raw.format,
            raw.hash_sha256,
            raw.parse_status,
            raw.failure_reason,
            kind.as_str(),
        ],
    )?;
    Ok(())
}

pub fn hash_exists(conn: &Connection, hash: &str) -> Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM raw_file WHERE hash_sha256 = ?1",
        params![hash],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

/// Every stored raw-file path — input for encryption crash-drift repair.
pub fn all_paths(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT path_in_vault FROM raw_file")?;
    let rows = stmt.query_map([], |row| row.get(0))?;
    rows.collect()
}

/// Update a raw file's stored path (used when encrypting/decrypting the vault
/// rewrites files under new names).
pub fn update_path(conn: &Connection, old_path: &str, new_path: &str) -> Result<()> {
    conn.execute(
        "UPDATE raw_file SET path_in_vault = ?1 WHERE path_in_vault = ?2",
        params![new_path, old_path],
    )?;
    Ok(())
}

/// Remove one raw_file row — the import path's compensation when a later
/// step fails after the row was written (a stranded row would keep its hash
/// blocking a re-import forever). The monitoring tables reference it with
/// ON DELETE SET NULL / CASCADE, so nothing else needs touching.
pub fn delete_by_id(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM raw_file WHERE id = ?1", params![id])?;
    Ok(())
}

/// Delete the raw_file rows tied to an activity. Called by delete_activity
/// BEFORE the activity row goes away: the FK is ON DELETE SET NULL, which
/// would orphan the rows — their hash stays in the dedup index forever, so
/// the same file could never be imported again (silently Skipped).
pub fn delete_for_activity(conn: &Connection, activity_id: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM raw_file WHERE activity_id = ?1",
        params![activity_id],
    )?;
    Ok(())
}

/// Read helper for an activity's raw source files (delete_activity uses it to
/// remove the files from the vault alongside the rows).
pub fn get_raw_files_for_activity(conn: &Connection, activity_id: &str) -> Result<Vec<RawFile>> {
    let mut stmt = conn.prepare(
        "SELECT id, activity_id, path_in_vault, original_path, format,
         hash_sha256, imported_at, parse_status, failure_reason
         FROM raw_file WHERE activity_id = ?1",
    )?;

    let rows = stmt.query_map(params![activity_id], |row| {
        Ok(RawFile {
            id: row.get(0)?,
            activity_id: row.get(1)?,
            path_in_vault: row.get(2)?,
            original_path: row.get(3)?,
            format: row.get(4)?,
            hash_sha256: row.get(5)?,
            imported_at: row.get(6)?,
            parse_status: row.get(7)?,
            failure_reason: row.get(8)?,
        })
    })?;

    let mut files = Vec::new();
    for row in rows {
        files.push(row?);
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn sample_raw(id: &str, hash: &str) -> RawFile {
        RawFile {
            id: id.to_string(),
            activity_id: None,
            path_in_vault: format!("raw/{}.fit", id),
            original_path: Some("/tmp/test.fit".to_string()),
            format: "fit".to_string(),
            hash_sha256: hash.to_string(),
            imported_at: String::new(),
            parse_status: "ok".to_string(),
            failure_reason: None,
        }
    }

    #[test]
    fn insert_and_check_hash() {
        let conn = db::test_db();
        let raw = sample_raw("rf-1", "abc123");
        insert_raw_file(&conn, &raw).unwrap();

        assert!(hash_exists(&conn, "abc123").unwrap());
        assert!(!hash_exists(&conn, "different").unwrap());
    }

    #[test]
    fn get_raw_files_for_activity_links() {
        let conn = db::test_db();
        // Need an activity first
        let a = crate::models::activity::Activity {
            id: "act-rf".to_string(),
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
        db::activities::insert_activity(&conn, &a).unwrap();

        let mut raw = sample_raw("rf-2", "xyz789");
        raw.activity_id = Some("act-rf".to_string());
        insert_raw_file(&conn, &raw).unwrap();

        let files = get_raw_files_for_activity(&conn, "act-rf").unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].hash_sha256, "xyz789");
    }

    /// delete_for_activity is scoped: other activities' rows AND unlinked
    /// (activity_id = NULL) rows survive. NULL rows are legitimate — the
    /// import pipeline stores content-duplicate files without a link — so
    /// they must keep guarding the dedup index.
    #[test]
    fn delete_for_activity_leaves_other_rows_alone() {
        let conn = db::test_db();
        for (act, id, hash) in [
            (Some("act-a"), "rf-a", "hash-a"),
            (Some("act-b"), "rf-b", "hash-b"),
            (None, "rf-dup", "hash-dup"),
        ] {
            conn.execute(
                "INSERT INTO activity (id, start_time) VALUES (?1, '2026-01-01T10:00:00+00:00')
                 ON CONFLICT DO NOTHING",
                params![act.unwrap_or("unused")],
            )
            .unwrap();
            let mut raw = sample_raw(id, hash);
            raw.activity_id = act.map(str::to_string);
            insert_raw_file(&conn, &raw).unwrap();
        }

        delete_for_activity(&conn, "act-a").unwrap();

        assert!(!hash_exists(&conn, "hash-a").unwrap(), "deleted activity's hash freed");
        assert!(hash_exists(&conn, "hash-b").unwrap(), "other activity untouched");
        assert!(
            hash_exists(&conn, "hash-dup").unwrap(),
            "unlinked content-duplicate rows must keep guarding the dedup index"
        );
    }

    #[test]
    fn update_path_rewrites_stored_path() {
        let conn = db::test_db();
        let raw = sample_raw("rf-3", "hash-3");
        insert_raw_file(&conn, &raw).unwrap();

        update_path(&conn, &raw.path_in_vault, "raw/rf-3.fit.enc").unwrap();

        let stored: String = conn
            .query_row(
                "SELECT path_in_vault FROM raw_file WHERE id = ?1",
                params!["rf-3"],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stored, "raw/rf-3.fit.enc");
    }
}
