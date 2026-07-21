//! De-risk: confirm the SQLCipher-backed rusqlite build actually encrypts.
use rusqlite::Connection;

#[test]
fn keyed_db_roundtrips_and_blocks_wrong_key() {
    let dir = std::env::temp_dir().join("syz_sqlcipher_smoke");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let db_path = dir.join("enc.db");

    {
        let conn = Connection::open(&db_path).unwrap();
        conn.pragma_update(None, "key", "correct horse").unwrap();
        conn.execute_batch("CREATE TABLE t(x TEXT); INSERT INTO t VALUES('secret');")
            .unwrap();
    }
    {
        let conn = Connection::open(&db_path).unwrap();
        conn.pragma_update(None, "key", "correct horse").unwrap();
        let v: String = conn.query_row("SELECT x FROM t", [], |r| r.get(0)).unwrap();
        assert_eq!(v, "secret");
    }
    {
        let conn = Connection::open(&db_path).unwrap();
        conn.pragma_update(None, "key", "wrong").unwrap();
        assert!(conn.query_row("SELECT x FROM t", [], |r| r.get::<_, String>(0)).is_err());
    }

    let bytes = std::fs::read(&db_path).unwrap();
    assert!(!bytes.windows(6).any(|w| w == b"secret"));
    assert_ne!(&bytes[..16], b"SQLite format 3\0");

    let _ = std::fs::remove_dir_all(&dir);
}
