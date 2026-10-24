use tauri::State;

use crate::error::CommandError;
use crate::maps::tile_cache;
use crate::state::AppState;

#[derive(serde::Serialize)]
pub struct CacheInfo {
    pub size_bytes: u64,
    pub size_display: String,
}

#[tauri::command]
pub fn get_tile_cache_info(state: State<AppState>) -> Result<CacheInfo, CommandError> {
    let tiles_dir = state.vault_path.join("tiles");
    let size = tile_cache::cache_size_bytes(&tiles_dir);
    Ok(CacheInfo {
        size_bytes: size,
        size_display: format_bytes(size),
    })
}

#[tauri::command]
pub fn clear_tile_cache(state: State<AppState>) -> Result<(), CommandError> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use tauri::Manager;

    #[test]
    fn the_cache_is_measured_and_cleared_through_the_state() {
        let vault = crate::test_support::ScratchDir::new("tiles_cmd");
        std::fs::create_dir_all(vault.join("tiles/osm")).unwrap();
        std::fs::write(vault.join("tiles/osm/1.png"), [0u8; 2048]).unwrap();
        let app = crate::test_support::mock_app_at(&vault);
        let info = get_tile_cache_info(app.state()).unwrap();
        assert_eq!((info.size_bytes, info.size_display.as_str()), (2048, "2.0 KB"));
        clear_tile_cache(app.state()).unwrap();
        assert_eq!(get_tile_cache_info(app.state()).unwrap().size_bytes, 0);
    }
}
