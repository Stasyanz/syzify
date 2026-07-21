use rusqlite::{params, Connection, OptionalExtension, Result};

/// Read a key-value setting. Returns `None` if the key is absent.
pub fn get_setting(conn: &Connection, key: &str) -> Result<Option<String>> {
    conn.query_row(
        "SELECT value FROM setting WHERE key = ?1",
        params![key],
        |row| row.get::<_, String>(0),
    )
    .optional()
}

/// Insert or replace a key-value setting.
pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO setting (key, value) VALUES (?1, ?2)",
        params![key, value],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[test]
    fn get_missing_returns_none() {
        let conn = db::test_db();
        assert_eq!(get_setting(&conn, "nope").unwrap(), None);
    }

    #[test]
    fn set_then_get_roundtrip() {
        let conn = db::test_db();
        set_setting(&conn, "units", "km").unwrap();
        assert_eq!(get_setting(&conn, "units").unwrap(), Some("km".to_string()));
    }

    #[test]
    fn set_overwrites_existing() {
        let conn = db::test_db();
        set_setting(&conn, "units", "km").unwrap();
        set_setting(&conn, "units", "mi").unwrap();
        assert_eq!(get_setting(&conn, "units").unwrap(), Some("mi".to_string()));
    }
}
