//! The launch catch-up (#221): what reached the watch folders, or a device
//! volume, while Syzify was closed. The folder watcher and the volume
//! monitor only see changes made while the app runs.
//!
//! A watch-folder file is offered when the vault does not hold its content
//! AND it changed since the previous catch-up. Absence from the vault alone
//! is not "new": an activity the user deleted, a Monitor file that stores
//! nothing, a file that never parses — all of them lack a `raw_file` row for
//! good and would come back on every launch. The first catch-up of a vault
//! offers nothing from the watch folders and only sets the mark.
//!
//! A file on a mounted device volume is offered on the vault test alone,
//! as plugging the device in does: a device's FAT keeps no copy time (its
//! ctime is its mtime, the moment the watch wrote the file), so a ride
//! recorded before the mark and seen by no earlier run would never be
//! offered.

use std::fs::{self, File, Metadata};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;
use sha2::{Digest, Sha256};

use crate::db;
use crate::import::pipeline::MAX_IMPORT_BYTES;

/// `setting` key: unix seconds at which the previous catch-up started.
pub const CATCH_UP_AT_KEY: &str = "watch_catch_up_at";

/// The SHA-256 of a file's bytes — the vault's dedup key — streamed, not
/// read whole; None when the file cannot be read.
pub fn content_hash(path: &Path) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher).ok()?;
    Some(hex::encode(hasher.finalize()))
}

/// When a file last reached this disk or changed, in unix seconds. A copy
/// keeps the source's modification time (Finder, `cp -p`, rsync all do), but
/// not its status-change time: that one is the moment of the copy.
pub fn changed_at(meta: &Metadata) -> Option<i64> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Some(meta.mtime().max(meta.ctime()))
    }
    #[cfg(not(unix))]
    {
        let secs = |t: std::io::Result<SystemTime>| {
            t.ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
        };
        secs(meta.modified()).max(secs(meta.created()))
    }
}

/// Slack under the mark: a FAT volume stores local time without a zone and
/// the clock itself may have moved back since; a repeat costs nothing (the
/// vault test drops what was imported), a gap loses a file.
pub const MARK_SLACK_SECS: i64 = 3600;

/// The time a watch-folder file must have changed at or after to be offered:
/// the mark, never later than now, less the slack.
pub fn since_mark(mark: i64, now: i64) -> i64 {
    mark.min(now).saturating_sub(MARK_SLACK_SECS)
}

/// Now, in unix seconds — the mark the next catch-up compares against.
pub fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// The files that changed at or after `since`, each with its content hash
/// (None when unreadable). A file over the import size limit is left out
/// unread: the import would refuse it anyway.
pub fn changed_files(files: Vec<String>, since: i64) -> Vec<(String, Option<String>)> {
    files
        .into_iter()
        .filter_map(|file| {
            let meta = fs::metadata(&file).ok()?;
            if meta.len() > MAX_IMPORT_BYTES || changed_at(&meta)? < since {
                return None;
            }
            let hash = content_hash(Path::new(&file));
            Some((file, hash))
        })
        .collect()
}

/// Of the given files, those whose content the vault does not hold yet —
/// each content once, under the first path that has it (a ride on a mounted
/// Edge and its copy in a watch folder are one file to offer). A file that
/// cannot be read is left out; the watcher and Import Now still report it.
pub fn unseen_files(
    conn: &Connection,
    hashed: Vec<(String, Option<String>)>,
) -> rusqlite::Result<Vec<String>> {
    let mut seen = std::collections::HashSet::new();
    let mut unseen = Vec::new();
    for (path, hash) in hashed {
        let Some(hash) = hash else { continue };
        if !seen.insert(hash.clone()) || db::raw_files::hash_exists(conn, &hash)? {
            continue;
        }
        unseen.push(path);
    }
    Ok(unseen)
}

/// A device's files the vault does not hold, for the volume monitor's
/// offer on plug-in: an Edge keeps every ride it ever recorded, and the
/// banner must not count them all again on each connection. A file that
/// could not be hashed (a bad read over USB, over the size limit) is still
/// offered, so its import says why it fails instead of it vanishing.
/// Hashing runs before the DB lock is taken. Fails open — every file, as
/// before — when the vault cannot be asked (locked, failed to open): the
/// import itself still skips what it holds.
pub fn device_files_not_in_vault(state: &crate::state::AppState, files: Vec<String>) -> Vec<String> {
    let hashed: Vec<(String, Option<String>)> = changed_files(files.clone(), i64::MIN)
        .into_iter()
        .filter(|(_, hash)| hash.is_some())
        .collect();
    let read: std::collections::HashSet<&str> = hashed.iter().map(|(path, _)| path.as_str()).collect();
    let unread: Vec<String> = files.iter().filter(|f| !read.contains(f.as_str())).cloned().collect();
    let Ok(conn) = state.db.lock() else { return files };
    match unseen_files(&conn, hashed) {
        Ok(mut unseen) => {
            unseen.extend(unread);
            unseen
        }
        Err(_) => files,
    }
}

/// The previous catch-up's mark; None for a vault that never had one (or
/// an unreadable value, which is treated the same: start over).
pub fn last_catch_up(conn: &Connection) -> rusqlite::Result<Option<i64>> {
    Ok(db::settings::get_setting(conn, CATCH_UP_AT_KEY)?.and_then(|v| v.parse().ok()))
}

/// Store the mark for the next catch-up.
pub fn mark_catch_up(conn: &Connection, at: i64) -> rusqlite::Result<()> {
    db::settings::set_setting(conn, CATCH_UP_AT_KEY, &at.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::raw_file::RawFile;
    use crate::test_support::ScratchDir;

    fn raw_file_with_hash(hash: &str) -> RawFile {
        RawFile {
            id: format!("rf-{hash}"),
            activity_id: None,
            path_in_vault: format!("raw/{hash}.fit"),
            original_path: None,
            format: "fit".into(),
            hash_sha256: hash.into(),
            imported_at: String::new(),
            parse_status: "ok".into(),
            failure_reason: None,
        }
    }

    /// Only content the vault lacks is offered, each content once under its
    /// first path; an unreadable file (no hash) is not offered at all.
    #[test]
    fn unseen_files_skips_known_repeated_and_unreadable_content() {
        let conn = crate::db::test_db();
        db::raw_files::insert_raw_file(&conn, &raw_file_with_hash("known")).unwrap();
        let hashed = vec![
            ("/w/old.fit".to_string(), Some("known".to_string())),
            ("/w/new.fit".to_string(), Some("fresh".to_string())),
            ("/Volumes/GARMIN/new.fit".to_string(), Some("fresh".to_string())),
            ("/w/locked.fit".to_string(), None),
            ("/w/other.fit".to_string(), Some("other".to_string())),
        ];
        assert_eq!(unseen_files(&conn, hashed).unwrap(), vec!["/w/new.fit", "/w/other.fit"]);
    }

    /// The streamed hash is the same SHA-256 the pipeline dedups on.
    #[test]
    fn content_hash_matches_the_pipeline_hash() {
        let root = ScratchDir::new("catch-up-hash");
        let file = root.join("ride.fit");
        fs::write(&file, b"hello").unwrap();
        assert_eq!(content_hash(&file).as_deref(), Some(&*hex::encode(Sha256::digest(b"hello"))));
        assert_eq!(content_hash(&root.join("gone.fit")), None);
    }

    /// A file that changed before the mark is not a candidate; one at or
    /// after it is, with its hash; a vanished file drops out.
    #[test]
    fn changed_files_keeps_only_what_changed_since_the_mark() {
        let root = ScratchDir::new("catch-up-changed");
        let file = root.join("ride.fit");
        fs::write(&file, b"ride").unwrap();
        let path = file.to_str().unwrap().to_string();
        let at = changed_at(&fs::metadata(&file).unwrap()).unwrap();
        let gone = root.join("gone.fit").to_str().unwrap().to_string();

        assert!(changed_files(vec![path.clone()], at + 1).is_empty());
        assert_eq!(
            changed_files(vec![path.clone(), gone], at),
            vec![(path, Some(hex::encode(Sha256::digest(b"ride"))))]
        );
    }

    /// A copy that keeps an old modification time still counts as changed:
    /// its status-change time is the moment it landed.
    #[cfg(unix)]
    #[test]
    fn changed_at_sees_a_copy_that_kept_an_old_mtime() {
        let root = ScratchDir::new("catch-up-ctime");
        let file = root.join("ride.fit");
        fs::write(&file, b"ride").unwrap();
        let old = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000_000);
        File::options().write(true).open(&file).unwrap().set_modified(old).unwrap();
        let meta = fs::metadata(&file).unwrap();
        use std::os::unix::fs::MetadataExt;
        assert_eq!(meta.mtime(), 1_000_000_000);
        assert!(changed_at(&meta).unwrap() >= now_secs() - 60);
    }

    /// The mark minus an hour, and never a mark from the future: a clock set
    /// back after the mark was written must not open a gap.
    #[test]
    fn since_mark_takes_the_slack_and_caps_at_now() {
        assert_eq!(since_mark(10_000, 20_000), 10_000 - MARK_SLACK_SECS);
        assert_eq!(since_mark(30_000, 20_000), 20_000 - MARK_SLACK_SECS);
        assert_eq!(since_mark(i64::MIN, 20_000), i64::MIN, "a garbage mark does not overflow");
    }

    /// A file over the import limit is not read, let alone offered: the
    /// import would refuse it. A sparse file costs no disk.
    #[test]
    fn changed_files_skips_a_file_over_the_import_limit() {
        let root = ScratchDir::new("catch-up-huge");
        let huge = root.join("huge.fit");
        File::create(&huge).unwrap().set_len(MAX_IMPORT_BYTES + 1).unwrap();
        assert!(changed_files(vec![huge.to_str().unwrap().to_string()], i64::MIN).is_empty());
    }

    /// On plug-in only what the vault lacks is offered, plus any file that
    /// could not be read — its import names the failure; when the vault
    /// cannot answer (no schema — a locked or failed vault's placeholder),
    /// every file is, as before the filter.
    #[test]
    fn device_files_not_in_vault_filters_and_fails_open() {
        use crate::test_support::app_state_on;
        let root = ScratchDir::new("catch-up-device");
        let old = root.join("old.fit");
        let new = root.join("new.fit");
        fs::write(&old, b"old ride").unwrap();
        fs::write(&new, b"new ride").unwrap();
        let gone = root.join("gone.fit").to_str().unwrap().to_string();
        let files = vec![old.to_str().unwrap().to_string(), new.to_str().unwrap().to_string(), gone];

        let conn = crate::db::test_db();
        let known = content_hash(&old).unwrap();
        db::raw_files::insert_raw_file(&conn, &raw_file_with_hash(&known)).unwrap();
        assert_eq!(
            device_files_not_in_vault(&app_state_on(conn), files.clone()),
            vec![files[1].clone(), files[2].clone()]
        );

        let bare = Connection::open_in_memory().unwrap();
        assert_eq!(device_files_not_in_vault(&app_state_on(bare), files.clone()), files);
    }

    /// The mark round-trips through `setting`; a vault without one (or with
    /// garbage in it) has none.
    #[test]
    fn the_mark_round_trips() {
        let conn = crate::db::test_db();
        assert_eq!(last_catch_up(&conn).unwrap(), None);
        mark_catch_up(&conn, 1_760_000_000).unwrap();
        assert_eq!(last_catch_up(&conn).unwrap(), Some(1_760_000_000));
        db::settings::set_setting(&conn, CATCH_UP_AT_KEY, "soon").unwrap();
        assert_eq!(last_catch_up(&conn).unwrap(), None);
    }
}
