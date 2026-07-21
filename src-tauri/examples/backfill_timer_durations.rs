//! Dev one-off: rewrite activity durations from elapsed to timer time.
//!
//! Older imports stored the FIT session's total_elapsed_time (wall clock,
//! pauses included) as duration_s; the pipeline now prefers total_timer_time.
//! This reparses every FIT raw file in the vault and updates activities whose
//! stored duration doesn't match, then refreshes merged multisport containers
//! (their duration is the sum of their legs').
//!
//! Dry-run by default; pass --apply to write. Close the app first (it holds
//! the vault). Reads the vault location the same way the app does. Encrypted
//! raw files are skipped with a warning — decrypt the vault first.
//!
//! Usage:
//!   cargo run --example backfill_timer_durations            # preview
//!   cargo run --example backfill_timer_durations -- --apply # write

use rusqlite::Connection;

fn vault_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").expect("HOME not set");
    let config = std::path::Path::new(&home).join("Library/Application Support/com.syzify.app");
    let marker = config.join("vault-location");
    match std::fs::read_to_string(&marker) {
        Ok(s) => std::path::PathBuf::from(s.trim()),
        Err(_) => config.join("vault"),
    }
}

fn main() {
    let apply = std::env::args().any(|a| a == "--apply");
    let vault = vault_path();
    let db = vault.join("vault.db");

    let header = std::fs::read(&db).expect("read vault.db");
    assert!(
        header.starts_with(b"SQLite format 3"),
        "vault.db is SQLCipher-encrypted — decrypt first"
    );

    let conn = Connection::open(&db).expect("open vault.db — is the app closed?");

    // One FIT raw file per activity (an activity can own several raw rows —
    // duplicates get linked to it — any parsing one carries the same session).
    let rows: Vec<(String, Option<f64>, String)> = conn
        .prepare(
            "SELECT a.id, a.duration_s, MIN(r.path_in_vault)
             FROM activity a JOIN raw_file r ON r.activity_id = a.id
             WHERE r.format = 'fit' AND r.parse_status = 'ok'
             GROUP BY a.id",
        )
        .expect("prepare")
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .expect("query")
        .collect::<rusqlite::Result<_>>()
        .expect("rows");

    eprintln!("checking {} FIT-backed activities…", rows.len());
    let mut fixes = 0usize;
    let mut skipped = 0usize;
    for (id, stored, path) in &rows {
        let bytes = match std::fs::read(vault.join(path)) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("{}: unreadable raw file {path} ({e})", &id[..8]);
                skipped += 1;
                continue;
            }
        };
        // FIT magic at offset 8; anything else is likely an encrypted blob.
        if bytes.len() < 12 || &bytes[8..12] != b".FIT" {
            eprintln!("{}: {path} is not plain FIT (encrypted vault?) — skipped", &id[..8]);
            skipped += 1;
            continue;
        }
        let Ok(parsed) = syzify_lib::parser::fit::parse_fit_bytes(&bytes, id) else {
            eprintln!("{}: {path} failed to parse — skipped", &id[..8]);
            skipped += 1;
            continue;
        };
        let Some(want) = parsed
            .session_metrics
            .as_ref()
            .and_then(|s| s.total_timer_time_s.or(s.total_elapsed_time_s))
        else {
            continue; // no session timing — the trackpoint-span duration stands
        };
        if stored.is_some_and(|have| (have - want).abs() < 0.5) {
            continue;
        }
        println!(
            "{}: {} → {} ({})",
            &id[..8],
            stored.map_or("none".into(), |v| hms(v)),
            hms(want),
            parsed.start_time.as_deref().unwrap_or("?")
        );
        fixes += 1;
        if apply {
            conn.execute(
                "UPDATE activity SET duration_s = ?1, updated_at = datetime('now') WHERE id = ?2",
                rusqlite::params![want, id],
            )
            .expect("update activity");
        }
    }

    // Merged multisport containers: duration is the sum of the legs'.
    let stale: Vec<(String, Option<f64>, f64)> = conn
        .prepare(
            "SELECT p.id, p.duration_s, SUM(c.duration_s)
             FROM activity p JOIN activity c ON c.parent_id = p.id
             GROUP BY p.id
             HAVING p.duration_s IS NULL OR abs(p.duration_s - SUM(c.duration_s)) >= 0.5",
        )
        .expect("prepare containers")
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .expect("query containers")
        .collect::<rusqlite::Result<_>>()
        .expect("container rows");
    let mut containers = 0usize;
    for (id, have, want) in &stale {
        println!(
            "container {}: {} → {}",
            &id[..8],
            have.map_or("none".into(), |v| hms(v)),
            hms(*want)
        );
        containers += 1;
        if apply {
            conn.execute(
                "UPDATE activity SET duration_s = ?1, updated_at = datetime('now') WHERE id = ?2",
                rusqlite::params![want, id],
            )
            .expect("update container");
        }
    }

    let mode = if apply { "applied" } else { "dry-run" };
    eprintln!("{mode}: {fixes} activity fix(es), {containers} container(s), {skipped} skipped.");
    if !apply && (fixes > 0 || containers > 0) {
        eprintln!("re-run with --apply (close the app first) to write.");
    }
}

fn hms(s: f64) -> String {
    let s = s.round() as i64;
    format!("{}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
}
