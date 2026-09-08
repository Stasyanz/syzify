//! Secret storage for plugins — `host_secret_set` / `host_secret_get`.
//!
//! The tokens a sync plugin holds. Kept apart from `plugin_kv`, behind
//! their own permission (`data:secret`, disclosed as "stores secrets"),
//! and sealed under the vault key whenever vault encryption is on — any
//! scope, not only `database`, so a backup of an `activities`-only vault
//! carries ciphertext too. Without encryption the value is stored in the
//! clear and the Plugins screen says so. Tauri-free, like the rest of the
//! host layer; the enable/disable flows in `commands/settings.rs` call
//! `encrypt_all` / `decrypt_all` to follow the key.

use rusqlite::Connection;

use crate::crypto;
use crate::db;

/// Longest secret name.
pub const SECRET_MAX_KEY: usize = 128;
/// Longest secret value (an OAuth token pair is under 2 KB).
pub const SECRET_MAX_VALUE: usize = 64 * 1024;
/// Secrets one plugin may hold: every row rides in each backup and is
/// re-sealed on every unlock, and a sync plugin needs a handful.
pub const SECRET_MAX_COUNT: usize = 32;

/// Store `value` for the plugin, sealed when `key` is held. An empty value
/// deletes the secret — a sign-out is one call per token.
pub fn set(
    conn: &Connection,
    key: Option<&[u8; 32]>,
    plugin_id: &str,
    name: &str,
    value: &str,
) -> Result<(), String> {
    validate_name(name)?;
    if value.is_empty() {
        return db::plugins::secret_delete(conn, plugin_id, name).map_err(|e| e.to_string());
    }
    if value.len() > SECRET_MAX_VALUE {
        return Err(format!(
            "secret {name:?} is {} bytes — larger than the {} KiB limit",
            value.len(),
            SECRET_MAX_VALUE / 1024
        ));
    }
    let is_new = db::plugins::secret_get(conn, plugin_id, name).map_err(|e| e.to_string())?.is_none();
    if is_new && db::plugins::secret_count(conn, plugin_id).map_err(|e| e.to_string())? >= SECRET_MAX_COUNT {
        return Err(format!("the plugin already holds {SECRET_MAX_COUNT} secrets — delete one first"));
    }
    let (blob, encrypted) = match key {
        Some(key) => (crypto::seal(key, &aad(plugin_id, name), value.as_bytes())?, true),
        None => (value.as_bytes().to_vec(), false),
    };
    db::plugins::secret_set(conn, plugin_id, name, &blob, encrypted).map_err(|e| e.to_string())
}

/// The stored value, or `None`. A sealed value needs the key: without it
/// (the vault locked, or its encryption just turned off and the rows not
/// yet unsealed) the read fails rather than returning ciphertext.
pub fn get(
    conn: &Connection,
    key: Option<&[u8; 32]>,
    plugin_id: &str,
    name: &str,
) -> Result<Option<String>, String> {
    validate_name(name)?;
    let Some((blob, encrypted)) = db::plugins::secret_get(conn, plugin_id, name).map_err(|e| e.to_string())? else {
        return Ok(None);
    };
    let bytes = if encrypted {
        let key = key.ok_or_else(|| format!("secret {name:?} is sealed and the vault key is not held"))?;
        crypto::open(key, &aad(plugin_id, name), &blob)?
    } else {
        blob
    };
    String::from_utf8(bytes).map(Some).map_err(|_| format!("secret {name:?} is not valid text"))
}

/// Seal every plaintext secret under `key` — when encryption is turned on,
/// and on every unlock (a row a crash left in the clear). One transaction:
/// all rows or none. Returns how many.
pub fn encrypt_all(conn: &Connection, key: &[u8; 32]) -> Result<usize, String> {
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    let rows = db::plugins::secrets_where(&tx, false).map_err(|e| e.to_string())?;
    for (plugin_id, name, value) in &rows {
        let blob = crypto::seal(key, &aad(plugin_id, name), value)?;
        db::plugins::secret_set(&tx, plugin_id, name, &blob, true).map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(rows.len())
}

/// Unseal every secret — when encryption is turned off, while the key is
/// still held. One transaction: a value that does not open under `key`
/// (named, so the user knows which plugin to sign out of) rolls the whole
/// pass back; nothing is lost, the caller keeps the lock and the key.
pub fn decrypt_all(conn: &Connection, key: &[u8; 32]) -> Result<usize, String> {
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    let rows = db::plugins::secrets_where(&tx, true).map_err(|e| e.to_string())?;
    for (plugin_id, name, blob) in &rows {
        let value = crypto::open(key, &aad(plugin_id, name), blob)
            .map_err(|e| format!("secret {name:?} of plugin {plugin_id}: {e}"))?;
        db::plugins::secret_set(&tx, plugin_id, name, &value, false).map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(rows.len())
}

fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name.len() > SECRET_MAX_KEY || name.chars().any(|c| c.is_control()) {
        return Err(format!("invalid secret name {name:?}: 1 to {SECRET_MAX_KEY} characters, no control characters"));
    }
    Ok(())
}

/// The row's identity, bound into the ciphertext: a sealed value moved
/// to another plugin's row, or another name, does not open.
fn aad(plugin_id: &str, name: &str) -> Vec<u8> {
    format!("syzify-plugin-secret-v1\0{plugin_id}\0{name}").into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::plugin::Plugin;

    fn db_with_plugin(id: &str) -> Connection {
        let conn = crate::db::test_db();
        db::plugins::upsert_plugin(
            &conn,
            &Plugin {
                id: id.to_string(),
                name: "P".to_string(),
                version: "0.1.0".to_string(),
                author: None,
                description: None,
                enabled: true,
                signed: false,
                manifest: "{}".to_string(),
                source: format!("plugins/{id}"),
                installed_at: String::new(),
                updated_at: String::new(),
            },
        )
        .unwrap();
        conn
    }

    fn raw(conn: &Connection, plugin_id: &str, name: &str) -> (Vec<u8>, bool) {
        db::plugins::secret_get(conn, plugin_id, name).unwrap().unwrap()
    }

    #[test]
    fn plain_without_a_key_sealed_with_one_and_an_empty_value_deletes() {
        let conn = db_with_plugin("com.a");
        let key = [7u8; 32];
        set(&conn, None, "com.a", "token", "plain-token").unwrap();
        assert_eq!(raw(&conn, "com.a", "token"), (b"plain-token".to_vec(), false));
        assert_eq!(get(&conn, None, "com.a", "token").unwrap().as_deref(), Some("plain-token"));
        // A plain row reads with or without a key.
        assert_eq!(get(&conn, Some(&key), "com.a", "token").unwrap().as_deref(), Some("plain-token"));

        set(&conn, Some(&key), "com.a", "token", "sealed-token").unwrap();
        let (blob, encrypted) = raw(&conn, "com.a", "token");
        assert!(encrypted);
        assert!(!blob.windows(6).any(|w| w == b"sealed"), "no plaintext in the row");
        assert_eq!(get(&conn, Some(&key), "com.a", "token").unwrap().as_deref(), Some("sealed-token"));
        let err = get(&conn, None, "com.a", "token").unwrap_err();
        assert!(err.contains("key is not held"), "{err}");
        assert!(get(&conn, Some(&[8u8; 32]), "com.a", "token").is_err(), "another key does not open it");

        set(&conn, Some(&key), "com.a", "token", "").unwrap();
        assert_eq!(get(&conn, Some(&key), "com.a", "token").unwrap(), None);
        assert_eq!(get(&conn, None, "com.a", "missing").unwrap(), None);
        // Deleting what is not there is fine.
        set(&conn, None, "com.a", "missing", "").unwrap();
    }

    #[test]
    fn a_sealed_value_is_bound_to_its_plugin_and_name() {
        let conn = db_with_plugin("com.a");
        db::plugins::upsert_plugin(&conn, &{
            let mut p = Plugin {
                id: "com.b".to_string(),
                name: "P".to_string(),
                version: "0.1.0".to_string(),
                author: None,
                description: None,
                enabled: true,
                signed: false,
                manifest: "{}".to_string(),
                source: "plugins/com.b".to_string(),
                installed_at: String::new(),
                updated_at: String::new(),
            };
            p.name.push('b');
            p
        })
        .unwrap();
        let key = [7u8; 32];
        set(&conn, Some(&key), "com.a", "token", "secret").unwrap();
        let (blob, _) = raw(&conn, "com.a", "token");
        // The same ciphertext copied to another plugin, or another name.
        db::plugins::secret_set(&conn, "com.b", "token", &blob, true).unwrap();
        db::plugins::secret_set(&conn, "com.a", "other", &blob, true).unwrap();
        assert!(get(&conn, Some(&key), "com.b", "token").is_err());
        assert!(get(&conn, Some(&key), "com.a", "other").is_err());
        assert_eq!(get(&conn, Some(&key), "com.a", "token").unwrap().as_deref(), Some("secret"));
    }

    #[test]
    fn names_and_sizes_are_bounded() {
        let conn = db_with_plugin("com.a");
        assert!(set(&conn, None, "com.a", "", "x").is_err());
        assert!(set(&conn, None, "com.a", &"k".repeat(SECRET_MAX_KEY + 1), "x").is_err());
        assert!(set(&conn, None, "com.a", "a\nb", "x").is_err());
        assert!(get(&conn, None, "com.a", "").is_err());
        let err = set(&conn, None, "com.a", "big", &"v".repeat(SECRET_MAX_VALUE + 1)).unwrap_err();
        assert!(err.contains("64 KiB"), "{err}");
        set(&conn, None, "com.a", &"k".repeat(SECRET_MAX_KEY), &"v".repeat(SECRET_MAX_VALUE)).unwrap();
        // Stored bytes that are not text do not read as a secret.
        db::plugins::secret_set(&conn, "com.a", "bin", &[0xff, 0xfe], false).unwrap();
        assert!(get(&conn, None, "com.a", "bin").unwrap_err().contains("not valid text"));
    }

    #[test]
    fn encrypt_all_and_decrypt_all_follow_the_key() {
        let conn = db_with_plugin("com.a");
        let key = [9u8; 32];
        set(&conn, None, "com.a", "t1", "one").unwrap();
        set(&conn, None, "com.a", "t2", "two").unwrap();
        set(&conn, Some(&key), "com.a", "t3", "three").unwrap();
        assert_eq!(encrypt_all(&conn, &key).unwrap(), 2, "only the plain rows");
        for (name, value) in [("t1", "one"), ("t2", "two"), ("t3", "three")] {
            assert!(raw(&conn, "com.a", name).1);
            assert_eq!(get(&conn, Some(&key), "com.a", name).unwrap().as_deref(), Some(value));
        }
        assert_eq!(encrypt_all(&conn, &key).unwrap(), 0, "idempotent");
        assert_eq!(decrypt_all(&conn, &key).unwrap(), 3);
        for (name, value) in [("t1", "one"), ("t2", "two"), ("t3", "three")] {
            assert_eq!(raw(&conn, "com.a", name), (value.as_bytes().to_vec(), false));
        }
        assert_eq!(decrypt_all(&conn, &key).unwrap(), 0);
        // A row sealed under another key — the LAST in pass order, so the
        // rows before it were already rewritten — rolls the pass back:
        // nothing changes, and the row is named.
        assert_eq!(encrypt_all(&conn, &key).unwrap(), 3);
        set(&conn, Some(&[1u8; 32]), "com.a", "t3", "foreign").unwrap();
        let err = decrypt_all(&conn, &key).unwrap_err();
        assert!(err.contains("\"t3\"") && err.contains("com.a"), "{err}");
        for name in ["t1", "t2", "t3"] {
            assert!(raw(&conn, "com.a", name).1, "{name} still sealed");
        }
    }

    #[test]
    fn a_plugin_holds_at_most_thirty_two_secrets() {
        let conn = db_with_plugin("com.a");
        for i in 0..SECRET_MAX_COUNT {
            set(&conn, None, "com.a", &format!("s{i}"), "v").unwrap();
        }
        let err = set(&conn, None, "com.a", "one-more", "v").unwrap_err();
        assert!(err.contains("32 secrets"), "{err}");
        // Overwriting and deleting are always fine; a delete makes room.
        set(&conn, None, "com.a", "s0", "v2").unwrap();
        set(&conn, None, "com.a", "s0", "").unwrap();
        set(&conn, None, "com.a", "one-more", "v").unwrap();
    }
}
