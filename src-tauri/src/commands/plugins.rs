use tauri::{AppHandle, Manager, State};

use crate::db;
use crate::models::plugin::{Plugin, PluginInfo, PluginManifest};
use crate::plugins::{install, runtime};
use crate::plugins::view::ViewSpec;
use crate::state::AppState;

/// Build the frontend-facing [`PluginInfo`] from a stored record. If the stored
/// manifest fails to parse (corrupt/old), degrade gracefully: show the columns
/// we have and empty capability lists rather than hiding the plugin.
fn plugin_to_info(p: &Plugin) -> PluginInfo {
    let manifest = serde_json::from_str::<PluginManifest>(&p.manifest).ok();
    let (contributes, permissions, network_hosts) = match &manifest {
        Some(m) => (m.contributes.clone(), m.permissions.clone(), m.network_hosts()),
        None => (Vec::new(), Vec::new(), Vec::new()),
    };
    let key_fingerprint = manifest
        .as_ref()
        .and_then(|m| m.public_key.as_ref())
        .map(|k| k.chars().take(16).collect());
    PluginInfo {
        id: p.id.clone(),
        name: p.name.clone(),
        version: p.version.clone(),
        author: p.author.clone(),
        description: p.description.clone(),
        enabled: p.enabled,
        contributes,
        permissions,
        network_hosts,
        signed: p.signed,
        key_fingerprint,
        source: p.source.clone(),
        installed_at: p.installed_at.clone(),
    }
}

#[tauri::command]
pub fn get_plugins(state: State<AppState>) -> Result<Vec<PluginInfo>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let plugins = db::plugins::list_plugins(&conn).map_err(|e| e.to_string())?;
    Ok(plugins.iter().map(plugin_to_info).collect())
}

/// Install (or upgrade) a plugin by sideloading a `plugin.json` the user picked.
#[tauri::command]
pub fn install_plugin_from_file(
    path: String,
    state: State<AppState>,
) -> Result<PluginInfo, String> {
    install::from_file(&state, &path).map(|p| plugin_to_info(&p))
}

/// Install (or upgrade) a plugin from a signed `.syzify-ext` package.
#[tauri::command]
pub fn install_plugin_from_package(
    path: String,
    state: State<AppState>,
) -> Result<PluginInfo, String> {
    install::from_package(&state, &path).map(|p| plugin_to_info(&p))
}

#[tauri::command]
pub fn set_plugin_enabled(
    id: String,
    enabled: bool,
    state: State<AppState>,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::plugins::set_enabled(&conn, &id, enabled).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn uninstall_plugin(id: String, state: State<AppState>) -> Result<(), String> {
    // Guard the id before it touches the filesystem (defense in depth — a bad id
    // should never have been stored, but never remove_dir_all outside plugins/).
    crate::models::plugin::validate_id(&id)?;
    {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        db::plugins::delete_plugin(&conn, &id).map_err(|e| e.to_string())?;
    }
    // Best-effort removal of the plugin's vault directory (wasm + manifest copy).
    let _ = std::fs::remove_dir_all(state.vault_path.join("plugins").join(&id));
    Ok(())
}

/// A network host an installed plugin may contact, for the privacy disclosure
/// in settings (PRD §16.2). Only enabled plugins are reported.
#[derive(serde::Serialize)]
pub struct PluginEndpoint {
    pub plugin_id: String,
    pub plugin_name: String,
    pub host: String,
}

#[tauri::command]
pub fn get_plugin_network_endpoints(
    state: State<AppState>,
) -> Result<Vec<PluginEndpoint>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let plugins = db::plugins::list_plugins(&conn).map_err(|e| e.to_string())?;

    let mut endpoints = Vec::new();
    for p in plugins.iter().filter(|p| p.enabled) {
        if let Ok(manifest) = serde_json::from_str::<PluginManifest>(&p.manifest) {
            for host in manifest.network_hosts() {
                endpoints.push(PluginEndpoint {
                    plugin_id: p.id.clone(),
                    plugin_name: p.name.clone(),
                    host,
                });
            }
        }
    }
    Ok(endpoints)
}

/// A plugin that hooks into a given contribution point.
#[derive(serde::Serialize)]
pub struct PluginContribution {
    pub plugin_id: String,
    pub name: String,
}

/// Enabled plugins contributing to `point` (e.g. "dashboard.widget").
#[tauri::command]
pub fn get_plugin_contributions(
    point: String,
    state: State<AppState>,
) -> Result<Vec<PluginContribution>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let plugins = db::plugins::list_plugins(&conn).map_err(|e| e.to_string())?;
    Ok(plugins
        .iter()
        .filter(|p| p.enabled)
        .filter_map(|p| {
            let manifest = serde_json::from_str::<PluginManifest>(&p.manifest).ok()?;
            // Only runnable plugins (with a wasm entry) can render a contribution;
            // manifest-only entries are skipped so they don't produce broken widgets.
            let contributes = manifest.entry.is_some() && manifest.contributes.iter().any(|c| c == &point);
            contributes.then(|| PluginContribution {
                plugin_id: p.id.clone(),
                name: p.name.clone(),
            })
        })
        .collect())
}

/// Whether the plugin's stored manifest contributes `point` — the same test
/// `get_plugin_contributions` applies, so a render cannot reach an export
/// the manifest never declared (the interactive budget and the continue
/// loop hang on the export name).
fn plugin_contributes(conn: &rusqlite::Connection, plugin_id: &str, point: &str) -> Result<bool, String> {
    let Some(plugin) = db::plugins::get_plugin(conn, plugin_id).map_err(|e| e.to_string())? else {
        return Ok(false);
    };
    // A corrupt manifest contributes nothing, as the list and the info say.
    let Ok(manifest) = serde_json::from_str::<PluginManifest>(&plugin.manifest) else {
        return Ok(false);
    };
    Ok(manifest.entry.is_some() && manifest.contributes.iter().any(|c| c == point))
}

/// Render a plugin's UI contribution. The contribution point maps to the WASM
/// export by replacing dots with underscores (`dashboard.widget` ->
/// `dashboard_widget`); a point the manifest does not contribute is refused. `context` is an opaque JSON string passed to the plugin
/// (e.g. the current activity id for a detail panel).
///
/// Off the main thread: a call may fetch over the network or import files
/// (up to the sandbox's budget — 5 s, 30 s for a network plugin's
/// `route.planner` / `sync.source` action), which would freeze the window
/// from a sync command.
#[tauri::command]
pub async fn render_plugin_view(
    plugin_id: String,
    point: String,
    context: String,
    app: AppHandle,
) -> Result<ViewSpec, String> {
    let export = point.replace('.', "_");
    let output = tauri::async_runtime::spawn_blocking({
        let plugin_id = plugin_id.clone();
        move || {
            // The DB lock waits behind a host call in flight: not on an
            // async worker.
            {
                let state = app.state::<AppState>();
                let conn = state.db.lock().map_err(|e| e.to_string())?;
                if !plugin_contributes(&conn, &plugin_id, &point)? {
                    return Err(format!("plugin {plugin_id} does not contribute {point}"));
                }
            }
            runtime::run_contribution(&app, &plugin_id, &export, &context)
        }
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))??;
    let spec = serde_json::from_str::<ViewSpec>(&output)
        .map_err(|e| format!("plugin {plugin_id} returned an invalid view: {e}"))?;
    spec.validate().map_err(|e| format!("plugin {plugin_id} returned an invalid view: {e}"))?;
    Ok(spec)
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_render_is_refused_for_a_point_the_manifest_does_not_contribute() {
        let conn = crate::db::test_db();
        let plugin = |id: &str, manifest: &str| crate::models::plugin::Plugin {
            id: id.to_string(),
            name: "P".to_string(),
            version: "0.1.0".to_string(),
            author: None,
            description: None,
            enabled: true,
            signed: false,
            manifest: manifest.to_string(),
            source: format!("plugins/{id}"),
            installed_at: String::new(),
            updated_at: String::new(),
        };
        crate::db::plugins::upsert_plugin(
            &conn,
            &plugin("com.w", r#"{"id":"com.w","name":"W","version":"0.1.0","entry":"plugin.wasm","contributes":["dashboard.widget"]}"#),
        )
        .unwrap();
        crate::db::plugins::upsert_plugin(
            &conn,
            &plugin("com.noentry", r#"{"id":"com.noentry","name":"N","version":"0.1.0","contributes":["sync.source"]}"#),
        )
        .unwrap();
        assert!(super::plugin_contributes(&conn, "com.w", "dashboard.widget").unwrap());
        assert!(!super::plugin_contributes(&conn, "com.w", "sync.source").unwrap(), "an export is not a contribution");
        assert!(!super::plugin_contributes(&conn, "com.noentry", "sync.source").unwrap(), "nothing to run");
        assert!(!super::plugin_contributes(&conn, "com.missing", "sync.source").unwrap());
        crate::db::plugins::upsert_plugin(&conn, &plugin("com.bad", "{not json")).unwrap();
        assert!(!super::plugin_contributes(&conn, "com.bad", "sync.source").unwrap(), "a corrupt manifest contributes nothing");
    }

    use super::*;

    fn record(manifest: &str) -> Plugin {
        Plugin {
            id: "com.x".to_string(),
            name: "X".to_string(),
            version: "1.0.0".to_string(),
            author: None,
            description: None,
            enabled: true,
            signed: false,
            manifest: manifest.to_string(),
            source: "sideload".to_string(),
            installed_at: String::new(),
            updated_at: String::new(),
        }
    }

    #[test]
    fn info_extracts_capabilities_from_manifest() {
        let p = record(
            r#"{"id":"com.x","name":"X","version":"1.0.0",
                "contributes":["dashboard.widget"],
                "permissions":["read:activities","net:host=api.example.com"]}"#,
        );
        let info = plugin_to_info(&p);
        assert_eq!(info.contributes, vec!["dashboard.widget"]);
        assert_eq!(info.network_hosts, vec!["api.example.com"]);
    }

    #[test]
    fn info_degrades_gracefully_on_corrupt_manifest() {
        let p = record("not valid json{");
        let info = plugin_to_info(&p);
        // Still surfaces the stored columns, just with empty capability lists.
        assert_eq!(info.name, "X");
        assert_eq!(info.version, "1.0.0");
        assert!(info.contributes.is_empty());
        assert!(info.permissions.is_empty());
        assert!(info.network_hosts.is_empty());
    }

}
