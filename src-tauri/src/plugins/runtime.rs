//! Loads a plugin's WASM module in an Extism (wasmtime) sandbox and calls one
//! of its exported contribution functions. The module is memory-isolated and
//! has no ambient authority — it can only call the host functions we wire in.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use extism::{Manifest as ExtismManifest, PluginBuilder, UserData, Wasm, PTR};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::db;
use crate::import::pipeline::{ImportResult, MonitoringBatch};
use crate::models::plugin::PluginManifest;
use crate::plugins::host::{self, PluginCtx, VaultAccess};
use crate::plugins::net::NetState;
use crate::state::{AppState, SingleFlightGuard};

/// Hard caps so a misbehaving plugin can't hang or exhaust memory.
const PLUGIN_TIMEOUT: Duration = Duration::from_secs(5);
/// The budget of a network plugin's interactive page: a login is five or
/// six round trips, a sync page a few more, and the epoch deadline would
/// otherwise fire the moment the host call returned. The user pressed the
/// button and waits; 30 s off the main thread is the cost of it
/// (`render_plugin_view` runs on a blocking thread).
const PLUGIN_NET_TIMEOUT: Duration = Duration::from_secs(30);
const PLUGIN_MAX_PAGES: u32 = 1024; // 1024 × 64 KiB = 64 MiB

/// The one contribution point where the user pressed a button and waits
/// for the answer. Widgets and panels render on their own (a remount, a
/// cache refresh) and every invocation queues behind `RUN`: a widget that
/// could hold the lock for 30 s would stall every other plugin's render.
const INTERACTIVE_EXPORT: &str = "route_planner";

/// How long one invocation may run before the sandbox traps it.
fn invocation_budget(talks_to_the_network: bool, export: &str) -> Duration {
    if talks_to_the_network && export == INTERACTIVE_EXPORT { PLUGIN_NET_TIMEOUT } else { PLUGIN_TIMEOUT }
}

/// The live `AppState` behind an `AppHandle`, for `host_import_file`. Every
/// read goes to the managed state at call time (see `VaultAccess`); nothing
/// is snapshotted when the context is built.
struct AppVault<R: Runtime>(AppHandle<R>);

impl<R: Runtime> VaultAccess for AppVault<R> {
    fn vault_path(&self) -> PathBuf {
        self.0.state::<AppState>().inner().vault_path()
    }
    fn ensure_unlocked(&self) -> Result<(), String> {
        self.0.state::<AppState>().inner().ensure_unlocked()
    }
    fn activities_key(&self) -> Result<Option<[u8; 32]>, String> {
        self.0.state::<AppState>().inner().activities_key()
    }
    fn claim_vault(&self) -> Option<SingleFlightGuard<'_>> {
        self.0.state::<AppState>().inner().claim_vault()
    }
}

/// Event the frontend refreshes its activity-derived queries on: a plugin
/// imported files during a contribution call. Emitted by the runtime (the
/// host layer has no `AppHandle`), whether the call then returned a view or
/// trapped — the files are in the vault either way.
pub const PLUGINS_IMPORTED_EVENT: &str = "plugins:imported";

/// One plugin invocation at a time. `render_plugin_view` runs off the main
/// thread, so two widgets could otherwise call in parallel and fight over
/// the vault-mutation slot (`host_import_file` claims it per file) — the
/// loser would trap for no reason of its own. A poisoned lock (a panicking
/// call) must not take every later plugin down with it.
static RUN: Mutex<()> = Mutex::new(());

/// Run an enabled plugin's exported contribution function, returning its raw
/// output string (a ViewSpec JSON for UI contributions).
///
/// INVARIANT: this must not be called re-entrantly. Invocations are
/// serialized by `RUN`, and host functions acquire `state.db` (a
/// non-reentrant `std::sync::Mutex`) during `plugin.call`, so a plugin that
/// could trigger another `run_contribution` from inside its call would
/// deadlock. There is no host function that runs a plugin today; keep it
/// that way (no plugin-invokes-plugin host call) unless the lock model changes.
pub fn run_contribution<R: Runtime>(
    app: &AppHandle<R>,
    plugin_id: &str,
    export: &str,
    input: &str,
) -> Result<String, String> {
    let _one_at_a_time = RUN.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
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
    // Same for the hosts: after this point they are the whole allow-list
    // of `host_http`, and a swapped or rolled-back vault DB must not turn
    // a metadata address into a declared host.
    for host in manifest.network_hosts() {
        crate::models::plugin::validate_net_host(&host)?;
    }
    // `source` is set by the installer to "plugins/<validated-id>", so plugins
    // survive backup/restore and a vault move and stay inside the vault.
    let path = vault_path.join(&record.source).join(entry);

    let budget = invocation_budget(!manifest.network_hosts().is_empty(), export);
    let ctx = PluginCtx {
        db: db_handle,
        plugin_id: plugin_id.to_string(),
        permissions: manifest.parsed_permissions(),
        vault: Some(Arc::new(AppVault(app.clone()))),
        imported: 0,
        imported_activities: 0,
        monitoring: MonitoringBatch::default(),
        // Set before the sandbox is built, so it ends no later than the
        // epoch deadline: no request outlives its invocation.
        net: NetState::with_deadline(Instant::now() + budget),
    };
    let ud = host::user_data(ctx);

    // Network goes through `host_http` only: it checks every request and
    // every redirect hop against the plugin's `net:host=` hosts (the ones
    // the user saw before enabling). Extism's built-in HTTP is given no
    // allowed host, so it refuses everything. Timeout and memory caps
    // bound a misbehaving plugin.
    let ext_manifest = ExtismManifest::new([Wasm::file(path)])
        .with_timeout(budget)
        .with_memory_max(PLUGIN_MAX_PAGES);
    let mut plugin = PluginBuilder::new(ext_manifest)
        .with_wasi(false)
        .with_function("host_query", [PTR], [PTR], ud.clone(), host::host_query)
        .with_function("host_data_set", [PTR], [PTR], ud.clone(), host::host_data_set)
        .with_function("host_data_get", [PTR], [PTR], ud.clone(), host::host_data_get)
        .with_function("host_kv_set", [PTR], [PTR], ud.clone(), host::host_kv_set)
        .with_function("host_kv_get", [PTR], [PTR], ud.clone(), host::host_kv_get)
        .with_function("host_import_file", [PTR, PTR], [PTR], ud.clone(), host::host_import_file)
        .with_function("host_http", [PTR, PTR], [PTR], ud.clone(), host::host_http)
        .with_function("host_http_meta", [], [PTR], ud.clone(), host::host_http_meta)
        .build()
        .map_err(|e| {
            // Detail (may include vault paths) goes to the log, not the UI.
            eprintln!("plugin {plugin_id} load error: {e}");
            format!("failed to load plugin {plugin_id}")
        })?;

    let out = plugin
        .call::<&str, &str>(export, input)
        .map(|s| s.to_string())
        .map_err(|e| {
            // A host function's refusal ("host X is not declared") is the
            // ROOT of the chain; the outer layers are wasmtime's "error
            // while executing at wasm backtrace" — the log gets those.
            // So a host function's error string is user-facing text: no
            // vault paths in it (those go to eprintln! in host.rs).
            eprintln!("plugin {plugin_id} export {export} failed: {e:?}");
            format!("plugin {plugin_id} export {export} failed: {}", e.root_cause())
        });
    // Before the `?`: a call that imported 40 files and then trapped on the
    // 41st (a bad name, the time budget) still changed the vault, and the
    // views must follow. Recompute first, then the event — the frontend
    // refetches on it and must not see the days half done.
    let done = finish_invocation(&ud);
    if done.imported > 0 {
        let _ = app.emit(
            PLUGINS_IMPORTED_EVENT,
            serde_json::json!({ "imported": done.imported, "monitoring_days": done.monitoring_days }),
        );
    }
    // Newly imported activities get their location names like a drop
    // import's do; Monitor files have nothing to geocode.
    if done.activities > 0 {
        let geo_handle = app.clone();
        std::thread::spawn(move || {
            crate::import::geocoding::run_background_geocoding(&geo_handle);
        });
    }
    out
}

/// What an invocation left behind, read once it is over.
#[derive(Debug, Default, PartialEq, Eq)]
struct Finished {
    /// Files imported (activities + Monitor files).
    imported: usize,
    /// Of those, activities.
    activities: usize,
    /// Monitor days recomputed at the end.
    monitoring_days: usize,
}

/// Close the invocation behind `ud`: recompute the Monitor days its
/// imports touched — once for all of them, a wellness sync lands 30 days
/// in 30 calls — and read the counters. A failed recompute is logged; no
/// caller is left to report it to, and the boot-time safety net
/// (`run_monitoring_recompute` over days with `computed_at IS NULL`) picks
/// the days up. A poisoned context (a panicking host call) still yields
/// its batch and counters: the files before the panic are in the vault.
///
/// The recompute runs after `host_import_file` released `vault_flight`.
/// A restore that wins the slot in between swaps the DB for an empty
/// in-memory one under the DB lock, so the recompute fails into the log
/// and the app restarts anyway; a relocation reopens the same DB at its
/// new path. Neither corrupts a day.
fn finish_invocation(ud: &UserData<PluginCtx>) -> Finished {
    let Ok(ctx) = ud.get() else { return Finished::default() };
    let mut ctx = ctx.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let batch = std::mem::take(&mut ctx.monitoring);
    let mut monitoring_days = 0;
    if !batch.days.is_empty() {
        let mut result = ImportResult::default();
        let recomputed = ctx
            .db
            .lock()
            .map_err(|e| e.to_string())
            .and_then(|conn| batch.finish(&conn, &mut result));
        match recomputed {
            Ok(()) => monitoring_days = result.monitoring_days,
            Err(e) => eprintln!("plugin {}: monitoring recompute failed: {e}", ctx.plugin_id),
        }
    }
    Finished { imported: ctx.imported, activities: ctx.imported_activities, monitoring_days }
}

#[cfg(test)]
mod tests {
    use crate::import::pipeline::MonitoringBatch;
    use crate::plugins::net::NetState;
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

    // The reference wasm is built locally / by the CI fixtures step (it is
    // gitignored); fail loudly (not silently skip) if it's
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
        use crate::models::plugin::Permission;
        use crate::plugins::host::{self, PluginCtx};
        use std::sync::{Arc, Mutex};

        let wasm = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../examples/plugins/smart-route/plugin.wasm"
        );
        assert!(
            std::path::Path::new(wasm).exists(),
            "missing {wasm} — rebuild the smart-route example (see examples/plugins/README.md)"
        );

        // The plugin reaches open-meteo through host_http; a context without
        // that host declared refuses the call at the host boundary (an error
        // card in the UI), and nothing is sent anywhere.
        let run = |permissions: Vec<Permission>, input: &str| {
            let ctx = PluginCtx::new(Arc::new(Mutex::new(crate::db::test_db())), "smart", permissions);
            let ud = host::user_data(ctx);
            PluginBuilder::new(ExtismManifest::new([Wasm::file(wasm)]))
                .with_wasi(false)
                .with_function("host_http", [PTR, PTR], [PTR], ud.clone(), host::host_http)
                .with_function("host_http_meta", [], [PTR], ud, host::host_http_meta)
                .build()
                .expect("load smart-route plugin")
                .call::<&str, &str>("route_planner", input)
                .map(|s| s.to_string())
        };
        // The initial form makes no request.
        assert!(run(vec![], "{}").is_ok());
        // "plan" fetches the weather: refused without the host, and with
        // another host declared.
        let err = run(vec![], r#"{"action":"plan"}"#).unwrap_err().root_cause().to_string();
        assert!(err.contains("lacks permission net:host"), "{err}");
        let other = vec![Permission::Net { host: "api.example.com".to_string() }];
        let err = run(other, r#"{"action":"plan"}"#).unwrap_err().root_cause().to_string();
        assert!(err.contains("api.open-meteo.com") && err.contains("not declared"), "{err}");
        // Extism's own HTTP gets no allowed host any more: a plugin built
        // against it (the pre-#108 ABI) is refused the same way.
        let mut old_style = PluginBuilder::new(ExtismManifest::new([Wasm::file(wasm)]).with_allowed_host("api.open-meteo.com"))
            .with_wasi(false)
            .with_function("host_http", [PTR, PTR], [PTR], host::user_data(PluginCtx::new(Arc::new(Mutex::new(crate::db::test_db())), "smart", vec![])), host::host_http)
            .with_function("host_http_meta", [], [PTR], host::user_data(PluginCtx::new(Arc::new(Mutex::new(crate::db::test_db())), "smart", vec![])), host::host_http_meta)
            .build()
            .unwrap();
        assert!(old_style.call::<&str, &str>("route_planner", r#"{"action":"plan"}"#).is_err(), "the manifest allow-list is not consulted by host_http");
    }

    fn net_probe_wasm() -> &'static str {
        let wasm = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../examples/plugins/net-probe/plugin.wasm"
        );
        assert!(
            std::path::Path::new(wasm).exists(),
            "missing {wasm}; build it: cargo build --release --target wasm32-unknown-unknown in examples/plugins/net-probe"
        );
        wasm
    }

    /// The net-probe wasm calling the REAL host_http / host_http_meta (two-
    /// pointer and zero-argument ABI) against the plain loopback server:
    /// the jar lives for one plugin instance, headers come back as pairs,
    /// and an undeclared host traps the call.
    #[test]
    fn http_permission_and_cookie_jar_hold_at_the_host_boundary_through_real_wasm() {
        use crate::models::plugin::Permission;
        use crate::plugins::host::{self, PluginCtx};
        use crate::plugins::net::test_server::{Reply, Server};
        use std::sync::{Arc, Mutex};

        let server = Server::start(|req| match req.path.as_str() {
            "/login" => Reply::ok("in")
                .header("Set-Cookie", "SESSIONID=abc; Path=/")
                .header("Set-Cookie", "CASTGC=TGT-1; Path=/"),
            "/hop" => Reply::redirect(302, "/home").header("Set-Cookie", "hop=1; Path=/"),
            _ => Reply::ok("home"),
        });
        let load = |permissions: Vec<Permission>| {
            let mut ctx = PluginCtx::new(Arc::new(Mutex::new(crate::db::test_db())), "probe", permissions);
            ctx.net.plain_loopback = true;
            let ud = host::user_data(ctx);
            PluginBuilder::new(ExtismManifest::new([Wasm::file(net_probe_wasm())]))
                .with_wasi(false)
                .with_function("host_http", [PTR, PTR], [PTR], ud.clone(), host::host_http)
                .with_function("host_http_meta", [], [PTR], ud, host::host_http_meta)
                .build()
                .expect("load net-probe plugin")
        };
        let probe = |plugin: &mut extism::Plugin, req: serde_json::Value| {
            plugin
                .call::<&str, &str>("probe", &req.to_string())
                .map(|s| serde_json::from_str::<serde_json::Value>(s).unwrap())
                .map_err(|e| e.root_cause().to_string())
        };
        // Beside another permission: only the hosts make the allow-list.
        let local = vec![Permission::Net { host: "127.0.0.1".to_string() }, Permission::ReadActivities];

        // The meta before any request is an error, not a stale one.
        let err = load(vec![]).call::<&str, &str>("meta_only", "{}").unwrap_err().root_cause().to_string();
        assert!(err.contains("no response yet"), "{err}");

        // No host declared: the call traps before anything is sent.
        let err = probe(&mut load(vec![]), serde_json::json!({ "url": server.url("/login") })).unwrap_err();
        assert!(err.contains("lacks permission net:host"), "{err}");
        let other = vec![Permission::Net { host: "api.example.com".to_string() }];
        let err = probe(&mut load(other), serde_json::json!({ "url": server.url("/login") })).unwrap_err();
        assert!(err.contains("not declared"), "{err}");
        assert!(server.requests().is_empty());

        // One instance = one invocation: the jar carries over between calls.
        let mut plugin = load(local.clone());
        let out = probe(&mut plugin, serde_json::json!({ "url": server.url("/login") })).unwrap();
        assert_eq!(out["status"], 200);
        assert_eq!(out["body"], "in");
        assert_eq!(out["url"], server.url("/login"));
        let set: Vec<&str> = out["headers"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|p| p[0] == "set-cookie")
            .map(|p| p[1].as_str().unwrap())
            .collect();
        assert_eq!(set, ["SESSIONID=abc; Path=/", "CASTGC=TGT-1; Path=/"]);
        let out = probe(&mut plugin, serde_json::json!({ "url": server.url("/hop"), "headers": [["X-Probe", "1"]] })).unwrap();
        assert_eq!((out["status"].as_u64(), out["url"].as_str()), (Some(200), Some(server.url("/home").as_str())));
        // A POST with a body, then a request from a fresh instance.
        probe(&mut plugin, serde_json::json!({ "url": server.url("/home"), "method": "POST", "body": "a=1" })).unwrap();
        probe(&mut load(local), serde_json::json!({ "url": server.url("/home") })).unwrap();

        let sent = server.requests();
        assert_eq!(sent.iter().map(|r| r.path.as_str()).collect::<Vec<_>>(), ["/login", "/hop", "/home", "/home", "/home"]);
        assert_eq!(sent[0].header("cookie"), None);
        assert_eq!(sent[1].header("cookie"), Some("SESSIONID=abc; CASTGC=TGT-1"));
        assert_eq!(sent[1].header("x-probe"), Some("1"));
        assert_eq!(sent[2].header("cookie"), Some("SESSIONID=abc; CASTGC=TGT-1; hop=1"));
        assert_eq!((sent[3].method.as_str(), sent[3].body.as_slice()), ("POST", &b"a=1"[..]));
        assert_eq!(sent[4].header("cookie"), None, "a new invocation starts with an empty jar");
    }

    #[test]
    fn network_plugins_get_the_longer_budget_on_the_interactive_page_only() {
        assert_eq!(super::invocation_budget(false, "route_planner"), super::PLUGIN_TIMEOUT);
        assert_eq!(super::invocation_budget(true, "dashboard_widget"), super::PLUGIN_TIMEOUT);
        assert_eq!(super::invocation_budget(true, "route_planner"), super::PLUGIN_NET_TIMEOUT);
        assert!(super::PLUGIN_NET_TIMEOUT >= crate::plugins::net::HTTP_REQUEST_TIMEOUT, "one hop may use the whole budget");
    }

    /// The runtime's wiring on a real (windowless) app: the stored
    /// manifest's `net:host=` is the allow-list, and only https passes —
    /// both refusals happen before any connection, so no network is needed.
    #[test]
    fn run_contribution_refuses_undeclared_and_plain_hosts_through_the_live_app() {
        use super::run_contribution;
        use crate::models::plugin::Plugin;
        use crate::state::AppState;
        use std::sync::{Arc, Mutex};
        use tauri::Manager;

        let plugin_id = "com.syzify.example.net-probe";
        let vault = std::env::temp_dir().join(format!("syz_runtime_net_{}", uuid::Uuid::new_v4()));
        let plugin_dir = vault.join("plugins").join(plugin_id);
        std::fs::create_dir_all(&plugin_dir).unwrap();
        std::fs::copy(net_probe_wasm(), plugin_dir.join("plugin.wasm")).unwrap();
        let state = AppState {
            db: Arc::new(Mutex::new(crate::db::test_db())),
            vault_path: vault.clone(),
            encryption_key: Mutex::new(None),
            watcher_handle: Mutex::new(None),
            db_locked: Mutex::new(false),
            vault_error: Mutex::new(None),
            services_started: Mutex::new(false),
            geocoding_flight: crate::state::SingleFlight::default(),
            vault_flight: crate::state::SingleFlight::default(),
        };
        {
            let conn = state.db.lock().unwrap();
            crate::db::plugins::upsert_plugin(
                &conn,
                &Plugin {
                    id: plugin_id.to_string(),
                    name: "Network Probe".to_string(),
                    version: "0.1.0".to_string(),
                    author: None,
                    description: None,
                    enabled: true,
                    signed: false,
                    manifest: format!(
                        r#"{{"id":"{plugin_id}","name":"Network Probe","version":"0.1.0","entry":"plugin.wasm","contributes":["route.planner"],"permissions":["net:host=api.example.com"]}}"#
                    ),
                    source: format!("plugins/{plugin_id}"),
                    installed_at: String::new(),
                    updated_at: String::new(),
                },
            )
            .unwrap();
            crate::db::plugins::set_enabled(&conn, plugin_id, true).unwrap();
        }
        let app = tauri::test::mock_builder()
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("mock app");
        app.manage(state);
        let handle = app.handle().clone();

        let out = run_contribution(&handle, plugin_id, "route_planner", "{}").unwrap();
        assert!(out.contains("Network probe"), "the form renders without a request: {out}");
        let err = run_contribution(&handle, plugin_id, "probe", r#"{"url":"https://other.example.com/"}"#).unwrap_err();
        assert!(err.contains("not declared (net:host=other.example.com)"), "{err}");
        let err = run_contribution(&handle, plugin_id, "probe", r#"{"url":"http://api.example.com/"}"#).unwrap_err();
        assert!(err.contains("only https://"), "{err}");
        let err = run_contribution(&handle, plugin_id, "probe", r#"{"url":"https://api.example.com:8443/"}"#).unwrap_err();
        assert!(err.contains("only the default port"), "{err}");

        // A stored manifest with a host the installer would refuse (a
        // rolled-back or edited vault DB) does not run at all.
        {
            let state = handle.state::<AppState>();
            let conn = state.db.lock().unwrap();
            conn.execute(
                "UPDATE plugin SET manifest = replace(manifest, 'api.example.com', '169.254.169.254') WHERE id = ?1",
                [plugin_id],
            )
            .unwrap();
        }
        let err = run_contribution(&handle, plugin_id, "probe", r#"{"url":"https://169.254.169.254/"}"#).unwrap_err();
        assert!(err.contains("invalid net host"), "{err}");
        let _ = std::fs::remove_dir_all(&vault);
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
            let ctx = PluginCtx::new(Arc::new(Mutex::new(crate::db::test_db())), "test", permissions);
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

    // End-to-end import: the paste-import example wasm calls the REAL
    // host_import_file (two-pointer ABI) against a real vault + db.
    fn paste_import_wasm() -> &'static str {
        let wasm = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../examples/plugins/paste-import/plugin.wasm"
        );
        assert!(
            std::path::Path::new(wasm).exists(),
            "missing {wasm}; build it: cargo build --release --target wasm32-unknown-unknown in examples/plugins/paste-import"
        );
        wasm
    }

    #[test]
    fn import_permission_is_enforced_at_host_boundary_through_real_wasm() {
        use crate::models::plugin::Permission;
        use crate::plugins::host::{self, PluginCtx};
        use crate::state::AppState;
        use std::sync::{Arc, Mutex};

        let vault = std::env::temp_dir().join(format!("syz_wasm_import_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&vault).unwrap();
        let state = Arc::new(AppState {
            db: Arc::new(Mutex::new(crate::db::test_db())),
            vault_path: vault.clone(),
            encryption_key: Mutex::new(None),
            watcher_handle: Mutex::new(None),
            db_locked: Mutex::new(false),
            vault_error: Mutex::new(None),
            services_started: Mutex::new(false),
            geocoding_flight: crate::state::SingleFlight::default(),
            vault_flight: crate::state::SingleFlight::default(),
        });
        let gpx = r#"<?xml version="1.0"?><gpx version="1.1" creator="t" xmlns="http://www.topografix.com/GPX/1/1"><trk><trkseg><trkpt lat="55.75" lon="37.62"><time>2025-06-01T08:00:00Z</time></trkpt><trkpt lat="55.7501" lon="37.6201"><time>2025-06-01T08:00:10Z</time></trkpt></trkseg></trk></gpx>"#;
        let input = |name: &str| {
            serde_json::json!({ "action": "import", "values": { "name": name, "content": gpx } }).to_string()
        };

        // One plugin instance per call, like the runtime: the context (and its
        // counter) lives for one invocation.
        let run = |permissions: Vec<Permission>, input: &str| {
            let ctx = PluginCtx {
                db: state.db.clone(),
                plugin_id: "com.syzify.example.paste-import".to_string(),
                permissions,
                vault: Some(state.clone()),
                imported: 0,
                imported_activities: 0,
                monitoring: MonitoringBatch::default(),
                net: NetState::default(),
            };
            let ud = host::user_data(ctx);
            let out = PluginBuilder::new(ExtismManifest::new([Wasm::file(paste_import_wasm())]))
                .with_wasi(false)
                .with_function("host_import_file", [PTR, PTR], [PTR], ud.clone(), host::host_import_file)
                .build()
                .expect("load paste-import plugin")
                .call::<&str, &str>("dashboard_widget", input)
                .map(|s| s.to_string());
            let done = super::finish_invocation(&ud);
            (out, (done.imported, done.activities))
        };
        let stats = |out: &str| {
            let spec: ViewSpec = serde_json::from_str(out).expect("valid ViewSpec");
            serde_json::to_string(&spec.elements).unwrap()
        };

        // The form alone needs no permission — no host call is made.
        let (out, imported) = run(vec![], "{}");
        assert!(out.is_ok(), "the initial form must render without import:files");
        assert_eq!(imported, (0, 0));

        // Importing without the permission fails at the host boundary.
        let (out, imported) = run(vec![], &input("run.gpx"));
        assert!(out.is_err(), "missing permission must be denied");
        assert_eq!(imported, (0, 0));
        let (out, _) = run(vec![Permission::ReadActivities], &input("run.gpx"));
        assert!(out.is_err(), "wrong permission must be denied");

        // With it: the activity lands, the counter says so.
        let (out, imported) = run(vec![Permission::ImportFiles], &input("run.gpx"));
        let el = stats(&out.expect("granted permission must pass"));
        assert!(el.contains(r#""label":"Imported","value":"1""#), "{el}");
        assert_eq!(imported, (1, 1), "(files, of which activities)");

        // Again: skipped by hash, nothing counted.
        let (out, imported) = run(vec![Permission::ImportFiles], &input("run.gpx"));
        let el = stats(&out.unwrap());
        assert!(el.contains(r#""label":"Skipped","value":"1""#), "{el}");
        assert_eq!(imported, (0, 0));

        // A bad name traps the call — the plugin's mistake, not a failed file.
        let (out, imported) = run(vec![Permission::ImportFiles], &input("../run.gpx"));
        assert!(out.is_err(), "a path-like name must be refused");
        assert_eq!(imported, (0, 0));

        let conn = state.db.lock().unwrap();
        let activities: i64 = conn.query_row("SELECT COUNT(*) FROM activity", [], |r| r.get(0)).unwrap();
        assert_eq!(activities, 1);
        drop(conn);
        assert_eq!(std::fs::read_dir(vault.join("raw")).unwrap().count(), 1);
        let _ = std::fs::remove_dir_all(&vault);
    }

    /// The batch a context accumulated is recomputed once when the
    /// invocation is finished — every touched day, however many calls
    /// touched it — and the counters come out with it.
    #[test]
    fn finish_invocation_recomputes_all_touched_days_once() {
        use crate::models::plugin::Permission;
        use crate::parser::fit_builder::monitoring_fixture;
        use crate::plugins::host::{self, PluginCtx};
        use crate::state::AppState;
        use std::sync::{Arc, Mutex};

        let vault = std::env::temp_dir().join(format!("syz_finish_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&vault).unwrap();
        let state = Arc::new(AppState {
            db: Arc::new(Mutex::new(crate::db::test_db())),
            vault_path: vault.clone(),
            encryption_key: Mutex::new(None),
            watcher_handle: Mutex::new(None),
            db_locked: Mutex::new(false),
            vault_error: Mutex::new(None),
            services_started: Mutex::new(false),
            geocoding_flight: crate::state::SingleFlight::default(),
            vault_flight: crate::state::SingleFlight::default(),
        });
        state
            .db
            .lock()
            .unwrap()
            .execute(
                "INSERT INTO activity (id, start_time, sport_type)
                 VALUES ('a1', '2026-09-03T07:35:00+03:00', 'ride')",
                [],
            )
            .unwrap();
        let mut ctx = PluginCtx {
            db: state.db.clone(),
            plugin_id: "com.test".to_string(),
            permissions: vec![Permission::ImportFiles],
            vault: Some(state.clone()),
            imported: 0,
            imported_activities: 0,
            monitoring: MonitoringBatch::default(),
            net: NetState::default(),
        };
        let midnight = 1_788_555_600; // 2026-09-05 00:00 +03:00
        let day = 86_400;
        // Three files over two days in three calls, as the host function
        // would leave them: two of the first day, one of the next.
        for (i, (serial, night)) in [(424242u32, midnight), (424243, midnight), (424244, midnight + day)].iter().enumerate() {
            let r = host::import_file(&mut ctx, &format!("M950000{i}.FIT"), &monitoring_fixture(*serial, *night)).unwrap();
            ctx.imported += r.imported + r.monitoring_files;
            ctx.imported_activities += r.imported;
        }
        assert_eq!(ctx.monitoring.days.len(), 2);
        let computed = |state: &AppState| -> i64 {
            state
                .db
                .lock()
                .unwrap()
                .query_row("SELECT COUNT(*) FROM monitoring_day WHERE computed_at IS NOT NULL", [], |r| r.get(0))
                .unwrap()
        };
        assert_eq!(computed(&state), 0, "nothing recomputed per call");

        let ud = host::user_data(ctx);
        let done = super::finish_invocation(&ud);
        assert_eq!(done, super::Finished { imported: 3, activities: 0, monitoring_days: 2 });
        assert_eq!(computed(&state), 2);
        // Finishing again has nothing left to do.
        assert_eq!(super::finish_invocation(&ud).monitoring_days, 0);
        let _ = std::fs::remove_dir_all(&vault);
    }

    /// A recompute that fails is logged, not lost with the counters — and
    /// a context poisoned by a panicking host call still yields both.
    #[test]
    fn finish_invocation_survives_a_failed_recompute_and_a_poisoned_context() {
        use crate::plugins::host::{self, PluginCtx};
        use std::sync::{Arc, Mutex};

        // A schemaless DB: recompute_days fails at prepare.
        let mut ctx = PluginCtx::new(
            Arc::new(Mutex::new(rusqlite::Connection::open_in_memory().unwrap())),
            "com.test",
            vec![],
        );
        ctx.monitoring.days.insert(20_700);
        ctx.imported = 2;
        let ud = host::user_data(ctx);
        assert_eq!(super::finish_invocation(&ud), super::Finished { imported: 2, activities: 0, monitoring_days: 0 });

        // Poison the context's lock from another thread.
        let mut ctx = PluginCtx::new(Arc::new(Mutex::new(crate::db::test_db())), "com.test", vec![]);
        ctx.imported = 3;
        ctx.imported_activities = 1;
        let ud = host::user_data(ctx);
        let poisoner = ud.clone();
        let _ = std::thread::spawn(move || {
            let arc = poisoner.get().unwrap();
            let _guard = arc.lock().unwrap();
            panic!("host call panicked mid-way");
        })
        .join();
        assert_eq!(super::finish_invocation(&ud), super::Finished { imported: 3, activities: 1, monitoring_days: 0 });
    }

    /// The whole runtime path on a real (windowless) Tauri app: the plugin
    /// record and wasm in the vault, the live `AppState` behind `AppVault`,
    /// the import through `host_import_file`, and the `plugins:imported`
    /// event the frontend refreshes on — emitted even when the call traps.
    #[test]
    fn run_contribution_imports_through_the_live_app_and_emits_the_event() {
        use super::{run_contribution, PLUGINS_IMPORTED_EVENT};
        use crate::models::plugin::Plugin;
        use crate::state::AppState;
        use std::sync::{Arc, Mutex};
        use tauri::{Listener, Manager};

        let plugin_id = "com.syzify.example.paste-import";
        let vault = std::env::temp_dir().join(format!("syz_runtime_app_{}", uuid::Uuid::new_v4()));
        let plugin_dir = vault.join("plugins").join(plugin_id);
        std::fs::create_dir_all(&plugin_dir).unwrap();
        std::fs::copy(paste_import_wasm(), plugin_dir.join("plugin.wasm")).unwrap();

        let state = AppState {
            db: Arc::new(Mutex::new(crate::db::test_db())),
            vault_path: vault.clone(),
            encryption_key: Mutex::new(None),
            watcher_handle: Mutex::new(None),
            db_locked: Mutex::new(false),
            vault_error: Mutex::new(None),
            services_started: Mutex::new(false),
            geocoding_flight: crate::state::SingleFlight::default(),
            vault_flight: crate::state::SingleFlight::default(),
        };
        {
            let conn = state.db.lock().unwrap();
            crate::db::plugins::upsert_plugin(
                &conn,
                &Plugin {
                    id: plugin_id.to_string(),
                    name: "Paste Import".to_string(),
                    version: "0.1.0".to_string(),
                    author: None,
                    description: None,
                    enabled: true,
                    signed: false,
                    manifest: format!(
                        r#"{{"id":"{plugin_id}","name":"Paste Import","version":"0.1.0","entry":"plugin.wasm","contributes":["dashboard.widget"],"permissions":["import:files"]}}"#
                    ),
                    source: format!("plugins/{plugin_id}"),
                    installed_at: String::new(),
                    updated_at: String::new(),
                },
            )
            .unwrap();
            crate::db::plugins::set_enabled(&conn, plugin_id, true).unwrap();
        }

        let app = tauri::test::mock_builder()
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("mock app");
        app.manage(state);
        let handle = app.handle().clone();
        let events: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = events.clone();
        handle.listen(PLUGINS_IMPORTED_EVENT, move |e| sink.lock().unwrap().push(e.payload().to_string()));

        let gpx = r#"<?xml version="1.0"?><gpx version="1.1" creator="t" xmlns="http://www.topografix.com/GPX/1/1"><trk><trkseg><trkpt lat="55.75" lon="37.62"><time>2025-06-01T08:00:00Z</time></trkpt><trkpt lat="55.7501" lon="37.6201"><time>2025-06-01T08:00:10Z</time></trkpt></trkseg></trk></gpx>"#;
        let input = |name: &str| {
            serde_json::json!({ "action": "import", "values": { "name": name, "content": gpx } }).to_string()
        };

        // The form: no import, no event.
        let out = run_contribution(&handle, plugin_id, "dashboard_widget", "{}").unwrap();
        assert!(out.contains("Paste import"));
        assert!(events.lock().unwrap().is_empty());

        // An import: the activity lands, the event says one file.
        let out = run_contribution(&handle, plugin_id, "dashboard_widget", &input("run.gpx")).unwrap();
        assert!(out.contains(r#""label":"Imported","value":"1""#), "{out}");
        assert_eq!(events.lock().unwrap().as_slice(), [r#"{"imported":1,"monitoring_days":0}"#]);

        // A trap after nothing imported: an error, no event. The message
        // is the host function's own (user-facing text): the vault's path
        // is not in it.
        let err = run_contribution(&handle, plugin_id, "dashboard_widget", &input("../run.gpx")).unwrap_err();
        assert!(err.contains("invalid import file name"), "{err}");
        assert!(!err.contains(vault.to_str().unwrap()), "{err}");
        assert_eq!(events.lock().unwrap().len(), 1);

        // Disabled plugins do not run at all.
        {
            let state = handle.state::<AppState>();
            let conn = state.db.lock().unwrap();
            crate::db::plugins::set_enabled(&conn, plugin_id, false).unwrap();
        }
        let err = run_contribution(&handle, plugin_id, "dashboard_widget", "{}").unwrap_err();
        assert!(err.contains("disabled"), "{err}");

        let state = handle.state::<AppState>();
        let conn = state.db.lock().unwrap();
        let activities: i64 = conn.query_row("SELECT COUNT(*) FROM activity", [], |r| r.get(0)).unwrap();
        assert_eq!(activities, 1);
        drop(conn);
        let _ = std::fs::remove_dir_all(&vault);
    }
}
