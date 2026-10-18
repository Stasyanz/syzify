// The volume monitor is only wired up on macOS (lib.rs gates the call on
// `target_os = "macos"`; it polls /Volumes), but we still compile it on the
// other platforms so Linux CI type-checks it — without dead-code noise.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

use crate::state::AppState;

const POLL_INTERVAL_SECS: u64 = 3;

/// Known device activity paths relative to the volume mount point.
/// Each entry: (volume_name_prefix, sub_paths_to_check)
const DEVICE_PATHS: &[(&str, &[&str])] = &[
    // Activities and the Monitor files behind the recovery index alike. A
    // watch keeps rides in Activity, an Edge bike computer in Activities.
    (
        "GARMIN",
        &[
            "Garmin/Activity",
            "GARMIN/Activity",
            "Garmin/Activities",
            "GARMIN/Activities",
            "Garmin/Monitor",
            "GARMIN/Monitor",
        ],
    ),
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
    volumes_under(Path::new("/Volumes"))
}

/// The directories directly under `volumes_dir` — the mount points.
fn volumes_under(volumes_dir: &Path) -> HashSet<PathBuf> {
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

/// The workout files on every device volume mounted right now. The poll
/// loop takes these volumes as already known, so the launch catch-up
/// (#221) is what offers a device plugged in before Syzify started.
pub fn mounted_device_files() -> Vec<String> {
    device_files_under(Path::new("/Volumes"))
}

/// [`mounted_device_files`] for any directory of mount points.
fn device_files_under(volumes_dir: &Path) -> Vec<String> {
    volumes_under(volumes_dir)
        .iter()
        .flat_map(|vol| check_volume_for_workouts(vol))
        .collect()
}

/// What a newly mounted volume offers: its device files the vault lacks.
fn files_to_offer(state: &AppState, volume_path: &Path) -> Vec<String> {
    crate::import::catch_up::device_files_not_in_vault(state, check_volume_for_workouts(volume_path))
}

/// Start a background thread that polls /Volumes for new mounts.
/// When a new volume appears and matches a known device, emits
/// `watch:files-detected` with the workout files the vault lacks.
pub fn start_volume_monitor(app_handle: AppHandle) {
    std::thread::spawn(move || {
        let mut known_volumes = current_volumes();

        loop {
            std::thread::sleep(Duration::from_secs(POLL_INTERVAL_SECS));

            let now = current_volumes();
            let new_volumes: Vec<PathBuf> = now.difference(&known_volumes).cloned().collect();

            for vol in &new_volumes {
                let files = files_to_offer(&app_handle.state::<AppState>(), vol);
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
        let root = crate::test_support::ScratchDir::new("volumes");
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
    }

    /// An Edge bike computer keeps its rides in Garmin/Activities, not the
    /// watch's Activity: the device walk takes both.
    #[test]
    fn an_edge_volume_yields_its_activities() {
        let root = crate::test_support::ScratchDir::new("edge-volume");
        let edge = root.join("GARMIN");
        std::fs::create_dir_all(edge.join("Garmin/Activities")).unwrap();
        std::fs::write(edge.join("Garmin/Activities/2026-10-18-18-31-04.fit"), b"x").unwrap();
        let found: Vec<String> = check_volume_for_workouts(&edge).iter().map(|f| f.to_lowercase()).collect();
        assert_eq!(
            found,
            vec![edge.join("Garmin/Activities/2026-10-18-18-31-04.fit").to_str().unwrap().to_lowercase()]
        );
    }

    /// A plugged-in device offers what the vault lacks, not every ride on it.
    #[test]
    fn a_new_volume_offers_only_what_the_vault_lacks() {
        let root = crate::test_support::ScratchDir::new("offer-volume");
        let edge = root.join("GARMIN");
        let rides = edge.join("Garmin/Activities");
        std::fs::create_dir_all(&rides).unwrap();
        std::fs::write(rides.join("old.fit"), b"old ride").unwrap();
        std::fs::write(rides.join("new.fit"), b"new ride").unwrap();
        let conn = crate::db::test_db();
        let old = crate::import::catch_up::content_hash(&rides.join("old.fit")).unwrap();
        crate::db::raw_files::insert_raw_file(
            &conn,
            &crate::models::raw_file::RawFile {
                id: "rf-old".into(),
                activity_id: None,
                path_in_vault: "raw/old.fit".into(),
                original_path: None,
                format: "fit".into(),
                hash_sha256: old,
                imported_at: String::new(),
                parse_status: "ok".into(),
                failure_reason: None,
            },
        )
        .unwrap();
        let offered: Vec<String> = files_to_offer(&crate::test_support::app_state_on(conn), &edge)
            .iter()
            .map(|f| f.to_lowercase())
            .collect();
        assert_eq!(offered, vec![rides.join("new.fit").to_str().unwrap().to_lowercase()]);
    }

    /// The real mount points: directories only, and none at all (not an
    /// error) where /Volumes does not exist, as on Linux.
    #[test]
    fn current_volumes_lists_directories_or_nothing() {
        assert!(current_volumes().iter().all(|vol| vol.is_dir()));
    }

    /// The launch catch-up's walk (#221): every device mount point under the
    /// directory, the device's files only; a stray file there is no volume.
    #[test]
    fn device_files_under_walks_every_device_mount() {
        let root = crate::test_support::ScratchDir::new("mounts");
        std::fs::create_dir_all(root.join("GARMIN/Garmin/Activity")).unwrap();
        std::fs::write(root.join("GARMIN/Garmin/Activity/ride.fit"), b"x").unwrap();
        std::fs::create_dir_all(root.join("BACKUP/Garmin/Activity")).unwrap();
        std::fs::write(root.join("BACKUP/Garmin/Activity/old.fit"), b"x").unwrap();
        std::fs::write(root.join("stray.fit"), b"x").unwrap();
        let found: Vec<String> = device_files_under(&root).iter().map(|f| f.to_lowercase()).collect();
        assert_eq!(found, vec![root.join("GARMIN/Garmin/Activity/ride.fit").to_str().unwrap().to_lowercase()]);
        assert!(device_files_under(&root.join("absent")).is_empty());
    }
}
