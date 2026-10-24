use tauri::State;

use crate::commands::activities::activity_not_found;
use crate::db;
use crate::db::gear::AssignRefusal;
use crate::error::CommandError;
use crate::models::activity::ActivityFilters;
use crate::models::gear::{Gear, GearInput, GearItem, GearTargets, RuleCandidates};
use crate::state::AppState;

fn gear_not_found(id: &str) -> CommandError {
    CommandError::coded("gear.notFound", format!("Gear not found: {id}")).param("id", id)
}

fn retired_takes_no_activities() -> CommandError {
    CommandError::coded("gear.retiredAssign", "Bring the gear back before assigning activities to it")
}

/// The app's words for a refusal, coded; a plugin reads the same refusal
/// through its `Display` (`plugins/host.rs`).
fn refused(refusal: AssignRefusal) -> CommandError {
    match refusal {
        AssignRefusal::NoActivity(id) => activity_not_found(&id),
        AssignRefusal::NoGear(id) => gear_not_found(&id),
        r @ AssignRefusal::Multisport => CommandError::coded("gear.multisport", r.to_string()),
        r @ AssignRefusal::Retired => CommandError::coded("gear.retiredPutOn", r.to_string()),
    }
}

#[tauri::command]
pub fn list_gear(state: State<AppState>) -> Result<Vec<GearItem>, CommandError> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    Ok(db::gear::list(&conn).map_err(|e| e.to_string())?)
}

#[tauri::command]
pub fn create_gear(input: GearInput, state: State<AppState>) -> Result<Gear, CommandError> {
    let input = input.normalized()?;
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    Ok(db::gear::insert(&conn, &input).map_err(|e| e.to_string())?)
}

#[tauri::command]
pub fn update_gear(id: String, input: GearInput, state: State<AppState>) -> Result<(), CommandError> {
    update_gear_core(&state, &id, &input)
}

pub(crate) fn update_gear_core(state: &AppState, id: &str, input: &GearInput) -> Result<(), CommandError> {
    let input = input.normalized()?;
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    // A retired item is no default for anything: the form hides the
    // field, and a caller that sends one anyway is told why it is refused
    // rather than having it dropped.
    match db::gear::is_retired(&conn, id).map_err(|e| e.to_string())? {
        None => return Err(gear_not_found(id)),
        Some(true) if !input.default_for.is_empty() => {
            return Err(CommandError::coded("gear.retiredDefault", "Bring the gear back before making it a default"));
        }
        _ => {}
    }
    // `is_retired` saw the row under this same lock: the update finds it.
    db::gear::update(&conn, id, &input).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn set_gear_retired(id: String, retired: bool, state: State<AppState>) -> Result<(), CommandError> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    match db::gear::set_retired(&conn, &id, retired).map_err(|e| e.to_string())? {
        true => Ok(()),
        false => Err(gear_not_found(&id)),
    }
}

#[tauri::command]
pub fn delete_gear(id: String, state: State<AppState>) -> Result<(), CommandError> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    match db::gear::delete(&conn, &id).map_err(|e| e.to_string())? {
        true => Ok(()),
        false => Err(gear_not_found(&id)),
    }
}

#[tauri::command]
pub fn set_activity_gear(
    activity_id: String,
    gear_id: Option<String>,
    state: State<AppState>,
) -> Result<(), CommandError> {
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
) -> Result<(), CommandError> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::gear::assign(&conn, activity_id, gear_id).map_err(|e| e.to_string())?.map_err(refused)
}

#[tauri::command]
pub fn assign_gear_history(id: String, state: State<AppState>) -> Result<usize, CommandError> {
    assign_gear_history_core(&state, &id)
}

/// The Garage card's quick action (see `db::gear::assign_history`); a
/// retired item is refused, as it is no default for anything.
pub(crate) fn assign_gear_history_core(state: &AppState, id: &str) -> Result<usize, CommandError> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    match db::gear::is_retired(&conn, id).map_err(|e| e.to_string())? {
        None => return Err(gear_not_found(id)),
        Some(true) => return Err(retired_takes_no_activities()),
        Some(false) => {}
    }
    Ok(db::gear::assign_history(&conn, id).map_err(|e| e.to_string())?)
}

#[tauri::command]
pub fn count_gear_targets(
    filters: ActivityFilters,
    gear_id: String,
    state: State<AppState>,
) -> Result<GearTargets, CommandError> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    Ok(db::gear::filtered_targets(&conn, &filters, &gear_id).map_err(|e| e.to_string())?)
}

#[tauri::command]
pub fn assign_gear_to_filtered(
    filters: ActivityFilters,
    gear_id: String,
    state: State<AppState>,
) -> Result<usize, CommandError> {
    assign_gear_to_filtered_core(&state, &filters, &gear_id)
}

/// The library's bulk action (see `db::gear::assign_filtered`); a retired
/// item is refused, as it takes no new activities.
pub(crate) fn assign_gear_to_filtered_core(
    state: &AppState,
    filters: &ActivityFilters,
    gear_id: &str,
) -> Result<usize, CommandError> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    match db::gear::is_retired(&conn, gear_id).map_err(|e| e.to_string())? {
        None => return Err(gear_not_found(gear_id)),
        Some(true) => return Err(retired_takes_no_activities()),
        Some(false) => {}
    }
    Ok(db::gear::assign_filtered(&conn, filters, gear_id).map_err(|e| e.to_string())?)
}

#[tauri::command]
pub fn gear_rule_candidates(state: State<AppState>) -> Result<RuleCandidates, CommandError> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    Ok(db::gear_rules::candidates(&conn).map_err(|e| e.to_string())?)
}

#[tauri::command]
pub fn apply_gear_rules(state: State<AppState>) -> Result<usize, CommandError> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    Ok(db::gear_rules::apply_all(&conn).map_err(|e| e.to_string())?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::gear::{GearInput, GearKind};
    use std::sync::{Arc, Mutex};

    /// The guard comes first in the tuple: tuple bindings drop in reverse
    /// order, so the state lets go of the path before the directory goes.
    fn test_state() -> (crate::test_support::ScratchDir, AppState) {
        let vault = crate::test_support::ScratchDir::new("gear_cmd");
        let state = AppState {
            db: Arc::new(Mutex::new(crate::db::test_db())),
            vault_path: vault.to_path_buf(),
            encryption_key: Mutex::new(None),
            watcher_handle: Mutex::new(None),
            db_locked: Mutex::new(false),
            vault_error: Mutex::new(None),
            services_started: Mutex::new(false),
            geocoding_flight: crate::state::SingleFlight::default(),
            vault_flight: crate::state::SingleFlight::default(),
        };
        (vault, state)
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

    /// The commands themselves, through `State`: each answers, and an
    /// unknown item is refused in coded words (#236).
    #[test]
    fn the_commands_answer_through_their_state() {
        use tauri::Manager;
        let vault = crate::test_support::ScratchDir::new("gear_state");
        let app = crate::test_support::mock_app_at(&vault);
        let road = create_gear(bike("Road", None), app.state()).unwrap();
        assert_eq!(list_gear(app.state()).unwrap().len(), 1);
        update_gear(road.id.clone(), bike("Road 2", None), app.state()).unwrap();
        activity(&app.state::<AppState>(), "a1", "ride", None);
        set_activity_gear("a1".into(), Some(road.id.clone()), app.state()).unwrap();
        assert_eq!(assign_gear_history(road.id.clone(), app.state()).unwrap(), 0);
        let filters = crate::models::activity::ActivityFilters::default();
        assert!(count_gear_targets(filters.clone(), road.id.clone(), app.state()).is_ok());
        assert!(assign_gear_to_filtered(filters, road.id.clone(), app.state()).is_ok());
        assert!(gear_rule_candidates(app.state()).is_ok());
        assert_eq!(apply_gear_rules(app.state()).unwrap(), 0);
        set_gear_retired(road.id.clone(), true, app.state()).unwrap();
        delete_gear(road.id.clone(), app.state()).unwrap();

        let unknown = CommandError::coded("gear.notFound", "Gear not found: ghost").param("id", "ghost");
        assert_eq!(set_gear_retired("ghost".into(), true, app.state()), Err(unknown.clone()));
        assert_eq!(delete_gear("ghost".into(), app.state()), Err(unknown));
        assert_eq!(create_gear(bike(" ", None), app.state()).unwrap_err().code, Some("gear.nameRequired"));
    }

    #[test]
    fn an_activity_is_put_on_gear_unless_it_is_a_multisport_whole_or_the_gear_is_retired() {
        let (_vault, state) = test_state();
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
        // The message is the refusal's Display, which a plugin reads too:
        // checked here as the English the dictionary has (#236).
        assert_eq!(
            set_activity_gear_core(&state, "container", Some(&road.id)).unwrap_err(),
            CommandError::coded(
                "gear.multisport",
                "A multisport activity spans several sports and carries no gear; its legs do"
            )
        );
        assert!(set_activity_gear_core(&state, "container", None).is_ok(), "taking off is always fine");
        assert_eq!(
            set_activity_gear_core(&state, "nope", Some(&road.id)).unwrap_err(),
            CommandError::coded("activity.notFound", "Activity not found: nope").param("id", "nope")
        );
        assert_eq!(
            set_activity_gear_core(&state, "a1", Some("ghost")).unwrap_err(),
            CommandError::coded("gear.notFound", "Gear not found: ghost").param("id", "ghost")
        );
        assert_eq!(
            set_activity_gear_core(&state, "a1", Some(&old.id)).unwrap_err(),
            CommandError::coded("gear.retiredPutOn", "Bring the gear back before putting an activity on it")
        );
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
        let (_vault, state) = test_state();
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
        assert_eq!(assign_gear_history_core(&state, &old.id).unwrap_err().code, Some("gear.retiredAssign"));
        assert_eq!(assign_gear_history_core(&state, "nope").unwrap_err().message, "Gear not found: nope");
    }

    #[test]
    fn an_update_refuses_an_unknown_item_and_a_default_on_a_retired_one() {
        let (_vault, state) = test_state();
        let old = {
            let conn = state.db.lock().unwrap();
            let old = db::gear::insert(&conn, &bike("Old", None)).unwrap();
            db::gear::set_retired(&conn, &old.id, true).unwrap();
            old
        };
        assert_eq!(update_gear_core(&state, "nope", &bike("X", None)).unwrap_err().code, Some("gear.notFound"));
        let e = update_gear_core(&state, &old.id, &bike("Old", None)).unwrap_err();
        assert_eq!(
            (e.code, e.message.as_str()),
            (Some("gear.retiredDefault"), "Bring the gear back before making it a default")
        );
        let mut no_default = bike("Old again", None);
        no_default.default_for.clear();
        assert_eq!(update_gear_core(&state, &old.id, &no_default), Ok(()));
        assert_eq!(update_gear_core(&state, &old.id, &bike(" ", None)).unwrap_err().code, Some("gear.nameRequired"));
    }

    #[test]
    fn filtered_assignment_counts_and_refuses_a_retired_item() {
        let (_vault, state) = test_state();
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
        assert_eq!(assign_gear_to_filtered_core(&state, &rides, &old.id).unwrap_err().code, Some("gear.retiredAssign"));
        assert_eq!(assign_gear_to_filtered_core(&state, &rides, "nope").unwrap_err().code, Some("gear.notFound"));
    }
}
