use tauri::State;

use crate::db;
use crate::models::tag::Tag;
use crate::state::AppState;

#[tauri::command]
pub fn get_tags(state: State<AppState>) -> Result<Vec<Tag>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::tags::get_all_tags(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_tag(name: String, state: State<AppState>) -> Result<Tag, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::tags::create_tag(&conn, &name).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_activity_tags(
    activity_id: String,
    tag_ids: Vec<i64>,
    state: State<AppState>,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::tags::set_activity_tags(&conn, &activity_id, &tag_ids).map_err(|e| e.to_string())
}
