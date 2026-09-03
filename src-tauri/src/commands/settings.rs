use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

fn is_workout_file(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());
    match ext.as_deref() {
        Some("gpx" | "fit" | "tcx") => true,
        Some("gz") => {
            path.file_stem()
                .and_then(|s| Path::new(s).extension())
                .and_then(|e| e.to_str())
                .map(|e| ["gpx", "fit", "tcx"].contains(&e.to_ascii_lowercase().as_str()))
                .unwrap_or(false)
        }
        _ => false,
    }
}
use tauri::{AppHandle, Emitter, Manager, State};

use crate::crypto;
use crate::db;
use crate::import::pipeline;
use crate::import::watcher;
use crate::state::AppState;
use crate::vault;

/// Recursively collect workout files from a directory.
fn collect_workout_files(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                files.extend(collect_workout_files(&path));
            } else if path.is_file() && is_workout_file(&path) {
                files.push(path);
            }
        }
    }
    files
}

#[tauri::command]
pub fn get_setting(key: String, state: State<AppState>) -> Result<Option<String>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::settings::get_setting(&conn, &key).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_setting(key: String, value: String, state: State<AppState>) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::settings::set_setting(&conn, &key, &value).map_err(|e| e.to_string())
}

/// Maps a legal-document id from the UI to its bundled resource filename.
/// Doubles as the allowlist: anything else must not reach the filesystem.
fn legal_doc_file(doc: &str) -> Option<&'static str> {
    match doc {
        "license" => Some("LICENSE"),
        "exception" => Some("LICENSE-PLUGIN-EXCEPTION.md"),
        "notices" => Some("THIRD-PARTY-NOTICES.md"),
        _ => None,
    }
}

/// Legal texts shipped as bundle resources (tauri.conf.json `bundle.resources`),
/// surfaced in Settings → About so the app itself carries its license terms
/// (AGPLv3 §5(d) "Appropriate Legal Notices" friendliness for forks too).
#[tauri::command]
pub fn get_legal_text(doc: String, app: AppHandle) -> Result<String, String> {
    let file = legal_doc_file(&doc).ok_or(format!("Unknown legal document: {doc}"))?;
    let path = app
        .path()
        .resolve(file, tauri::path::BaseDirectory::Resource)
        .map_err(|e| e.to_string())?;
    fs::read_to_string(path).map_err(|e| e.to_string())
}

/// Kick off a background geocoding pass now — called when the user flips the
/// "Automatic location names" toggle on, so existing activities get their
/// names without waiting for a restart or the next import. The pass itself
/// re-reads the setting and stops if it gets flipped back off.
#[tauri::command]
pub fn start_geocoding(app: AppHandle) {
    std::thread::spawn(move || {
        crate::import::geocoding::run_background_geocoding(&app);
    });
}

pub use db::watch_folders::WatchFolder;

#[derive(serde::Serialize)]
pub struct ScanResult {
    pub new_files: Vec<String>,
    pub import_result: Option<pipeline::ImportResult>,
}

#[tauri::command]
pub fn get_watch_folders(state: State<AppState>) -> Result<Vec<WatchFolder>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::watch_folders::list(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn add_watch_folder(path: String, state: State<AppState>) -> Result<WatchFolder, String> {
    // Validate path exists
    if !Path::new(&path).is_dir() {
        return Err(format!("Directory does not exist: {}", path));
    }
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::watch_folders::add(&conn, &path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn remove_watch_folder(id: i64, state: State<AppState>) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::watch_folders::remove(&conn, id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn scan_watch_folders(state: State<AppState>) -> Result<ScanResult, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;

    // Get all watch folders
    let folders = db::watch_folders::list_paths(&conn).map_err(|e| e.to_string())?;

    // Find new files (recursively)
    let mut new_files: Vec<String> = Vec::new();

    for folder in &folders {
        let dir = Path::new(folder);
        if !dir.is_dir() {
            continue;
        }
        for path in collect_workout_files(dir) {
            new_files.push(path.to_string_lossy().to_string());
        }
    }

    if new_files.is_empty() {
        return Ok(ScanResult {
            new_files: Vec::new(),
            import_result: None,
        });
    }

    // Import (pipeline handles dedup). Key only when the `activities` scope
    // is on — see AppState::encryption_key_for.
    let key = state.encryption_key_for(|s| s.activities)?;
    let result = pipeline::import_files(&conn, &state.vault_path, &new_files, key.as_ref(), |_, _, _| {});

    Ok(ScanResult {
        new_files,
        import_result: Some(result),
    })
}

#[tauri::command]
pub fn get_vault_path(state: State<AppState>) -> Result<String, String> {
    Ok(state.vault_path.to_string_lossy().to_string())
}

/// A boot-time vault error (e.g. the vault sits in a macOS-protected folder
/// and the app lacks Full Disk Access), or None when the vault opened fine.
#[tauri::command]
pub fn get_vault_error(state: State<AppState>) -> Result<Option<String>, String> {
    Ok(state.vault_error.lock().map_err(|e| e.to_string())?.clone())
}

#[derive(Clone, serde::Serialize)]
struct RelocateProgress {
    processed: u64,
    total: u64,
}

/// Move the whole vault to a user-picked directory. Holds the DB lock for the
/// duration (no writes mid-move), swaps the live connection to the new
/// location, and persists it for the next launch. The caller restarts the app
/// afterwards: `AppState.vault_path` is immutable and the photo/tile
/// protocols and import services keep reading the old path until then.
#[tauri::command]
pub async fn relocate_vault(
    dest_path: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let src = state.vault_path.clone();
    let db = state.db.clone();
    // The vault may be unlocked with the database scope encrypted — capture the
    // key ([u8; 32] is Copy) so the reopen after the move can open the keyed DB
    // rather than failing on a SQLCipher file opened as plaintext.
    let key = *state.encryption_key.lock().map_err(|e| e.to_string())?;

    tauri::async_runtime::spawn_blocking(move || {
        // One vault mutation at a time: moving the vault out from under a
        // running backup or restore would tear whichever loses the race.
        // Claimed before the connection swap, so failing here is side-effect-free.
        let flight_state = app.state::<AppState>();
        let _flight = flight_state
            .vault_flight
            .try_begin()
            .ok_or("Another vault operation is already in progress")?;

        let config_dir = app
            .path()
            .app_config_dir()
            .map_err(|e| format!("No config dir: {}", e))?;

        let mut conn = db.lock().map_err(|e| e.to_string())?;

        // Flush the WAL and close the file connection before moving vault.db.
        conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
            .map_err(|e| format!("Failed to checkpoint database: {}", e))?;
        let placeholder = rusqlite::Connection::open_in_memory()
            .map_err(|e| format!("Failed to open placeholder db: {}", e))?;
        let old_conn = std::mem::replace(&mut *conn, placeholder);
        old_conn
            .close()
            .map_err(|(_, e)| format!("Failed to close database: {}", e))?;

        let progress = |processed: u64, total: u64| {
            let _ = app.emit("vault:relocate:progress", RelocateProgress { processed, total });
        };

        // Reopen keyed when that vault's database scope is encrypted; plain
        // otherwise. vault.lock lives inside the vault, so read it at `root`.
        let reopen = |root: &Path| -> Result<rusqlite::Connection, String> {
            let db_encrypted = crypto::read_vault_lock(root)
                .ok()
                .flatten()
                .map(|l| l.scopes.database)
                .unwrap_or(false);
            if db_encrypted {
                let key = key.ok_or("Vault is locked")?;
                crate::init_vault_encrypted(root, &key)
            } else {
                crate::init_vault(root)
            }
        };

        match vault::relocate(&src, Path::new(&dest_path), &config_dir, &progress) {
            Ok(target) => {
                *conn = reopen(&target)?;
                // The app restarts from here: keep the vault-operation slot
                // taken until then (see switch_vault_core for why).
                std::mem::forget(_flight);
                Ok(target.to_string_lossy().to_string())
            }
            Err(e) => {
                // The move failed and was rolled back — reattach the old vault
                // so the app keeps working.
                *conn = reopen(&src)?;
                Err(e)
            }
        }
    })
    .await
    .map_err(|e| format!("Relocate task failed: {}", e))?
}

/// Point the app at a different vault root WITHOUT moving any data: write the
/// location marker and let the caller restart. Works from the boot-error
/// screen and from Settings alike — the current vault is left untouched on
/// disk, and the UI confirms that with the user before calling. With
/// `expect_existing` the picked folder must already hold a vault; otherwise
/// an empty folder (or its `Syzify` subfolder) gets a fresh vault on reboot,
/// never inside an existing vault.
#[tauri::command]
pub async fn switch_vault(
    dest_path: String,
    expect_existing: bool,
    app: AppHandle,
) -> Result<String, String> {
    // Off the main thread: the checks stat every ancestor of the pick, which
    // on a stale network volume can take seconds.
    tauri::async_runtime::spawn_blocking(move || {
        let config_dir = app
            .path()
            .app_config_dir()
            .map_err(|e| format!("No config dir: {}", e))?;
        let state = app.state::<AppState>();
        switch_vault_core(Path::new(&dest_path), expect_existing, &config_dir, &state)
            .map(|root| root.to_string_lossy().to_string())
    })
    .await
    .map_err(|e| format!("Switch task failed: {}", e))?
}

/// The switch itself, `AppHandle`-free so it is testable. On success the
/// `vault_flight` slot is deliberately NOT released: the app is about to
/// restart, and until it does no backup/restore/encryption toggle may start —
/// the restart would tear it mid-way, and the marker already points the next
/// boot (and its stale-file scrubbers) at a different vault, so the damage
/// would sit in the old one unrepaired.
pub(crate) fn switch_vault_core(
    dest: &Path,
    expect_existing: bool,
    config_dir: &Path,
    state: &AppState,
) -> Result<PathBuf, String> {
    let flight = state
        .vault_flight
        .try_begin()
        .ok_or("Another vault operation is already in progress")?;
    let root = if expect_existing {
        vault::resolve_existing_vault_root(dest)?
    } else {
        vault::resolve_new_vault_root(dest)?
    };
    // Compare canonical forms: APFS is case-insensitive and /tmp is a symlink,
    // and a marker spelled differently from the running root would later slip
    // past relocate's "into itself" checks.
    let current = vault::normalize_path(&state.vault_path)
        .unwrap_or_else(|_| state.vault_path.clone());
    if root == current {
        return Err("This is already the current vault".into());
    }
    // Fold the WAL into vault.db before leaving: people tend to copy or
    // archive a vault they just left by hand. Best effort — a background
    // writer may add to the WAL again before the restart, which SQLite
    // recovers on the next open anyway.
    {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
            .map_err(|e| format!("Failed to checkpoint database: {}", e))?;
    }
    vault::write_location_checked(config_dir, &root)?;
    std::mem::forget(flight);
    Ok(root)
}

/// Relaunch the app — used after a vault relocation so every service picks up
/// the new location.
#[tauri::command]
pub fn restart_app(app: AppHandle) {
    app.restart();
}

// --- Device Detection ---

#[tauri::command]
pub fn get_detected_devices(
    state: State<AppState>,
) -> Result<Vec<crate::models::activity::DeviceStats>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::activities::get_detected_devices(&conn).map_err(|e| e.to_string())
}

#[derive(serde::Serialize)]
pub struct FilePreviewItem {
    pub path: String,
    pub filename: String,
    pub is_new: bool,
}

#[derive(serde::Serialize)]
pub struct FolderPreview {
    pub folder: String,
    pub files: Vec<FilePreviewItem>,
}

#[derive(serde::Serialize)]
pub struct ScanPreview {
    pub folders: Vec<FolderPreview>,
    pub total_files: usize,
    pub new_files: usize,
}

#[tauri::command]
pub fn preview_watch_folders(state: State<AppState>) -> Result<ScanPreview, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;

    let folders = db::watch_folders::list_paths(&conn).map_err(|e| e.to_string())?;

    let mut preview = ScanPreview {
        folders: Vec::new(),
        total_files: 0,
        new_files: 0,
    };

    for folder in &folders {
        let dir = Path::new(folder);
        if !dir.is_dir() {
            continue;
        }
        let mut folder_preview = FolderPreview {
            folder: folder.clone(),
            files: Vec::new(),
        };

        for path in collect_workout_files(dir) {
            let path_str = path.to_string_lossy().to_string();
            let filename = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            // Check if file is new by computing hash
            let is_new = match fs::read(&path) {
                Ok(bytes) => {
                    let hash = hex::encode(Sha256::digest(&bytes));
                    !db::raw_files::hash_exists(&conn, &hash).unwrap_or(true)
                }
                Err(_) => true,
            };

            preview.total_files += 1;
            if is_new {
                preview.new_files += 1;
            }

            folder_preview.files.push(FilePreviewItem {
                path: path_str,
                filename,
                is_new,
            });
        }

        if !folder_preview.files.is_empty() {
            preview.folders.push(folder_preview);
        }
    }

    Ok(preview)
}

#[derive(serde::Serialize)]
pub struct SuggestedPath {
    pub label: String,
    pub path: String,
    pub exists: bool,
}

#[tauri::command]
pub fn get_suggested_watch_paths() -> Result<Vec<SuggestedPath>, String> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();

    let downloads_path = format!("{}/Downloads", home);
    let candidates = vec![
        // Garmin
        ("Garmin (USB)", "/Volumes/GARMIN/Garmin/Activity"),
        ("Garmin (USB alt)", "/Volumes/GARMIN/GARMIN/Activity"),
        // Wahoo
        ("Wahoo ELEMNT", "/Volumes/ELEMNT/activities"),
        // Coros
        ("COROS (USB)", "/Volumes/COROS/Activity"),
        // Suunto
        ("Suunto (USB)", "/Volumes/SUUNTO/moves"),
        // Polar
        ("Polar (USB)", "/Volumes/POLAR/DATA"),
        // Downloads
        ("Downloads", downloads_path.as_str()),
    ];

    let mut suggestions: Vec<SuggestedPath> = Vec::new();
    for (label, path) in candidates {
        suggestions.push(SuggestedPath {
            label: label.to_string(),
            path: path.to_string(),
            exists: Path::new(path).is_dir(),
        });
    }

    Ok(suggestions)
}

// --- Encryption ---

#[derive(serde::Serialize)]
pub struct EncryptionStatus {
    pub enabled: bool,
    pub locked: bool,
    pub scopes: crypto::EncryptionScopes,
}

#[tauri::command]
pub fn get_encryption_status(state: State<AppState>) -> Result<EncryptionStatus, String> {
    let vault_lock = crypto::read_vault_lock(&state.vault_path)?;
    let key_guard = state.encryption_key.lock().map_err(|e| e.to_string())?;
    let has_key = key_guard.is_some();

    match vault_lock {
        Some(lock) => Ok(EncryptionStatus {
            enabled: true,
            locked: !has_key,
            scopes: lock.scopes,
        }),
        None => Ok(EncryptionStatus {
            enabled: false,
            locked: false,
            scopes: crypto::EncryptionScopes { activities: false, database: false, photos: false },
        }),
    }
}

// The three password commands below are async + spawn_blocking: PBKDF2 alone
// costs ~0.15s (release) to seconds (dev), and bulk file encryption scales
// with the vault. Running them synchronously would execute on the main
// thread, freezing the window (macOS beachball) and never letting the UI
// paint its own busy state.

#[tauri::command]
pub async fn unlock_vault(password: String, app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || unlock_vault_blocking(&password, &app))
        .await
        .map_err(|e| format!("Unlock task failed: {}", e))?
}

/// Derive the key from `password`, verify it against vault.lock, and — if the
/// database scope is encrypted and the vault booted locked — open the real
/// keyed DB, swap out the in-memory placeholder, and start deferred services.
fn unlock_vault_blocking(password: &str, app: &AppHandle) -> Result<(), String> {
    unlock_vault_core(password, &app.state::<AppState>())?;

    // Start the deferred background services now that the key is loaded — for
    // ALL scopes, not just database. Idempotent: a no-op if a plaintext vault
    // already started them at boot.
    crate::start_background_services(app);

    Ok(())
}

/// The AppHandle-free part of unlock, testable against a bare [`AppState`].
fn unlock_vault_core(password: &str, state: &AppState) -> Result<(), String> {
    let mut lock = crypto::read_vault_lock(&state.vault_path)?
        .ok_or_else(|| "No vault.lock found — encryption is not enabled".to_string())?;

    let key = derive_and_verify(password, &lock)?;

    // Open the encrypted database on first unlock of a locked boot. `db_locked`
    // may be set even when the lock's flag says otherwise — a vault whose
    // database was encrypted but whose flag never got written (crash mid-enable)
    // still boots locked, and unlocking it heals the flag.
    {
        let mut db_locked = state.db_locked.lock().map_err(|e| e.to_string())?;
        if *db_locked {
            let real = crate::init_vault_encrypted(&state.vault_path, &key)?;
            {
                let mut conn = state.db.lock().map_err(|e| e.to_string())?;
                *conn = real;
            }
            *db_locked = false;

            // Self-heal: the DB opened with the key, so it really is encrypted
            // — make the lock say so if it didn't.
            if !lock.scopes.database {
                lock.scopes.database = true;
                crypto::write_vault_lock(&state.vault_path, &lock)?;
            }
        }
    }

    // Store the key before resuming, so the resume and any read use it.
    {
        let mut key_guard = state.encryption_key.lock().map_err(|e| e.to_string())?;
        *key_guard = Some(key);
    }

    // Finish any file encryption a crashed `enable` left incomplete: encrypt_all
    // skips `.enc`, so this is a no-op in steady state but guarantees the
    // enabled file scopes are fully encrypted after every unlock.
    //
    // BEST-EFFORT: the password is already verified and the database is open —
    // the unlock has SUCCEEDED. encrypt_all_* is fail-fast, so one unreadable
    // file (chmod 000, cloud placeholder, antivirus lock, a corrupt .enc) must
    // NOT turn every unlock into a permanent lockout. Log and carry on; the
    // straggler is retried on the next unlock.
    if let Err(e) = resume_file_encryption(state, &key, &lock) {
        eprintln!("unlock: file-encryption resume incomplete (will retry): {e}");
    }

    Ok(())
}

/// Re-run bulk file encryption for the enabled file scopes. Idempotent
/// (already-`.enc` files are skipped), so it both completes a crashed enable
/// and re-protects any file that somehow landed in plaintext.
fn resume_file_encryption(
    state: &AppState,
    key: &[u8; 32],
    lock: &crypto::VaultLock,
) -> Result<(), String> {
    if !lock.scopes.activities && !lock.scopes.photos {
        return Ok(());
    }
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    if lock.scopes.activities {
        reconcile_raw_paths(&conn, &state.vault_path)?;
        crypto::encrypt_all_raw_files(key, &state.vault_path, &mut |old, new| {
            db::raw_files::update_path(&conn, old, new)
                .map_err(|e| format!("Failed to update raw_file path: {}", e))
        })?;
    }
    if lock.scopes.photos {
        reconcile_photo_paths(&conn, &state.vault_path)?;
        crypto::encrypt_all_photos(key, &state.vault_path, &mut |old, new| {
            db::photos::update_path(&conn, old, new)
                .map_err(|e| format!("Failed to update photo path: {}", e))
        })?;
    }
    Ok(())
}

/// Derive the vault key from a password and confirm it matches the lock's
/// verifier. Shared by unlock and disable flows.
fn derive_and_verify(password: &str, lock: &crypto::VaultLock) -> Result<[u8; 32], String> {
    let salt_bytes = hex::decode(&lock.salt)
        .map_err(|e| format!("Invalid salt in vault.lock: {}", e))?;
    let mut salt = [0u8; 32];
    if salt_bytes.len() != 32 {
        return Err("Invalid salt length in vault.lock".to_string());
    }
    salt.copy_from_slice(&salt_bytes);

    let key = crypto::derive_key(password, &salt);

    let verifier_bytes = hex::decode(&lock.verifier)
        .map_err(|e| format!("Invalid verifier in vault.lock: {}", e))?;
    let nonce_bytes = hex::decode(&lock.nonce)
        .map_err(|e| format!("Invalid nonce in vault.lock: {}", e))?;
    let mut nonce = [0u8; 12];
    if nonce_bytes.len() != 12 {
        return Err("Invalid nonce length in vault.lock".to_string());
    }
    nonce.copy_from_slice(&nonce_bytes);

    if !crypto::verify_password(&key, &verifier_bytes, &nonce) {
        return Err("Wrong password".to_string());
    }
    Ok(key)
}

/// Repair encryption crash drift: DB rows whose file sits on disk under the
/// other extension (`x` ↔ `x.enc`) are re-pointed at the real file.
fn reconcile_raw_paths(conn: &rusqlite::Connection, vault_path: &Path) -> Result<(), String> {
    let db_paths = db::raw_files::all_paths(conn).map_err(|e| e.to_string())?;
    for (old_path, new_path) in crypto::reconcile_paths(vault_path, &db_paths) {
        db::raw_files::update_path(conn, &old_path, &new_path)
            .map_err(|e| format!("Failed to reconcile raw_file path: {}", e))?;
    }
    Ok(())
}

// ─── Vault crypto failure model ─────────────────────────────────────────────
//
// enable / disable / unlock mutate three things with no transaction across
// them: the on-disk files, the live DB connection (SQLCipher swaps it), and
// vault.lock. A failure between steps must still leave ONE of three coherent
// states, never a half-state that loses data or serves an empty DB:
//
//   PLAINTEXT (disabled): no vault.lock, key = None, db_locked = false, the
//     live connection is a real plaintext DB. Reached only by a fully-successful
//     disable, or the full-undo of a fresh enable that encrypted nothing.
//   LOCKED (enabled, needs unlock): vault.lock present, db_locked = true,
//     key = None, vault.db is SQLCipher-encrypted (the connection may be a
//     placeholder). A restart boots locked; the UnlockModal re-derives the key
//     and heals. Every "the DB is encrypted but we can't hold a keyed
//     connection" error path settles here via settle_locked_encrypted().
//   UNLOCKED (enabled, in use): vault.lock present, key = Some, db_locked =
//     false, the connection is keyed. The normal running state.
//
// Two invariants keep failures from destroying data:
//   INV-1  vault.lock (the ONLY copy of the salt) is removed ONLY when nothing
//          on disk is ciphertext — a plaintext-readable DB and no encrypted
//          files. While any ciphertext remains, the lock stays.
//   INV-2  no error path is left serving an in-memory placeholder while
//          reporting unlocked: if the DB is encrypted it settles to LOCKED
//          (key cleared), otherwise it reopens a real plaintext connection.

/// Force the LOCKED terminal state: the DB is encrypted on disk but no keyed
/// connection could be held. Writes `scopes` (which MUST reflect what is
/// actually ciphertext on disk — the database plus any still-encrypted file
/// scopes, so the next unlock's resume pass finishes them instead of skipping
/// them), boots locked, and clears the in-memory key so status reports
/// locked=true (the UnlockModal appears and a restart+unlock re-opens the keyed
/// DB). Never removes the lock (INV-1).
fn settle_locked_encrypted(
    state: &AppState,
    lock: &mut crypto::VaultLock,
    scopes: crypto::EncryptionScopes,
) {
    lock.scopes = crypto::EncryptionScopes { database: true, ..scopes };
    let _ = crypto::write_vault_lock(&state.vault_path, lock);
    if let Ok(mut dl) = state.db_locked.lock() {
        *dl = true;
    }
    if let Ok(mut k) = state.encryption_key.lock() {
        *k = None;
    }
}

#[tauri::command]
pub async fn enable_encryption(
    password: String,
    scopes: crypto::EncryptionScopes,
    app: AppHandle,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || enable_encryption_blocking(&password, scopes, &app))
        .await
        .map_err(|e| format!("Encryption task failed: {}", e))?
}

fn enable_encryption_blocking(
    password: &str,
    scopes: crypto::EncryptionScopes,
    app: &AppHandle,
) -> Result<(), String> {
    enable_encryption_core(password, scopes, &app.state::<AppState>())
}

/// The AppHandle-free part of enable, testable against a bare [`AppState`].
fn enable_encryption_core(
    password: &str,
    scopes: crypto::EncryptionScopes,
    state: &AppState,
) -> Result<(), String> {
    // The heaviest vault mutation of all — the DB is re-encrypted in place
    // and every raw/photo file renamed to .enc — must not interleave with a
    // backup walking those same directories (or a restore/relocation moving
    // them). Claimed before anything is written, so a refusal changes nothing.
    let _flight = state
        .vault_flight
        .try_begin()
        .ok_or("Another vault operation is already in progress")?;

    if crypto::read_vault_lock(&state.vault_path)?.is_some() {
        return Err("Encryption is already enabled".to_string());
    }
    if !scopes.any() {
        return Err("Select at least one thing to encrypt".to_string());
    }
    // A pre-restore-* quarantine holds a complete copy of a replaced vault
    // that no scope's bulk pass touches — enabling around it would leave the
    // old data in the clear INSIDE the "encrypted" vault (and relocate would
    // carry it along). It's data the user chose to keep, so never scrub it
    // silently: refuse and let them delete it or move it out first.
    let leftovers = crate::export::vault_backup::list_pre_restore_dirs(&state.vault_path);
    if !leftovers.is_empty() {
        return Err(format!(
            "The vault still contains data preserved by a restore ({}) that encryption would not cover. Delete it or move it out of the vault first",
            leftovers.join(", ")
        ));
    }

    let salt = crypto::generate_salt();
    let key = crypto::derive_key(password, &salt);
    let (verifier, nonce) = crypto::create_verifier(&key)?;

    // Store the key in memory BEFORE writing the lock's scopes on. A concurrent
    // writer gates on encryption_key_for = (lock scope AND in-memory key): if
    // the scope were on while the key were still None it would treat the vault
    // as plaintext and write an UNENCRYPTED file into a vault the user just
    // encrypted. Ordering key-first means every writer that sees the scope also
    // sees the key and encrypts; one that runs before the lock write sees no
    // scope and writes plaintext, which the bulk pass below then encrypts.
    {
        let mut key_guard = state.encryption_key.lock().map_err(|e| e.to_string())?;
        *key_guard = Some(key);
    }

    // Persist recovery data (salt/verifier) FIRST with every scope still false,
    // so a crash mid-enable never claims a scope whose data isn't encrypted yet.
    let mut lock = crypto::VaultLock {
        salt: hex::encode(salt),
        verifier: hex::encode(verifier),
        nonce: hex::encode(nonce),
        created_at: chrono::Utc::now().to_rfc3339(),
        scopes: crypto::EncryptionScopes { activities: false, database: false, photos: false },
    };
    crypto::write_vault_lock(&state.vault_path, &lock)?;

    // Database FIRST. It swaps the live connection, so its failure would leave a
    // dead placeholder that can't drive a file rollback — doing it before any
    // file is touched makes the plaintext-DB case a trivial rollback.
    if scopes.database {
        if let Err(e) = encrypt_database_in_place(&state, &key) {
            // The disk state — plaintext_db_readable — is the discriminator, NOT
            // whether the reopen happens to succeed: gating the undo on
            // init_vault() would settle a still-PLAINTEXT vault to LOCKED and
            // then fail every unlock (SQLCipher rejecting the plaintext file),
            // permanently locking the user out of intact data.
            if crate::plaintext_db_readable(&state.vault_path) {
                // DB is plaintext (INV-1: DB runs before any file, so nothing is
                // ciphertext). Removing the lock is correct. Reopen a working
                // connection — init_vault, else a bare open (plaintext_db_readable
                // just proved the file opens), so the session is never left on an
                // empty placeholder reporting disabled (INV-2).
                let conn = crate::init_vault(&state.vault_path)
                    .or_else(|_| {
                        rusqlite::Connection::open(state.vault_path.join("vault.db"))
                            .map_err(|e| e.to_string())
                    })
                    .unwrap_or_else(|_| rusqlite::Connection::open_in_memory().unwrap());
                *state.db.lock().map_err(|e| e.to_string())? = conn;
                let _ = crypto::remove_vault_lock(&state.vault_path);
                *state.encryption_key.lock().map_err(|e| e.to_string())? = None;
            } else {
                // DB is encrypted, only the reopen failed → LOCKED.
                settle_locked_encrypted(&state, &mut lock, scopes);
            }
            return Err(e);
        }
        lock.scopes.database = true;
        crypto::write_vault_lock(&state.vault_path, &lock)?;
    }

    // Files next. Flip their scopes ON in the lock BEFORE encrypting, so a
    // concurrent writer sees scope-on + the already-stored key and encrypts
    // (rather than writing plaintext).
    if scopes.activities || scopes.photos {
        lock.scopes.activities = scopes.activities;
        lock.scopes.photos = scopes.photos;
        crypto::write_vault_lock(&state.vault_path, &lock)?;

        // BEST-EFFORT, like the unlock resume pass. The vault is now genuinely
        // enabled (lock written, key held, DB encrypted if that scope ran), so a
        // file straggler must NOT trigger a rollback: rolling back here used to
        // delete vault.lock (the salt) while ciphertext could remain on disk —
        // permanent data loss. Any file left plaintext is re-encrypted by the
        // next unlock's resume pass (encrypt_all skips `.enc`); the lock and key
        // stay put so nothing is stranded.
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        if scopes.activities {
            if let Err(e) = reconcile_raw_paths(&conn, &state.vault_path).and_then(|_| {
                crypto::encrypt_all_raw_files(&key, &state.vault_path, &mut |old, new| {
                    db::raw_files::update_path(&conn, old, new)
                        .map_err(|e| format!("Failed to update raw_file path: {}", e))
                })
            }) {
                eprintln!("enable: raw-file encryption incomplete (resumes on unlock): {e}");
            }
        }
        if scopes.photos {
            if let Err(e) = reconcile_photo_paths(&conn, &state.vault_path).and_then(|_| {
                crypto::encrypt_all_photos(&key, &state.vault_path, &mut |old, new| {
                    db::photos::update_path(&conn, old, new)
                        .map_err(|e| format!("Failed to update photo path: {}", e))
                })
            }) {
                eprintln!("enable: photo encryption incomplete (resumes on unlock): {e}");
            }
        }
    }

    Ok(())
}

/// Encrypt the open vault database in place: checkpoint + close the live
/// connection, run the SQLCipher export, and reopen keyed. Holds the DB lock
/// throughout so no query runs against a half-swapped file.
fn encrypt_database_in_place(state: &AppState, key: &[u8; 32]) -> Result<(), String> {
    let mut conn = state.db.lock().map_err(|e| e.to_string())?;
    let _ = conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()));
    *conn = rusqlite::Connection::open_in_memory().map_err(|e| e.to_string())?;

    let db_path = state.vault_path.join("vault.db");
    db::dbcrypt::encrypt_database(&db_path, key)?;
    *conn = crate::init_vault_encrypted(&state.vault_path, key)?;
    Ok(())
}

/// Reverse of [`encrypt_database_in_place`].
fn decrypt_database_in_place(state: &AppState, key: &[u8; 32]) -> Result<(), String> {
    let mut conn = state.db.lock().map_err(|e| e.to_string())?;
    let _ = conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()));
    *conn = rusqlite::Connection::open_in_memory().map_err(|e| e.to_string())?;

    let db_path = state.vault_path.join("vault.db");
    db::dbcrypt::decrypt_database(&db_path, key)?;
    *conn = crate::init_vault(&state.vault_path)?;
    Ok(())
}

/// Decrypt every `.enc` raw file and photo, deliberately ignoring which scopes
/// the lock claims: disable discards the key forever, so ANY ciphertext left
/// behind (e.g. a file encrypted outside its scope by an older build) would be
/// unrecoverable. Non-`.enc` files are skipped, so out-of-scope sweeps are
/// cheap no-ops.
fn decrypt_all_vault_files(
    conn: &rusqlite::Connection,
    vault_path: &Path,
    key: &[u8; 32],
) -> Result<(), String> {
    reconcile_raw_paths(conn, vault_path)?;
    crypto::decrypt_all_raw_files(key, vault_path, &mut |old, new| {
        db::raw_files::update_path(conn, old, new)
            .map_err(|e| format!("Failed to update raw_file path: {}", e))
    })?;
    reconcile_photo_paths(conn, vault_path)?;
    crypto::decrypt_all_photos(key, vault_path, &mut |old, new| {
        db::photos::update_path(conn, old, new)
            .map_err(|e| format!("Failed to update photo path: {}", e))
    })?;
    Ok(())
}

/// Photo-path crash-drift repair (the photo analogue of reconcile_raw_paths).
fn reconcile_photo_paths(conn: &rusqlite::Connection, vault_path: &Path) -> Result<(), String> {
    let db_paths = db::photos::all_paths(conn).map_err(|e| e.to_string())?;
    for (old_path, new_path) in crypto::reconcile_paths(vault_path, &db_paths) {
        db::photos::update_path(conn, &old_path, &new_path)
            .map_err(|e| format!("Failed to reconcile photo path: {}", e))?;
    }
    Ok(())
}

#[tauri::command]
pub async fn disable_encryption(password: String, app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || disable_encryption_blocking(&password, &app))
        .await
        .map_err(|e| format!("Decryption task failed: {}", e))?
}

fn disable_encryption_blocking(password: &str, app: &AppHandle) -> Result<(), String> {
    disable_encryption_core(password, &app.state::<AppState>())
}

/// The AppHandle-free part of disable, testable against a bare [`AppState`].
fn disable_encryption_core(password: &str, state: &AppState) -> Result<(), String> {
    // Same vault-wide gate as enable: the decrypt sweep renames every .enc
    // back while a concurrent backup/restore/relocation would be walking or
    // moving those same directories.
    let _flight = state
        .vault_flight
        .try_begin()
        .ok_or("Another vault operation is already in progress")?;

    let mut lock = crypto::read_vault_lock(&state.vault_path)?
        .ok_or_else(|| "Encryption is not enabled".to_string())?;

    let key = derive_and_verify(password, &lock)?;
    let orig_scopes = lock.scopes;

    // Turn every scope OFF in the lock up front (keeping it on disk for its
    // salt/verifier so a crash mid-disable is still resumable). encryption_key_for
    // now returns None, so a concurrent attach/import writes PLAINTEXT — without
    // this a writer waking after the decrypt sweep but before the lock is
    // removed would see scopes still on + the key still loaded, encrypt a NEW
    // .enc, and that file becomes unrecoverable the moment the key is discarded.
    lock.scopes = crypto::EncryptionScopes { activities: false, database: false, photos: false };
    crypto::write_vault_lock(&state.vault_path, &lock)?;

    // Database: turn the connection back to plaintext (uses the still-held key).
    if orig_scopes.database {
        if let Err(e) = decrypt_database_in_place(&state, &key) {
            // The DB is still encrypted and the connection a dead placeholder,
            // and the files are still .enc (the sweep hasn't run) → settle to
            // LOCKED with the ORIGINAL scopes, so a restart+unlock decrypts the
            // DB and the resume pass keeps the file scopes consistent. Marking
            // only database would leave the lock claiming the files are plaintext
            // while their .enc still sit on disk.
            settle_locked_encrypted(&state, &mut lock, orig_scopes);
            return Err(e);
        }
        let mut db_locked = state.db_locked.lock().map_err(|e| e.to_string())?;
        *db_locked = false;
    }

    // File sweep (scope-agnostic: decrypts every .enc regardless of scope).
    let sweep = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        decrypt_all_vault_files(&conn, &state.vault_path, &key)
    };
    if let Err(e) = sweep {
        // Some .enc remain. Restore the FILE scopes in the lock so it reflects
        // the ciphertext still on disk (database stays false — it's plaintext
        // now); the lock+key are kept, so retrying Disable re-runs the
        // scope-agnostic sweep and completes. Never remove the lock while
        // ciphertext remains (INV-1).
        lock.scopes = crypto::EncryptionScopes {
            activities: orig_scopes.activities,
            photos: orig_scopes.photos,
            database: false,
        };
        let _ = crypto::write_vault_lock(&state.vault_path, &lock);
        return Err(e);
    }

    // Remove vault.lock only after everything is back to plaintext — while it
    // exists, a partial disable can always be resumed with the password. If the
    // remove fails, propagate the error and KEEP the key: returning Ok while a
    // stray lock lingers (and the key was cleared) would flip status to
    // encrypted+locked over a fully-plaintext vault and pop the UnlockModal.
    // With the key kept, status stays enabled+unlocked and a retry completes.
    crypto::remove_vault_lock(&state.vault_path)?;

    let mut key_guard = state.encryption_key.lock().map_err(|e| e.to_string())?;
    *key_guard = None;

    Ok(())
}

#[tauri::command]
pub fn restart_watcher(
    app_handle: tauri::AppHandle,
    state: State<AppState>,
) -> Result<(), String> {
    // Stop existing watcher (drop it)
    {
        let mut wh = state.watcher_handle.lock().map_err(|e| e.to_string())?;
        *wh = None;
    }

    // Watch folders are paused (see crate::WATCH_FOLDERS_ENABLED) — stopping
    // above is fine, restarting is not.
    if !crate::WATCH_FOLDERS_ENABLED {
        return Ok(());
    }

    // Read current watch folders
    let paths = watcher::get_watch_paths_from_db(&app_handle);

    if paths.is_empty() {
        return Ok(());
    }

    // Start new watcher
    let w = watcher::start_watching(app_handle, paths)?;
    let mut wh = state.watcher_handle.lock().map_err(|e| e.to_string())?;
    *wh = Some(w);

    Ok(())
}

/// Manual update check (Settings → General). The only network call happens
/// on the user's click — see `crate::updates` for the privacy contract.
#[tauri::command]
pub async fn check_for_updates() -> Result<crate::models::update::UpdateCheck, String> {
    tauri::async_runtime::spawn_blocking(crate::updates::check)
        .await
        .map_err(|e| format!("Update check task failed: {}", e))?
}

/// Download the signed update bundle, install it and restart. Only reachable
/// from the Settings row after an explicit manual check — this never runs on
/// its own. The updater plugin re-reads `latest.json` itself and verifies the
/// minisign signature against the public key baked into tauri.conf.json
/// before touching the installed app; on success `restart()` never returns.
#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    use tauri::Emitter;
    use tauri_plugin_updater::UpdaterExt;
    let update = app
        .updater()
        .map_err(|e| format!("Updater unavailable: {}", e))?
        .check()
        .await
        .map_err(|e| format!("Update lookup failed: {}", e))?
        // The Settings row only shows the install button after the manual
        // check saw a newer release on api.github.com; the updater re-checks
        // against latest.json, which can lag a freshly published release.
        .ok_or_else(|| {
            "The update isn't downloadable yet — retry in a few minutes, \
             or use the release page"
                .to_string()
        })?;
    let progress = app.clone();
    let mut downloaded: u64 = 0;
    update
        .download_and_install(
            move |chunk, total| {
                downloaded += chunk as u64;
                let _ = progress.emit(
                    "update:progress",
                    serde_json::json!({ "downloaded": downloaded, "total": total }),
                );
            },
            || {},
        )
        .await
        .map_err(|e| format!("Update install failed: {}", e))?;
    app.restart();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::raw_file::RawFile;
    use std::sync::{Arc, Mutex};

    fn test_state(vault: &Path) -> AppState {
        test_state_with(vault, crate::db::test_db())
    }

    /// The allowlist is the only path from a UI string to the filesystem —
    /// every id maps to a bundled resource, everything else is rejected.
    #[test]
    fn legal_doc_allowlist() {
        assert_eq!(legal_doc_file("license"), Some("LICENSE"));
        assert_eq!(legal_doc_file("exception"), Some("LICENSE-PLUGIN-EXCEPTION.md"));
        assert_eq!(legal_doc_file("notices"), Some("THIRD-PARTY-NOTICES.md"));
        assert_eq!(legal_doc_file("../../etc/passwd"), None);
        assert_eq!(legal_doc_file(""), None);
    }

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

    /// The live connection answers a schema query (i.e. it's not a dead
    /// placeholder and, for an encrypted DB, the key matched).
    fn db_usable(state: &AppState) -> bool {
        state
            .db
            .lock()
            .unwrap()
            .query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0))
            .is_ok()
    }

    /// vault.db opens and reads WITHOUT a key — i.e. it is plaintext SQLite.
    fn plaintext_db_opens(vault: &Path) -> bool {
        match rusqlite::Connection::open(vault.join("vault.db")) {
            Ok(c) => c
                .query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0))
                .is_ok(),
            Err(_) => false,
        }
    }

    fn lock_with(activities: bool, photos: bool) -> crypto::VaultLock {
        crypto::VaultLock {
            salt: String::new(),
            verifier: String::new(),
            nonce: String::new(),
            created_at: String::new(),
            scopes: crypto::EncryptionScopes { activities, database: false, photos },
        }
    }

    /// resume_file_encryption finishes a crashed enable: a plaintext raw file
    /// left on disk gets encrypted and its DB row repointed, and a second run
    /// is a no-op (idempotent).
    #[test]
    fn resume_encrypts_leftover_plaintext_and_is_idempotent() {
        let vault = std::env::temp_dir().join("syz_resume_test");
        let _ = std::fs::remove_dir_all(&vault);
        std::fs::create_dir_all(vault.join("raw")).unwrap();
        std::fs::write(vault.join("raw/a.fit"), b"workout").unwrap();

        let state = test_state(&vault);
        {
            let conn = state.db.lock().unwrap();
            crate::db::raw_files::insert_raw_file(
                &conn,
                &RawFile {
                    id: "rf-1".into(),
                    activity_id: None,
                    path_in_vault: "raw/a.fit".into(),
                    original_path: None,
                    format: "fit".into(),
                    hash_sha256: "h".into(),
                    imported_at: String::new(),
                    parse_status: "ok".into(),
                    failure_reason: None,
                },
            )
            .unwrap();
        }

        let key = crypto::derive_key("pw", &[1u8; 32]);
        resume_file_encryption(&state, &key, &lock_with(true, false)).unwrap();

        // File encrypted on disk, DB row repointed to the .enc path.
        assert!(!vault.join("raw/a.fit").exists());
        assert!(vault.join("raw/a.fit.enc").exists());
        let stored: String = state
            .db
            .lock()
            .unwrap()
            .query_row("SELECT path_in_vault FROM raw_file WHERE id='rf-1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(stored, "raw/a.fit.enc");

        // Second run: nothing new to encrypt.
        resume_file_encryption(&state, &key, &lock_with(true, false)).unwrap();
        assert!(vault.join("raw/a.fit.enc").exists());

        let _ = std::fs::remove_dir_all(&vault);
    }

    /// The disable sweep decrypts ALL ciphertext regardless of what the lock's
    /// scopes claim: an out-of-scope `.enc` (left by an older build that
    /// encrypted without gating on scope) must not survive the key being
    /// discarded forever.
    #[test]
    fn decrypt_all_vault_files_ignores_lock_scopes() {
        let vault = std::env::temp_dir().join("syz_disable_sweep_test");
        let _ = std::fs::remove_dir_all(&vault);
        std::fs::create_dir_all(vault.join("raw")).unwrap();
        std::fs::create_dir_all(vault.join("photos/act-1")).unwrap();

        let key = [4u8; 32];

        // Ciphertext on disk: a raw file (with its DB row on the .enc path)
        // and a photo.
        std::fs::write(vault.join("raw/a.fit"), b"workout").unwrap();
        crypto::encrypt_file(&key, &vault.join("raw/a.fit")).unwrap();
        std::fs::write(vault.join("photos/act-1/p.jpg"), b"pixels").unwrap();
        crypto::encrypt_file(&key, &vault.join("photos/act-1/p.jpg")).unwrap();

        let state = test_state(&vault);
        let conn = state.db.lock().unwrap();
        crate::db::raw_files::insert_raw_file(
            &conn,
            &RawFile {
                id: "rf-1".into(),
                activity_id: None,
                path_in_vault: "raw/a.fit.enc".into(),
                original_path: None,
                format: "fit".into(),
                hash_sha256: "h".into(),
                imported_at: String::new(),
                parse_status: "ok".into(),
                failure_reason: None,
            },
        )
        .unwrap();

        decrypt_all_vault_files(&conn, &vault, &key).unwrap();

        // Everything is plaintext again and the raw_file row is repointed.
        assert_eq!(std::fs::read(vault.join("raw/a.fit")).unwrap(), b"workout");
        assert!(!vault.join("raw/a.fit.enc").exists());
        assert_eq!(std::fs::read(vault.join("photos/act-1/p.jpg")).unwrap(), b"pixels");
        assert!(!vault.join("photos/act-1/p.jpg.enc").exists());
        let stored: String = conn
            .query_row("SELECT path_in_vault FROM raw_file WHERE id='rf-1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(stored, "raw/a.fit");

        drop(conn);
        let _ = std::fs::remove_dir_all(&vault);
    }

    /// With no file scopes enabled, resume touches nothing.
    #[test]
    fn resume_noop_when_no_file_scopes() {
        let vault = std::env::temp_dir().join("syz_resume_noop_test");
        let _ = std::fs::remove_dir_all(&vault);
        std::fs::create_dir_all(vault.join("raw")).unwrap();
        std::fs::write(vault.join("raw/b.fit"), b"data").unwrap();

        let state = test_state(&vault);
        let key = crypto::derive_key("pw", &[2u8; 32]);
        resume_file_encryption(&state, &key, &lock_with(false, false)).unwrap();

        // Untouched — database-only scope leaves raw files alone.
        assert!(vault.join("raw/b.fit").exists());
        assert!(!vault.join("raw/b.fit.enc").exists());

        let _ = std::fs::remove_dir_all(&vault);
    }

    /// Full enable → disable orchestration against a real on-disk vault with
    /// all three scopes: lock lifecycle, file ciphertext, SQLCipher swap of
    /// the live connection, key storage, and the guard clauses around them.
    #[test]
    fn enable_disable_orchestration_roundtrip_all_scopes() {
        let vault = std::env::temp_dir().join("syz_orch_roundtrip");
        let _ = std::fs::remove_dir_all(&vault);
        std::fs::create_dir_all(vault.join("raw")).unwrap();
        std::fs::create_dir_all(vault.join("photos/act-1")).unwrap();
        std::fs::write(vault.join("raw/a.fit"), b"workout").unwrap();
        std::fs::write(vault.join("photos/act-1/p.jpg"), b"pixels").unwrap();

        // A real vault.db file — the database scope encrypts it in place.
        let conn = crate::init_vault(&vault).unwrap();
        let state = test_state_with(&vault, conn);
        let scopes =
            crypto::EncryptionScopes { activities: true, database: true, photos: true };

        // No scopes selected → rejected before anything is written.
        let none =
            crypto::EncryptionScopes { activities: false, database: false, photos: false };
        assert!(enable_encryption_core("pw", none, &state).is_err());
        assert!(crypto::read_vault_lock(&vault).unwrap().is_none());

        enable_encryption_core("pw", scopes, &state).unwrap();

        // Lock has all scopes (database flipped only after the actual swap).
        let lock = crypto::read_vault_lock(&vault).unwrap().unwrap();
        assert!(lock.scopes.activities && lock.scopes.database && lock.scopes.photos);
        // Files are ciphertext, the DB no longer opens plaintext, yet the live
        // (keyed) connection still works and the key is held for new writes.
        assert!(vault.join("raw/a.fit.enc").exists());
        assert!(!vault.join("raw/a.fit").exists());
        assert!(vault.join("photos/act-1/p.jpg.enc").exists());
        assert!(!plaintext_db_opens(&vault));
        assert!(db_usable(&state));
        assert!(state.encryption_key.lock().unwrap().is_some());

        // Enabling twice is rejected.
        let err = enable_encryption_core("pw", scopes, &state).unwrap_err();
        assert!(err.contains("already enabled"), "{}", err);

        // A wrong password can't disable, and tears nothing down.
        let err = disable_encryption_core("wrong", &state).unwrap_err();
        assert!(err.contains("Wrong password"), "{}", err);
        assert!(crypto::read_vault_lock(&vault).unwrap().is_some());
        assert!(vault.join("raw/a.fit.enc").exists());

        disable_encryption_core("pw", &state).unwrap();

        // Everything is plaintext again, the lock is gone, the key discarded.
        assert!(crypto::read_vault_lock(&vault).unwrap().is_none());
        assert_eq!(std::fs::read(vault.join("raw/a.fit")).unwrap(), b"workout");
        assert!(!vault.join("raw/a.fit.enc").exists());
        assert_eq!(
            std::fs::read(vault.join("photos/act-1/p.jpg")).unwrap(),
            b"pixels"
        );
        assert!(plaintext_db_opens(&vault));
        assert!(db_usable(&state));
        assert!(state.encryption_key.lock().unwrap().is_none());

        let _ = std::fs::remove_dir_all(&vault);
    }

    /// enable/disable rename every raw/photo file and rewrite vault.db — they
    /// share the vault-wide gate with backup/restore/relocate, are refused
    /// side-effect-free while it's held, and proceed once it frees.
    #[test]
    fn encryption_toggles_are_refused_while_another_vault_operation_runs() {
        let vault = std::env::temp_dir().join("syz_toggle_flight");
        let _ = std::fs::remove_dir_all(&vault);
        std::fs::create_dir_all(vault.join("raw")).unwrap();
        std::fs::write(vault.join("raw/a.fit"), b"workout").unwrap();
        let conn = crate::init_vault(&vault).unwrap();
        let state = test_state_with(&vault, conn);
        let scopes =
            crypto::EncryptionScopes { activities: true, database: false, photos: false };

        {
            let _running = state.vault_flight.try_begin().unwrap();
            let err = enable_encryption_core("pw", scopes, &state).unwrap_err();
            assert!(err.contains("already in progress"), "got: {}", err);
            // Side-effect-free refusal: no lock, no key, file untouched.
            assert!(crypto::read_vault_lock(&vault).unwrap().is_none());
            assert!(state.encryption_key.lock().unwrap().is_none());
            assert!(vault.join("raw/a.fit").exists());
        }
        // Gate freed — enable proceeds, and disable hits the same gate.
        enable_encryption_core("pw", scopes, &state).unwrap();
        {
            let _running = state.vault_flight.try_begin().unwrap();
            let err = disable_encryption_core("pw", &state).unwrap_err();
            assert!(err.contains("already in progress"), "got: {}", err);
            // Still fully enabled: lock intact, ciphertext untouched.
            assert!(crypto::read_vault_lock(&vault).unwrap().is_some());
            assert!(vault.join("raw/a.fit.enc").exists());
        }
        disable_encryption_core("pw", &state).unwrap();
        assert!(crypto::read_vault_lock(&vault).unwrap().is_none());
        assert!(vault.join("raw/a.fit").exists());

        let _ = std::fs::remove_dir_all(&vault);
    }

    /// A pre-restore-* quarantine is a complete copy of a replaced vault that
    /// no encryption pass manages: enable must refuse while one exists —
    /// otherwise the "encrypted" vault carries the old data in plaintext —
    /// and proceed normally once the user has dealt with it.
    #[test]
    fn enable_refuses_while_a_pre_restore_dir_exists() {
        let vault = std::env::temp_dir().join("syz_enable_prerestore");
        let _ = std::fs::remove_dir_all(&vault);
        std::fs::create_dir_all(vault.join("raw")).unwrap();
        let conn = crate::init_vault(&vault).unwrap();
        let state = test_state_with(&vault, conn);
        let scopes =
            crypto::EncryptionScopes { activities: true, database: false, photos: false };

        // The quarantine a restore left behind, holding the old vault.
        let quarantine = vault.join("pre-restore-20260708-090000");
        std::fs::create_dir_all(&quarantine).unwrap();
        std::fs::write(quarantine.join("vault.db"), b"old plaintext vault").unwrap();

        let err = enable_encryption_core("pw", scopes, &state).unwrap_err();
        assert!(err.contains("preserved by a restore"), "got: {}", err);
        // Nothing was enabled: no lock, no key, quarantine untouched.
        assert!(crypto::read_vault_lock(&vault).unwrap().is_none());
        assert!(state.encryption_key.lock().unwrap().is_none());
        assert!(quarantine.join("vault.db").exists());

        // User moved it out — enable proceeds.
        std::fs::remove_dir_all(&quarantine).unwrap();
        enable_encryption_core("pw", scopes, &state).unwrap();
        assert!(crypto::read_vault_lock(&vault).unwrap().is_some());

        let _ = std::fs::remove_dir_all(&vault);
    }

    /// The crash window enable defends against: the DB got encrypted but the
    /// lock's database flag never flipped, and the app rebooted locked with a
    /// placeholder connection. Unlock must open the keyed DB, self-heal the
    /// flag, finish leftover file encryption, and be idempotent.
    #[test]
    fn unlock_heals_crashed_enable_and_resumes_files() {
        let vault = std::env::temp_dir().join("syz_orch_unlock_heal");
        let _ = std::fs::remove_dir_all(&vault);
        std::fs::create_dir_all(vault.join("raw")).unwrap();
        std::fs::write(vault.join("raw/a.fit"), b"workout").unwrap();

        let conn = crate::init_vault(&vault).unwrap();
        let state = test_state_with(&vault, conn);
        let scopes =
            crypto::EncryptionScopes { activities: true, database: true, photos: false };
        enable_encryption_core("pw", scopes, &state).unwrap();

        // Simulate the crash + locked reboot: flag rolled back, in-memory
        // placeholder connection, no key in memory, plus a plaintext straggler
        // the crashed enable never reached.
        let mut lock = crypto::read_vault_lock(&vault).unwrap().unwrap();
        lock.scopes.database = false;
        crypto::write_vault_lock(&vault, &lock).unwrap();
        *state.db.lock().unwrap() = rusqlite::Connection::open_in_memory().unwrap();
        *state.db_locked.lock().unwrap() = true;
        *state.encryption_key.lock().unwrap() = None;
        std::fs::write(vault.join("raw/late.fit"), b"late").unwrap();

        // Wrong password: still locked, still no key.
        let err = unlock_vault_core("nope", &state).unwrap_err();
        assert!(err.contains("Wrong password"), "{}", err);
        assert!(*state.db_locked.lock().unwrap());
        assert!(state.encryption_key.lock().unwrap().is_none());

        unlock_vault_core("pw", &state).unwrap();

        // Keyed DB swapped in, flag healed, straggler encrypted, key held.
        assert!(!*state.db_locked.lock().unwrap());
        assert!(db_usable(&state));
        assert!(crypto::read_vault_lock(&vault).unwrap().unwrap().scopes.database);
        assert!(!vault.join("raw/late.fit").exists());
        assert!(vault.join("raw/late.fit.enc").exists());
        assert!(state.encryption_key.lock().unwrap().is_some());

        // A second unlock is a no-op, not a re-encryption or an error.
        unlock_vault_core("pw", &state).unwrap();
        assert!(db_usable(&state));

        let _ = std::fs::remove_dir_all(&vault);
    }

    /// Unlock on a vault that was never encrypted fails cleanly.
    #[test]
    fn unlock_requires_vault_lock() {
        let vault = std::env::temp_dir().join("syz_orch_unlock_nolock");
        let _ = std::fs::remove_dir_all(&vault);
        std::fs::create_dir_all(&vault).unwrap();

        let state = test_state(&vault);
        let err = unlock_vault_core("pw", &state).unwrap_err();
        assert!(err.contains("encryption is not enabled"), "{}", err);

        let _ = std::fs::remove_dir_all(&vault);
    }

    /// Enable makes the key available through encryption_key_for the moment the
    /// scope is on — never scope-on-but-key-None, which would make a concurrent
    /// writer treat the vault as plaintext and leak an unencrypted file.
    #[test]
    fn enable_exposes_key_for_writers_under_its_scope() {
        let vault = std::env::temp_dir().join("syz_enable_keyfor");
        let _ = std::fs::remove_dir_all(&vault);
        std::fs::create_dir_all(vault.join("photos")).unwrap();
        let conn = crate::init_vault(&vault).unwrap();
        let state = test_state_with(&vault, conn);

        enable_encryption_core(
            "pw",
            crypto::EncryptionScopes { activities: false, database: false, photos: true },
            &state,
        )
        .unwrap();

        // A photo writer gets the key (scope on + key present); a raw-file
        // writer, whose scope is off, must get None (else it strands ciphertext).
        assert!(state.encryption_key_for(|s| s.photos).unwrap().is_some());
        assert!(state.encryption_key_for(|s| s.activities).unwrap().is_none());

        // After disable every scope reads None and the key is gone.
        disable_encryption_core("pw", &state).unwrap();
        assert!(state.encryption_key_for(|s| s.photos).unwrap().is_none());
        assert!(state.encryption_key.lock().unwrap().is_none());
        assert!(crypto::read_vault_lock(&vault).unwrap().is_none());
        let _ = std::fs::remove_dir_all(&vault);
    }

    /// If the database step fails, enable rolls back to a clean disabled state:
    /// no lock, no key, and the already-encrypted files decrypted again — so
    /// the status reads disabled and the user can retry.
    #[test]
    fn enable_rolls_back_when_the_database_step_fails() {
        let vault = std::env::temp_dir().join("syz_enable_rollback");
        let _ = std::fs::remove_dir_all(&vault);
        std::fs::create_dir_all(vault.join("raw")).unwrap();
        std::fs::write(vault.join("raw/a.fit"), b"workout").unwrap();
        let conn = crate::init_vault(&vault).unwrap();
        let state = test_state_with(&vault, conn);
        {
            let c = state.db.lock().unwrap();
            crate::db::raw_files::insert_raw_file(
                &c,
                &RawFile {
                    id: "rf-1".into(),
                    activity_id: None,
                    path_in_vault: "raw/a.fit".into(),
                    original_path: None,
                    format: "fit".into(),
                    hash_sha256: "h".into(),
                    imported_at: String::new(),
                    parse_status: "ok".into(),
                    failure_reason: None,
                },
            )
            .unwrap();
        }

        // Force the DB export to fail while leaving the ORIGINAL vault.db a
        // valid, plaintext-readable SQLite file: block the temp export path by
        // pre-creating "vault.db.migrating" as a directory. plaintext_db_readable
        // stays true, so this is the safe-to-fully-roll-back branch.
        std::fs::create_dir(vault.join("vault.db.migrating")).unwrap();

        let err = enable_encryption_core(
            "pw",
            crypto::EncryptionScopes { activities: true, database: true, photos: false },
            &state,
        )
        .unwrap_err();
        assert!(!err.is_empty());

        // Rolled back to a clean disabled state: lock gone, key gone, the raw
        // file never got encrypted (DB is first, so files were untouched).
        assert!(crypto::read_vault_lock(&vault).unwrap().is_none());
        assert!(state.encryption_key.lock().unwrap().is_none());
        assert!(vault.join("raw/a.fit").exists());
        assert!(!vault.join("raw/a.fit.enc").exists());
        assert!(crate::plaintext_db_readable(&vault), "vault.db stays plaintext");

        let _ = std::fs::remove_dir_all(&vault);
    }

    /// A DB-encrypt failure that leaves vault.db PLAINTEXT must undo to a
    /// plaintext state (no lock) — never settle to LOCKED, which would then
    /// reject every unlock (SQLCipher over a plaintext file) and lock the user
    /// out of intact data. The discriminator is the disk state, not whether the
    /// reopen happens to succeed.
    #[test]
    fn enable_db_fail_over_plaintext_never_locks_out() {
        let vault = std::env::temp_dir().join("syz_enable_plaintext_nolock");
        let _ = std::fs::remove_dir_all(&vault);
        std::fs::create_dir_all(&vault).unwrap();
        let conn = crate::init_vault(&vault).unwrap();
        let state = test_state_with(&vault, conn);

        // Block the encrypt export so vault.db stays plaintext.
        std::fs::create_dir(vault.join("vault.db.migrating")).unwrap();

        let _ = enable_encryption_core(
            "pw",
            crypto::EncryptionScopes { activities: false, database: true, photos: false },
            &state,
        )
        .unwrap_err();

        // PLAINTEXT terminal: no lock (no bogus locked status), key cleared, and
        // the DB still opens as plaintext with a working live connection.
        assert!(crypto::read_vault_lock(&vault).unwrap().is_none());
        assert!(state.encryption_key.lock().unwrap().is_none());
        assert!(crate::plaintext_db_readable(&vault));
        assert!(db_usable(&state), "live connection still serves the plaintext DB");

        let _ = std::fs::remove_dir_all(&vault);
    }

    /// disable whose database decrypt fails must settle to the LOCKED terminal
    /// state (vault.lock kept with database=true, db_locked, key cleared) — not
    /// a half-disabled vault with a plaintext-claiming lock over an encrypted DB.
    #[test]
    fn disable_settles_locked_when_the_db_decrypt_fails() {
        let vault = std::env::temp_dir().join("syz_disable_settle");
        let _ = std::fs::remove_dir_all(&vault);
        std::fs::create_dir_all(&vault).unwrap();
        let conn = crate::init_vault(&vault).unwrap();
        let state = test_state_with(&vault, conn);

        // Encrypt BOTH a file scope and the database, so the DB-decrypt-fail
        // recovery must preserve the file scope too.
        std::fs::create_dir_all(vault.join("raw")).unwrap();
        std::fs::write(vault.join("raw/a.fit"), b"workout").unwrap();
        {
            let c = state.db.lock().unwrap();
            crate::db::raw_files::insert_raw_file(
                &c,
                &RawFile {
                    id: "rf-1".into(),
                    activity_id: None,
                    path_in_vault: "raw/a.fit".into(),
                    original_path: None,
                    format: "fit".into(),
                    hash_sha256: "h".into(),
                    imported_at: String::new(),
                    parse_status: "ok".into(),
                    failure_reason: None,
                },
            )
            .unwrap();
        }
        enable_encryption_core(
            "pw",
            crypto::EncryptionScopes { activities: true, database: true, photos: false },
            &state,
        )
        .unwrap();
        assert!(!crate::plaintext_db_readable(&vault));

        // Block the decrypt export path so decrypt_database fails while vault.db
        // stays SQLCipher-encrypted.
        std::fs::create_dir(vault.join("vault.db.migrating")).unwrap();

        let _ = disable_encryption_core("pw", &state).unwrap_err();

        // LOCKED terminal: lock kept + database AND the file scope preserved
        // (the .enc raw file is still on disk), booted locked, key cleared.
        let lock = crypto::read_vault_lock(&vault).unwrap().expect("lock kept");
        assert!(lock.scopes.database);
        assert!(lock.scopes.activities, "file scope preserved, not lost");
        assert!(*state.db_locked.lock().unwrap());
        assert!(state.encryption_key.lock().unwrap().is_none());
        assert!(!crate::plaintext_db_readable(&vault), "DB stays encrypted");

        let _ = std::fs::remove_dir_all(&vault);
    }

    /// The critical data-loss guard: if the DB was encrypted but the reopen
    /// failed, enable must NOT delete vault.lock — that lock holds the only copy
    /// of the salt, and removing it makes the encrypted database unrecoverable.
    /// Here the DB IS unreadable-as-plaintext, so the lock is preserved and the
    /// vault is left locked for a restart+unlock to heal.
    #[test]
    fn enable_keeps_the_lock_when_the_db_is_encrypted_but_unreadable() {
        let vault = std::env::temp_dir().join("syz_enable_keeplock");
        let _ = std::fs::remove_dir_all(&vault);
        std::fs::create_dir_all(&vault).unwrap();
        let conn = crate::init_vault(&vault).unwrap();
        let state = test_state_with(&vault, conn);

        // Simulate "encrypted on disk but not openable as plaintext" by making
        // vault.db a directory (Connection::open fails → plaintext_db_readable
        // is false), the same branch a post-encryption reopen failure takes.
        std::fs::remove_file(vault.join("vault.db")).unwrap();
        std::fs::create_dir(vault.join("vault.db")).unwrap();

        let _ = enable_encryption_core(
            "pw",
            crypto::EncryptionScopes { activities: false, database: true, photos: false },
            &state,
        )
        .unwrap_err();

        // The lock (salt) is PRESERVED with database=true and the vault boots
        // locked — never discarded, so the data stays recoverable.
        let lock = crypto::read_vault_lock(&vault).unwrap().expect("lock preserved");
        assert!(lock.scopes.database);
        assert!(*state.db_locked.lock().unwrap());
        // The in-memory key is cleared so status reports locked=true (UnlockModal
        // appears) rather than unlocked-over-a-dead-placeholder.
        assert!(state.encryption_key.lock().unwrap().is_none());

        let _ = std::fs::remove_dir_all(&vault);
    }

    /// A file-scope encryption failure must NOT roll back / delete the lock:
    /// the vault is genuinely enabled and any straggler is retried on unlock —
    /// deleting the lock while ciphertext remained was permanent data loss.
    #[cfg(unix)]
    #[test]
    fn enable_file_failure_keeps_the_vault_enabled() {
        use std::os::unix::fs::PermissionsExt;
        let vault = std::env::temp_dir().join("syz_enable_file_besteffort");
        let _ = std::fs::remove_dir_all(&vault);
        std::fs::create_dir_all(vault.join("raw")).unwrap();
        std::fs::write(vault.join("raw/ok.fit"), b"a").unwrap();
        // An unreadable raw file the bulk pass will choke on.
        let bad = vault.join("raw/bad.fit");
        std::fs::write(&bad, b"b").unwrap();
        std::fs::set_permissions(&bad, std::fs::Permissions::from_mode(0o000)).unwrap();
        let conn = crate::init_vault(&vault).unwrap();
        let state = test_state_with(&vault, conn);

        // Activities-only enable (no DB scope) — the branch that used to delete
        // the lock on a file failure.
        let _ = enable_encryption_core(
            "pw",
            crypto::EncryptionScopes { activities: true, database: false, photos: false },
            &state,
        );

        // Enabled and intact: lock present with the scope on, key still held.
        let lock = crypto::read_vault_lock(&vault).unwrap().expect("lock kept");
        assert!(lock.scopes.activities);
        assert!(state.encryption_key.lock().unwrap().is_some());

        let _ = std::fs::set_permissions(&bad, std::fs::Permissions::from_mode(0o644));
        let _ = std::fs::remove_dir_all(&vault);
    }

    /// A single unreadable file must not turn every unlock into a lockout: the
    /// password is verified and the DB opens, so unlock SUCCEEDS and the
    /// best-effort resume just logs the straggler for next time.
    #[cfg(unix)]
    #[test]
    fn unlock_is_best_effort_when_a_file_cant_be_read() {
        use std::os::unix::fs::PermissionsExt;
        let vault = std::env::temp_dir().join("syz_unlock_besteffort");
        let _ = std::fs::remove_dir_all(&vault);
        std::fs::create_dir_all(vault.join("raw")).unwrap();
        let conn = crate::init_vault(&vault).unwrap();
        let state = test_state_with(&vault, conn);

        // Enable the activities scope, then drop in an unreadable plaintext file
        // the next resume pass will choke on.
        enable_encryption_core(
            "pw",
            crypto::EncryptionScopes { activities: true, database: false, photos: false },
            &state,
        )
        .unwrap();
        let bad = vault.join("raw/locked.fit");
        std::fs::write(&bad, b"workout").unwrap();
        std::fs::set_permissions(&bad, std::fs::Permissions::from_mode(0o000)).unwrap();

        // Clear the in-memory key so unlock runs its resume pass for real.
        *state.encryption_key.lock().unwrap() = None;
        // Unlock still succeeds despite the unreadable straggler.
        unlock_vault_core("pw", &state).unwrap();
        assert!(state.encryption_key.lock().unwrap().is_some());

        let _ = std::fs::set_permissions(&bad, std::fs::Permissions::from_mode(0o644));
        let _ = std::fs::remove_dir_all(&vault);
    }

    fn switch_fixture(name: &str) -> (PathBuf, PathBuf, PathBuf) {
        let tmp = std::env::temp_dir().join(format!("syz_switch_{}", name));
        let _ = std::fs::remove_dir_all(&tmp);
        let current = tmp.join("current");
        let other = tmp.join("other");
        for v in [&current, &other] {
            std::fs::create_dir_all(v).unwrap();
            std::fs::write(v.join("vault.db"), b"db").unwrap();
        }
        (tmp, current, other)
    }

    /// Switching only ever writes the marker — and keeps the vault-operation
    /// slot taken afterwards, so nothing mutates the old vault before the
    /// restart that follows.
    #[test]
    fn switch_vault_writes_marker_and_holds_the_flight_slot() {
        let (tmp, current, other) = switch_fixture("ok");
        let cfg = tmp.join("cfg");
        let state = test_state(&current);

        let root = switch_vault_core(&other, true, &cfg, &state).unwrap();
        assert_eq!(root, std::fs::canonicalize(&other).unwrap());
        assert_eq!(vault::read_location(&cfg), Some(root));
        assert!(state.vault_flight.try_begin().is_none(), "slot must stay taken");
        // Both vaults untouched.
        assert!(current.join("vault.db").exists());
        assert!(other.join("vault.db").exists());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Every refusal is side-effect-free: no marker, slot released.
    #[test]
    fn switch_vault_refusals_leave_no_marker() {
        let (tmp, current, other) = switch_fixture("refuse");
        let cfg = tmp.join("cfg");
        let state = test_state(&current);

        {
            let _running = state.vault_flight.try_begin().unwrap();
            let err = switch_vault_core(&other, true, &cfg, &state).unwrap_err();
            assert!(err.contains("already in progress"), "{err}");
        }
        let err = switch_vault_core(&current, true, &cfg, &state).unwrap_err();
        assert!(err.contains("already the current vault"), "{err}");
        // Spelled through a symlink it is still the current vault.
        #[cfg(unix)]
        {
            let alias = tmp.join("alias");
            std::os::unix::fs::symlink(&current, &alias).unwrap();
            let err = switch_vault_core(&alias, true, &cfg, &state).unwrap_err();
            assert!(err.contains("already the current vault"), "{err}");
        }
        let empty = tmp.join("empty");
        std::fs::create_dir(&empty).unwrap();
        let err = switch_vault_core(&empty, true, &cfg, &state).unwrap_err();
        assert!(err.contains("No vault found"), "{err}");
        let err = switch_vault_core(&current.join("nested"), false, &cfg, &state).unwrap_err();
        assert!(err.contains("inside the vault at"), "{err}");

        // A root the marker can't hold verbatim (trailing space is legal on
        // unix filesystems) is refused after the write — and rolled back.
        #[cfg(unix)]
        {
            let trailing = tmp.join("trailing ");
            std::fs::create_dir(&trailing).unwrap();
            std::fs::write(trailing.join("vault.db"), b"db").unwrap();
            let err = switch_vault_core(&trailing, true, &cfg, &state).unwrap_err();
            assert!(err.contains("can't be stored"), "{err}");
        }

        assert_eq!(vault::read_location(&cfg), None);
        assert!(state.vault_flight.try_begin().is_some(), "slot must be free");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// The parent of a vault (the usual one-level-up miss) opens the vault in
    /// its `Syzify` subfolder; a fresh vault in an empty folder is allowed.
    #[test]
    fn switch_vault_resolves_parent_pick_and_fresh_folder() {
        let (tmp, current, _other) = switch_fixture("parent");
        let cfg = tmp.join("cfg");
        let home = tmp.join("home");
        std::fs::create_dir_all(home.join("Syzify")).unwrap();
        std::fs::write(home.join("Syzify/vault.db"), b"db").unwrap();
        std::fs::write(home.join("notes.txt"), b"x").unwrap();
        let state = test_state(&current);
        let root = switch_vault_core(&home, true, &cfg, &state).unwrap();
        assert_eq!(root, std::fs::canonicalize(home.join("Syzify")).unwrap());

        let state = test_state(&current);
        let fresh = tmp.join("fresh");
        let root = switch_vault_core(&fresh, false, &cfg, &state).unwrap();
        assert_eq!(root, std::fs::canonicalize(&tmp).unwrap().join("fresh"));
        assert_eq!(vault::read_location(&cfg), Some(root));
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
