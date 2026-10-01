use rusqlite::{params, Connection, Result};

use crate::parser::SensorInfo;

/// The sensors an activity's file named, replacing what was stored: the
/// one-time backfill and a re-import both write the file's full list.
pub fn replace(conn: &Connection, activity_id: &str, sensors: &[SensorInfo]) -> Result<()> {
    conn.execute("DELETE FROM activity_sensor WHERE activity_id = ?1", params![activity_id])?;
    for s in sensors {
        conn.execute(
            "INSERT OR REPLACE INTO activity_sensor (activity_id, serial, device_type, manufacturer, product) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![activity_id, s.serial, s.device_type, s.manufacturer, s.product],
        )?;
    }
    Ok(())
}

/// The serials an activity's file named.
pub fn serials_of(conn: &Connection, activity_id: &str) -> Result<Vec<String>> {
    let mut stmt =
        conn.prepare("SELECT serial FROM activity_sensor WHERE activity_id = ?1 ORDER BY serial")?;
    let rows = stmt.query_map(params![activity_id], |r| r.get::<_, String>(0))?;
    rows.collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn sensor(serial: &str, kind: &str) -> SensorInfo {
        SensorInfo {
            serial: serial.into(),
            device_type: Some(kind.into()),
            manufacturer: Some("favero_electronics".into()),
            product: Some("assioma_duo".into()),
        }
    }

    #[test]
    fn replace_writes_the_file_s_list_and_goes_with_the_activity() {
        let conn = db::test_db();
        conn.execute("INSERT INTO activity (id, start_time, sport_type) VALUES ('a1', '2026-01-01T08:00:00+03:00', 'ride')", []).unwrap();
        replace(&conn, "a1", &[sensor("3632674300", "bike_power"), sensor("111", "heart_rate")]).unwrap();
        assert_eq!(serials_of(&conn, "a1").unwrap(), vec!["111", "3632674300"]);
        // A second write is the new truth, not an addition.
        replace(&conn, "a1", &[sensor("222", "bike_speed")]).unwrap();
        assert_eq!(serials_of(&conn, "a1").unwrap(), vec!["222"]);
        conn.execute("DELETE FROM activity WHERE id = 'a1'", []).unwrap();
        let left: i64 = conn.query_row("SELECT count(*) FROM activity_sensor", [], |r| r.get(0)).unwrap();
        assert_eq!(left, 0);
    }
}
