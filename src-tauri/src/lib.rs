mod commands;
pub mod crypto;
// Public like `parser`/`crypto`: dev examples (one-off vault maintenance)
// call the real db layer instead of duplicating its logic.
pub mod db;
mod export;
mod geocoding;
mod import;
mod maps;
mod models;
// pub for dev tooling (examples/find_multisport); not a public API promise.
pub mod parser;
mod plugins;
pub mod state;
mod util;
mod vault;

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use tauri::Manager;
use state::AppState;
use maps::tile_cache;

fn ensure_vault_dirs(vault_path: &Path) -> Result<(), String> {
    // NOTE: no subdirectories under raw/ — encrypt_all_raw_files walks it
    // non-recursively, so a nested file would silently stay plaintext.
    for sub in ["raw", "tiles", "photos", "plugins"] {
        fs::create_dir_all(vault_path.join(sub))
            .map_err(|e| format!("Failed to create vault/{}: {}", sub, e))?;
    }
    Ok(())
}

/// WAL + migrations on an already-open connection. Kept cheap so it can run in
/// the synchronous boot path; heavy data backfills run separately off-thread
/// (see `run_startup_backfills`).
fn configure_and_migrate(conn: &mut Connection) -> Result<(), String> {
    conn.pragma_update(None, "journal_mode", "WAL")
        .map_err(|e| format!("Failed to enable WAL: {}", e))?;
    conn.pragma_update(None, "foreign_keys", "ON")
        .map_err(|e| format!("Failed to enable foreign keys: {}", e))?;

    db::migrations::run_migrations(conn)
        .map_err(|e| format!("Failed to run migrations: {}", e))?;
    Ok(())
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
    Ok(())
}

/// Open the plaintext vault database (database scope off).
pub(crate) fn init_vault(vault_path: &Path) -> Result<Connection, String> {
    ensure_vault_dirs(vault_path)?;
    let mut conn = Connection::open(vault_path.join("vault.db"))
        .map_err(|e| format!("Failed to open database: {}", e))?;
    configure_and_migrate(&mut conn)?;
    Ok(conn)
}

/// Open the SQLCipher-encrypted vault database with `key` (database scope on).
/// Called from `unlock_vault`, not at boot.
pub(crate) fn init_vault_encrypted(
    vault_path: &Path,
    key: &[u8; 32],
) -> Result<Connection, String> {
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

/// Watch-folders kill switch: the Settings card is hidden (6ab5a14), and the
/// background watcher is paused with it so folders added earlier don't keep
/// auto-importing with no UI to manage them. Flip to true to bring the
/// runtime back — the folder list and the auto-import setting survive in the
/// DB untouched.
pub(crate) const WATCH_FOLDERS_ENABLED: bool = false;

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

    if WATCH_FOLDERS_ENABLED {
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
        let conn = match state.db.lock() {
            Ok(c) => c,
            Err(_) => return,
        };
        if let Err(e) = run_startup_backfills(&conn) {
            eprintln!("Startup backfill failed (will retry next launch): {}", e);
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
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
            let mut vault_error: Option<String> = None;
            let conn = if db_encrypted {
                Connection::open_in_memory()
                    .map_err(|e| format!("Failed to open placeholder db: {}", e))?
            } else {
                match init_vault(&vault_path) {
                    Ok(c) => c,
                    Err(e) => {
                        vault_error = Some(e);
                        Connection::open_in_memory()
                            .map_err(|e| format!("Failed to open placeholder db: {}", e))?
                    }
                }
            };

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
            commands::activities::get_activity_locations,
            commands::activities::get_adjacent_activities,
            commands::activities::get_activity_record_badges,
            commands::activities::get_calendar_data,
            commands::activities::get_used_sport_types,
            commands::activities::get_activity_year_range,
            commands::export::export_activity_gpx,
            commands::export::backup_vault,
            commands::export::restore_vault,
            commands::tags::get_tags,
            commands::tags::create_tag,
            commands::tags::set_activity_tags,
            commands::tiles::get_tile_cache_info,
            commands::tiles::clear_tile_cache,
            commands::settings::get_watch_folders,
            commands::settings::add_watch_folder,
            commands::settings::remove_watch_folder,
            commands::settings::scan_watch_folders,
            commands::settings::get_vault_path,
            commands::settings::get_vault_error,
            commands::settings::relocate_vault,
            commands::settings::switch_vault,
            commands::settings::restart_app,
            commands::settings::get_detected_devices,
            commands::settings::preview_watch_folders,
            commands::settings::get_suggested_watch_paths,
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
