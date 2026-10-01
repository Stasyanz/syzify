use tauri::State;

use crate::db;
use crate::models::activity::ActivityFilters;
use crate::models::gear::{Gear, GearInput, GearItem, GearTargets, RuleCandidates};
use crate::state::AppState;

#[tauri::command]
pub fn list_gear(state: State<AppState>) -> Result<Vec<GearItem>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::gear::list(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_gear(input: GearInput, state: State<AppState>) -> Result<Gear, String> {
    let input = input.normalized()?;
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::gear::insert(&conn, &input).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_gear(id: String, input: GearInput, state: State<AppState>) -> Result<(), String> {
    let input = input.normalized()?;
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    // A retired item is no default for anything: the form hides the
    // field, and a caller that sends one anyway is told why it is refused
    // rather than having it dropped.
    match db::gear::is_retired(&conn, &id).map_err(|e| e.to_string())? {
        None => return Err(format!("Gear not found: {id}")),
        Some(true) if !input.default_for.is_empty() => {
            return Err("Bring the gear back before making it a default".into());
        }
        _ => {}
    }
    match db::gear::update(&conn, &id, &input).map_err(|e| e.to_string())? {
        true => Ok(()),
        false => Err(format!("Gear not found: {id}")),
    }
}

#[tauri::command]
pub fn set_gear_retired(id: String, retired: bool, state: State<AppState>) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    match db::gear::set_retired(&conn, &id, retired).map_err(|e| e.to_string())? {
        true => Ok(()),
        false => Err(format!("Gear not found: {id}")),
    }
}

#[tauri::command]
pub fn delete_gear(id: String, state: State<AppState>) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    match db::gear::delete(&conn, &id).map_err(|e| e.to_string())? {
        true => Ok(()),
        false => Err(format!("Gear not found: {id}")),
    }
}

#[tauri::command]
pub fn set_activity_gear(
    activity_id: String,
    gear_id: Option<String>,
    state: State<AppState>,
) -> Result<(), String> {
    set_activity_gear_core(&state, &activity_id, gear_id.as_deref())
}

/// Put an activity on an item or take it off. A multisport whole (a merged
/// container, a FIT-native multisport file) is refused: its aggregate
/// spans several sports, and a bike on it would count the swim. A retired
/// item can stay on an activity but is not offered anew.
pub(crate) fn set_activity_gear_core(
    state: &AppState,
    activity_id: &str,
    gear_id: Option<&str>,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::gear::assign(&conn, activity_id, gear_id)
        .map_err(|e| e.to_string())?
        .map_err(|refusal| refusal.to_string())
}

#[tauri::command]
pub fn assign_gear_history(id: String, state: State<AppState>) -> Result<usize, String> {
    assign_gear_history_core(&state, &id)
}

/// The Garage card's quick action (see `db::gear::assign_history`); a
/// retired item is refused, as it is no default for anything.
pub(crate) fn assign_gear_history_core(state: &AppState, id: &str) -> Result<usize, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    match db::gear::is_retired(&conn, id).map_err(|e| e.to_string())? {
        None => return Err(format!("Gear not found: {id}")),
        Some(true) => return Err("Bring the gear back before assigning activities to it".into()),
        Some(false) => {}
    }
    db::gear::assign_history(&conn, id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn count_gear_targets(
    filters: ActivityFilters,
    gear_id: String,
    state: State<AppState>,
) -> Result<GearTargets, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::gear::filtered_targets(&conn, &filters, &gear_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn assign_gear_to_filtered(
    filters: ActivityFilters,
    gear_id: String,
    state: State<AppState>,
) -> Result<usize, String> {
    assign_gear_to_filtered_core(&state, &filters, &gear_id)
}

/// The library's bulk action (see `db::gear::assign_filtered`); a retired
/// item is refused, as it takes no new activities.
pub(crate) fn assign_gear_to_filtered_core(
    state: &AppState,
    filters: &ActivityFilters,
    gear_id: &str,
) -> Result<usize, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    match db::gear::is_retired(&conn, gear_id).map_err(|e| e.to_string())? {
        None => return Err(format!("Gear not found: {gear_id}")),
        Some(true) => return Err("Bring the gear back before assigning activities to it".into()),
        Some(false) => {}
    }
    db::gear::assign_filtered(&conn, filters, gear_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn gear_rule_candidates(state: State<AppState>) -> Result<RuleCandidates, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::gear_rules::candidates(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn apply_gear_rules(state: State<AppState>) -> Result<usize, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::gear_rules::apply_all(&conn).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::gear::{GearInput, GearKind};
    use std::sync::{Arc, Mutex};

    fn test_state() -> AppState {
        AppState {
            db: Arc::new(Mutex::new(crate::db::test_db())),
            vault_path: std::env::temp_dir(),
            encryption_key: Mutex::new(None),
            watcher_handle: Mutex::new(None),
            db_locked: Mutex::new(false),
            vault_error: Mutex::new(None),
            services_started: Mutex::new(false),
            geocoding_flight: crate::state::SingleFlight::default(),
            vault_flight: crate::state::SingleFlight::default(),
        }
    }

    fn bike(name: &str, purchased: Option<&str>) -> GearInput {
        GearInput {
            kind: GearKind::Bike,
            name: name.into(),
            brand: None,
            model: None,
            purchased_at: purchased.map(str::to_string),
            initial_distance_m: 0.0,
            distance_limit_m: None,
            notes: None,
            default_for: vec!["ride".into()],
            rules: vec![],
        }
    }

    fn activity(state: &AppState, id: &str, sport: &str, parent: Option<&str>) {
        let conn = state.db.lock().unwrap();
        conn.execute(
            "INSERT INTO activity (id, start_time, sport_type, parent_id) VALUES (?1, '2026-09-01T08:00:00+03:00', ?2, ?3)",
            rusqlite::params![id, sport, parent],
        )
        .unwrap();
    }

    #[test]
    fn an_activity_is_put_on_gear_unless_it_is_a_multisport_whole_or_the_gear_is_retired() {
        let state = test_state();
        let (road, old) = {
            let conn = state.db.lock().unwrap();
            let road = db::gear::insert(&conn, &bike("Road", None)).unwrap();
            let old = db::gear::insert(&conn, &bike("Old", None)).unwrap();
            db::gear::set_retired(&conn, &old.id, true).unwrap();
            (road, old)
        };
        activity(&state, "a1", "ride", None);
        activity(&state, "container", "triathlon", None);
        activity(&state, "leg", "ride", Some("container"));

        assert_eq!(set_activity_gear_core(&state, "a1", Some(&road.id)), Ok(()));
        assert_eq!(set_activity_gear_core(&state, "leg", Some(&road.id)), Ok(()), "a merged leg carries its own");
        assert!(set_activity_gear_core(&state, "container", Some(&road.id)).unwrap_err().contains("multisport"));
        assert!(set_activity_gear_core(&state, "container", None).is_ok(), "taking off is always fine");
        assert_eq!(set_activity_gear_core(&state, "nope", Some(&road.id)).unwrap_err(), "Activity not found: nope");
        assert_eq!(set_activity_gear_core(&state, "a1", Some("ghost")).unwrap_err(), "Gear not found: ghost");
        assert!(set_activity_gear_core(&state, "a1", Some(&old.id)).unwrap_err().contains("Bring the gear back"));
        // Already on the retired item: a save that keeps it is not refused.
        {
            let conn = state.db.lock().unwrap();
            db::gear::set_activity_gear(&conn, "a1", Some(&old.id)).unwrap();
        }
        assert_eq!(set_activity_gear_core(&state, "a1", Some(&old.id)), Ok(()));
        assert_eq!(set_activity_gear_core(&state, "a1", None), Ok(()));
    }

    #[test]
    fn history_assignment_counts_and_refuses_a_retired_item() {
        let state = test_state();
        let (road, old) = {
            let conn = state.db.lock().unwrap();
            let old = db::gear::insert(&conn, &bike("Old", None)).unwrap();
            db::gear::set_retired(&conn, &old.id, true).unwrap();
            let road = db::gear::insert(&conn, &bike("Road", Some("2026-01-01"))).unwrap();
            (road, old)
        };
        activity(&state, "a1", "ride", None);
        activity(&state, "a2", "ride", None);
        activity(&state, "r1", "run", None);
        assert_eq!(assign_gear_history_core(&state, &road.id), Ok(2));
        assert_eq!(assign_gear_history_core(&state, &road.id), Ok(0));
        assert!(assign_gear_history_core(&state, &old.id).unwrap_err().contains("Bring the gear back"));
        assert_eq!(assign_gear_history_core(&state, "nope").unwrap_err(), "Gear not found: nope");
    }

    #[test]
    fn filtered_assignment_counts_and_refuses_a_retired_item() {
        let state = test_state();
        let (road, old) = {
            let conn = state.db.lock().unwrap();
            let old = db::gear::insert(&conn, &bike("Old", None)).unwrap();
            db::gear::set_retired(&conn, &old.id, true).unwrap();
            (db::gear::insert(&conn, &bike("Road", None)).unwrap(), old)
        };
        activity(&state, "a1", "ride", None);
        activity(&state, "r1", "run", None);
        let rides = ActivityFilters { sport_types: Some(vec!["ride".into()]), ..Default::default() };
        assert_eq!(assign_gear_to_filtered_core(&state, &rides, &road.id), Ok(1));
        assert_eq!(assign_gear_to_filtered_core(&state, &rides, &road.id), Ok(0));
        assert!(assign_gear_to_filtered_core(&state, &rides, &old.id).unwrap_err().contains("Bring the gear back"));
        assert_eq!(assign_gear_to_filtered_core(&state, &rides, "nope").unwrap_err(), "Gear not found: nope");
    }
}
