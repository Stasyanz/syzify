//! Plugin install flow: validate + verify, copy into the vault, register.
//! Kept out of the command layer so the Tauri commands stay thin.

use std::path::Path;

use crate::db;
use crate::models::plugin::{Plugin, PluginManifest};
use crate::plugins::package;
use crate::state::AppState;

/// Current host app version, used for plugin compatibility checks.
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Install (or upgrade) a plugin by sideloading a `plugin.json` the user picked.
/// The manifest and its WASM module are copied into `vault/plugins/<id>/` so the
/// plugin is self-contained, included in backups, and survives a vault move.
pub fn from_file(state: &AppState, path: &str) -> Result<Plugin, String> {
    let manifest_json =
        std::fs::read_to_string(path).map_err(|e| format!("Failed to read {path}: {e}"))?;
    let manifest = PluginManifest::parse_and_validate(&manifest_json, APP_VERSION)?;
    check_replace_allowed(state, &manifest.id, false, None)?;

    let vault_dir = write_into_vault(state, &manifest.id, &manifest_json)?;
    if let Some(entry) = &manifest.entry {
        let src_dir = Path::new(path).parent().unwrap_or_else(|| Path::new("."));
        std::fs::copy(src_dir.join(entry), vault_dir.join(entry))
            .map_err(|e| format!("Failed to copy plugin module {entry}: {e}"))?;
    }
    register(state, manifest_json, format!("plugins/{}", manifest.id), false)
}

/// Install (or upgrade) from a signed `.syzify-ext` package: verify the Ed25519
/// signature, enforce trust-on-first-use, then write contents into the vault.
pub fn from_package(state: &AppState, path: &str) -> Result<Plugin, String> {
    let pkg = package::open_package(Path::new(path))?;
    let manifest = PluginManifest::parse_and_validate(&pkg.manifest_json, APP_VERSION)?;
    let public_key = manifest
        .public_key
        .clone()
        .ok_or_else(|| "package manifest has no publicKey".to_string())?;

    package::verify(&pkg.manifest_json, &pkg.wasm, &pkg.signature_hex, &public_key)?;
    // Run before touching the filesystem so a rejected upgrade can't corrupt files.
    check_replace_allowed(state, &manifest.id, true, Some(&public_key))?;

    let vault_dir = write_into_vault(state, &manifest.id, &pkg.manifest_json)?;
    let entry = manifest.entry.as_deref().unwrap_or("plugin.wasm");
    std::fs::write(vault_dir.join(entry), &pkg.wasm)
        .map_err(|e| format!("Failed to write plugin module: {e}"))?;
    register(state, pkg.manifest_json, format!("plugins/{}", manifest.id), true)
}

/// Create `vault/plugins/<id>/` and write the manifest copy; returns the dir.
/// `id` is already validated by `parse_and_validate`, so it is a safe segment.
fn write_into_vault(state: &AppState, id: &str, manifest_json: &str) -> Result<std::path::PathBuf, String> {
    let vault_dir = state.vault_path.join("plugins").join(id);
    std::fs::create_dir_all(&vault_dir).map_err(|e| format!("Failed to create plugins/{id}: {e}"))?;
    std::fs::write(vault_dir.join("plugin.json"), manifest_json)
        .map_err(|e| format!("Failed to write manifest: {e}"))?;
    Ok(vault_dir)
}

/// Decide whether installing `id` may replace an already-installed plugin.
/// Blocks author/trust hijacking: you can't silently overwrite a plugin with a
/// different author key, turn an unsigned plugin into a signed one, or replace a
/// signed plugin with an unsigned build. The user must uninstall first. Ok for a
/// fresh install, a same-key signed upgrade, or an unsigned→unsigned reinstall.
fn check_replace_allowed(
    state: &AppState,
    id: &str,
    new_signed: bool,
    new_key: Option<&str>,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let Some(existing) = db::plugins::get_plugin(&conn, id).map_err(|e| e.to_string())? else {
        return Ok(()); // fresh install
    };
    let prev_key = serde_json::from_str::<PluginManifest>(&existing.manifest)
        .ok()
        .and_then(|m| m.public_key);

    let allowed = if existing.signed {
        // Replacing a signed plugin: only a signed package from the same key.
        new_signed && new_key.is_some() && prev_key.as_deref() == new_key
    } else {
        // Replacing an unsigned dev sideload: only with another unsigned sideload.
        !new_signed
    };

    if allowed {
        Ok(())
    } else {
        Err(format!(
            "'{id}' is already installed by a different author or trust level — uninstall it first"
        ))
    }
}

/// Persist a plugin record (disabled by default) and return the stored row.
/// Reinstalling keeps the previous enabled state.
fn register(
    state: &AppState,
    manifest_json: String,
    source: String,
    signed: bool,
) -> Result<Plugin, String> {
    let manifest = PluginManifest::parse_and_validate(&manifest_json, APP_VERSION)?;
    let record = Plugin {
        id: manifest.id.clone(),
        name: manifest.name.clone(),
        version: manifest.version.clone(),
        author: manifest.author.clone(),
        description: manifest.description.clone(),
        enabled: false,
        signed,
        manifest: manifest_json,
        source,
        installed_at: String::new(),
        updated_at: String::new(),
    };
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::plugins::upsert_plugin(&conn, &record).map_err(|e| e.to_string())?;
    db::plugins::get_plugin(&conn, &manifest.id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Plugin vanished right after install".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn test_state() -> AppState {
        AppState {
            db: Arc::new(Mutex::new(crate::db::test_db())),
            vault_path: std::env::temp_dir(),
            encryption_key: Mutex::new(None),
            watcher_handle: Mutex::new(None),
            db_locked: Mutex::new(false),
            vault_error: Mutex::new(None),
            services_started: Mutex::new(false),
            geocoding_flight: crate::state::SingleFlight::default(),
            vault_flight: crate::state::SingleFlight::default(),
        }
    }

    fn store(state: &AppState, id: &str, signed: bool, key: Option<&str>) {
        let manifest = match key {
            Some(k) => format!(r#"{{"id":"{id}","name":"P","version":"1.0.0","publicKey":"{k}"}}"#),
            None => format!(r#"{{"id":"{id}","name":"P","version":"1.0.0"}}"#),
        };
        let record = Plugin {
            id: id.to_string(),
            name: "P".to_string(),
            version: "1.0.0".to_string(),
            author: None,
            description: None,
            enabled: false,
            signed,
            manifest,
            source: format!("plugins/{id}"),
            installed_at: String::new(),
            updated_at: String::new(),
        };
        let conn = state.db.lock().unwrap();
        db::plugins::upsert_plugin(&conn, &record).unwrap();
    }

    #[test]
    fn replace_rules_block_author_and_trust_hijack() {
        let state = test_state();

        // Fresh install: anything goes.
        assert!(check_replace_allowed(&state, "com.fresh", false, None).is_ok());
        assert!(check_replace_allowed(&state, "com.fresh", true, Some("KEY1")).is_ok());

        // Existing UNSIGNED plugin.
        store(&state, "com.unsigned", false, None);
        assert!(check_replace_allowed(&state, "com.unsigned", false, None).is_ok(), "unsigned->unsigned ok");
        assert!(check_replace_allowed(&state, "com.unsigned", true, Some("KEY1")).is_err(), "unsigned->signed blocked");

        // Existing SIGNED plugin with KEY1.
        store(&state, "com.signed", true, Some("KEY1"));
        assert!(check_replace_allowed(&state, "com.signed", true, Some("KEY1")).is_ok(), "same key ok");
        assert!(check_replace_allowed(&state, "com.signed", true, Some("KEY2")).is_err(), "different key blocked");
        assert!(check_replace_allowed(&state, "com.signed", false, None).is_err(), "signed->unsigned blocked");
    }
}
