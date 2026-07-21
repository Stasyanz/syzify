use tauri::State;

use crate::db;
use crate::models::activity::{
    Activity, ActivityFilters, ActivityLocation, ActivitySummary, ActivityUpdate, DaySummary,
};
use crate::models::exercise_set::ExerciseSet;
use crate::models::hrv_sample::HrvSample;
use crate::models::lap::Lap;
use crate::models::swim_length::SwimLength;
use crate::models::time_in_zone::TimeInZone;
use crate::models::trackpoint::TrackPointColumns;
use crate::state::AppState;

#[tauri::command]
pub fn get_activities(
    filters: ActivityFilters,
    state: State<AppState>,
) -> Result<Vec<ActivitySummary>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut activities = db::activities::get_activities(&conn, &filters).map_err(|e| e.to_string())?;

    // Fill tags for each activity
    for activity in &mut activities {
        activity.tags = db::tags::get_tags_for_activity(&conn, &activity.id)
            .unwrap_or_default();
    }

    Ok(activities)
}

#[tauri::command]
pub fn get_activity_detail(
    id: String,
    state: State<AppState>,
) -> Result<ActivityDetail, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;

    let activity = db::activities::get_activity_by_id(&conn, &id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Activity not found: {}", id))?;

    let trackpoints = db::trackpoints::get_trackpoints_columnar(&conn, &id)
        .map_err(|e| e.to_string())?;

    let tags = db::tags::get_tags_for_activity(&conn, &id).unwrap_or_default();
    let laps = db::laps::get_laps(&conn, &id).map_err(|e| e.to_string())?;
    let legs = db::multisport_legs::get_legs(&conn, &id).map_err(|e| e.to_string())?;
    let lengths = db::swim_lengths::get_swim_lengths(&conn, &id).map_err(|e| e.to_string())?;
    let sets = db::exercise_sets::get_exercise_sets(&conn, &id).map_err(|e| e.to_string())?;
    let time_in_zones = db::time_in_zones::get_time_in_zones(&conn, &id).map_err(|e| e.to_string())?;
    let hrv_samples = db::hrv_samples::get_hrv_samples(&conn, &id).map_err(|e| e.to_string())?;

    Ok(ActivityDetail {
        activity,
        trackpoints,
        tags,
        laps,
        legs,
        lengths,
        sets,
        time_in_zones,
        hrv_samples,
    })
}

#[derive(serde::Serialize)]
pub struct ActivityDetail {
    pub activity: Activity,
    pub trackpoints: TrackPointColumns,
    pub tags: Vec<String>,
    pub laps: Vec<Lap>,
    pub legs: Vec<crate::models::multisport_leg::MultisportLeg>,
    pub lengths: Vec<SwimLength>,
    pub sets: Vec<ExerciseSet>,
    pub time_in_zones: Vec<TimeInZone>,
    pub hrv_samples: Vec<HrvSample>,
}

#[tauri::command]
pub fn update_activity(
    id: String,
    updates: ActivityUpdate,
    state: State<AppState>,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::activities::update_activity(&conn, &id, &updates).map_err(|e| e.to_string())?;
    // A sport change flips whether this activity earns running distance PBs —
    // recompute (or clear) its best-effort splits so records stay correct.
    if updates.sport_type.is_some() {
        db::best_efforts::recompute_for_activity(&conn, &id).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn delete_activity(id: String, state: State<AppState>) -> Result<(), String> {
    delete_activity_core(&state, &id)
}

/// Combine several standalone same-day activities into one triathlon
/// container. Returns the new container's id (the frontend navigates to it).
#[tauri::command]
pub fn merge_into_triathlon(
    activity_ids: Vec<String>,
    state: State<AppState>,
) -> Result<String, String> {
    let new_id = uuid::Uuid::new_v4().to_string();
    let mut conn = state.db.lock().map_err(|e| e.to_string())?;
    db::multisport_legs::merge_into_triathlon(&mut conn, &new_id, &activity_ids)
        .map_err(|e| e.to_string())
}

/// Reverse a merge: free the legs and delete the container.
#[tauri::command]
pub fn unmerge_triathlon(id: String, state: State<AppState>) -> Result<(), String> {
    let mut conn = state.db.lock().map_err(|e| e.to_string())?;
    db::multisport_legs::unmerge(&mut conn, &id).map_err(|e| e.to_string())
}

/// The State-free part of delete, testable against a bare [`AppState`].
pub(crate) fn delete_activity_core(state: &AppState, id: &str) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;

    // Remove photo files from vault before deleting the activity row
    // (DB rows themselves are removed via ON DELETE CASCADE on photo.activity_id)
    if let Ok(photos) = db::photos::get_photos_for_activity(&conn, id) {
        for p in &photos {
            let _ = std::fs::remove_file(state.vault_path.join(&p.path_in_vault));
            if let Some(thumb) = &p.thumbnail_path {
                let _ = std::fs::remove_file(state.vault_path.join(thumb));
            }
        }
    }
    let _ = std::fs::remove_dir_all(state.vault_path.join("photos").join(id));

    // Remove the raw source files and their rows too. The FK is ON DELETE SET
    // NULL, so leaving the rows would (a) keep the file's hash in the dedup
    // index forever — the same workout could never be imported again, silently
    // Skipped — and (b) leave the GPS track sitting in raw/ after the user
    // deleted it.
    if let Ok(raws) = db::raw_files::get_raw_files_for_activity(&conn, id) {
        for r in &raws {
            let _ = std::fs::remove_file(state.vault_path.join(&r.path_in_vault));
            // Crash-drift sibling (x ↔ x.enc): the DB path can lag the disk
            // name after an interrupted encryption toggle — remove both.
            let sibling = match r.path_in_vault.strip_suffix(".enc") {
                Some(plain) => plain.to_string(),
                None => format!("{}.enc", r.path_in_vault),
            };
            let _ = std::fs::remove_file(state.vault_path.join(&sibling));
        }
    }
    db::raw_files::delete_for_activity(&conn, id).map_err(|e| e.to_string())?;

    db::activities::delete_activity(&conn, id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_calendar_data(
    year: i32,
    month: u32,
    filters: Option<ActivityFilters>,
    state: State<AppState>,
) -> Result<Vec<DaySummary>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let filters = filters.unwrap_or_default();
    db::activities::get_calendar_data(&conn, year, month, &filters)
        .map_err(|e| e.to_string())
}

// Sport-type and best-effort recomputes are no longer manual actions: they run
// as one-time startup backfills (see configure_and_migrate in lib.rs). The
// db-layer functions (db::activities::recompute_sport_types,
// db::best_efforts::recompute_running) live on for those backfills.

#[tauri::command]
pub fn get_used_sport_types(state: State<AppState>) -> Result<Vec<String>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::activities::get_used_sport_types(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_activity_year_range(
    state: State<AppState>,
) -> Result<Option<(i32, i32)>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::activities::get_activity_year_range(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_activity_record_badges(
    id: String,
    state: State<AppState>,
) -> Result<Vec<crate::models::activity::RecordBadge>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::activities::get_record_badges(&conn, &id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_adjacent_activities(
    id: String,
    state: State<AppState>,
) -> Result<AdjacentActivities, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let (prev_id, next_id) =
        db::activities::get_adjacent_activity_ids(&conn, &id).map_err(|e| e.to_string())?;
    Ok(AdjacentActivities { prev_id, next_id })
}

#[derive(serde::Serialize)]
pub struct AdjacentActivities {
    pub prev_id: Option<String>,
    pub next_id: Option<String>,
}

#[tauri::command]
pub fn get_activity_locations(
    filters: Option<ActivityFilters>,
    state: State<AppState>,
) -> Result<Vec<ActivityLocation>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let filters = filters.unwrap_or_default();
    db::activities::get_activity_start_locations(&conn, &filters)
        .map_err(|e| e.to_string())
}

#[derive(serde::Serialize)]
pub struct LocationUpdateResult {
    pub geocoded: bool,
    pub location_name: String,
}

#[tauri::command]
pub fn update_activity_location(
    id: String,
    location_text: String,
    state: State<AppState>,
) -> Result<LocationUpdateResult, String> {
    update_activity_location_core(&state, &id, &location_text)
}

/// The State-free part of the location update, testable against a bare
/// [`AppState`].
pub(crate) fn update_activity_location_core(
    state: &AppState,
    id: &str,
    location_text: &str,
) -> Result<LocationUpdateResult, String> {
    let trimmed = location_text.trim().to_string();
    if trimmed.is_empty() {
        // Clear location
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        db::activities::clear_location(&conn, id).map_err(|e| e.to_string())?;
        return Ok(LocationUpdateResult {
            geocoded: false,
            location_name: String::new(),
        });
    }

    // Forward geocoding sends the typed text to nominatim.openstreetmap.org —
    // gated behind the same Settings toggle as the background pass (PRD §16:
    // no network call the user didn't opt into). Toggle off, like a network
    // failure, degrades to saving the text as typed. Read the toggle and drop
    // the lock BEFORE the network call — never hold the DB mutex across it.
    let allowed = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        crate::import::geocoding::geocoding_enabled(&conn)
    };
    let resolved = if allowed {
        crate::geocoding::forward_geocode(&trimmed).ok()
    } else {
        None
    };

    match resolved {
        Some((lat, lon, resolved_name)) => {
            let conn = state.db.lock().map_err(|e| e.to_string())?;
            let updates = ActivityUpdate {
                title: None,
                notes: None,
                sport_type: None,
                location_name: Some(resolved_name.clone()),
                start_lat: Some(lat),
                start_lon: Some(lon),
            };
            db::activities::update_activity(&conn, id, &updates).map_err(|e| e.to_string())?;
            Ok(LocationUpdateResult {
                geocoded: true,
                location_name: resolved_name,
            })
        }
        None => {
            // Geocoding off or network failure — save text as-is, no coordinates.
            let conn = state.db.lock().map_err(|e| e.to_string())?;
            let updates = ActivityUpdate {
                title: None,
                notes: None,
                sport_type: None,
                location_name: Some(trimmed.clone()),
                start_lat: None,
                start_lon: None,
            };
            db::activities::update_activity(&conn, id, &updates).map_err(|e| e.to_string())?;
            Ok(LocationUpdateResult {
                geocoded: false,
                location_name: trimmed,
            })
        }
    }
}

#[tauri::command]
pub fn set_activity_location_point(
    id: String,
    lat: f64,
    lon: f64,
    state: State<AppState>,
) -> Result<LocationUpdateResult, String> {
    set_activity_location_point_core(&state, &id, lat, lon)
}

/// "Set as destination point" on the route map: a picked trackpoint becomes
/// the activity's location — the same (location_name, start_lat, start_lon)
/// triple that manual text entry writes, so the library map and search pick
/// it up with no extra schema. State-free part, testable against a bare
/// [`AppState`].
pub(crate) fn set_activity_location_point_core(
    state: &AppState,
    id: &str,
    lat: f64,
    lon: f64,
) -> Result<LocationUpdateResult, String> {
    if !(lat.is_finite() && lon.is_finite() && (-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lon))
    {
        return Err(format!("invalid coordinates: {lat}, {lon}"));
    }

    // Reverse geocoding sends the picked point to nominatim.openstreetmap.org —
    // gated behind the same Settings toggle as every other lookup (PRD §16).
    // Read the toggle and drop the lock BEFORE the network call.
    let allowed = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        crate::import::geocoding::geocoding_enabled(&conn)
    };
    let resolved = if allowed {
        crate::geocoding::reverse_geocode(lat, lon).ok().flatten()
    } else {
        None
    };

    // Toggle off, network failure or "Nominatim knows no name here" all
    // degrade to showing the coordinates themselves (~1 m at 5 decimals):
    // the user asked for THIS point, an empty location would discard it.
    let (geocoded, name) = match resolved {
        Some(name) => (true, name),
        None => (false, format!("{lat:.5}, {lon:.5}")),
    };

    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let updates = ActivityUpdate {
        title: None,
        notes: None,
        sport_type: None,
        location_name: Some(name.clone()),
        start_lat: Some(lat),
        start_lon: Some(lon),
    };
    db::activities::update_activity(&conn, id, &updates).map_err(|e| e.to_string())?;
    Ok(LocationUpdateResult {
        geocoded,
        location_name: name,
    })
}

#[tauri::command]
pub fn search_activities(
    query: String,
    state: State<AppState>,
) -> Result<Vec<ActivitySummary>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::activities::search_activities(&conn, &query).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::raw_file::RawFile;
    use std::sync::{Arc, Mutex};

    fn test_state(vault: &std::path::Path) -> AppState {
        AppState {
            db: Arc::new(Mutex::new(crate::db::test_db())),
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

    /// Deleting an activity must free its raw file's dedup hash and remove the
    /// file from the vault. The FK is ON DELETE SET NULL, so before this fix
    /// the orphaned row kept the hash in the dedup index forever — the same
    /// workout could never be imported again (silently Skipped) — and the GPS
    /// track outlived the deletion on disk.
    #[test]
    fn delete_activity_frees_the_raw_hash_and_removes_the_files() {
        let vault = std::env::temp_dir().join(format!("syz_del_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(vault.join("raw")).unwrap();
        let state = test_state(&vault);

        {
            let conn = state.db.lock().unwrap();
            conn.execute(
                "INSERT INTO activity (id, start_time) VALUES ('act-1', '2026-01-01T10:00:00+00:00')",
                [],
            )
            .unwrap();
            db::raw_files::insert_raw_file(
                &conn,
                &RawFile {
                    id: "rf-1".into(),
                    activity_id: Some("act-1".into()),
                    path_in_vault: "raw/rf-1.gpx".into(),
                    original_path: None,
                    format: "gpx".into(),
                    hash_sha256: "h-1".into(),
                    imported_at: String::new(),
                    parse_status: "ok".into(),
                    failure_reason: None,
                },
            )
            .unwrap();
        }
        std::fs::write(vault.join("raw/rf-1.gpx"), b"<gpx/>").unwrap();
        // Crash-drift sibling: the DB path can lag the on-disk name after an
        // interrupted encryption toggle — deletion must catch both.
        std::fs::write(vault.join("raw/rf-1.gpx.enc"), b"ciphertext").unwrap();

        delete_activity_core(&state, "act-1").unwrap();

        let conn = state.db.lock().unwrap();
        assert!(
            !db::raw_files::hash_exists(&conn, "h-1").unwrap(),
            "hash must be freed so the file can be reimported"
        );
        assert!(!vault.join("raw/rf-1.gpx").exists(), "raw file must be removed");
        assert!(
            !vault.join("raw/rf-1.gpx.enc").exists(),
            "drift sibling must be removed"
        );

        let _ = std::fs::remove_dir_all(&vault);
    }

    /// Manual location entry must respect the geocoding opt-in exactly like
    /// the background pass: with the toggle off (the default) the typed text
    /// is saved as-is and nominatim.openstreetmap.org is never contacted —
    /// this test runs without network and must stay deterministic.
    #[test]
    fn manual_location_respects_the_geocoding_opt_in() {
        let vault = std::env::temp_dir().join(format!("syz_loc_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&vault).unwrap();
        let state = test_state(&vault);
        {
            let conn = state.db.lock().unwrap();
            conn.execute(
                "INSERT INTO activity (id, start_time) VALUES ('act-1', '2026-01-01T10:00:00+00:00')",
                [],
            )
            .unwrap();
        }

        let result = update_activity_location_core(&state, "act-1", "  Berlin  ").unwrap();
        assert!(!result.geocoded, "toggle off must not geocode");
        assert_eq!(result.location_name, "Berlin");

        let conn = state.db.lock().unwrap();
        let (name, lat): (String, Option<f64>) = conn
            .query_row(
                "SELECT location_name, start_lat FROM activity WHERE id='act-1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(name, "Berlin");
        assert!(lat.is_none(), "no coordinates without geocoding");

        let _ = std::fs::remove_dir_all(&vault);
    }

    /// Deletion is scoped to the one activity: another activity's raw file,
    /// dedup hash and photos survive untouched — and the deleted activity's
    /// photo files (with thumbnails) leave the vault with it.
    #[test]
    fn delete_activity_is_scoped_and_takes_photos_along() {
        let vault = std::env::temp_dir().join(format!("syz_del_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(vault.join("raw")).unwrap();
        std::fs::create_dir_all(vault.join("photos/thumbs")).unwrap();
        let state = test_state(&vault);

        {
            let conn = state.db.lock().unwrap();
            for (act, rf, hash) in [("act-1", "rf-1", "h-1"), ("act-2", "rf-2", "h-2")] {
                conn.execute(
                    "INSERT INTO activity (id, start_time) VALUES (?1, '2026-01-01T10:00:00+00:00')",
                    rusqlite::params![act],
                )
                .unwrap();
                db::raw_files::insert_raw_file(
                    &conn,
                    &RawFile {
                        id: rf.into(),
                        activity_id: Some(act.into()),
                        path_in_vault: format!("raw/{}.gpx", rf),
                        original_path: None,
                        format: "gpx".into(),
                        hash_sha256: hash.into(),
                        imported_at: String::new(),
                        parse_status: "ok".into(),
                        failure_reason: None,
                    },
                )
                .unwrap();
                std::fs::write(vault.join(format!("raw/{}.gpx", rf)), b"<gpx/>").unwrap();
            }
            db::photos::insert_photo(
                &conn,
                &crate::models::photo::Photo {
                    id: "ph-1".into(),
                    activity_id: "act-1".into(),
                    path_in_vault: "photos/ph-1.jpg".into(),
                    thumbnail_path: Some("photos/thumbs/ph-1.jpg".into()),
                    original_path: None,
                    mime_type: "image/jpeg".into(),
                    width: None,
                    height: None,
                    size_bytes: 4,
                    hash_sha256: "ph-hash".into(),
                    taken_at: None,
                    caption: None,
                    sort_order: 0,
                    created_at: String::new(),
                },
            )
            .unwrap();
        }
        std::fs::write(vault.join("photos/ph-1.jpg"), b"jpeg").unwrap();
        std::fs::write(vault.join("photos/thumbs/ph-1.jpg"), b"thmb").unwrap();

        delete_activity_core(&state, "act-1").unwrap();

        let conn = state.db.lock().unwrap();
        // The deleted activity's files are gone, photos included.
        assert!(!vault.join("raw/rf-1.gpx").exists());
        assert!(!vault.join("photos/ph-1.jpg").exists(), "photo file removed");
        assert!(
            !vault.join("photos/thumbs/ph-1.jpg").exists(),
            "thumbnail removed"
        );
        // The other activity is untouched.
        assert!(db::raw_files::hash_exists(&conn, "h-2").unwrap());
        assert!(vault.join("raw/rf-2.gpx").exists());
        let survivors: i64 = conn
            .query_row("SELECT COUNT(*) FROM activity", [], |r| r.get(0))
            .unwrap();
        assert_eq!(survivors, 1);

        let _ = std::fs::remove_dir_all(&vault);
    }

    /// "Set as destination point" with geocoding off must not touch the
    /// network: the coordinates become the visible location and the start
    /// point, and the background geocoding pass must NOT pick the activity
    /// up again (its location_name is no longer NULL).
    #[test]
    fn destination_point_offline_saves_coords_and_stays_geocode_free() {
        let state = test_state(&std::env::temp_dir());
        {
            let conn = state.db.lock().unwrap();
            conn.execute(
                "INSERT INTO activity (id, start_time) VALUES ('act-1', '2026-01-01T10:00:00+00:00')",
                [],
            )
            .unwrap();
        }

        let res = set_activity_location_point_core(&state, "act-1", 55.751244, 37.618423).unwrap();
        assert!(!res.geocoded);
        assert_eq!(res.location_name, "55.75124, 37.61842");

        let conn = state.db.lock().unwrap();
        let (name, lat, lon): (String, f64, f64) = conn
            .query_row(
                "SELECT location_name, start_lat, start_lon FROM activity WHERE id = 'act-1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(name, "55.75124, 37.61842");
        assert!((lat - 55.751244).abs() < 1e-9);
        assert!((lon - 37.618423).abs() < 1e-9);
        assert!(db::activities::get_activities_without_location(&conn)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn destination_point_rejects_invalid_coordinates() {
        let state = test_state(&std::env::temp_dir());
        assert!(set_activity_location_point_core(&state, "x", 90.5, 0.0).is_err());
        assert!(set_activity_location_point_core(&state, "x", 0.0, -180.5).is_err());
        assert!(set_activity_location_point_core(&state, "x", f64::NAN, 0.0).is_err());
    }
}
