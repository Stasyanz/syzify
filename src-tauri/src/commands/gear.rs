use tauri::State;

use crate::db;
use crate::models::gear::{Gear, GearInput, GearItem};
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
