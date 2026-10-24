use tauri::State;

use crate::db;
use crate::error::CommandError;
use crate::models::dashboard::DashboardData;
use crate::state::AppState;

// The app dashboard shows fixed windows only (this week / this month /
// all-time records), so the command takes no period — "all" merely scopes the
// summary fields the UI doesn't render. The period-aware db-layer entry point
// stays: the plugin host API (host_query kind "dashboard") passes a period.
#[tauri::command]
pub fn get_dashboard_data(state: State<AppState>) -> Result<DashboardData, CommandError> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    Ok(db::dashboard::get_dashboard_data(&conn, "all", None).map_err(|e| e.to_string())?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri::Manager;

    #[test]
    fn an_empty_vault_has_a_dashboard() {
        let vault = crate::test_support::ScratchDir::new("dashboard_cmd");
        let app = crate::test_support::mock_app_at(&vault);
        assert!(get_dashboard_data(app.state()).is_ok());
    }
}
