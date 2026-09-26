// The volume monitor is only wired up on macOS (lib.rs gates the call on
// `target_os = "macos"`; it polls /Volumes), but we still compile it on the
// other platforms so Linux CI type-checks it — without dead-code noise.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use tauri::{AppHandle, Emitter};

const POLL_INTERVAL_SECS: u64 = 3;

/// Known device activity paths relative to the volume mount point.
/// Each entry: (volume_name_prefix, sub_paths_to_check)
const DEVICE_PATHS: &[(&str, &[&str])] = &[
    // Activities and the Monitor files behind the recovery index alike.
    ("GARMIN", &["Garmin/Activity", "GARMIN/Activity", "Garmin/Monitor", "GARMIN/Monitor"]),
    ("ELEMNT", &["activities"]),
    ("COROS", &["Activity"]),
    ("SUUNTO", &["moves"]),
    ("POLAR", &["DATA"]),
];

/// Given a newly mounted volume path, check if it matches any known device
/// and return the workout and monitoring files found under every known
/// folder of it. A folder the walk cannot read is logged, not silently
/// "empty".
fn check_volume_for_workouts(volume_path: &Path) -> Vec<String> {
    let volume_name = match volume_path.file_name().and_then(|n| n.to_str()) {
        Some(name) => name.to_uppercase(),
        None => return Vec::new(),
    };

    let mut files = Vec::new();
    // The spellings differ only in case, and a device's FAT32 (like macOS)
    // is case-insensitive: "Garmin/Activity" and "GARMIN/Activity" are the
    // same folder, walked once.
    let mut walked = HashSet::new();
    for (prefix, sub_paths) in DEVICE_PATHS {
        if !volume_name.starts_with(prefix) {
            continue;
        }
        for sub in *sub_paths {
            let dir = volume_path.join(sub);
            if !dir.is_dir() {
                continue;
            }
            let identity = std::fs::canonicalize(&dir).unwrap_or_else(|_| dir.clone());
            if !walked.insert(identity) {
                continue;
            }
            // The pipeline's own bounded walk: a device folder is never
            // large, and the filter stays defined once.
            match crate::import::pipeline::folder_files(&dir) {
                Ok(found) => files.extend(found),
                Err(e) => eprintln!("Volume monitor: cannot read {}: {}", dir.display(), e),
            }
        }
    }
    files
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A Garmin mount yields its Activity AND Monitor files, each once even
    /// where the two spellings of the folder are one folder (case-insensitive
    /// filesystems); a volume with an unknown name yields nothing.
    #[test]
    fn a_garmin_volume_yields_activity_and_monitor_files() {
        let root = std::env::temp_dir().join(format!("syzify-volumes-{}", uuid::Uuid::new_v4()));
        let garmin = root.join("GARMIN");
        std::fs::create_dir_all(garmin.join("GARMIN/Activity")).unwrap();
        std::fs::create_dir_all(garmin.join("GARMIN/Monitor")).unwrap();
        std::fs::write(garmin.join("GARMIN/Activity/ride.fit"), b"x").unwrap();
        std::fs::write(garmin.join("GARMIN/Monitor/M1.FIT"), b"x").unwrap();
        std::fs::write(garmin.join("GARMIN/Monitor/notes.txt"), b"x").unwrap();
        // Which spelling the paths carry depends on the filesystem's case
        // rules; what matters is each file once.
        let mut found: Vec<String> = check_volume_for_workouts(&garmin).iter().map(|f| f.to_lowercase()).collect();
        found.sort();
        assert_eq!(
            found,
            vec![
                garmin.join("GARMIN/Activity/ride.fit").to_str().unwrap().to_lowercase(),
                garmin.join("GARMIN/Monitor/M1.FIT").to_str().unwrap().to_lowercase()
            ]
        );
        let other = root.join("BACKUP");
        std::fs::create_dir_all(other.join("GARMIN/Activity")).unwrap();
        std::fs::write(other.join("GARMIN/Activity/ride.fit"), b"x").unwrap();
        assert!(check_volume_for_workouts(&other).is_empty());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
