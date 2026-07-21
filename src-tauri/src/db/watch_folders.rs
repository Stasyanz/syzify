use rusqlite::{params, Connection, Result};

#[derive(serde::Serialize, serde::Deserialize)]
pub struct WatchFolder {
    pub id: i64,
    pub path: String,
}

/// List all configured watch folders, ordered by id.
pub fn list(conn: &Connection) -> Result<Vec<WatchFolder>> {
    let mut stmt = conn.prepare("SELECT id, path FROM watch_folder ORDER BY id")?;
    let rows = stmt.query_map([], |row| {
        Ok(WatchFolder {
            id: row.get(0)?,
            path: row.get(1)?,
        })
    })?;
    let mut folders = Vec::new();
    for row in rows {
        folders.push(row?);
    }
    Ok(folders)
}

/// List just the configured watch folder paths.
pub fn list_paths(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT path FROM watch_folder ORDER BY id")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut paths = Vec::new();
    for row in rows {
        paths.push(row?);
    }
    Ok(paths)
}

/// Insert a watch folder (ignored if the path already exists) and return its row.
pub fn add(conn: &Connection, path: &str) -> Result<WatchFolder> {
    conn.execute(
        "INSERT OR IGNORE INTO watch_folder (path) VALUES (?1)",
        params![path],
    )?;
    // INSERT OR IGNORE may not insert; fetch the canonical row by unique path.
    let id = conn.query_row(
        "SELECT id FROM watch_folder WHERE path = ?1",
        params![path],
        |row| row.get(0),
    )?;
    Ok(WatchFolder {
        id,
        path: path.to_string(),
    })
}

/// Remove a watch folder by id.
pub fn remove(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM watch_folder WHERE id = ?1", params![id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[test]
    fn add_list_remove_roundtrip() {
        let conn = db::test_db();
        let wf = add(&conn, "/tmp/a").unwrap();
        assert_eq!(wf.path, "/tmp/a");

        let folders = list(&conn).unwrap();
        assert_eq!(folders.len(), 1);
        assert_eq!(folders[0].path, "/tmp/a");
        assert_eq!(list_paths(&conn).unwrap(), vec!["/tmp/a".to_string()]);

        remove(&conn, wf.id).unwrap();
        assert!(list(&conn).unwrap().is_empty());
    }

    #[test]
    fn add_duplicate_path_is_idempotent() {
        let conn = db::test_db();
        let first = add(&conn, "/tmp/dup").unwrap();
        let second = add(&conn, "/tmp/dup").unwrap();
        assert_eq!(first.id, second.id);
        assert_eq!(list(&conn).unwrap().len(), 1);
    }
}
