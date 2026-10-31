mod commands;
pub mod crypto;
// Public like `parser`/`crypto`: dev examples (one-off vault maintenance)
// call the real db layer instead of duplicating its logic.
pub mod db;
pub mod error;
mod export;
mod geocoding;
mod import;
mod maps;
mod models;
// pub for dev tooling (examples/find_multisport); not a public API promise.
pub mod parser;
mod plugins;
pub mod recovery;
pub mod state;
#[cfg(test)]
pub(crate) mod test_support;
mod updates;
mod util;
mod vault;

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use tauri::Manager;
use error::CommandError;
use state::AppState;
use maps::tile_cache;

fn ensure_vault_dirs(vault_path: &Path) -> Result<(), CommandError> {
    // NOTE: no subdirectories under raw/ — encrypt_all_raw_files walks it
    // non-recursively, so a nested file would silently stay plaintext.
    for sub in ["raw", "tiles", "photos", "plugins"] {
        fs::create_dir_all(vault_path.join(sub)).map_err(|e| {
            CommandError::coded("vault.createFolderFailed", format!("Failed to create vault/{}: {}", sub, e))
                .param("folder", sub)
                .param("detail", e.to_string())
        })?;
    }
    Ok(())
}

/// WAL + migrations on an already-open connection. Kept cheap so it can run in
/// the synchronous boot path; heavy data backfills run separately off-thread
/// (see `run_startup_backfills`). Its refusals are what the vault error
/// screen says at boot (#244), so they are coded; the library's words
/// stay in `{detail}`.
fn configure_and_migrate(conn: &mut Connection) -> Result<(), CommandError> {
    conn.pragma_update(None, "journal_mode", "WAL").map_err(|e| {
        CommandError::coded("vault.walFailed", format!("Failed to enable WAL: {}", e)).param("detail", e.to_string())
    })?;
    // Uncoded: a connection that just took the WAL pragma takes this one —
    // a guard, not a reason anyone meets.
    conn.pragma_update(None, "foreign_keys", "ON")
        .map_err(|e| format!("Failed to enable foreign keys: {}", e))?;

    match db::migrations::run_migrations(conn) {
        Ok(()) => Ok(()),
        Err(db::migrations::MigrateError::TooNew { vault, app }) => Err(CommandError::coded(
            "vault.tooNew",
            format!(
                "This vault was last used by a newer version of Syzify (data format {}, this app reads up to {}). Update the app to open it.",
                vault, app
            ),
        )
        .param("vault", vault)
        .param("app", app)),
        Err(db::migrations::MigrateError::Failed(e)) => Err(CommandError::coded(
            "vault.migrateFailed",
            format!("Failed to run migrations: {}", e),
        )
        .param("detail", e.to_string())),
    }
}

/// One-time data backfills, each guarded by a settings flag so it runs once.
/// These scan/recompute across the whole library (seconds on a large vault),
/// so they run in a background thread AFTER boot — never in `setup()` on the
/// main thread, which would freeze the window before it paints.
pub(crate) fn run_startup_backfills(conn: &Connection) -> Result<(), String> {
    // Best-effort splits for runs imported before the running-records feature.
    // v2 recomputes after the max-speed guard that clears GPS-glitch splits.
    const BACKFILL_FLAG: &str = "best_efforts_backfilled_v2";
    if db::settings::get_setting(conn, BACKFILL_FLAG)
        .map_err(|e| format!("Failed to read settings: {}", e))?
        .is_none()
    {
        db::best_efforts::recompute_running(conn)
            .map_err(|e| format!("Failed to backfill best efforts: {}", e))?;
        db::settings::set_setting(conn, BACKFILL_FLAG, "1")
            .map_err(|e| format!("Failed to mark backfill done: {}", e))?;
    }

    // Upgrade sport types from stored sub-sport for activities imported before
    // the 7→24 sport expansion (replaces the old manual button; idempotent).
    const SPORT_BACKFILL_FLAG: &str = "sport_types_backfilled_v1";
    if db::settings::get_setting(conn, SPORT_BACKFILL_FLAG)
        .map_err(|e| format!("Failed to read settings: {}", e))?
        .is_none()
    {
        db::activities::recompute_sport_types(conn)
            .map_err(|e| format!("Failed to backfill sport types: {}", e))?;
        db::settings::set_setting(conn, SPORT_BACKFILL_FLAG, "1")
            .map_err(|e| format!("Failed to mark sport backfill done: {}", e))?;
    }

    // Rides imported before indoor_ride / virtual_ride existed (#189) sit
    // under `ride` with the trainer kind in their sub_sport: file them.
    const TRAINER_RIDES_FLAG: &str = "trainer_rides_refiled_v1";
    if db::settings::get_setting(conn, TRAINER_RIDES_FLAG)
        .map_err(|e| format!("Failed to read settings: {}", e))?
        .is_none()
    {
        db::activities::refile_rides_by_sub_sport(conn)
            .map_err(|e| format!("Failed to refile trainer rides: {}", e))?;
        db::settings::set_setting(conn, TRAINER_RIDES_FLAG, "1")
            .map_err(|e| format!("Failed to mark trainer-ride backfill done: {}", e))?;
    }

    // Virtual rides re-filed by the step above carried a geocoded name
    // and segment efforts from their days as a ride (#190): take them off.
    const SIMULATED_COURSE_FLAG: &str = "simulated_course_scrubbed_v1";
    if db::settings::get_setting(conn, SIMULATED_COURSE_FLAG)
        .map_err(|e| format!("Failed to read settings: {}", e))?
        .is_none()
    {
        let geocoder_on = import::geocoding::geocoding_enabled(conn);
        db::activities::scrub_simulated_course(conn, None, geocoder_on)
            .map_err(|e| format!("Failed to scrub simulated courses: {}", e))?;
        db::settings::set_setting(conn, SIMULATED_COURSE_FLAG, "1")
            .map_err(|e| format!("Failed to mark simulated-course scrub done: {}", e))?;
    }

    // Runs done on a simulator sat under `run` before virtual_run existed
    // (#192): file them, and take their simulated course's traces off.
    const VIRTUAL_RUNS_FLAG: &str = "virtual_runs_refiled_v1";
    if db::settings::get_setting(conn, VIRTUAL_RUNS_FLAG)
        .map_err(|e| format!("Failed to read settings: {}", e))?
        .is_none()
    {
        let geocoder_on = import::geocoding::geocoding_enabled(conn);
        db::activities::refile_virtual_runs(conn, geocoder_on)
            .map_err(|e| format!("Failed to refile virtual runs: {}", e))?;
        db::settings::set_setting(conn, VIRTUAL_RUNS_FLAG, "1")
            .map_err(|e| format!("Failed to mark virtual-run backfill done: {}", e))?;
    }

    // Efforts for segments saved before the matching engine existed. New
    // segments backfill on save and new imports match on arrival, so this
    // only needs to run once.
    const SEGMENT_EFFORTS_FLAG: &str = "segment_efforts_backfilled_v1";
    if db::settings::get_setting(conn, SEGMENT_EFFORTS_FLAG)
        .map_err(|e| format!("Failed to read settings: {}", e))?
        .is_none()
    {
        db::segment_efforts::backfill_all_segments(conn)
            .map_err(|e| format!("Failed to backfill segment efforts: {}", e))?;
        db::settings::set_setting(conn, SEGMENT_EFFORTS_FLAG, "1")
            .map_err(|e| format!("Failed to mark segment-efforts backfill done: {}", e))?;
    }

    // Average power for segment efforts recorded before the avg_power_w
    // column existed. Touches only powered activities that have efforts —
    // a tiny intersection, safe to run under the held lock.
    const EFFORT_POWER_FLAG: &str = "segment_effort_power_backfilled_v2";
    if db::settings::get_setting(conn, EFFORT_POWER_FLAG)
        .map_err(|e| format!("Failed to read settings: {}", e))?
        .is_none()
    {
        db::segment_efforts::backfill_effort_power(conn)
            .map_err(|e| format!("Failed to backfill effort power: {}", e))?;
        db::settings::set_setting(conn, EFFORT_POWER_FLAG, "1")
            .map_err(|e| format!("Failed to mark effort-power backfill done: {}", e))?;
    }
    Ok(())
}

/// One-time mean-max power-curve backfill. Unlike `run_startup_backfills`
/// this walks every powered activity's full trackpoint stream, which on a
/// large power-meter library takes real time — so it takes the DB lock per
/// CHUNK instead of holding it throughout, letting commands interleave.
/// Best-effort like the rest: any failure leaves the flag unset to retry
/// next launch.
fn run_power_curve_backfill(state: &AppState) {
    const POWER_CURVE_FLAG: &str = "power_curve_backfilled_v1";
    const CHUNK: usize = 25;

    let ids = {
        let conn = match state.db.lock() {
            Ok(c) => c,
            Err(_) => return,
        };
        match db::settings::get_setting(&conn, POWER_CURVE_FLAG) {
            Ok(None) => {}
            Ok(Some(_)) => return,
            Err(e) => {
                eprintln!("Failed to read settings for power-curve backfill: {}", e);
                return;
            }
        }
        match db::power_curve::powered_activity_ids(&conn) {
            Ok(ids) => ids,
            Err(e) => {
                eprintln!("Power-curve backfill failed to list activities: {}", e);
                return;
            }
        }
    };

    for chunk in ids.chunks(CHUNK) {
        let conn = match state.db.lock() {
            Ok(c) => c,
            Err(_) => return,
        };
        if let Err(e) = db::power_curve::recompute_for(&conn, chunk) {
            eprintln!("Power-curve backfill failed (will retry next launch): {}", e);
            return;
        }
    }

    if let Ok(conn) = state.db.lock() {
        if let Err(e) = db::settings::set_setting(&conn, POWER_CURVE_FLAG, "1") {
            eprintln!("Failed to mark power-curve backfill done: {}", e);
        }
    }
}

const ZERO_READINGS_FLAG: &str = "zero_readings_cleared_v1";
const POWER_SUMMARY_FLAG: &str = "power_summary_filled_v1";

/// One-time fill of the power summaries older imports lack (#224). Best
/// effort like the power-curve backfill: a failure leaves the flag unset
/// to retry next launch.
fn run_power_summary_fill(state: &AppState) {
    if let Err(e) = power_summary_fill(state) {
        eprintln!("Power-summary fill failed (will retry next launch): {}", e);
    }
}

/// First the summary zeros are cleared — an NP of 0 must read as missing
/// before the fill looks at it — then every activity, oldest first, loses
/// its records' 0 bpm and gets what `power_summary::fill_missing` gives a
/// new import. The lock is taken per chunk of activities, never over a
/// scan of every record, so commands interleave. One activity failing is
/// logged and skipped: the walk must end, or it reruns every launch.
fn power_summary_fill(state: &AppState) -> Result<(), String> {
    const CHUNK: usize = 25;
    let activities = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        if db::settings::get_setting(&conn, ZERO_READINGS_FLAG).map_err(|e| e.to_string())?.is_none() {
            db::activities::clear_zero_summaries(&conn).map_err(|e| e.to_string())?;
            db::settings::set_setting(&conn, ZERO_READINGS_FLAG, "1").map_err(|e| e.to_string())?;
        }
        if db::settings::get_setting(&conn, POWER_SUMMARY_FLAG).map_err(|e| e.to_string())?.is_some() {
            return Ok(());
        }
        db::activities::all_by_start(&conn).map_err(|e| e.to_string())?
    };
    for chunk in activities.chunks(CHUNK) {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        for (id, sport) in chunk {
            let filled = db::activities::clear_zero_heart_rates(&conn, id)
                .and_then(|_| import::power_summary::fill_missing(&conn, id, sport));
            if let Err(e) = filled {
                eprintln!("Power-summary fill skipped {}: {}", id, e);
            }
        }
    }
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::settings::set_setting(&conn, POWER_SUMMARY_FLAG, "1").map_err(|e| e.to_string())
}

/// Recompute the monitoring day aggregates (ADR 0002): every launch picks
/// up the days stored but never aggregated (an import interrupted before
/// its batch recompute), and once per formula version every day is
/// rebuilt — the flag's suffix is the version, bump it with the formula.
/// The lock is taken per chunk of days so UI commands are not starved.
/// Best-effort like the power-curve backfill: a failure leaves the flag
/// unset to retry next time.
fn run_monitoring_recompute(state: &AppState) {
    const MONITORING_FLAG: &str = "monitoring_days_computed_v1";
    const CHUNK: usize = 30;

    let (days, mark_done) = {
        let conn = match state.db.lock() {
            Ok(c) => c,
            Err(_) => return,
        };
        let versioned = match db::settings::get_setting(&conn, MONITORING_FLAG) {
            Ok(v) => v.is_some(),
            Err(e) => {
                eprintln!("Failed to read settings for monitoring recompute: {}", e);
                return;
            }
        };
        let list = if versioned {
            db::monitoring::pending_days(&conn)
        } else {
            db::monitoring::all_days(&conn)
        };
        match list {
            Ok(days) => (days, !versioned),
            Err(e) => {
                eprintln!("Monitoring recompute failed to list days: {}", e);
                return;
            }
        }
    };

    for chunk in days.chunks(CHUNK) {
        let conn = match state.db.lock() {
            Ok(c) => c,
            Err(_) => return,
        };
        if let Err(e) = db::monitoring::recompute_days(&conn, chunk) {
            eprintln!("Monitoring recompute failed (will retry next launch): {}", e);
            return;
        }
    }

    if mark_done {
        if let Ok(conn) = state.db.lock() {
            if let Err(e) = db::settings::set_setting(&conn, MONITORING_FLAG, "1") {
                eprintln!("Failed to mark monitoring recompute done: {}", e);
            }
        }
    }
}

/// Open the plaintext vault database (database scope off). A root without a
/// vault.db becomes a fresh vault here — unless it sits inside another vault
/// (a marker from an older build could still point there). At boot its
/// error is the vault error screen's reason, so it stays a `CommandError`.
pub(crate) fn init_vault(vault_path: &Path) -> Result<Connection, CommandError> {
    vault::ensure_not_nested(vault_path)?;
    ensure_vault_dirs(vault_path)?;
    let mut conn = Connection::open(vault_path.join("vault.db")).map_err(|e| {
        CommandError::coded("vault.dbOpenFailed", format!("Failed to open database: {}", e)).param("detail", e.to_string())
    })?;
    configure_and_migrate(&mut conn)?;
    Ok(conn)
}

/// The connection the app boots on, and why the vault would not open when it
/// did not — what `get_vault_error` hands the vault error screen. An
/// encrypted database waits for unlock on an in-memory placeholder; so does
/// a vault that failed to open, until it is reopened.
fn open_at_boot(vault_path: &Path, db_encrypted: bool) -> rusqlite::Result<(Connection, Option<CommandError>)> {
    if db_encrypted {
        return Ok((Connection::open_in_memory()?, None));
    }
    match init_vault(vault_path) {
        Ok(conn) => Ok((conn, None)),
        Err(e) => Ok((Connection::open_in_memory()?, Some(e))),
    }
}

/// Open the SQLCipher-encrypted vault database with `key` (database scope on).
/// Called from `unlock_vault`, not at boot.
pub(crate) fn init_vault_encrypted(
    vault_path: &Path,
    key: &[u8; 32],
) -> Result<Connection, CommandError> {
    ensure_vault_dirs(vault_path)?;
    let mut conn = db::dbcrypt::open_with_key(&vault_path.join("vault.db"), key)?;
    configure_and_migrate(&mut conn)?;
    Ok(conn)
}

/// Whether vault.db opens and reads as a plaintext SQLite database. False for
/// a SQLCipher-encrypted file (reads fail without the key). A missing file
/// counts as readable — a fresh vault is plaintext until encryption is enabled.
pub(crate) fn plaintext_db_readable(vault_path: &Path) -> bool {
    let db_path = vault_path.join("vault.db");
    if !db_path.exists() {
        return true;
    }
    match Connection::open(&db_path) {
        Ok(conn) => conn
            .query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0))
            .is_ok(),
        Err(_) => false,
    }
}

/// Start the DB-dependent background services (file watcher, volume monitor,
/// geocoding). Idempotent via `services_started`: called at boot for a
/// plaintext vault, or after `unlock_vault` for an encrypted one — never
/// before the key is loaded, and never twice (a second call would spawn a
/// duplicate geocoding thread / watcher).
pub(crate) fn start_background_services(handle: &tauri::AppHandle) {
    {
        let state = handle.state::<AppState>();
        let mut started = match state.services_started.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        if *started {
            return;
        }
        *started = true;
    }

    let paths = import::watcher::get_watch_paths_from_db(handle);
    if !paths.is_empty() {
        match import::watcher::start_watching(handle.clone(), paths) {
            Ok(w) => {
                let state = handle.state::<AppState>();
                let mut wh = state.watcher_handle.lock().unwrap();
                *wh = Some(w);
            }
            Err(e) => eprintln!("Failed to start file watcher: {}", e),
        }
    }

    #[cfg(target_os = "macos")]
    import::volume_monitor::start_volume_monitor(handle.clone());

    let geo_handle = handle.clone();
    std::thread::spawn(move || {
        import::geocoding::run_background_geocoding(&geo_handle);
    });

    // One-time data backfills off the main thread (both boot paths reach here
    // with the DB open + key loaded). Best-effort: a failure just leaves the
    // flag unset to retry next launch, never blocks the UI.
    let bf_handle = handle.clone();
    std::thread::spawn(move || {
        let state = bf_handle.state::<AppState>();
        // Heavy per-activity work first, with per-chunk locking — the block
        // below holds the lock for its whole run, so it must come after.
        run_power_curve_backfill(&state);
        run_power_summary_fill(&state);
        import::device_backfill::run(&state);
        import::gear_keys_backfill::run(&state);
        import::exercise_names_backfill::run(&state);
        // After the gear keys: it reads the sensor lists they stored.
        import::gps_clean_backfill::run(&state);
        run_monitoring_recompute(&state);
        let conn = match state.db.lock() {
            Ok(c) => c,
            Err(_) => return,
        };
        if let Err(e) = run_startup_backfills(&conn) {
            eprintln!("Startup backfill failed (will retry next launch): {}", e);
        }

        // Photos attached before EXIF-orientation support have sideways
        // thumbnails and pre-rotation dimensions; regenerate them once. Needs
        // the vault path and photo key, so it can't live inside
        // run_startup_backfills(&conn).
        const PHOTO_ORIENT_FLAG: &str = "photo_thumbs_oriented_v1";
        match db::settings::get_setting(&conn, PHOTO_ORIENT_FLAG) {
            Ok(None) => {
                let key = state.encryption_key.lock().ok().and_then(|g| *g);
                match commands::photos::orient_existing_photos(&conn, &state.vault_path, key.as_ref())
                {
                    Ok(()) => {
                        if let Err(e) = db::settings::set_setting(&conn, PHOTO_ORIENT_FLAG, "1") {
                            eprintln!("Failed to mark photo orientation backfill done: {}", e);
                        }
                    }
                    Err(e) => eprintln!(
                        "Photo orientation backfill failed (will retry next launch): {}",
                        e
                    ),
                }
            }
            Ok(Some(_)) => {}
            Err(e) => eprintln!("Failed to read settings for photo backfill: {}", e),
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .register_asynchronous_uri_scheme_protocol("photo", |ctx, request, responder| {
            let app_handle = ctx.app_handle().clone();
            std::thread::spawn(move || {
                let uri = request.uri().to_string();
                let state = app_handle.state::<AppState>();
                let vault_path = state.vault_path.clone();
                let photo_key = state
                    .encryption_key
                    .lock()
                    .ok()
                    .and_then(|g| *g);
                let conn_guard = match state.db.lock() {
                    Ok(g) => g,
                    Err(_) => {
                        responder.respond(
                            tauri::http::Response::builder()
                                .status(500)
                                .body(Vec::new())
                                .unwrap(),
                        );
                        return;
                    }
                };
                match commands::photos::resolve_photo_request(&vault_path, &conn_guard, photo_key.as_ref(), &uri) {
                    Ok((mime, data)) => {
                        responder.respond(
                            tauri::http::Response::builder()
                                .status(200)
                                .header("Content-Type", mime)
                                .header("Cache-Control", "private, max-age=600")
                                .body(data)
                                .unwrap(),
                        );
                    }
                    Err(_) => {
                        responder.respond(
                            tauri::http::Response::builder()
                                .status(404)
                                .body(Vec::new())
                                .unwrap(),
                        );
                    }
                }
            });
        })
        .register_asynchronous_uri_scheme_protocol("tile", |ctx, request, responder| {
            let vault_path = ctx.app_handle().state::<AppState>().vault_path.clone();
            std::thread::spawn(move || {
                let uri = request.uri().to_string();
                match handle_tile_request(&vault_path, &uri) {
                    Ok(data) => {
                        responder.respond(
                            tauri::http::Response::builder()
                                .status(200)
                                .header("Content-Type", "image/png")
                                .header("Cache-Control", "max-age=86400")
                                .body(data)
                                .unwrap(),
                        );
                    }
                    Err(_) => {
                        responder.respond(
                            tauri::http::Response::builder()
                                .status(404)
                                .body(Vec::new())
                                .unwrap(),
                        );
                    }
                }
            });
        })
        .setup(|app| {
            // Vault location: the marker file in the app config dir wins
            // (written by relocate_vault); default is ~/Syzify.
            let vault_path = app
                .path()
                .app_config_dir()
                .ok()
                .as_deref()
                .and_then(vault::read_location)
                .unwrap_or_else(|| {
                    dirs_next_home()
                        .unwrap_or_else(|| PathBuf::from("."))
                        .join("Syzify")
                });

            // Scrub any vault.db.migrating stranded by a crash mid encrypt/
            // decrypt toggle — after a crashed disable it is a full PLAINTEXT
            // dump of the database, and only a retried toggle would remove it.
            if db::dbcrypt::remove_stale_migrating(&vault_path.join("vault.db")) {
                eprintln!("Removed stale vault.db.migrating left by an interrupted encryption toggle");
            }
            // Same class: a .backup-snapshot stranded by a crash mid-backup is
            // a plaintext copy of vault.db the encryption passes don't know about.
            if commands::export::scrub_stale_backup_snapshot(&vault_path) {
                eprintln!("Removed stale .backup-snapshot left by an interrupted backup");
            }

            // If the `database` scope is encrypted we cannot open vault.db
            // without the password. Boot "locked": hold an in-memory
            // placeholder connection, defer migrations and services until
            // unlock_vault opens the real keyed DB (see commands::settings).
            //
            // We trust the lock's flag, but also treat the DB as encrypted when
            // a vault.lock exists yet the plaintext DB won't read — that heals a
            // vault whose flag lags the on-disk reality after a crash mid-toggle.
            let has_lock = crypto::read_vault_lock(&vault_path).ok().flatten();
            // Any lock means the vault is encrypted and boots locked (the
            // frontend shows UnlockModal). Services wait for the key regardless
            // of scope — otherwise a watch-folder import while "locked" would
            // write NEW files in plaintext (activities/photos scope) with no
            // key to encrypt them.
            let encrypted = has_lock.is_some();
            // The database scope additionally can't open vault.db without the
            // key, so its connection is a placeholder until unlock.
            let db_encrypted = match &has_lock {
                Some(lock) => lock.scopes.database || !plaintext_db_readable(&vault_path),
                None => false,
            };

            // Open the plaintext vault, or hold an in-memory placeholder for
            // an encrypted (deferred to unlock) or unreachable vault. A vault
            // in a macOS-protected folder (Documents/Desktop/Downloads) fails
            // to open until Full Disk Access is granted — record the error and
            // let the frontend show a recoverable screen rather than panicking.
            let (conn, vault_error) = open_at_boot(&vault_path, db_encrypted)
                .map_err(|e| format!("Failed to open placeholder db: {}", e))?;

            // Serve now only for a fully-open, unencrypted vault. Encrypted
            // vaults (any scope) start services after unlock; errored ones
            // not until reopened.
            let can_serve = !encrypted && vault_error.is_none();
            app.manage(AppState {
                db: Arc::new(Mutex::new(conn)),
                vault_path,
                encryption_key: Mutex::new(None),
                watcher_handle: Mutex::new(None),
                db_locked: Mutex::new(db_encrypted),
                vault_error: Mutex::new(vault_error),
                services_started: Mutex::new(false),
            geocoding_flight: crate::state::SingleFlight::default(),
            vault_flight: crate::state::SingleFlight::default(),
            });

            // Only a fully-open plaintext vault starts services now; encrypted
            // vaults start after unlock, errored ones not until reopened.
            if can_serve {
                start_background_services(&app.handle().clone());
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::import::import_files,
            commands::import::get_import_datasources,
            commands::import::run_import_datasource,
            commands::activities::get_activities,
            commands::activities::get_activity_detail,
            commands::activities::update_activity,
            commands::activities::delete_activity,
            commands::activities::merge_into_triathlon,
            commands::activities::unmerge_triathlon,
            commands::activities::search_activities,
            commands::activities::update_activity_location,
            commands::activities::set_activity_location_point,
            commands::activities::search_locations,
            commands::activities::set_activity_location_named,
            commands::activities::set_activity_ftp,
            commands::activities::get_activity_locations,
            commands::activities::get_adjacent_activities,
            commands::activities::get_activity_record_badges,
            commands::activities::get_power_curve,
            commands::activities::get_calendar_data,
            commands::activities::get_used_sport_types,
            commands::activities::get_activity_year_range,
            commands::export::export_activity_gpx,
            commands::export::backup_vault,
            commands::export::restore_vault,
            commands::segments::check_similar_segments,
            commands::segments::save_segment,
            commands::segments::get_activity_segment_efforts,
            commands::segments::list_segments,
            commands::segments::rename_segment,
            commands::segments::delete_segment,
            commands::segments::get_segment_efforts,
            commands::tiles::get_tile_cache_info,
            commands::tiles::clear_tile_cache,
            commands::gear::list_gear,
            commands::gear::create_gear,
            commands::gear::update_gear,
            commands::gear::set_gear_retired,
            commands::gear::delete_gear,
            commands::gear::set_activity_gear,
            commands::gear::assign_gear_history,
            commands::gear::count_gear_targets,
            commands::gear::assign_gear_to_filtered,
            commands::gear::gear_rule_candidates,
            commands::gear::apply_gear_rules,
            commands::settings::get_watch_folders,
            commands::settings::add_watch_folder,
            commands::settings::remove_watch_folder,
            commands::settings::scan_watch_folders,
            commands::settings::get_vault_path,
            commands::settings::get_vault_error,
            commands::settings::relocate_vault,
            commands::settings::switch_vault,
            commands::settings::restart_app,
            commands::settings::check_for_updates,
            commands::settings::install_update,
            commands::settings::get_detected_devices,
            commands::settings::preview_watch_folders,
            commands::settings::catch_up_watch_files,
            commands::settings::get_setting,
            commands::settings::set_setting,
            commands::settings::get_legal_text,
            commands::settings::start_geocoding,
            commands::settings::restart_watcher,
            commands::settings::get_encryption_status,
            commands::settings::unlock_vault,
            commands::settings::enable_encryption,
            commands::settings::disable_encryption,
            commands::dashboard::get_dashboard_data,
            commands::recovery::get_recovery_nights,
            commands::monitoring::get_monitoring_summary,
            commands::monitoring::delete_monitoring_range,
            commands::photos::attach_photos,
            commands::photos::get_photos,
            commands::photos::delete_photo,
            commands::photos::update_photo_caption,
            commands::photos::reorder_photos,
            commands::photos::save_share_image,
            commands::photos::get_photo_data_url,
            commands::plugins::get_plugins,
            commands::plugins::install_plugin_from_file,
            commands::plugins::install_plugin_from_package,
            commands::plugins::set_plugin_enabled,
            commands::plugins::uninstall_plugin,
            commands::plugins::get_plugin_network_endpoints,
            commands::plugins::get_plugin_contributions,
            commands::plugins::render_plugin_view,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Map a layer ID to its upstream tile URL.
/// Tile layers we serve — the single source of truth for both cache-path
/// validation and upstream URL selection.
const VALID_TILE_LAYERS: [&str; 5] = ["osm", "topo", "cycling", "satellite", "dark"];

fn tile_upstream_url(layer: &str, z: u32, x: u32, y: u32) -> Result<String, String> {
    match layer {
        "osm" => Ok(format!(
            "https://tile.openstreetmap.org/{}/{}/{}.png",
            z, x, y
        )),
        "topo" => Ok(format!(
            "https://tile.opentopomap.org/{}/{}/{}.png",
            z, x, y
        )),
        "cycling" => Ok(format!(
            "https://a.tile-cyclosm.openstreetmap.fr/cyclosm/{}/{}/{}.png",
            z, x, y
        )),
        "satellite" => Ok(format!(
            "https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{}/{}/{}",
            z, y, x
        )),
        "dark" => Ok(format!(
            "https://basemaps.cartocdn.com/dark_all/{}/{}/{}@2x.png",
            z, x, y
        )),
        _ => Err(format!("Unknown layer: {}", layer)),
    }
}

/// Parse a `tile://localhost/{layer}/{z}/{x}/{y}` (or legacy `{z}/{x}/{y}`)
/// URI into validated parts. The layer is checked against the known set BEFORE
/// it can reach a filesystem path — `tile_cache` interpolates `layer` into the
/// cache path, so an unchecked value like ".." would escape the tiles dir.
/// z/x/y parse as u32, so they can't traverse.
fn parse_tile_uri(uri: &str) -> Result<(String, u32, u32, u32), String> {
    let path_part = uri
        .strip_prefix("tile://localhost/")
        .or_else(|| uri.strip_prefix("tile://localhost\\"))
        .ok_or_else(|| format!("Invalid tile URI: {}", uri))?;

    let clean = path_part.trim_end_matches(".png");
    let parts: Vec<&str> = clean.split('/').collect();

    let (layer, z, x, y) = if parts.len() == 4 {
        (
            parts[0],
            parts[1].parse::<u32>().map_err(|_| "Invalid z".to_string())?,
            parts[2].parse::<u32>().map_err(|_| "Invalid x".to_string())?,
            parts[3].parse::<u32>().map_err(|_| "Invalid y".to_string())?,
        )
    } else if parts.len() == 3 {
        (
            "osm",
            parts[0].parse::<u32>().map_err(|_| "Invalid z".to_string())?,
            parts[1].parse::<u32>().map_err(|_| "Invalid x".to_string())?,
            parts[2].parse::<u32>().map_err(|_| "Invalid y".to_string())?,
        )
    } else {
        return Err(format!("Expected layer/z/x/y or z/x/y, got: {}", clean));
    };

    if !VALID_TILE_LAYERS.contains(&layer) {
        return Err(format!("Unknown tile layer: {}", layer));
    }
    Ok((layer.to_string(), z, x, y))
}

/// Handle a tile://localhost/{layer}/{z}/{x}/{y} request.
/// Checks cache first, downloads from upstream if not cached.
fn handle_tile_request(vault_path: &Path, uri: &str) -> Result<Vec<u8>, String> {
    let (layer, z, x, y) = parse_tile_uri(uri)?;
    let layer = layer.as_str();

    let tiles_dir = vault_path.join("tiles");

    // Check cache
    if let Some(cached_path) = tile_cache::get_cached_tile(&tiles_dir, layer, z, x, y) {
        let mut data = Vec::new();
        std::fs::File::open(&cached_path)
            .and_then(|mut f| f.read_to_end(&mut data))
            .map_err(|e| format!("Failed to read cached tile: {}", e))?;
        return Ok(data);
    }

    // Download from upstream
    let url = tile_upstream_url(layer, z, x, y)?;
    // Timeouts bound each request so a hung upstream can't pile up stuck
    // threads (one per tile) while the user pans/zooms the map.
    let response = reqwest::blocking::Client::builder()
        .user_agent("Syzify/1.0 (desktop app)")
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?
        .get(&url)
        .send()
        .map_err(|e| format!("Failed to download tile: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Tile download failed: {}", response.status()));
    }

    let data = response
        .bytes()
        .map_err(|e| format!("Failed to read tile bytes: {}", e))?
        .to_vec();

    // Cache the tile (best-effort, don't fail if cache write fails)
    let _ = tile_cache::save_tile(&tiles_dir, layer, z, x, y, &data);

    Ok(data)
}

fn dirs_next_home() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        std::env::var("HOME").ok().map(PathBuf::from)
    }
    #[cfg(target_os = "windows")]
    {
        std::env::var("USERPROFILE").ok().map(PathBuf::from)
    }
    #[cfg(target_os = "linux")]
    {
        std::env::var("HOME").ok().map(PathBuf::from)
    }
}

#[cfg(test)]
mod tile_tests {
    use super::*;

    #[test]
    fn parse_tile_uri_layered_and_legacy() {
        assert_eq!(
            parse_tile_uri("tile://localhost/topo/12/34/56.png").unwrap(),
            ("topo".into(), 12, 34, 56)
        );
        // Legacy 3-part form defaults to osm.
        assert_eq!(
            parse_tile_uri("tile://localhost/5/1/2").unwrap(),
            ("osm".into(), 5, 1, 2)
        );
    }

    #[test]
    fn parse_tile_uri_rejects_unknown_layer_and_traversal() {
        // Path-traversal via layer must be rejected before touching the FS.
        assert!(parse_tile_uri("tile://localhost/../3/4/5.png").is_err());
        assert!(parse_tile_uri("tile://localhost/bogus/1/2/3").is_err());
    }

    #[test]
    fn parse_tile_uri_rejects_malformed() {
        assert!(parse_tile_uri("https://evil/1/2/3").is_err());
        assert!(parse_tile_uri("tile://localhost/osm/z/2/3").is_err());
        assert!(parse_tile_uri("tile://localhost/1/2").is_err());
    }

    #[test]
    fn upstream_url_for_each_valid_layer_and_reject_unknown() {
        for layer in VALID_TILE_LAYERS {
            let url = tile_upstream_url(layer, 3, 1, 2).unwrap();
            assert!(url.starts_with("https://"), "{layer} → {url}");
        }
        assert!(tile_upstream_url("nope", 3, 1, 2).is_err());
    }
}

#[cfg(test)]
mod startup_backfill_tests {
    use super::*;

    /// The virtual-run backfill (#192) runs once at startup and marks
    /// itself done.
    #[test]
    fn startup_files_virtual_runs_once() {
        let conn = db::test_db();
        conn.execute(
            "INSERT INTO activity (id, start_time, sport_type, sub_sport) VALUES ('zr', '2026-01-01T10:00:00+00:00', 'run', 'virtual_activity')",
            [],
        )
        .unwrap();
        run_startup_backfills(&conn).unwrap();
        let sport: String = conn
            .query_row("SELECT sport_type FROM activity WHERE id = 'zr'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(sport, "virtual_run");
        assert_eq!(
            db::settings::get_setting(&conn, "virtual_runs_refiled_v1").unwrap().as_deref(),
            Some("1")
        );
    }

    /// The simulated-course scrub at startup (#190) follows the geocoder
    /// switch: with it off, a virtual ride's name is the user's and stays,
    /// while the segment efforts go; and the flag keeps it from running
    /// again, so a name typed later is never looked at.
    #[test]
    fn startup_scrub_keeps_typed_names_while_the_geocoder_is_off() {
        let conn = db::test_db();
        conn.execute(
            "INSERT INTO activity (id, start_time, sport_type, location_name) VALUES ('zw', '2026-01-01T10:00:00+00:00', 'virtual_ride', 'Home trainer')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO segment (id, name, sport, source_activity_id, created_at, distance_m, start_lat, start_lon, end_lat, end_lon, min_lat, max_lat, min_lon, max_lon) VALUES ('s', 'S', 'ride', 'zw', '', 100, 0, 0, 0, 0, 0, 0, 0, 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO segment_effort (segment_id, activity_id, start_idx, end_idx, elapsed_s, distance_m) VALUES ('s', 'zw', 0, 1, 10, 100)",
            [],
        )
        .unwrap();

        run_startup_backfills(&conn).unwrap();

        let name: Option<String> = conn
            .query_row("SELECT location_name FROM activity WHERE id = 'zw'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(name.as_deref(), Some("Home trainer"));
        let efforts: i64 = conn.query_row("SELECT COUNT(*) FROM segment_effort", [], |r| r.get(0)).unwrap();
        assert_eq!(efforts, 0);
        assert_eq!(
            db::settings::get_setting(&conn, "simulated_course_scrubbed_v1").unwrap().as_deref(),
            Some("1")
        );

        // The geocoder switched on later: the one-time pass is done and
        // does not come back for the name.
        db::settings::set_setting(&conn, import::geocoding::GEOCODING_SETTING, "true").unwrap();
        run_startup_backfills(&conn).unwrap();
        let name: Option<String> = conn
            .query_row("SELECT location_name FROM activity WHERE id = 'zw'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(name.as_deref(), Some("Home trainer"));
    }

    /// A Zwift ride imported before #224: its stored zeros go, its power
    /// summary is filled from the track with the earlier ride's FTP, once.
    fn zwift_before_224() -> rusqlite::Connection {
        let conn = db::test_db();
        conn.execute_batch(
            "INSERT INTO activity (id, start_time, sport_type, threshold_power_w)
               VALUES ('edge', '2026-10-10T08:00:00+00:00', 'ride', 250);
             INSERT INTO activity (id, start_time, sport_type, duration_s, avg_hr, normalized_power_w)
               VALUES ('zwift', '2026-10-17T18:00:00+00:00', 'virtual_ride', 120, 0, 0);",
        )
        .unwrap();
        for i in 0..120 {
            conn.execute(
                "INSERT INTO trackpoint (activity_id, t, power_w) VALUES ('zwift', ?1, 200)",
                [format!("2026-10-17T18:{:02}:{:02}+00:00", i / 60, i % 60)],
            )
            .unwrap();
        }
        conn
    }

    fn zwift_summary(state: &AppState) -> (Option<f64>, Option<f64>, Option<f64>) {
        let conn = state.db.lock().unwrap();
        conn.query_row(
            "SELECT avg_hr, normalized_power_w, threshold_power_w FROM activity WHERE id = 'zwift'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap()
    }

    fn flag(state: &AppState, name: &str) -> bool {
        db::settings::get_setting(&state.db.lock().unwrap(), name).unwrap().is_some()
    }

    #[test]
    fn startup_fills_old_power_summaries_once() {
        let state = test_support::app_state_on(zwift_before_224());
        run_power_summary_fill(&state);
        let (hr, np, ftp) = zwift_summary(&state);
        assert_eq!(hr, None, "the stored 0 bpm is gone");
        assert!((np.unwrap() - 200.0).abs() < 1e-9, "an NP of 0 read as missing, then filled");
        assert_eq!(ftp, Some(250.0));
        assert!(flag(&state, ZERO_READINGS_FLAG) && flag(&state, POWER_SUMMARY_FLAG));

        // Once: a gap that opens later is not the startup fill's to close.
        state
            .db
            .lock()
            .unwrap()
            .execute("UPDATE activity SET avg_hr = 0, normalized_power_w = NULL WHERE id = 'zwift'", [])
            .unwrap();
        run_power_summary_fill(&state);
        assert_eq!(zwift_summary(&state).0, Some(0.0));
        assert_eq!(zwift_summary(&state).1, None);
    }

    /// One activity that cannot be filled is rolled back and skipped: the
    /// rest of the vault is filled and the walk is done.
    #[test]
    fn a_failed_activity_is_skipped_and_the_rest_filled() {
        let conn = zwift_before_224();
        conn.execute_batch(
            "INSERT INTO activity (id, start_time, sport_type) VALUES ('zwift2', '2026-10-18T18:00:00+00:00', 'virtual_ride');
             CREATE TRIGGER refuse_fill BEFORE INSERT ON time_in_zone WHEN NEW.activity_id = 'zwift'
             BEGIN SELECT RAISE(ABORT, 'fill refused'); END;",
        )
        .unwrap();
        for i in 0..60 {
            conn.execute(
                "INSERT INTO trackpoint (activity_id, t, power_w) VALUES ('zwift2', ?1, 180)",
                [format!("2026-10-18T18:00:{i:02}+00:00")],
            )
            .unwrap();
        }
        let state = test_support::app_state_on(conn);
        run_power_summary_fill(&state);
        let (hr, np, ftp) = zwift_summary(&state);
        assert_eq!(hr, None, "the zeros went before the fill");
        assert_eq!((np, ftp), (None, None), "rolled back");
        let ftp2: Option<f64> = state
            .db
            .lock()
            .unwrap()
            .query_row("SELECT threshold_power_w FROM activity WHERE id = 'zwift2'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(ftp2, Some(250.0), "the next one is filled all the same");
        assert!(flag(&state, POWER_SUMMARY_FLAG));
    }

    /// A failure of the run itself leaves both flags unset, to retry next launch.
    #[test]
    fn a_failed_power_summary_fill_retries_next_launch() {
        let state = test_support::app_state_on(zwift_before_224());
        state
            .db
            .lock()
            .unwrap()
            .execute_batch(
                "CREATE TRIGGER refuse_update BEFORE UPDATE ON activity
                 BEGIN SELECT RAISE(ABORT, 'update refused'); END;",
            )
            .unwrap();
        run_power_summary_fill(&state);
        assert!(!flag(&state, ZERO_READINGS_FLAG) && !flag(&state, POWER_SUMMARY_FLAG));
        assert_eq!(zwift_summary(&state), (Some(0.0), Some(0.0), None));

        state.db.lock().unwrap().execute_batch("DROP TRIGGER refuse_update").unwrap();
        run_power_summary_fill(&state);
        assert_eq!(zwift_summary(&state).2, Some(250.0));
        assert!(flag(&state, ZERO_READINGS_FLAG) && flag(&state, POWER_SUMMARY_FLAG));
    }
}

/// Why the vault would not open at boot (#244): each reason the app words
/// itself is coded, with the library's words in `{detail}`, so the vault
/// error screen says it in the interface language.
#[cfg(test)]
mod vault_boot_tests {
    use super::*;
    use test_support::ScratchDir;

    fn boot_error(vault: &Path) -> CommandError {
        let (_, error) = open_at_boot(vault, false).unwrap();
        error.expect("the vault opened")
    }

    #[test]
    fn a_good_vault_opens_and_an_encrypted_one_waits_for_unlock() {
        let dir = ScratchDir::new("boot_ok");
        let (conn, error) = open_at_boot(&dir, false).unwrap();
        assert!(error.is_none());
        assert!(conn.path().is_some_and(|p| p.ends_with("vault.db")));
        // Encrypted: nothing is opened from disk, nothing is wrong.
        let other = ScratchDir::new("boot_encrypted");
        let (conn, error) = open_at_boot(&other, true).unwrap();
        assert!(error.is_none());
        assert!(conn.path().is_none_or(|p| p.is_empty()));
        assert!(!other.join("vault.db").exists());
    }

    fn user_version(conn: &Connection) -> i64 {
        conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap()
    }

    /// At the last migration a vault opens as it is; one past it, it is a
    /// vault a newer build migrated, said with both versions.
    #[test]
    fn a_vault_newer_than_the_app_says_both_versions() {
        let dir = ScratchDir::new("boot_too_new");
        let latest = user_version(&init_vault(&dir).unwrap());
        let (conn, error) = open_at_boot(&dir, false).unwrap();
        assert!(error.is_none());
        assert_eq!(user_version(&conn), latest);
        conn.pragma_update(None, "user_version", latest + 1).unwrap();
        drop(conn);

        let err = boot_error(&dir);
        assert_eq!(
            err,
            CommandError::coded(
                "vault.tooNew",
                format!(
                    "This vault was last used by a newer version of Syzify (data format {}, this app reads up to {}). Update the app to open it.",
                    latest + 1,
                    latest
                ),
            )
            .param("vault", latest + 1)
            .param("app", latest)
        );
    }

    #[test]
    fn a_vault_inside_another_is_not_created() {
        let dir = ScratchDir::new("boot_nested");
        init_vault(&dir).unwrap();
        let inner = dir.join("Syzify");
        let err = boot_error(&inner);
        assert_eq!(err.code, Some("vault.nested"));
        assert_eq!(err.params["path"], inner.display().to_string());
        assert_eq!(err.params["vault"], dir.display().to_string());
        assert!(!inner.exists());
    }

    #[test]
    fn a_vault_folder_that_cannot_be_made_names_it() {
        let dir = ScratchDir::new("boot_dirs");
        // A file where the vault should be: no folder can go under it.
        let root = dir.join("not-a-folder");
        fs::write(&root, b"x").unwrap();
        let err = boot_error(&root);
        assert_eq!(err.code, Some("vault.createFolderFailed"));
        assert_eq!(err.params["folder"], "raw");
        let detail = err.params["detail"].as_str().unwrap().to_string();
        assert_eq!(err.message, format!("Failed to create vault/raw: {detail}"));
    }

    #[test]
    fn a_database_that_will_not_open_keeps_the_library_words() {
        let dir = ScratchDir::new("boot_db_dir");
        fs::create_dir_all(dir.join("vault.db")).unwrap();
        let err = boot_error(&dir);
        assert_eq!(err.code, Some("vault.dbOpenFailed"));
        let detail = err.params["detail"].as_str().unwrap().to_string();
        assert!(!detail.is_empty());
        assert_eq!(err.message, format!("Failed to open database: {detail}"));
    }

    #[test]
    fn a_file_that_is_no_database_fails_at_wal() {
        let dir = ScratchDir::new("boot_garbage");
        fs::write(dir.join("vault.db"), vec![b'x'; 4096]).unwrap();
        let err = boot_error(&dir);
        assert_eq!(err.code, Some("vault.walFailed"));
        let detail = err.params["detail"].as_str().unwrap().to_string();
        assert!(detail.contains("not a database"), "{detail}");
        assert_eq!(err.message, format!("Failed to enable WAL: {detail}"));
    }

    #[test]
    fn a_migration_that_fails_keeps_the_library_words() {
        let dir = ScratchDir::new("boot_migrate");
        Connection::open(dir.join("vault.db")).unwrap().execute_batch("CREATE TABLE trackpoint (x INTEGER);").unwrap();
        let err = boot_error(&dir);
        assert_eq!(err.code, Some("vault.migrateFailed"));
        let detail = err.params["detail"].as_str().unwrap().to_string();
        assert!(detail.contains("activity_id"), "{detail}");
        assert_eq!(err.message, format!("Failed to run migrations: {detail}"));
    }
}
