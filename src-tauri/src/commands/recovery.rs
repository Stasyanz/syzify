use std::collections::BTreeMap;

use tauri::State;

use crate::db;
use crate::models::recovery::{RecoveryCard, RecoveryNight};
use crate::recovery;
use crate::state::AppState;

/// The dashboard's Recovery card for today (ADR 0002): every stored
/// monitoring day (one row per day — the card promises the LAST computed
/// index however old it is, so no window) and the daily hrTSS of the
/// whole vault (the chronic load needs history), folded by the pure
/// recovery module.
#[tauri::command]
pub fn get_recovery(state: State<AppState>) -> Result<RecoveryCard, String> {
    let today = chrono::Local::now().date_naive();
    let (days, daily_tss) = load(&state, today)?;
    Ok(recovery::card(&days, &daily_tss, today))
}

/// Every indexed night for the dashboard calendar (#97) — same inputs as
/// the card; the frontend cuts the month it shows, so flipping months
/// costs nothing here.
#[tauri::command]
pub fn get_recovery_nights(state: State<AppState>) -> Result<Vec<RecoveryNight>, String> {
    let today = chrono::Local::now().date_naive();
    let (days, daily_tss) = load(&state, today)?;
    Ok(recovery::nights(&days, &daily_tss, today))
}

type Inputs = (Vec<crate::models::monitoring::MonitoringDay>, BTreeMap<String, f64>);

/// Every stored monitoring day up to today and the daily hrTSS of the whole
/// vault — what both commands fold.
fn load(state: &AppState, today: chrono::NaiveDate) -> Result<Inputs, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let days = db::monitoring::get_days(&conn, "0000-01-01", &today.to_string())
        .map_err(|e| e.to_string())?;
    let daily_tss = db::training_load::daily_hrtss(&conn).map_err(|e| e.to_string())?;
    Ok((days, daily_tss))
}
