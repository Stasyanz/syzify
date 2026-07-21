use tauri::State;

use crate::maps::tile_cache;
use crate::state::AppState;

#[derive(serde::Serialize)]
pub struct CacheInfo {
    pub size_bytes: u64,
    pub size_display: String,
}

#[tauri::command]
pub fn get_tile_cache_info(state: State<AppState>) -> Result<CacheInfo, String> {
    let tiles_dir = state.vault_path.join("tiles");
    let size = tile_cache::cache_size_bytes(&tiles_dir);
    Ok(CacheInfo {
        size_bytes: size,
        size_display: format_bytes(size),
    })
}

#[tauri::command]
pub fn clear_tile_cache(state: State<AppState>) -> Result<(), String> {
    let tiles_dir = state.vault_path.join("tiles");
    tile_cache::clear_cache(&tiles_dir)
}

fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}
