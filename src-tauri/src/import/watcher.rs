use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tauri::{AppHandle, Emitter, Manager};

use crate::state::AppState;

const DEBOUNCE_SECS: u64 = 2;
fn is_workout_file(path: &std::path::Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());
    match ext.as_deref() {
        Some("gpx" | "fit" | "tcx") => true,
        Some("gz") => {
            // Only accept .fit.gz, .gpx.gz, .tcx.gz
            path.file_stem()
                .and_then(|s| std::path::Path::new(s).extension())
                .and_then(|e| e.to_str())
                .map(|e| ["gpx", "fit", "tcx"].contains(&e.to_ascii_lowercase().as_str()))
                .unwrap_or(false)
        }
        _ => false,
    }
}

/// Start watching the given folder paths for new workout files.
/// Returns a `RecommendedWatcher` that must be kept alive.
pub fn start_watching(
    app_handle: AppHandle,
    paths: Vec<String>,
) -> Result<RecommendedWatcher, String> {
    let pending: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));
    let last_event: Arc<Mutex<Instant>> = Arc::new(Mutex::new(Instant::now()));
    let recently_flushed: Arc<Mutex<HashMap<String, Instant>>> =
        Arc::new(Mutex::new(HashMap::new()));

    let pending_clone = Arc::clone(&pending);
    let last_event_clone = Arc::clone(&last_event);
    let recently_flushed_clone = Arc::clone(&recently_flushed);
    let app_clone = app_handle.clone();

    // Spawn a debounce flusher thread
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_millis(500));

            let should_flush = {
                let last = last_event_clone.lock().unwrap();
                let p = pending_clone.lock().unwrap();
                !p.is_empty() && last.elapsed() >= Duration::from_secs(DEBOUNCE_SECS)
            };

            if should_flush {
                let files: Vec<String> = {
                    let mut p = pending_clone.lock().unwrap();
                    let drained: Vec<String> = p.drain().collect();
                    drained
                };

                if !files.is_empty() {
                    let now = Instant::now();
                    let mut rf = recently_flushed_clone.lock().unwrap();
                    for f in &files {
                        rf.insert(f.clone(), now);
                    }
                    // Evict entries older than 30 seconds
                    rf.retain(|_, t| t.elapsed() < Duration::from_secs(30));

                    let _ = app_clone.emit("watch:files-detected", serde_json::json!({ "files": files }));
                }
            }
        }
    });

    let pending_ev = Arc::clone(&pending);
    let last_event_ev = Arc::clone(&last_event);
    let recently_flushed_ev = Arc::clone(&recently_flushed);

    let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
        if let Ok(event) = res {
            match event.kind {
                EventKind::Create(_) | EventKind::Modify(_) => {
                    let mut found_any = false;
                    let rf = recently_flushed_ev.lock().unwrap();
                    for path in &event.paths {
                        if path.is_file() && is_workout_file(path) {
                            if let Some(s) = path.to_str() {
                                // Skip files that were recently flushed
                                if rf.contains_key(s) {
                                    continue;
                                }
                                let mut p = pending_ev.lock().unwrap();
                                p.insert(s.to_string());
                                found_any = true;
                            }
                        }
                    }
                    if found_any {
                        let mut last = last_event_ev.lock().unwrap();
                        *last = Instant::now();
                    }
                }
                _ => {}
            }
        }
    })
    .map_err(|e| format!("Failed to create watcher: {}", e))?;

    for path_str in &paths {
        let path = PathBuf::from(path_str);
        if path.is_dir() {
            watcher
                .watch(&path, RecursiveMode::Recursive)
                .map_err(|e| format!("Failed to watch {}: {}", path_str, e))?;
        }
    }

    Ok(watcher)
}

/// Read current watch folder paths from the database.
pub fn get_watch_paths_from_db(app_handle: &AppHandle) -> Vec<String> {
    let state = app_handle.state::<AppState>();
    let conn = match state.db.lock() {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let mut stmt = match conn.prepare("SELECT path FROM watch_folder") {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let rows = match stmt.query_map([], |row| row.get::<_, String>(0)) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    rows.filter_map(|r| r.ok()).collect()
}
