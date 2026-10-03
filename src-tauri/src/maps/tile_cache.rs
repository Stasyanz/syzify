use std::fs;
use std::path::{Path, PathBuf};

/// Get a tile from cache. Returns the file path if cached, None otherwise.
pub fn get_cached_tile(tiles_dir: &Path, layer: &str, z: u32, x: u32, y: u32) -> Option<PathBuf> {
    let path = tile_path(tiles_dir, layer, z, x, y);
    if path.exists() {
        Some(path)
    } else {
        None
    }
}

/// Save tile bytes to cache.
pub fn save_tile(tiles_dir: &Path, layer: &str, z: u32, x: u32, y: u32, data: &[u8]) -> Result<PathBuf, String> {
    let path = tile_path(tiles_dir, layer, z, x, y);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create tile dir: {}", e))?;
    }
    fs::write(&path, data).map_err(|e| format!("Failed to write tile: {}", e))?;
    Ok(path)
}

/// Get total cache size in bytes.
pub fn cache_size_bytes(tiles_dir: &Path) -> u64 {
    if !tiles_dir.exists() {
        return 0;
    }
    dir_size(tiles_dir)
}

/// Clear the entire tile cache.
pub fn clear_cache(tiles_dir: &Path) -> Result<(), String> {
    if tiles_dir.exists() {
        fs::remove_dir_all(tiles_dir).map_err(|e| format!("Failed to clear cache: {}", e))?;
        fs::create_dir_all(tiles_dir).map_err(|e| format!("Failed to recreate cache dir: {}", e))?;
    }
    Ok(())
}

fn tile_path(tiles_dir: &Path, layer: &str, z: u32, x: u32, y: u32) -> PathBuf {
    tiles_dir.join(format!("{}/{}/{}/{}.png", layer, z, x, y))
}

fn dir_size(path: &Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                total += dir_size(&p);
            } else if let Ok(meta) = p.metadata() {
                total += meta.len();
            }
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_and_get_tile() {
        // The cache root does not exist yet: a read must say "no tile" and
        // the first save must create it.
        let scratch = crate::test_support::ScratchDir::new("tile_test");
        let tmp = scratch.join("tiles");

        assert!(get_cached_tile(&tmp, "osm", 10, 500, 300).is_none());

        let path = save_tile(&tmp, "osm", 10, 500, 300, b"fake png data").unwrap();
        assert!(path.exists());

        let cached = get_cached_tile(&tmp, "osm", 10, 500, 300);
        assert!(cached.is_some());
        assert_eq!(fs::read(cached.unwrap()).unwrap(), b"fake png data");

        // Different layer should not find the tile
        assert!(get_cached_tile(&tmp, "topo", 10, 500, 300).is_none());

    }

    #[test]
    fn cache_size_and_clear() {
        let scratch = crate::test_support::ScratchDir::new("tile_size_test");
        let tmp = scratch.join("tiles");

        assert_eq!(cache_size_bytes(&tmp), 0);

        save_tile(&tmp, "osm", 1, 0, 0, &[0u8; 1000]).unwrap();
        save_tile(&tmp, "topo", 1, 0, 1, &[0u8; 2000]).unwrap();

        assert_eq!(cache_size_bytes(&tmp), 3000);

        clear_cache(&tmp).unwrap();
        assert_eq!(cache_size_bytes(&tmp), 0);

    }
}
