use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use tauri::{AppHandle, Emitter};

const POLL_INTERVAL_SECS: u64 = 3;

/// Known device activity paths relative to the volume mount point.
/// Each entry: (volume_name_prefix, sub_paths_to_check)
const DEVICE_PATHS: &[(&str, &[&str])] = &[
    ("GARMIN", &["Garmin/Activity", "GARMIN/Activity"]),
    ("ELEMNT", &["activities"]),
    ("COROS", &["Activity"]),
    ("SUUNTO", &["moves"]),
    ("POLAR", &["DATA"]),
];

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

/// Recursively scan a directory for workout files.
fn scan_workout_files(dir: &Path) -> Vec<String> {
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                files.extend(scan_workout_files(&path));
            } else if path.is_file() && is_workout_file(&path) {
                if let Some(s) = path.to_str() {
                    files.push(s.to_string());
                }
            }
        }
    }
    files
}

/// Given a newly mounted volume path, check if it matches any known device
/// and return workout files found.
fn check_volume_for_workouts(volume_path: &Path) -> Vec<String> {
    let volume_name = match volume_path.file_name().and_then(|n| n.to_str()) {
        Some(name) => name.to_uppercase(),
        None => return Vec::new(),
    };

    for (prefix, sub_paths) in DEVICE_PATHS {
        if volume_name.starts_with(prefix) {
            for sub in *sub_paths {
                let activity_dir = volume_path.join(sub);
                if activity_dir.is_dir() {
                    let files = scan_workout_files(&activity_dir);
                    if !files.is_empty() {
                        return files;
                    }
                }
            }
        }
    }

    Vec::new()
}

/// Get the set of currently mounted volumes under /Volumes.
fn current_volumes() -> HashSet<PathBuf> {
    let volumes_dir = Path::new("/Volumes");
    let mut set = HashSet::new();
    if let Ok(entries) = std::fs::read_dir(volumes_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                set.insert(path);
            }
        }
    }
    set
}

/// Start a background thread that polls /Volumes for new mounts.
/// When a new volume appears and matches a known device, emits
/// `watch:files-detected` with the found workout files.
pub fn start_volume_monitor(app_handle: AppHandle) {
    std::thread::spawn(move || {
        let mut known_volumes = current_volumes();

        loop {
            std::thread::sleep(Duration::from_secs(POLL_INTERVAL_SECS));

            let now = current_volumes();
            let new_volumes: Vec<PathBuf> = now.difference(&known_volumes).cloned().collect();

            for vol in &new_volumes {
                let files = check_volume_for_workouts(vol);
                if !files.is_empty() {
                    let volume_name = vol
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("device");
                    eprintln!(
                        "Volume monitor: detected {} workout files on {}",
                        files.len(),
                        volume_name
                    );
                    let _ = app_handle.emit(
                        "watch:files-detected",
                        serde_json::json!({ "files": files }),
                    );
                }
            }

            known_volumes = now;
        }
    });
}
