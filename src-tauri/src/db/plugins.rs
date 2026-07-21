use rusqlite::{params, Connection, OptionalExtension, Result};

use crate::models::plugin::Plugin;

fn row_to_plugin(row: &rusqlite::Row) -> Result<Plugin> {
    Ok(Plugin {
        id: row.get(0)?,
        name: row.get(1)?,
        version: row.get(2)?,
        author: row.get(3)?,
        description: row.get(4)?,
        enabled: row.get::<_, i64>(5)? != 0,
        manifest: row.get(6)?,
        source: row.get(7)?,
        installed_at: row.get(8)?,
        updated_at: row.get(9)?,
        signed: row.get::<_, i64>(10)? != 0,
    })
}

const PLUGIN_COLUMNS: &str =
    "id, name, version, author, description, enabled, manifest, source, installed_at, updated_at, signed";

/// Insert a new plugin or replace an existing one with the same id (reinstall /
/// upgrade). Preserves the `enabled` flag and `installed_at` of the prior row.
pub fn upsert_plugin(conn: &Connection, plugin: &Plugin) -> Result<()> {
    let existing = get_plugin(conn, &plugin.id)?;
    match existing {
        Some(prev) => {
            conn.execute(
                "UPDATE plugin SET name = ?2, version = ?3, author = ?4, description = ?5, \
                 manifest = ?6, source = ?7, signed = ?8, updated_at = datetime('now') WHERE id = ?1",
                params![
                    plugin.id,
                    plugin.name,
                    plugin.version,
                    plugin.author,
                    plugin.description,
                    plugin.manifest,
                    plugin.source,
                    plugin.signed as i64,
                ],
            )?;
            // keep prev.enabled / prev.installed_at untouched
            let _ = prev;
        }
        None => {
            conn.execute(
                "INSERT INTO plugin (id, name, version, author, description, enabled, manifest, source, signed) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    plugin.id,
                    plugin.name,
                    plugin.version,
                    plugin.author,
                    plugin.description,
                    plugin.enabled as i64,
                    plugin.manifest,
                    plugin.source,
                    plugin.signed as i64,
                ],
            )?;
        }
    }
    Ok(())
}

pub fn list_plugins(conn: &Connection) -> Result<Vec<Plugin>> {
    let mut stmt =
        conn.prepare(&format!("SELECT {PLUGIN_COLUMNS} FROM plugin ORDER BY name COLLATE NOCASE"))?;
    let rows = stmt.query_map([], row_to_plugin)?;
    let mut plugins = Vec::new();
    for row in rows {
        plugins.push(row?);
    }
    Ok(plugins)
}

pub fn get_plugin(conn: &Connection, id: &str) -> Result<Option<Plugin>> {
    conn.query_row(
        &format!("SELECT {PLUGIN_COLUMNS} FROM plugin WHERE id = ?1"),
        params![id],
        row_to_plugin,
    )
    .optional()
}

pub fn set_enabled(conn: &Connection, id: &str, enabled: bool) -> Result<()> {
    conn.execute(
        "UPDATE plugin SET enabled = ?2, updated_at = datetime('now') WHERE id = ?1",
        params![id, enabled as i64],
    )?;
    Ok(())
}

/// Remove a plugin. CASCADE drops its `plugin_data` and `plugin_kv` rows.
pub fn delete_plugin(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM plugin WHERE id = ?1", params![id])?;
    Ok(())
}

// --- plugin-scoped key/value store ---

pub fn kv_set(conn: &Connection, plugin_id: &str, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO plugin_kv (plugin_id, key, value) VALUES (?1, ?2, ?3)",
        params![plugin_id, key, value],
    )?;
    Ok(())
}

pub fn kv_get(conn: &Connection, plugin_id: &str, key: &str) -> Result<Option<String>> {
    conn.query_row(
        "SELECT value FROM plugin_kv WHERE plugin_id = ?1 AND key = ?2",
        params![plugin_id, key],
        |row| row.get::<_, String>(0),
    )
    .optional()
}

// --- plugin-owned structured data ---

/// Append a structured data record owned by a plugin. Returns the new row id.
pub fn insert_data(
    conn: &Connection,
    plugin_id: &str,
    kind: &str,
    activity_id: Option<&str>,
    key: Option<&str>,
    json: &str,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO plugin_data (plugin_id, kind, activity_id, key, json) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![plugin_id, kind, activity_id, key, json],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Fetch a plugin's data of a given kind, optionally scoped to one activity.
/// Returns the raw JSON payloads, newest first.
pub fn get_data(
    conn: &Connection,
    plugin_id: &str,
    kind: &str,
    activity_id: Option<&str>,
) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT json FROM plugin_data \
         WHERE plugin_id = ?1 AND kind = ?2 AND (?3 IS NULL OR activity_id = ?3) \
         ORDER BY id DESC",
    )?;
    let rows = stmt.query_map(params![plugin_id, kind, activity_id], |row| {
        row.get::<_, String>(0)
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn sample_plugin(id: &str) -> Plugin {
        Plugin {
            id: id.to_string(),
            name: "Sleep Analytics".to_string(),
            version: "1.0.0".to_string(),
            author: Some("Acme".to_string()),
            description: Some("Deep sleep analysis".to_string()),
            enabled: false,
            signed: false,
            manifest: r#"{"id":"x","name":"Sleep Analytics","version":"1.0.0"}"#.to_string(),
            source: "sideload".to_string(),
            installed_at: String::new(),
            updated_at: String::new(),
        }
    }

    #[test]
    fn install_list_enable_delete() {
        let conn = db::test_db();
        upsert_plugin(&conn, &sample_plugin("com.acme.sleep")).unwrap();

        let plugins = list_plugins(&conn).unwrap();
        assert_eq!(plugins.len(), 1);
        assert!(!plugins[0].enabled);

        set_enabled(&conn, "com.acme.sleep", true).unwrap();
        assert!(get_plugin(&conn, "com.acme.sleep").unwrap().unwrap().enabled);

        delete_plugin(&conn, "com.acme.sleep").unwrap();
        assert!(list_plugins(&conn).unwrap().is_empty());
    }

    #[test]
    fn upsert_preserves_enabled_state_on_reinstall() {
        let conn = db::test_db();
        upsert_plugin(&conn, &sample_plugin("com.acme.sleep")).unwrap();
        set_enabled(&conn, "com.acme.sleep", true).unwrap();

        let mut upgraded = sample_plugin("com.acme.sleep");
        upgraded.version = "2.0.0".to_string();
        upsert_plugin(&conn, &upgraded).unwrap();

        let p = get_plugin(&conn, "com.acme.sleep").unwrap().unwrap();
        assert_eq!(p.version, "2.0.0");
        assert!(p.enabled, "enabled flag should survive a reinstall");
    }

    #[test]
    fn kv_roundtrip() {
        let conn = db::test_db();
        upsert_plugin(&conn, &sample_plugin("com.acme.sleep")).unwrap();

        assert_eq!(kv_get(&conn, "com.acme.sleep", "cursor").unwrap(), None);
        kv_set(&conn, "com.acme.sleep", "cursor", "2026-01-01").unwrap();
        kv_set(&conn, "com.acme.sleep", "cursor", "2026-02-01").unwrap();
        assert_eq!(
            kv_get(&conn, "com.acme.sleep", "cursor").unwrap(),
            Some("2026-02-01".to_string())
        );
    }

    #[test]
    fn data_insert_and_query_by_kind() {
        let conn = db::test_db();
        upsert_plugin(&conn, &sample_plugin("com.acme.sleep")).unwrap();

        insert_data(&conn, "com.acme.sleep", "sleep", None, Some("2026-02-01"), r#"{"hours":7.5}"#)
            .unwrap();
        insert_data(&conn, "com.acme.sleep", "sleep", None, Some("2026-02-02"), r#"{"hours":8.0}"#)
            .unwrap();

        let rows = get_data(&conn, "com.acme.sleep", "sleep", None).unwrap();
        assert_eq!(rows.len(), 2);
        let other = get_data(&conn, "com.acme.sleep", "route", None).unwrap();
        assert!(other.is_empty());
    }

    #[test]
    fn delete_plugin_cascades_data_and_kv() {
        let conn = db::test_db();
        upsert_plugin(&conn, &sample_plugin("com.acme.sleep")).unwrap();
        kv_set(&conn, "com.acme.sleep", "k", "v").unwrap();
        insert_data(&conn, "com.acme.sleep", "sleep", None, None, "{}").unwrap();

        delete_plugin(&conn, "com.acme.sleep").unwrap();

        assert_eq!(kv_get(&conn, "com.acme.sleep", "k").unwrap(), None);
        assert!(get_data(&conn, "com.acme.sleep", "sleep", None).unwrap().is_empty());
    }
}
