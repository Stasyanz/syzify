use tauri::State;

use crate::db;
use crate::models::dashboard::DashboardData;
use crate::state::AppState;

// The app dashboard shows fixed windows only (this week / this month /
// all-time records), so the command takes no period — "all" merely scopes the
// summary fields the UI doesn't render. The period-aware db-layer entry point
// stays: the plugin host API (host_query kind "dashboard") passes a period.
#[tauri::command]
pub fn get_dashboard_data(state: State<AppState>) -> Result<DashboardData, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::dashboard::get_dashboard_data(&conn, "all", None).map_err(|e| e.to_string())
}
