use std::fs;
use std::path::Path;

use tauri::{AppHandle, Emitter, Manager, State};

use crate::db;
use crate::export::{gpx_writer, vault_backup};
use crate::state::AppState;

#[derive(Clone, Debug, serde::Serialize)]
pub struct RestoreOutcome {
    /// True once the live DB connection has been torn down — the caller MUST
    /// restart the app to reopen from the restored files, EVEN IF `error` is
    /// set (a mid-extraction failure past that point leaves a dead in-memory
    /// placeholder; only a restart recovers).
    pub restored: bool,
    /// Set when extraction failed after the point of no return — the vault may
    /// be partially restored; the frontend shows this before restarting.
    pub error: Option<String>,
    /// Where the pre-restore vault contents were moved (None for an empty
    /// vault). The frontend surfaces this so the user knows the replaced data
    /// still exists and where to reclaim disk space from.
    pub preserved_at: Option<String>,
}

#[derive(Clone, serde::Serialize)]
struct BackupProgress {
    processed: u64,
    total: u64,
}

/// These commands write to paths supplied by the frontend. Restrict each to
/// its own extension so they can't be abused (e.g. via XSS) to overwrite
/// arbitrary files — same policy as save_share_image's .png gate.
fn ensure_extension(path: &str, ext: &str, what: &str) -> Result<(), String> {
    let ok = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case(ext))
        .unwrap_or(false);
    if ok {
        Ok(())
    } else {
        Err(format!("{} must be saved with a .{} extension", what, ext))
    }
}

/// A privacy radius is frontend-supplied — reject nonsense (NaN, negatives)
/// and absurd zones that would blank entire tracks.
fn ensure_privacy_radius(radius_m: f64) -> Result<(), String> {
    if radius_m.is_finite() && radius_m > 0.0 && radius_m <= 10_000.0 {
        Ok(())
    } else {
        Err("Privacy radius must be between 0 and 10000 m".to_string())
    }
}

/// Whether the activity's altitude stream is barometric: its source FIT
/// recorded a barometer channel (device_info). Best-effort — a missing or
/// unreadable raw file (locked vault, moved bytes) just means no baro hint
/// in the export, never a failed export.
fn activity_has_barometer(
    state: &AppState,
    conn: &rusqlite::Connection,
    activity_id: &str,
) -> bool {
    let raws = match db::raw_files::get_raw_files_for_activity(conn, activity_id) {
        Ok(raws) => raws,
        Err(_) => return false,
    };
    raws.iter()
        .filter(|rf| rf.format == "fit")
        .any(|rf| {
            let full = state.vault_path.join(&rf.path_in_vault);
            let bytes = if rf.path_in_vault.ends_with(".enc") {
                match state.encryption_key_for(|s| s.activities) {
                    Ok(Some(key)) => crate::crypto::decrypt_file_to_memory(&key, &full).ok(),
                    _ => None,
                }
            } else {
                fs::read(&full).ok()
            };
            bytes.is_some_and(|b| crate::parser::fit::fit_has_barometer(&b))
        })
}

#[tauri::command]
pub fn export_activity_gpx(
    id: String,
    dest_path: String,
    privacy_radius_m: Option<f64>,
    state: State<AppState>,
) -> Result<(), String> {
    ensure_extension(&dest_path, "gpx", "GPX export")?;
    let conn = state.db.lock().map_err(|e| e.to_string())?;

    let activity = db::activities::get_activity_by_id(&conn, &id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Activity not found: {}", id))?;

    let mut trackpoints = db::trackpoints::get_trackpoints(&conn, &id)
        .map_err(|e| e.to_string())?;

    if let Some(radius_m) = privacy_radius_m {
        ensure_privacy_radius(radius_m)?;
        trackpoints = gpx_writer::privacy_trim(&trackpoints, radius_m);
    }

    let barometric = activity_has_barometer(&state, &conn, &id);
    let gpx_content = gpx_writer::activity_to_gpx(&activity, &trackpoints, barometric);

    if let Some(parent) = Path::new(&dest_path).parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory: {}", e))?;
    }

    fs::write(&dest_path, gpx_content)
        .map_err(|e| format!("Failed to write GPX file: {}", e))?;

    Ok(())
}

#[tauri::command]
pub async fn backup_vault(dest_path: String, app: AppHandle) -> Result<(), String> {
    ensure_extension(&dest_path, "zip", "Vault backup")?;
    tauri::async_runtime::spawn_blocking(move || {
        let progress = |processed: u64, total: u64| {
            let _ = app.emit("backup:progress", BackupProgress { processed, total });
        };
        backup_vault_core(&app.state::<AppState>(), Path::new(&dest_path), &progress)
    })
    .await
    .map_err(|e| format!("Backup task failed: {}", e))?
}

/// Remove a `.backup-snapshot` stranded by a crash mid-backup. Called at boot:
/// the snapshot is a copy of vault.db (plaintext whenever the database scope
/// is off) living inside the vault, and nothing else knows about it —
/// enable_encryption wouldn't encrypt it and relocate would carry it along.
/// Returns true if there was one to remove.
pub(crate) fn scrub_stale_backup_snapshot(vault_path: &Path) -> bool {
    let dir = vault_path.join(".backup-snapshot");
    if !dir.exists() {
        return false;
    }
    fs::remove_dir_all(&dir).is_ok()
}

/// The AppHandle-free part of backup, testable against a bare [`AppState`].
///
/// The database runs in WAL mode, so the vault's `vault.db` alone is NOT the
/// database: committed transactions sit in `vault.db-wal` until a checkpoint,
/// and a copy taken while a checkpoint runs can be torn. So, under the
/// connection lock (no writer can interleave), flush the WAL and copy the DB
/// into a snapshot dir; the archive then reads the snapshot while the app
/// keeps working. On a LOCKED vault the checkpoint no-ops against the
/// placeholder connection — then any leftover WAL is snapshotted too and
/// replayed by SQLite after restore.
pub(crate) fn backup_vault_core(
    state: &AppState,
    dest_path: &Path,
    progress: &dyn Fn(u64, u64),
) -> Result<(), String> {
    // One vault mutation at a time: a concurrent backup would clobber this
    // run's .backup-snapshot mid-archive, and a concurrent restore/relocation
    // would move raw/ and photos/ out from under the archive walk.
    let _flight = state
        .vault_flight
        .try_begin()
        .ok_or("Another vault operation is already in progress")?;

    let vault_path = &state.vault_path;
    let snapshot_dir = vault_path.join(".backup-snapshot");
    let _ = fs::remove_dir_all(&snapshot_dir); // stale leftover from a crash
    fs::create_dir_all(&snapshot_dir)
        .map_err(|e| format!("Failed to create backup snapshot dir: {}", e))?;

    {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        // Best-effort: no-op on the in-memory placeholder of a locked vault.
        let _ = conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()));

        let db_path = vault_path.join("vault.db");
        if db_path.exists() {
            fs::copy(&db_path, snapshot_dir.join("vault.db"))
                .map_err(|e| format!("Failed to snapshot vault.db: {}", e))?;
        }
        // Non-empty after the checkpoint = the locked-vault case: the WAL is
        // SQLCipher-encrypted and can't be flushed without the key. Carry it.
        let wal_path = vault_path.join("vault.db-wal");
        if fs::metadata(&wal_path).map(|m| m.len() > 0).unwrap_or(false) {
            fs::copy(&wal_path, snapshot_dir.join("vault.db-wal"))
                .map_err(|e| format!("Failed to snapshot vault.db-wal: {}", e))?;
        }
    } // lock released — the archive write below runs against the snapshot

    let result =
        vault_backup::create_backup_with_progress(vault_path, dest_path, &snapshot_dir, progress);
    let _ = fs::remove_dir_all(&snapshot_dir);
    result
}

/// Restore the vault from a backup zip. The live SQLCipher/rusqlite connection
/// is open on the CURRENT vault.db in WAL mode, so we must close it before the
/// files are replaced — otherwise the next WAL checkpoint writes stale
/// pages over the restored database and corrupts it. And because a backup can
/// carry a different encryption state than the running session (plaintext vs a
/// different password), the only safe way to reopen is a full app restart that
/// re-runs boot detection. The current vault contents are never extracted
/// over: they move to a `pre-restore-<ts>/` quarantine first, so the restored
/// state is purely the archive's (no stale vault.lock → no permanent lockout;
/// the old salt leaves with its ciphertext → INV-1; no old/new file mixing).
/// So: validate → checkpoint+close the live DB → quarantine → extract → the
/// frontend restarts on `restored: true`.
#[tauri::command]
pub async fn restore_vault(
    backup_path: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<RestoreOutcome, String> {
    let _ = state; // resolved inside the blocking task via the moved AppHandle
    tauri::async_runtime::spawn_blocking(move || {
        restore_vault_core(&app.state::<AppState>(), Path::new(&backup_path))
    })
    .await
    .map_err(|e| format!("Restore task failed: {}", e))?
}

/// The AppHandle-free part of restore, testable against a bare [`AppState`].
pub(crate) fn restore_vault_core(
    state: &AppState,
    backup: &Path,
) -> Result<RestoreOutcome, String> {
    let vault_path = &state.vault_path;

    // One vault mutation at a time: restoring while a backup streams raw/ and
    // photos/ (or another restore/relocation runs) would tear the archive or
    // mix states. Failing here is pre-flight semantics — nothing was touched.
    let _flight = state
        .vault_flight
        .try_begin()
        .ok_or("Another vault operation is already in progress")?;

    // Pre-flight (deep: safe entry names, no zip bomb, vault.db present): a
    // bad or wrong-kind archive fails here with the live DB still untouched,
    // so the app keeps working and no restart is needed.
    vault_backup::validate_backup(backup)?;

    // Prepare the placeholder BEFORE the swap so a failure here leaves the
    // live connection intact and safely returns Err (pre-flight semantics).
    let placeholder = rusqlite::Connection::open_in_memory()
        .map_err(|e| format!("Failed to open placeholder db: {}", e))?;

    // Point of no return: flush the WAL and swap the live handle for the
    // placeholder so no checkpoint can race the overwrite. Past the swap the
    // app MUST restart regardless of outcome, so close is best-effort — a
    // close error still drops `old` (which closes the fd) and must not turn
    // into an Err that skips the restart.
    {
        let mut conn = state.db.lock().map_err(|e| e.to_string())?;
        let _ = conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()));
        let old = std::mem::replace(&mut *conn, placeholder);
        if let Err((_, e)) = old.close() {
            eprintln!("restore: closing the old DB handle failed (dropped anyway): {e}");
        }
    }

    // From here on the session serves a dead placeholder while db_locked is
    // false — INV-2 territory. The frontend restarts the app, but if that
    // restart fails (it is fire-and-forget), nothing must LOOK healthy:
    // vault_error puts the blocking "reopen" screen in front of the UI, so
    // writes can't silently vanish into the placeholder. A successful restart
    // discards this with the process.
    if let Ok(mut ve) = state.vault_error.lock() {
        *ve = Some(
            "The vault was restored from a backup — the app must restart to reopen it."
                .to_string(),
        );
    }

    // Move the current vault aside so extraction lands on a clean slate
    // (encryption invariants — see quarantine_current_vault). A failure
    // here rolled the renames back, so the restart below boots the intact
    // ORIGINAL vault; report the error but keep restored:true — the live
    // connection is already a placeholder either way.
    let preserved = match vault_backup::quarantine_current_vault(vault_path) {
        Ok(dir) => dir,
        Err(e) => {
            return Ok(RestoreOutcome {
                restored: true,
                error: Some(e),
                preserved_at: None,
            })
        }
    };
    let preserved_at = preserved.map(|p| p.display().to_string());

    // A failure here leaves a possibly-partial vault and a dead placeholder
    // connection — return restored:true WITH the error so the frontend
    // still restarts (a plain Err would skip the restart and strand the
    // app on the empty in-memory DB). The quarantine dir still holds the
    // complete pre-restore data for manual recovery.
    match vault_backup::restore_backup(vault_path, backup) {
        Ok(()) => Ok(RestoreOutcome {
            restored: true,
            error: None,
            preserved_at,
        }),
        Err(e) => Ok(RestoreOutcome {
            restored: true,
            error: Some(e),
            preserved_at,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    fn test_state_with(vault: &Path, conn: rusqlite::Connection) -> AppState {
        AppState {
            db: Arc::new(Mutex::new(conn)),
            vault_path: vault.to_path_buf(),
            encryption_key: Mutex::new(None),
            watcher_handle: Mutex::new(None),
            db_locked: Mutex::new(false),
            vault_error: Mutex::new(None),
            services_started: Mutex::new(false),
            geocoding_flight: crate::state::SingleFlight::default(),
            vault_flight: crate::state::SingleFlight::default(),
        }
    }

    fn unique_dir(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("syz_export_{}_{}", tag, uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    /// Concurrent backups share one .backup-snapshot dir — the second call
    /// must be refused while the first is running, and allowed after it.
    #[test]
    fn only_one_backup_at_a_time() {
        let tmp = unique_dir("vault_flight");
        let vault = tmp.join("vault");
        fs::create_dir_all(&vault).unwrap();
        fs::write(vault.join("vault.db"), b"db").unwrap();
        let state = test_state_with(&vault, crate::db::test_db());

        {
            let _running = state.vault_flight.try_begin().unwrap();
            let err = backup_vault_core(&state, &tmp.join("b1.zip"), &|_, _| {}).unwrap_err();
            assert!(err.contains("already in progress"), "got: {}", err);
        }
        // Slot freed — a new backup succeeds.
        backup_vault_core(&state, &tmp.join("b2.zip"), &|_, _| {}).unwrap();
        assert!(tmp.join("b2.zip").exists());

        let _ = fs::remove_dir_all(&tmp);
    }

    /// Backup, restore and relocation share one vault-wide gate: a restore
    /// starting while a backup streams raw/ and photos/ would move them into
    /// quarantine mid-archive. Refused up front, with the session intact.
    #[test]
    fn restore_is_refused_while_another_vault_operation_runs() {
        let tmp = unique_dir("vault_mutex");
        let vault = tmp.join("vault");
        let src = tmp.join("src");
        fs::create_dir_all(&vault).unwrap();
        fs::create_dir_all(&src).unwrap();
        fs::write(vault.join("vault.db"), b"current db").unwrap();
        fs::write(src.join("vault.db"), b"backup db").unwrap();
        let backup = tmp.join("backup.zip");
        vault_backup::create_backup(&src, &backup).unwrap();

        let state = test_state_with(&vault, crate::db::test_db());
        {
            let _running = state.vault_flight.try_begin().unwrap();
            let err = restore_vault_core(&state, &backup).unwrap_err();
            assert!(err.contains("already in progress"), "got: {}", err);
            // Pre-flight semantics: nothing was torn down or quarantined.
            assert_eq!(fs::read(vault.join("vault.db")).unwrap(), b"current db");
            assert!(state.vault_error.lock().unwrap().is_none());
            assert!(!fs::read_dir(&vault)
                .unwrap()
                .flatten()
                .any(|e| e.file_name().to_string_lossy().starts_with("pre-restore")));
        }
        // Gate freed — the restore proceeds.
        let outcome = restore_vault_core(&state, &backup).unwrap();
        assert!(outcome.restored);
        assert_eq!(fs::read(vault.join("vault.db")).unwrap(), b"backup db");

        let _ = fs::remove_dir_all(&tmp);
    }

    /// A .backup-snapshot stranded by a crash mid-backup is a plaintext copy
    /// of vault.db nothing else manages — boot must scrub it.
    #[test]
    fn stale_backup_snapshot_is_scrubbed() {
        let tmp = unique_dir("snapshot_scrub");
        let vault = tmp.join("vault");
        fs::create_dir_all(vault.join(".backup-snapshot")).unwrap();
        fs::write(vault.join(".backup-snapshot/vault.db"), b"plaintext copy").unwrap();

        assert!(scrub_stale_backup_snapshot(&vault));
        assert!(!vault.join(".backup-snapshot").exists());
        // Idempotent: nothing left to remove.
        assert!(!scrub_stale_backup_snapshot(&vault));

        let _ = fs::remove_dir_all(&tmp);
    }

    /// A wrong-kind zip (here: a Runkeeper-style export) must be rejected at
    /// pre-flight with the session fully intact: live DB still usable, vault
    /// files untouched, no restart demanded via vault_error.
    #[test]
    fn restore_refuses_a_non_vault_zip_before_touching_anything() {
        use std::io::Write as _;
        use zip::write::FileOptions;

        let tmp = unique_dir("restore_wrong_zip");
        let vault = tmp.join("vault");
        fs::create_dir_all(vault.join("raw")).unwrap();
        fs::write(vault.join("vault.db"), b"the real db").unwrap();
        fs::write(vault.join("raw/a.gpx"), b"track").unwrap();

        let zip_path = tmp.join("runkeeper.zip");
        {
            let mut zip = zip::ZipWriter::new(fs::File::create(&zip_path).unwrap());
            let opts: FileOptions<'_, ()> = FileOptions::default();
            zip.start_file("cardioActivities.csv", opts).unwrap();
            zip.write_all(b"Date,Type\n").unwrap();
            zip.finish().unwrap();
        }

        let state = test_state_with(&vault, crate::db::test_db());
        let err = restore_vault_core(&state, &zip_path).unwrap_err();
        assert!(err.contains("not a Syzify vault backup"), "got: {}", err);

        // Session intact: the connection still answers, the vault untouched,
        // no restart-required flag, nothing quarantined.
        assert!(state
            .db
            .lock()
            .unwrap()
            .query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0))
            .is_ok());
        assert_eq!(fs::read(vault.join("vault.db")).unwrap(), b"the real db");
        assert!(vault.join("raw/a.gpx").exists());
        assert!(state.vault_error.lock().unwrap().is_none());
        assert!(!fs::read_dir(&vault)
            .unwrap()
            .flatten()
            .any(|e| e.file_name().to_string_lossy().starts_with("pre-restore")));

        let _ = fs::remove_dir_all(&tmp);
    }

    /// Past the point of no return the session serves a dead placeholder with
    /// db_locked=false — vault_error must be set so a failed restart shows the
    /// blocking screen instead of the app looking healthy (INV-2 in spirit).
    #[test]
    fn restore_demands_a_restart_via_vault_error() {
        let tmp = unique_dir("restore_restart_flag");
        let vault = tmp.join("vault");
        let src = tmp.join("src");
        fs::create_dir_all(&vault).unwrap();
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("vault.db"), b"backup db").unwrap();
        let backup = tmp.join("backup.zip");
        vault_backup::create_backup(&src, &backup).unwrap();

        fs::write(vault.join("vault.db"), b"current db").unwrap();
        let state = test_state_with(&vault, crate::db::test_db());

        let outcome = restore_vault_core(&state, &backup).unwrap();
        assert!(outcome.restored);
        assert!(outcome.error.is_none());
        assert!(outcome.preserved_at.is_some(), "current vault must be quarantined");

        // Restart demanded, restored files in place.
        assert!(state.vault_error.lock().unwrap().is_some());
        assert_eq!(fs::read(vault.join("vault.db")).unwrap(), b"backup db");

        let _ = fs::remove_dir_all(&tmp);
    }

    /// The privacy radius comes from the frontend — NaN/zero/negative and
    /// track-erasing values must be refused.
    #[test]
    fn privacy_radius_is_validated() {
        assert!(ensure_privacy_radius(200.0).is_ok());
        assert!(ensure_privacy_radius(10_000.0).is_ok());
        for bad in [0.0, -1.0, 10_001.0, f64::NAN, f64::INFINITY] {
            assert!(ensure_privacy_radius(bad).is_err(), "{} must be rejected", bad);
        }
    }

    /// Frontend-supplied write paths are gated to their own extension so a
    /// compromised webview can't overwrite arbitrary files.
    #[test]
    fn write_paths_are_gated_to_their_extension() {
        assert!(ensure_extension("/tmp/a.gpx", "gpx", "GPX export").is_ok());
        assert!(ensure_extension("/tmp/a.GPX", "gpx", "GPX export").is_ok());
        assert!(ensure_extension("/tmp/backup.zip", "zip", "Vault backup").is_ok());
        for bad in ["/tmp/a.sh", "/tmp/a", "/tmp/.zshrc", "/tmp/a.zip.sh"] {
            assert!(
                ensure_extension(bad, "gpx", "GPX export").is_err(),
                "{} must be rejected",
                bad
            );
        }
    }

    /// The WAL bug: in WAL mode committed transactions live in vault.db-wal
    /// until a checkpoint, so archiving the bare vault.db file loses them. The
    /// backup must checkpoint under the connection lock first — this test
    /// fails against the un-checkpointed copy (the restored DB has no rows,
    /// or no table at all).
    #[test]
    fn backup_includes_transactions_still_in_the_wal() {
        let tmp = unique_dir("wal");
        let vault = tmp.join("vault");
        fs::create_dir_all(&vault).unwrap();
        let backup_file = tmp.join("backup.zip");

        // A real file-backed DB in WAL mode with an uncheckpointed write,
        // exactly like the live app (see configure_and_migrate).
        let conn = rusqlite::Connection::open(vault.join("vault.db")).unwrap();
        conn.pragma_update(None, "journal_mode", "WAL").unwrap();
        conn.execute("CREATE TABLE t (v TEXT)", []).unwrap();
        conn.execute("INSERT INTO t (v) VALUES ('fresh, only in the WAL')", [])
            .unwrap();
        let state = test_state_with(&vault, conn);

        backup_vault_core(&state, &backup_file, &|_, _| {}).unwrap();

        // The snapshot dir must not linger inside the vault.
        assert!(!vault.join(".backup-snapshot").exists());

        // Restore into a fresh dir and read the row back.
        let restored = tmp.join("restored");
        fs::create_dir_all(&restored).unwrap();
        vault_backup::restore_backup(&restored, &backup_file).unwrap();
        let check = rusqlite::Connection::open(restored.join("vault.db")).unwrap();
        let v: String = check.query_row("SELECT v FROM t", [], |r| r.get(0)).unwrap();
        assert_eq!(v, "fresh, only in the WAL");

        let _ = fs::remove_dir_all(&tmp);
    }

    /// LOCKED vault: the connection is an in-memory placeholder, so the
    /// checkpoint can't flush the (encrypted) WAL — the backup must carry
    /// vault.db-wal so SQLite replays it after restore.
    #[test]
    fn backup_of_a_locked_vault_carries_the_leftover_wal() {
        let tmp = unique_dir("locked_wal");
        let vault = tmp.join("vault");
        fs::create_dir_all(&vault).unwrap();
        let backup_file = tmp.join("backup.zip");

        fs::write(vault.join("vault.db"), b"sqlcipher main file").unwrap();
        fs::write(vault.join("vault.db-wal"), b"sqlcipher wal frames").unwrap();
        // Placeholder connection, as boot installs for a locked database.
        let state =
            test_state_with(&vault, rusqlite::Connection::open_in_memory().unwrap());
        *state.db_locked.lock().unwrap() = true;

        backup_vault_core(&state, &backup_file, &|_, _| {}).unwrap();

        let restored = tmp.join("restored");
        fs::create_dir_all(&restored).unwrap();
        vault_backup::restore_backup(&restored, &backup_file).unwrap();
        assert_eq!(
            fs::read(restored.join("vault.db")).unwrap(),
            b"sqlcipher main file"
        );
        assert_eq!(
            fs::read(restored.join("vault.db-wal")).unwrap(),
            b"sqlcipher wal frames"
        );

        let _ = fs::remove_dir_all(&tmp);
    }
}
