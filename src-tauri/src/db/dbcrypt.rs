//! SQLCipher plumbing for the `database` encryption scope.
//!
//! The 32-byte vault key is passed to SQLCipher as a RAW key (`PRAGMA key =
//! "x'<hex>'"`), which skips SQLCipher's own passphrase KDF — we already ran
//! PBKDF2 to derive it. Encrypting/decrypting an existing database uses the
//! `sqlcipher_export` recipe: attach a fresh DB under the target keying and
//! copy the schema+data across, then swap files.

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::Connection;

/// `PRAGMA key` a connection with the raw 32-byte vault key.
pub fn apply_key(conn: &Connection, key: &[u8; 32]) -> Result<(), String> {
    conn.pragma_update(None, "key", format!("x'{}'", hex::encode(key)))
        .map_err(|e| format!("Failed to apply database key: {}", e))
}

/// Whether the connection can actually read the database — the cheap way to
/// tell a correct SQLCipher key from a wrong one (a wrong key errors here).
pub fn is_readable(conn: &Connection) -> bool {
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0))
        .is_ok()
}

fn tmp_path(db_path: &Path) -> PathBuf {
    db_path.with_extension("db.migrating")
}

/// Delete a stranded `vault.db.migrating` sidecar and report whether one was
/// removed. A crash between writing the sidecar and the swap leaves it behind;
/// after a crashed disable it holds a FULL PLAINTEXT dump of the still-encrypted
/// database, and nothing else deletes it unless the user retries the toggle —
/// so call this at every boot, before anything opens the database.
pub fn remove_stale_migrating(db_path: &Path) -> bool {
    fs::remove_file(tmp_path(db_path)).is_ok()
}

/// Encrypt the plaintext database at `db_path` in place with `key`.
/// Caller must guarantee no other connection is open to it.
pub fn encrypt_database(db_path: &Path, key: &[u8; 32]) -> Result<(), String> {
    let tmp = tmp_path(db_path);
    let _ = fs::remove_file(&tmp);

    let conn = Connection::open(db_path)
        .map_err(|e| format!("Failed to open database: {}", e))?;
    // Flush WAL so the whole DB is in the main file before exporting.
    let _ = conn.pragma_update(None, "wal_checkpoint", "TRUNCATE");
    let version = user_version(&conn)?;
    conn.execute_batch(&format!(
        "ATTACH DATABASE '{}' AS encrypted KEY \"x'{}'\";
         SELECT sqlcipher_export('encrypted');
         PRAGMA encrypted.user_version = {version};
         DETACH DATABASE encrypted;",
        tmp.to_string_lossy().replace('\'', "''"),
        hex::encode(key),
    ))
    .map_err(|e| format!("Failed to encrypt database: {}", e))?;
    drop(conn);

    swap(&tmp, db_path)
}

/// Read `PRAGMA user_version`. sqlcipher_export copies schema+data but NOT the
/// version stamp rusqlite_migration relies on, so it must be carried over
/// explicitly — otherwise the migrated schema looks unmigrated (version 0).
fn user_version(conn: &Connection) -> Result<i64, String> {
    conn.query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(|e| format!("Failed to read user_version: {}", e))
}

/// Decrypt the SQLCipher database at `db_path` in place. `key` must be the
/// current key. Caller must guarantee no other connection is open to it.
pub fn decrypt_database(db_path: &Path, key: &[u8; 32]) -> Result<(), String> {
    let tmp = tmp_path(db_path);
    let _ = fs::remove_file(&tmp);

    let conn = open_with_key(db_path, key)?;
    let _ = conn.pragma_update(None, "wal_checkpoint", "TRUNCATE");
    let version = user_version(&conn)?;
    // KEY '' produces a plaintext database.
    conn.execute_batch(&format!(
        "ATTACH DATABASE '{}' AS plaintext KEY '';
         SELECT sqlcipher_export('plaintext');
         PRAGMA plaintext.user_version = {version};
         DETACH DATABASE plaintext;",
        tmp.to_string_lossy().replace('\'', "''"),
    ))
    .map_err(|e| format!("Failed to decrypt database: {}", e))?;
    drop(conn);

    swap(&tmp, db_path)
}

/// Open a SQLCipher database with `key` and confirm the key is correct.
pub fn open_with_key(db_path: &Path, key: &[u8; 32]) -> Result<Connection, String> {
    let conn = Connection::open(db_path)
        .map_err(|e| format!("Failed to open database: {}", e))?;
    apply_key(&conn, key)?;
    if !is_readable(&conn) {
        return Err("Wrong password".to_string());
    }
    Ok(conn)
}

/// Replace `dest` with `tmp` (with a best-effort rollback marker), removing the
/// stale WAL/SHM sidecars so the swapped file opens cleanly.
fn swap(tmp: &Path, dest: &Path) -> Result<(), String> {
    for suffix in ["-wal", "-shm"] {
        let side = PathBuf::from(format!("{}{}", dest.to_string_lossy(), suffix));
        let _ = fs::remove_file(&side);
    }
    fs::rename(tmp, dest).map_err(|e| format!("Failed to swap database file: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("syz_dbcrypt_{}", name));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn make_plain_db(path: &Path) {
        let conn = Connection::open(path).unwrap();
        conn.execute_batch("CREATE TABLE t(x TEXT); INSERT INTO t VALUES('gps-track');")
            .unwrap();
        // Stamp a version like a migrated DB would carry.
        conn.pragma_update(None, "user_version", 22).unwrap();
    }

    #[test]
    fn encrypt_then_open_with_key_roundtrips() {
        let dir = scratch("enc");
        let db = dir.join("vault.db");
        make_plain_db(&db);
        let key = [7u8; 32];

        encrypt_database(&db, &key).unwrap();

        // On-disk bytes no longer contain the plaintext row or the SQLite header.
        let bytes = fs::read(&db).unwrap();
        assert!(!bytes.windows(9).any(|w| w == b"gps-track"));
        assert_ne!(&bytes[..16], b"SQLite format 3\0");

        // Right key reads it; wrong key fails.
        let conn = open_with_key(&db, &key).unwrap();
        let v: String = conn.query_row("SELECT x FROM t", [], |r| r.get(0)).unwrap();
        assert_eq!(v, "gps-track");
        // user_version must survive the export (else migrations re-run).
        let ver: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(ver, 22);
        drop(conn);
        assert!(open_with_key(&db, &[8u8; 32]).is_err());

        let _ = fs::remove_dir_all(&dir);
    }

    /// A crash between writing the tmp dump and the swap strands
    /// `vault.db.migrating`; after a crashed disable it is a full plaintext
    /// dump sitting next to the still-encrypted DB. Boot cleanup removes it
    /// without touching the database itself.
    #[test]
    fn remove_stale_migrating_deletes_orphaned_dump() {
        let dir = scratch("stale");
        let db = dir.join("vault.db");
        make_plain_db(&db);
        let key = [3u8; 32];
        encrypt_database(&db, &key).unwrap();

        // Simulate the crashed disable: plaintext dump stranded next to the DB.
        let tmp = tmp_path(&db);
        fs::write(&tmp, b"SQLite format 3\0plaintext-dump").unwrap();

        assert!(remove_stale_migrating(&db));
        assert!(!tmp.exists());
        // Idempotent: nothing left to remove on a clean boot.
        assert!(!remove_stale_migrating(&db));
        // The encrypted database is untouched and still opens with the key.
        assert!(open_with_key(&db, &key).is_ok());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn decrypt_restores_plaintext() {
        let dir = scratch("dec");
        let db = dir.join("vault.db");
        make_plain_db(&db);
        let key = [9u8; 32];

        encrypt_database(&db, &key).unwrap();
        decrypt_database(&db, &key).unwrap();

        // Plain SQLite again: opens with no key, header restored.
        let conn = Connection::open(&db).unwrap();
        let v: String = conn.query_row("SELECT x FROM t", [], |r| r.get(0)).unwrap();
        assert_eq!(v, "gps-track");
        let ver: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(ver, 22);
        let bytes = fs::read(&db).unwrap();
        assert_eq!(&bytes[..16], b"SQLite format 3\0");

        let _ = fs::remove_dir_all(&dir);
    }
}
