use tauri::State;

use crate::db;
use crate::models::recovery::RecoveryCard;
use crate::recovery;
use crate::state::AppState;

/// The dashboard's Recovery card for today (ADR 0002): every stored
/// monitoring day (one row per day — the card promises the LAST computed
/// index however old it is, so no window) and the daily hrTSS of the
/// whole vault (the chronic load needs history), folded by the pure
/// recovery module.
#[tauri::command]
pub fn get_recovery(state: State<AppState>) -> Result<RecoveryCard, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let today = chrono::Local::now().date_naive();
    let days = db::monitoring::get_days(&conn, "0000-01-01", &today.to_string())
        .map_err(|e| e.to_string())?;
    let daily_tss = db::training_load::daily_hrtss(&conn).map_err(|e| e.to_string())?;
    Ok(recovery::card(&days, &daily_tss, today))
}
