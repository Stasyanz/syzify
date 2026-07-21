//! Loads a plugin's WASM module in an Extism (wasmtime) sandbox and calls one
//! of its exported contribution functions. The module is memory-isolated and
//! has no ambient authority — it can only call the host functions we wire in.

use std::time::Duration;

use extism::{Manifest as ExtismManifest, PluginBuilder, Wasm, PTR};
use tauri::{AppHandle, Manager};

use crate::db;
use crate::models::plugin::PluginManifest;
use crate::plugins::host::{self, PluginCtx};
use crate::state::AppState;

/// Hard caps so a misbehaving plugin can't hang or exhaust memory.
const PLUGIN_TIMEOUT: Duration = Duration::from_secs(5);
const PLUGIN_MAX_PAGES: u32 = 1024; // 1024 × 64 KiB = 64 MiB
const PLUGIN_MAX_HTTP_BYTES: u64 = 5 * 1024 * 1024; // 5 MiB per network response

/// Run an enabled plugin's exported contribution function, returning its raw
/// output string (a ViewSpec JSON for UI contributions).
///
/// INVARIANT: this must not be called re-entrantly. Host functions acquire
/// `state.db` (a non-reentrant `std::sync::Mutex`) during `plugin.call`, so a
/// plugin that could trigger another `run_contribution` on the same thread
/// would deadlock. There is no host function that runs a plugin today; keep it
/// that way (no plugin-invokes-plugin host call) unless the lock model changes.
pub fn run_contribution(
    app: &AppHandle,
    plugin_id: &str,
    export: &str,
    input: &str,
) -> Result<String, String> {
    let db_handle = app.state::<AppState>().db.clone();
    let vault_path = app.state::<AppState>().vault_path.clone();
    let record = {
        let conn = db_handle.lock().map_err(|e| e.to_string())?;
        db::plugins::get_plugin(&conn, plugin_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("plugin not found: {plugin_id}"))?
    }; // db lock released before running the plugin (host fns lock per call)

    if !record.enabled {
        return Err(format!("plugin is disabled: {plugin_id}"));
    }

    let manifest: PluginManifest =
        serde_json::from_str(&record.manifest).map_err(|e| e.to_string())?;
    let entry = manifest
        .entry
        .as_deref()
        .ok_or_else(|| format!("plugin {plugin_id} has no wasm entry"))?;
    // Re-validate the entry from the stored manifest before joining it into a
    // path — the stored manifest is not re-run through parse_and_validate.
    crate::models::plugin::validate_entry(entry)?;
    // `source` is set by the installer to "plugins/<validated-id>", so plugins
    // survive backup/restore and a vault move and stay inside the vault.
    let path = vault_path.join(&record.source).join(entry);

    let ctx = PluginCtx {
        db: db_handle,
        plugin_id: plugin_id.to_string(),
        permissions: manifest.parsed_permissions(),
    };
    let ud = host::user_data(ctx);

    // Network is default-deny: the sandbox can only reach hosts the plugin
    // declared via `net:host=` permissions (and that the user saw before
    // enabling). Timeout and memory caps bound a misbehaving plugin.
    let mut ext_manifest = ExtismManifest::new([Wasm::file(path)])
        .with_timeout(PLUGIN_TIMEOUT)
        .with_memory_max(PLUGIN_MAX_PAGES);
    // Bound network responses too (public field — avoids an extra dependency).
    ext_manifest.memory.max_http_response_bytes = Some(PLUGIN_MAX_HTTP_BYTES);
    for host in manifest.network_hosts() {
        ext_manifest = ext_manifest.with_allowed_host(host);
    }
    let mut plugin = PluginBuilder::new(ext_manifest)
        .with_wasi(false)
        .with_function("host_query", [PTR], [PTR], ud.clone(), host::host_query)
        .with_function("host_data_set", [PTR], [PTR], ud.clone(), host::host_data_set)
        .with_function("host_data_get", [PTR], [PTR], ud.clone(), host::host_data_get)
        .with_function("host_kv_set", [PTR], [PTR], ud.clone(), host::host_kv_set)
        .with_function("host_kv_get", [PTR], [PTR], ud.clone(), host::host_kv_get)
        .build()
        .map_err(|e| {
            // Detail (may include vault paths) goes to the log, not the UI.
            eprintln!("plugin {plugin_id} load error: {e}");
            format!("failed to load plugin {plugin_id}")
        })?;

    let out = plugin
        .call::<&str, &str>(export, input)
        .map_err(|e| format!("plugin {plugin_id} export {export} failed: {e}"))?;
    Ok(out.to_string())
}

#[cfg(test)]
mod tests {
    use crate::plugins::view::ViewSpec;
    use extism::{host_fn, Manifest as ExtismManifest, PluginBuilder, UserData, Wasm, PTR};

    // Stub host_query returning canned 4-week dashboard totals, so the test
    // exercises the real plugin WASM + host-function ABI without Tauri/db.
    host_fn!(stub_query(_user_data: (); _req: String) -> String {
        Ok(r#"{"total_activities":8,"total_distance_m":40000.0}"#.to_string())
    });

    // Stub returning a single activity, for the detail-panel export.
    host_fn!(stub_activity(_user_data: (); _req: String) -> String {
        Ok(r#"{"distance_m":10000.0,"duration_s":3000.0}"#.to_string())
    });

    // The reference wasm is committed; fail loudly (not silently skip) if it's
    // missing, so a broken artifact can't pass as a green security test.
    fn reference_wasm() -> &'static str {
        let wasm = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../examples/plugins/consistency-widget/plugin.wasm"
        );
        assert!(
            std::path::Path::new(wasm).exists(),
            "missing {wasm}; build it: cargo build --release --target wasm32-unknown-unknown in examples/plugins/consistency-widget"
        );
        wasm
    }

    #[test]
    fn reference_plugin_renders_consistency_widget() {
        let wasm = reference_wasm();

        let manifest = ExtismManifest::new([Wasm::file(wasm)]);
        let ud = UserData::new(());
        let mut plugin = PluginBuilder::new(manifest)
            .with_wasi(false)
            .with_function("host_query", [PTR], [PTR], ud, stub_query)
            .build()
            .expect("load reference plugin");

        let out = plugin
            .call::<&str, &str>("dashboard_widget", "{}")
            .expect("call dashboard_widget");
        let spec: ViewSpec = serde_json::from_str(out).expect("valid ViewSpec");

        assert!(spec.title.unwrap().contains("Consistency"));
        let elements = serde_json::to_string(&spec.elements).unwrap();
        assert!(elements.contains("\"value\":\"8\""), "8 activities");
        assert!(elements.contains("2.0"), "2.0 per week");
        assert!(elements.contains("40 km"), "40 km distance");
    }

    #[test]
    fn reference_plugin_renders_activity_detail_panel() {
        let wasm = reference_wasm();

        let manifest = ExtismManifest::new([Wasm::file(wasm)]);
        let ud = UserData::new(());
        let mut plugin = PluginBuilder::new(manifest)
            .with_wasi(false)
            .with_function("host_query", [PTR], [PTR], ud, stub_activity)
            .build()
            .expect("load reference plugin");

        let out = plugin
            .call::<&str, &str>("activity_detail_panel", r#"{"activity_id":"a1"}"#)
            .expect("call activity_detail_panel");
        let spec: ViewSpec = serde_json::from_str(out).expect("valid ViewSpec");

        let elements = serde_json::to_string(&spec.elements).unwrap();
        // 10 km in 3000 s = 5.00 min/km
        assert!(elements.contains("5.00 min/km"), "pace");
        assert!(elements.contains("10.0 km"), "distance");
    }

    #[test]
    fn route_planner_network_is_fail_closed_without_permission() {
        let wasm = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../examples/plugins/smart-route/plugin.wasm"
        );
        assert!(
            std::path::Path::new(wasm).exists(),
            "missing committed {wasm} — rebuild the smart-route example"
        );

        // Built with NO allowed_hosts. The plugin tries to reach open-meteo, so
        // the call must fail (default-deny enforced at the WASM boundary). The
        // host surfaces this as an error card; it never reaches a host the user
        // didn't approve.
        let manifest = ExtismManifest::new([Wasm::file(wasm)]);
        let mut plugin = PluginBuilder::new(manifest)
            .with_wasi(false)
            .build()
            .expect("load smart-route plugin");

        // The "plan" action triggers the weather fetch; with no allowed host it
        // must fail (the initial form, which makes no request, would succeed).
        let result = plugin.call::<&str, &str>("route_planner", r#"{"action":"plan"}"#);
        assert!(result.is_err(), "network must be denied without an allowed host");
    }

    // End-to-end capability gate: a REAL plugin wasm calling the REAL host_query
    // is denied without the permission and allowed with it. Exercises the actual
    // with_function wiring + PluginCtx.permissions + ctx.require (not a stub).
    #[test]
    fn permission_is_enforced_at_host_boundary_through_real_wasm() {
        use crate::models::plugin::Permission;
        use crate::plugins::host::{self, PluginCtx};
        use std::sync::{Arc, Mutex};

        let wasm = reference_wasm(); // dashboard_widget calls host_query{dashboard} (needs read:dashboard)

        let run = |permissions: Vec<Permission>| {
            let ctx = PluginCtx {
                db: Arc::new(Mutex::new(crate::db::test_db())),
                plugin_id: "test".to_string(),
                permissions,
            };
            PluginBuilder::new(ExtismManifest::new([Wasm::file(wasm)]))
                .with_wasi(false)
                .with_function("host_query", [PTR], [PTR], host::user_data(ctx), host::host_query)
                .build()
                .expect("load reference plugin")
                .call::<&str, &str>("dashboard_widget", "{}")
                .map(|s| s.to_string())
        };

        // Without read:dashboard the host denies the query → the call fails.
        assert!(run(vec![]).is_err(), "missing permission must be denied");
        // Wrong permission doesn't satisfy it either.
        assert!(run(vec![Permission::ReadActivities]).is_err(), "wrong permission must be denied");
        // With the right permission it succeeds (empty db → zeroed dashboard).
        let out = run(vec![Permission::ReadDashboard]).expect("granted permission must pass");
        let spec: ViewSpec = serde_json::from_str(&out).unwrap();
        assert!(spec.title.unwrap().contains("Consistency"));
    }
}
