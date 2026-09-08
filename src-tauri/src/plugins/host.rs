//! Capability-gated host functions exposed to plugin WASM modules.
//!
//! Every function checks the calling plugin's granted permissions before
//! touching data, and reads/writes go through the `db/` layer (file imports
//! through the import pipeline). Plugins get no ambient authority: no DOM,
//! no IPC, no filesystem, and no network beyond `host_http` to the hosts
//! the manifest's `net:host=` permissions declare (`plugins/net.rs`).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use extism::{host_fn, UserData};
use serde::Deserialize;

use crate::db;
use crate::import::pipeline::{self, ImportResult, MonitoringBatch};
use crate::models::activity::ActivityFilters;
use crate::models::plugin::Permission;
use crate::plugins::net::{self, NetState};
use crate::plugins::secrets;
use crate::state::{AppState, Db, SingleFlightGuard};

/// Cap on one file a plugin hands to `host_import_file` — what the pipeline
/// will parse. A 3 h ride with Cycling Dynamics is ~2 MB, Garmin's all-day
/// Monitor files a few hundred KB; the drop import's 512 MiB is for files
/// a user picked by hand. Not a memory bound: the ABI copies the whole
/// argument out of the sandbox before this check runs (the plugin's 64 MiB
/// memory cap bounds that copy), and a `.gz` may still expand to the
/// pipeline's 100 MiB decompression cap.
pub const PLUGIN_IMPORT_MAX_BYTES: usize = 32 * 1024 * 1024;

/// Prefix of the `host_import_file` error while a backup, restore, relocation
/// or encryption toggle holds the vault — transient, retry later. Part of the
/// Host SDK contract (examples/plugins/README.md).
pub const VAULT_BUSY: &str = "vault busy";
/// Prefix of the `host_import_file` error while the vault is locked — transient.
pub const VAULT_LOCKED: &str = "vault locked";

/// What an import needs from the app beyond the DB handle. A trait so the
/// host layer stays Tauri-free: the runtime hands in the live `AppState`
/// behind an `AppHandle`, tests hand in a plain `AppState`.
pub trait VaultAccess: Send + Sync {
    fn vault_path(&self) -> PathBuf;
    /// Refuse while the vault is LOCKED (lock on disk, key not in memory)
    /// — the same gate as the drop import's, for the same reason: the raw
    /// file would land in plaintext inside an encrypted vault.
    fn ensure_unlocked(&self) -> Result<(), String>;
    /// The activities-scope key, read fresh on every call: a plugin loops
    /// through many imports per invocation, and an encryption toggle in
    /// between must be honoured, not a key snapshotted at the start.
    fn activities_key(&self) -> Result<Option<[u8; 32]>, String>;
    /// The vault-mutation slot, held for the length of one import: a
    /// restore or relocation must not move `raw/` out from under the write,
    /// and the encryption toggle must not re-key files while one lands.
    fn claim_vault(&self) -> Option<SingleFlightGuard<'_>>;
    /// The key plugin secrets are sealed under: the vault key whenever ANY
    /// scope is encrypted (a token must not sit in the clear because only
    /// `activities` was chosen), `None` for a plaintext vault. Read fresh
    /// per call and only under the vault slot (`with_secrets`), like the
    /// activities key under an import: a key read outside the slot could
    /// be one Disable is discarding.
    fn secrets_key(&self) -> Result<Option<[u8; 32]>, String>;
}

impl VaultAccess for AppState {
    fn vault_path(&self) -> PathBuf {
        self.vault_path.clone()
    }
    fn ensure_unlocked(&self) -> Result<(), String> {
        crate::commands::import::ensure_vault_unlocked(self)
    }
    fn activities_key(&self) -> Result<Option<[u8; 32]>, String> {
        self.encryption_key_for(|s| s.activities)
    }
    fn claim_vault(&self) -> Option<SingleFlightGuard<'_>> {
        self.vault_flight.try_begin()
    }
    fn secrets_key(&self) -> Result<Option<[u8; 32]>, String> {
        self.encryption_key_for(|s| s.any())
    }
}

/// Per-invocation context handed to host functions via Extism `UserData`.
/// Holds the DB handle directly (not the Tauri `AppHandle`), so the host layer
/// is decoupled from Tauri and constructible in tests.
pub struct PluginCtx {
    pub db: Db,
    pub plugin_id: String,
    pub permissions: Vec<Permission>,
    /// The vault `host_import_file` writes into. `None` where nothing can be
    /// imported (a context built without one refuses the call).
    pub vault: Option<Arc<dyn VaultAccess>>,
    /// Files this invocation landed in the vault (activities + Monitor
    /// files). The host cannot emit events, so the runtime reads it back
    /// after the call and the frontend refreshes on it.
    pub imported: usize,
    /// Of those, activities — the ones a geocoding pass has work for.
    pub imported_activities: usize,
    /// The Monitor days this invocation's imports touched, recomputed once
    /// by the runtime when the invocation ends — a wellness sync lands one
    /// day per call and the watch writes several files per day, so a
    /// per-call recompute would run many times over under the DB lock.
    pub monitoring: MonitoringBatch,
    /// The invocation's network side (`host_http`): its cookie jar, the
    /// last response's meta, the budget's deadline.
    pub net: NetState,
}

#[cfg(test)]
impl PluginCtx {
    /// A context that can query and store but not import.
    pub fn new(db: Db, plugin_id: impl Into<String>, permissions: Vec<Permission>) -> Self {
        PluginCtx {
            db,
            plugin_id: plugin_id.into(),
            permissions,
            vault: None,
            imported: 0,
            imported_activities: 0,
            monitoring: MonitoringBatch::default(),
            net: NetState::default(),
        }
    }
}

impl PluginCtx {
    /// The hosts the plugin declared via `net:host=` — `host_http`'s
    /// allow-list, for the first request and every redirect hop.
    fn network_hosts(&self) -> Vec<String> {
        self.permissions
            .iter()
            .filter_map(|p| match p {
                Permission::Net { host } => Some(host.clone()),
                _ => None,
            })
            .collect()
    }

    fn require(&self, needed: &Permission) -> Result<(), extism::Error> {
        require_permission(&self.permissions, needed).map_err(|_| {
            extism::Error::msg(format!(
                "plugin {} lacks permission {:?}",
                self.plugin_id, needed
            ))
        })
    }
}

/// Build a JSON array string from stored row payloads, through serde so the
/// result is always well-formed. Returns an error (never panics/corrupts) if a
/// stored row isn't valid JSON.
fn rows_as_json_array(rows: &[String]) -> Result<String, serde_json::Error> {
    let values = rows
        .iter()
        .map(|s| serde_json::from_str::<serde_json::Value>(s))
        .collect::<Result<Vec<_>, _>>()?;
    serde_json::to_string(&values)
}

/// Pure capability check: the granted set must contain exactly the needed
/// permission (e.g. `read:activities` does NOT satisfy `read:dashboard`).
fn require_permission(granted: &[Permission], needed: &Permission) -> Result<(), ()> {
    if granted.contains(needed) {
        Ok(())
    } else {
        Err(())
    }
}

#[derive(Deserialize)]
struct QueryRequest {
    kind: String,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    period: Option<String>,
    #[serde(default)]
    sport_type: Option<String>,
    #[serde(default)]
    limit: Option<u32>,
}

#[derive(Deserialize)]
struct DataSetRequest {
    kind: String,
    #[serde(default)]
    activity_id: Option<String>,
    #[serde(default)]
    key: Option<String>,
    json: String,
}

#[derive(Deserialize)]
struct DataGetRequest {
    kind: String,
    #[serde(default)]
    activity_id: Option<String>,
}

#[derive(Deserialize)]
struct KvSetRequest {
    key: String,
    value: String,
}

/// Strict: an empty value deletes, so a misspelt `value` must not pass
/// as "delete the token".
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SecretSetRequest {
    key: String,
    /// Empty deletes the secret.
    #[serde(default)]
    value: String,
}

// Read-only data access. `{"kind":"activities"|"dashboard", ...}` -> JSON.
host_fn!(pub host_query(user_data: PluginCtx; req: String) -> String {
    let ud = user_data.get()?;
    let ctx = ud.lock().map_err(|e| extism::Error::msg(format!("plugin state lock poisoned: {e}")))?;
    let request: QueryRequest = serde_json::from_str(&req)?;

    match request.kind.as_str() {
        "activities" => {
            ctx.require(&Permission::ReadActivities)?;
            let filters = ActivityFilters { limit: request.limit, ..Default::default() };
            let conn = ctx.db.lock().map_err(|e| extism::Error::msg(e.to_string()))?;
            let rows = db::activities::get_activities(&conn, &filters)?;
            Ok(serde_json::to_string(&rows)?)
        }
        "activity" => {
            ctx.require(&Permission::ReadActivities)?;
            let id = request.id.ok_or_else(|| extism::Error::msg("activity query needs an id"))?;
            let conn = ctx.db.lock().map_err(|e| extism::Error::msg(e.to_string()))?;
            let activity = db::activities::get_activity_by_id(&conn, &id)?;
            Ok(serde_json::to_string(&activity)?)
        }
        "dashboard" => {
            ctx.require(&Permission::ReadDashboard)?;
            let period = request.period.as_deref().unwrap_or("all");
            let conn = ctx.db.lock().map_err(|e| extism::Error::msg(e.to_string()))?;
            let data = db::dashboard::get_dashboard_data(&conn, period, request.sport_type.as_deref())?;
            Ok(serde_json::to_string(&data)?)
        }
        other => Err(extism::Error::msg(format!("unknown query kind: {other}"))),
    }
});

// Append a record to the plugin's private structured store. Returns the row id.
host_fn!(pub host_data_set(user_data: PluginCtx; req: String) -> String {
    let ud = user_data.get()?;
    let ctx = ud.lock().map_err(|e| extism::Error::msg(format!("plugin state lock poisoned: {e}")))?;
    ctx.require(&Permission::DataOwn)?;
    let r: DataSetRequest = serde_json::from_str(&req)?;
    // The store holds JSON — reject a non-JSON payload so reads stay well-formed.
    serde_json::from_str::<serde_json::Value>(&r.json)
        .map_err(|e| extism::Error::msg(format!("data payload is not valid JSON: {e}")))?;
    let conn = ctx.db.lock().map_err(|e| extism::Error::msg(e.to_string()))?;
    let id = db::plugins::insert_data(
        &conn, &ctx.plugin_id, &r.kind, r.activity_id.as_deref(), r.key.as_deref(), &r.json,
    )?;
    Ok(id.to_string())
});

// Read records of a kind from the plugin's private store. Returns a JSON array.
host_fn!(pub host_data_get(user_data: PluginCtx; req: String) -> String {
    let ud = user_data.get()?;
    let ctx = ud.lock().map_err(|e| extism::Error::msg(format!("plugin state lock poisoned: {e}")))?;
    ctx.require(&Permission::DataOwn)?;
    let r: DataGetRequest = serde_json::from_str(&req)?;
    let conn = ctx.db.lock().map_err(|e| extism::Error::msg(e.to_string()))?;
    let rows = db::plugins::get_data(&conn, &ctx.plugin_id, &r.kind, r.activity_id.as_deref())?;
    Ok(rows_as_json_array(&rows)?)
});

host_fn!(pub host_kv_set(user_data: PluginCtx; req: String) -> String {
    let ud = user_data.get()?;
    let ctx = ud.lock().map_err(|e| extism::Error::msg(format!("plugin state lock poisoned: {e}")))?;
    ctx.require(&Permission::DataOwn)?;
    let r: KvSetRequest = serde_json::from_str(&req)?;
    let conn = ctx.db.lock().map_err(|e| extism::Error::msg(e.to_string()))?;
    db::plugins::kv_set(&conn, &ctx.plugin_id, &r.key, &r.value)?;
    Ok(String::new())
});

host_fn!(pub host_kv_get(user_data: PluginCtx; key: String) -> String {
    let ud = user_data.get()?;
    let ctx = ud.lock().map_err(|e| extism::Error::msg(format!("plugin state lock poisoned: {e}")))?;
    ctx.require(&Permission::DataOwn)?;
    let conn = ctx.db.lock().map_err(|e| extism::Error::msg(e.to_string()))?;
    Ok(db::plugins::kv_get(&conn, &ctx.plugin_id, &key)?.unwrap_or_default())
});

// Store one secret (a token) of the plugin's: `{"key", "value"}`, an empty
// value deletes it. Sealed under the vault key when any scope is
// encrypted, plain otherwise (disclosed on the Plugins screen). A vault
// operation in flight or a locked vault traps with the [`VAULT_BUSY`] /
// [`VAULT_LOCKED`] prefix — transient, like an import.
host_fn!(pub host_secret_set(user_data: PluginCtx; req: String) -> String {
    let ud = user_data.get()?;
    let ctx = ud.lock().map_err(|e| extism::Error::msg(format!("plugin state lock poisoned: {e}")))?;
    ctx.require(&Permission::DataSecret)?;
    let r: SecretSetRequest = serde_json::from_str(&req)?;
    with_secrets(&ctx, |conn, key| secrets::set(conn, key, &ctx.plugin_id, &r.key, &r.value))
        .map_err(extism::Error::msg)?;
    Ok(String::new())
});

// The secret, or an empty string when there is none (the kv contract).
host_fn!(pub host_secret_get(user_data: PluginCtx; key: String) -> String {
    let ud = user_data.get()?;
    let ctx = ud.lock().map_err(|e| extism::Error::msg(format!("plugin state lock poisoned: {e}")))?;
    ctx.require(&Permission::DataSecret)?;
    let value = with_secrets(&ctx, |conn, vault_key| secrets::get(conn, vault_key, &ctx.plugin_id, &key))
        .map_err(extism::Error::msg)?;
    Ok(value.unwrap_or_default())
});

/// The gate both secret calls share, in the import's order: the vault
/// slot first, then the lock gate and the key under it, then the row.
/// Nothing may re-key the vault between reading the key and writing the
/// row — a `set` that sealed under a key Disable was discarding would
/// leave a row no key ever opens again, and a `get` during an enable
/// could read `None` for a row just sealed.
fn with_secrets<T>(
    ctx: &PluginCtx,
    f: impl FnOnce(&rusqlite::Connection, Option<&[u8; 32]>) -> Result<T, String>,
) -> Result<T, String> {
    let vault = ctx
        .vault
        .clone()
        .ok_or_else(|| "secret storage is not available in this context".to_string())?;
    let _flight = vault
        .claim_vault()
        .ok_or_else(|| format!("{VAULT_BUSY}: another vault operation is in progress"))?;
    vault.ensure_unlocked().map_err(|e| format!("{VAULT_LOCKED}: {e}"))?;
    let key = vault.secrets_key()?;
    let conn = ctx.db.lock().map_err(|e| e.to_string())?;
    f(&conn, key.as_ref())
}

// Import one file the plugin holds in memory — the drop import's pipeline
// (format by extension, hash dedup, parse, store, monitoring recompute) on
// the plugin's bytes. `name` is a bare file name; it decides the format and
// becomes the raw file's original_path. A `.zip` (Garmin's downloads) is
// expanded here, entry by entry, into the same result. Returns the ImportResult JSON the
// drop import returns to the frontend: a file the pipeline refuses is a
// `failed` entry there, not a trap — a sync loop goes on with the next one.
// A Monitor file counts in `monitoring_files` only; its day is recomputed
// when the invocation ends, so `monitoring_days` / `monitoring_range` /
// `monitoring_night` stay 0 / null / false per call.
// Traps (errors) are for the plugin's own mistakes and an unavailable vault:
// missing permission, a bad name, an oversized file, a locked vault, a
// vault operation in flight. The last two are transient and their messages
// start with [`VAULT_BUSY`] / [`VAULT_LOCKED`], so a sync loop can tell
// "back off and retry later" from "you handed over garbage".
host_fn!(pub host_import_file(user_data: PluginCtx; name: String, bytes: Vec<u8>) -> String {
    let ud = user_data.get()?;
    let mut ctx = ud.lock().map_err(|e| extism::Error::msg(format!("plugin state lock poisoned: {e}")))?;
    ctx.require(&Permission::ImportFiles)?;
    let result = import_file(&mut ctx, &name, &bytes).map_err(extism::Error::msg)?;
    ctx.imported += result.imported + result.monitoring_files;
    ctx.imported_activities += result.imported;
    Ok(serde_json::to_string(&result)?)
});

// One brokered HTTP request. The host follows redirects itself (as many
// as the plugin's `max_redirects` allows, 10 at most) — every hop checked
// against the plugin's `net:host=` list, the invocation's cookie jar
// reading every hop's Set-Cookie and lending its cookies to the next —
// and returns the final response's body. Its status, URL, headers and
// the hops taken are read with `host_http_meta` right after: the
// two-call shape of Extism's own HTTP, so a plugin ports with a local
// change. A refused hop, a bad request, a spent budget and a transport
// failure all trap.
host_fn!(pub host_http(user_data: PluginCtx; req: String, body: Vec<u8>) -> Vec<u8> {
    let ud = user_data.get()?;
    let mut ctx = ud.lock().map_err(|e| extism::Error::msg(format!("plugin state lock poisoned: {e}")))?;
    let allowed = ctx.network_hosts();
    if allowed.is_empty() {
        return Err(extism::Error::msg(format!("plugin {} lacks permission net:host=<host>", ctx.plugin_id)));
    }
    net::fetch(&mut ctx.net, &allowed, &req, &body).map_err(|e| extism::Error::msg(format!("host_http: {e}")))
});

// `{status, url, headers: [[name, value], …], hops: [{status, url,
// location}, …]}` of the last `host_http` response of this invocation;
// headers as pairs, so a repeated one (five Set-Cookie of a login)
// survives. An error before any response.
host_fn!(pub host_http_meta(user_data: PluginCtx;) -> String {
    let ud = user_data.get()?;
    let ctx = ud.lock().map_err(|e| extism::Error::msg(format!("plugin state lock poisoned: {e}")))?;
    let meta = ctx
        .net
        .last
        .as_ref()
        .ok_or_else(|| extism::Error::msg("host_http_meta: no response yet — call host_http first"))?;
    Ok(serde_json::to_string(meta)?)
});

/// The import behind `host_import_file`, permission already checked. The
/// touched Monitor days land in `ctx.monitoring` for the runtime to finish.
pub(crate) fn import_file(ctx: &mut PluginCtx, name: &str, bytes: &[u8]) -> Result<ImportResult, String> {
    validate_import_name(name)?;
    if bytes.len() > PLUGIN_IMPORT_MAX_BYTES {
        return Err(format!(
            "{name} is {} MB — larger than the {} MB plugin import limit",
            bytes.len() / (1024 * 1024),
            PLUGIN_IMPORT_MAX_BYTES / (1024 * 1024)
        ));
    }
    let vault = ctx
        .vault
        .clone()
        .ok_or_else(|| "file import is not available in this context".to_string())?;
    // Slot first, then the gates and the key under it: nothing may re-key
    // or move the vault between reading the key and writing the file.
    let _flight = vault
        .claim_vault()
        .ok_or_else(|| format!("{VAULT_BUSY}: another vault operation is in progress"))?;
    vault.ensure_unlocked().map_err(|e| format!("{VAULT_LOCKED}: {e}"))?;
    let key = vault.activities_key()?;
    let conn = ctx.db.lock().map_err(|e| e.to_string())?;

    let mut result = ImportResult::default();
    if pipeline::is_zip(Path::new(name)) {
        // Garmin's downloads: every entry its own import, one result. The
        // archive's clock is what is left of the invocation's budget, at
        // most its own — the epoch deadline does not cut a host call.
        let left = ctx.net.deadline.map(|d| d.saturating_duration_since(std::time::Instant::now()));
        let limits = pipeline::ArchiveLimits {
            time: left.map_or(pipeline::ZIP_TIME_BUDGET, |l| l.min(pipeline::ZIP_TIME_BUDGET)),
            ..pipeline::ArchiveLimits::default()
        };
        pipeline::import_archive(&conn, &vault.vault_path(), name, bytes, key.as_ref(), &mut result, &mut ctx.monitoring, limits);
    } else {
        let outcome = pipeline::import_bytes(&conn, &vault.vault_path(), name, bytes, key.as_ref());
        result.record(name, outcome, &mut ctx.monitoring);
    }
    Ok(result)
}

/// A file name a plugin may import under: one plain path component of ASCII
/// letters, digits, '.', '-' and '_' — Garmin's "2026-09-06-18-05-31.fit",
/// "M9500000.FIT", "12345_ACTIVITY.fit.gz", "12345.zip" — starting with a
/// letter or digit, at most [`pipeline::MAX_IMPORT_NAME`] bytes, ending in an
/// extension the pipeline imports (`.fit`/`.gpx`/`.tcx`, optionally `.gz`)
/// or `.zip` (expanded by the pipeline, entry by entry). Nothing else: the
/// name becomes the raw file's `original_path` and decides its format.
pub fn validate_import_name(name: &str) -> Result<(), String> {
    pipeline::validate_bare_name(name).map_err(|e| format!("invalid import file name: {e}"))?;
    if pipeline::is_zip(Path::new(name)) {
        return Ok(());
    }
    pipeline::import_format(Path::new(name))
        .map(|_| ())
        .map_err(|e| format!("invalid import file name {name:?}: {e}"))
}

/// Marker so `UserData` is constructed consistently at call sites.
pub fn user_data(ctx: PluginCtx) -> UserData<PluginCtx> {
    UserData::new(ctx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn require_permission_gates_exactly() {
        let granted = vec![Permission::ReadActivities, Permission::DataOwn];
        assert!(require_permission(&granted, &Permission::ReadActivities).is_ok());
        assert!(require_permission(&granted, &Permission::DataOwn).is_ok());
        // A held permission must not satisfy a different one.
        assert!(require_permission(&granted, &Permission::ReadDashboard).is_err());
        assert!(require_permission(&granted, &Permission::ReadHrv).is_err());
        // Empty grant denies everything.
        assert!(require_permission(&[], &Permission::ReadActivities).is_err());
    }

    #[test]
    fn validate_import_name_accepts_bare_importable_names_only() {
        for good in [
            "run.gpx",
            "M9500000.FIT",
            "2026-09-06-18-05-31.fit",
            "12345_ACTIVITY.fit.gz",
            "swim.TCX.GZ",
            "12345.zip",
            "wellness_2026-09-08.ZIP",
        ] {
            assert!(validate_import_name(good).is_ok(), "{good:?} should be accepted");
        }
        let long = format!("{}.fit", "a".repeat(pipeline::MAX_IMPORT_NAME));
        for bad in [
            "",
            "../run.gpx",
            "a/b.gpx",
            "a\\b.gpx",
            "/run.gpx",
            ".hidden.gpx",
            "a..b.gpx",
            "run",
            "run.txt",
            "run.gz",
            "x.zip.gz",
            "run.gpx\0",
            "run 1.gpx",
            "трек.gpx",
            long.as_str(),
        ] {
            assert!(validate_import_name(bad).is_err(), "{bad:?} should be rejected");
        }
    }

    // --- host_import_file ---------------------------------------------------

    use crate::state::AppState;
    use std::sync::{Arc, Mutex};

    const GPX: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<gpx version="1.1" creator="TestDevice" xmlns="http://www.topografix.com/GPX/1/1">
  <trk><name>Plugin run</name><type>Running</type><trkseg>
    <trkpt lat="55.75" lon="37.62"><ele>150.0</ele><time>2025-06-01T08:00:00Z</time></trkpt>
    <trkpt lat="55.7501" lon="37.6201"><ele>155.0</ele><time>2025-06-01T08:00:10Z</time></trkpt>
  </trkseg></trk>
</gpx>"#;

    fn fresh_vault(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("syz_host_import_{tag}_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn app_state(vault: &Path, key: Option<[u8; 32]>) -> Arc<AppState> {
        Arc::new(AppState {
            db: Arc::new(Mutex::new(crate::db::test_db())),
            vault_path: vault.to_path_buf(),
            encryption_key: Mutex::new(key),
            watcher_handle: Mutex::new(None),
            db_locked: Mutex::new(false),
            vault_error: Mutex::new(None),
            services_started: Mutex::new(false),
            geocoding_flight: crate::state::SingleFlight::default(),
            vault_flight: crate::state::SingleFlight::default(),
        })
    }

    /// The secret gate: plain without a lock, sealed under the key with
    /// one (any scope), refused while locked, refused without a vault.
    #[test]
    fn secrets_follow_the_vault_key_and_the_lock() {
        let vault = std::env::temp_dir().join(format!("syz_secret_gate_{}", uuid::Uuid::new_v4()));
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
        {
            let conn = state.db.lock().unwrap();
            crate::db::plugins::upsert_plugin(
                &conn,
                &crate::models::plugin::Plugin {
                    id: "com.test".to_string(),
                    name: "T".to_string(),
                    version: "0.1.0".to_string(),
                    author: None,
                    description: None,
                    enabled: true,
                    signed: false,
                    manifest: "{}".to_string(),
                    source: "plugins/com.test".to_string(),
                    installed_at: String::new(),
                    updated_at: String::new(),
                },
            )
            .unwrap();
        }
        let ctx = PluginCtx {
            db: state.db.clone(),
            plugin_id: "com.test".to_string(),
            permissions: vec![Permission::DataSecret],
            vault: Some(state.clone()),
            imported: 0,
            imported_activities: 0,
            monitoring: MonitoringBatch::default(),
            net: NetState::default(),
        };
        let row = |name: &str| {
            let conn = state.db.lock().unwrap();
            crate::db::plugins::secret_get(&conn, "com.test", name).unwrap()
        };

        let key_seen = |ctx: &PluginCtx| with_secrets(ctx, |_, key| Ok(key.copied()));

        // Plaintext vault: no key, stored in the clear.
        assert_eq!(key_seen(&ctx).unwrap(), None);
        with_secrets(&ctx, |conn, key| secrets::set(conn, key, "com.test", "a", "plain")).unwrap();
        assert_eq!(row("a"), Some((b"plain".to_vec(), false)));

        // A vault operation in flight (a backup, an encryption toggle):
        // refused with the prefix before the key is read, nothing written.
        {
            let _busy = state.vault_flight.try_begin().unwrap();
            let err = with_secrets(&ctx, |conn, key| secrets::set(conn, key, "com.test", "x", "lost")).unwrap_err();
            assert!(err.starts_with(VAULT_BUSY), "{err}");
        }
        assert_eq!(row("x"), None);

        // Encryption on (photos only) and the key held: sealed.
        let key = [5u8; 32];
        crate::crypto::write_vault_lock(
            &vault,
            &crate::crypto::VaultLock {
                salt: "00".repeat(32),
                verifier: String::new(),
                nonce: String::new(),
                created_at: String::new(),
                scopes: crate::crypto::EncryptionScopes { activities: false, database: false, photos: true },
            },
        )
        .unwrap();
        *state.encryption_key.lock().unwrap() = Some(key);
        assert_eq!(key_seen(&ctx).unwrap(), Some(key));
        with_secrets(&ctx, |conn, key| secrets::set(conn, key, "com.test", "b", "sealed")).unwrap();
        assert!(row("b").unwrap().1);

        // Locked (lock on disk, key gone): the gate refuses with the prefix.
        *state.encryption_key.lock().unwrap() = None;
        let err = key_seen(&ctx).unwrap_err();
        assert!(err.starts_with(VAULT_LOCKED), "{err}");

        // No vault at all: not available.
        let bare = PluginCtx::new(state.db.clone(), "com.test", vec![Permission::DataSecret]);
        assert!(key_seen(&bare).unwrap_err().contains("not available"));
        let _ = std::fs::remove_dir_all(&vault);
    }

    /// A context whose db IS the state's db, as the runtime builds it.
    fn import_ctx(state: &Arc<AppState>) -> PluginCtx {
        PluginCtx {
            db: state.db.clone(),
            plugin_id: "com.test".to_string(),
            permissions: vec![Permission::ImportFiles],
            vault: Some(state.clone()),
            imported: 0,
            imported_activities: 0,
            monitoring: MonitoringBatch::default(),
            net: NetState::default(),
        }
    }

    fn count(state: &AppState, sql: &str) -> i64 {
        state.db.lock().unwrap().query_row(sql, [], |r| r.get(0)).unwrap()
    }

    fn raw_files(vault: &Path) -> Vec<String> {
        match std::fs::read_dir(vault.join("raw")) {
            Ok(rd) => rd.map(|e| e.unwrap().file_name().to_string_lossy().to_string()).collect(),
            Err(_) => Vec::new(),
        }
    }

    #[test]
    fn import_file_runs_the_drop_pipeline_on_plugin_bytes() {
        let vault = fresh_vault("gpx");
        let state = app_state(&vault, None);
        let mut ctx = import_ctx(&state);

        let r = import_file(&mut ctx, "run.gpx", GPX.as_bytes()).unwrap();
        assert_eq!((r.imported, r.skipped, r.failed.len()), (1, 0, 0));
        assert_eq!(count(&state, "SELECT COUNT(*) FROM activity"), 1);
        let original: String = state
            .db
            .lock()
            .unwrap()
            .query_row("SELECT original_path FROM raw_file", [], |r| r.get(0))
            .unwrap();
        assert_eq!(original, "run.gpx", "the bare name is the original_path, not a temp path");
        assert_eq!(raw_files(&vault).len(), 1);

        // The same bytes again: a duplicate by hash, nothing new on disk.
        let again = import_file(&mut ctx, "run-copy.gpx", GPX.as_bytes()).unwrap();
        assert_eq!((again.imported, again.skipped), (0, 1));
        assert_eq!(raw_files(&vault).len(), 1);

        // A file the pipeline refuses is a failed entry, not an error — the
        // plugin's sync loop must be able to go on with the next file.
        // A zip: every entry its own import, one result, entries named
        // under the archive; the dedup sees the entry, not the archive.
        let zip = {
            use std::io::Write;
            let mut cursor = std::io::Cursor::new(Vec::new());
            {
                let mut w = zip::ZipWriter::new(&mut cursor);
                let o: zip::write::FileOptions<'static, ()> = zip::write::FileOptions::default();
                w.start_file("run.gpx", o).unwrap();
                w.write_all(GPX.as_bytes()).unwrap();
                w.start_file("other.gpx", o).unwrap();
                w.write_all(GPX.replace("2025-06-01", "2025-06-02").as_bytes()).unwrap();
                w.start_file("readme.txt", o).unwrap();
                w.write_all(b"x").unwrap();
                w.finish().unwrap();
            }
            cursor.into_inner()
        };
        let z = import_file(&mut ctx, "12345.zip", &zip).unwrap();
        assert_eq!((z.imported, z.skipped, z.failed.len()), (1, 1, 1), "{:?}", z.failed);
        assert_eq!(z.failed[0].path, "12345.zip/readme.txt");
        assert!(import_file(&mut ctx, "junk.zip", b"nope").unwrap().failed[0].reason.starts_with("Not a zip"));

        let bad = import_file(&mut ctx, "broken.gpx", b"<gpx>not really").unwrap();
        assert_eq!(bad.failed.len(), 1);
        assert_eq!(bad.failed[0].path, "broken.gpx");
        assert_eq!(count(&state, "SELECT COUNT(*) FROM activity"), 2, "run.gpx and the zip's other.gpx");

        let _ = std::fs::remove_dir_all(&vault);
    }

    #[test]
    fn import_file_lands_monitor_files_and_recomputes_their_day() {
        use crate::parser::fit_builder::monitoring_fixture;
        let vault = fresh_vault("monitor");
        let state = app_state(&vault, None);
        // Pins the device clock to +03:00 (see the pipeline's monitoring test).
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
        let mut ctx = import_ctx(&state);
        let midnight = 1_788_555_600; // 2026-09-05 00:00 +03:00

        let r = import_file(&mut ctx, "M9500000.FIT", &monitoring_fixture(424242, midnight)).unwrap();
        // Stored and counted, but the day is NOT recomputed per call…
        assert_eq!((r.imported, r.monitoring_files, r.monitoring_days), (0, 1, 0));
        assert_eq!(r.monitoring_range, None);
        assert!(!r.monitoring_night);
        assert_eq!(count(&state, "SELECT COUNT(*) FROM monitoring_sample WHERE kind = 'hr'"), 3);
        assert_eq!(count(&state, "SELECT COUNT(*) FROM monitoring_day WHERE computed_at IS NOT NULL"), 0);
        // …it waits in the context, once per day however many files touch it.
        let r2 = import_file(&mut ctx, "M9500001.FIT", &monitoring_fixture(424243, midnight)).unwrap();
        assert_eq!(r2.monitoring_files, 1);
        assert_eq!(ctx.monitoring.days.len(), 1);

        // The runtime finishes the batch when the invocation ends.
        let mut finished = ImportResult::default();
        let batch = std::mem::take(&mut ctx.monitoring);
        batch.finish(&state.db.lock().unwrap(), &mut finished).unwrap();
        assert_eq!(finished.monitoring_days, 1);
        assert_eq!(finished.monitoring_range, Some(("2026-09-05".to_string(), "2026-09-05".to_string())));
        assert_eq!(count(&state, "SELECT COUNT(*) FROM monitoring_day WHERE computed_at IS NOT NULL"), 1);

        let _ = std::fs::remove_dir_all(&vault);
    }

    #[test]
    fn import_file_refuses_before_touching_the_vault() {
        let vault = fresh_vault("refuse");
        let state = app_state(&vault, None);
        let mut ctx = import_ctx(&state);

        let err = import_file(&mut ctx, "../run.gpx", GPX.as_bytes()).unwrap_err();
        assert!(err.contains("invalid import file name"), "got: {err}");
        let err = import_file(&mut ctx, "run.txt", GPX.as_bytes()).unwrap_err();
        assert!(err.contains("Unsupported format"), "got: {err}");

        let huge = vec![0u8; PLUGIN_IMPORT_MAX_BYTES + 1];
        let err = import_file(&mut ctx, "huge.fit", &huge).unwrap_err();
        assert!(err.contains("plugin import limit"), "got: {err}");

        // No vault in the context (a runtime that cannot import).
        let mut no_vault = PluginCtx::new(state.db.clone(), "com.test", vec![Permission::ImportFiles]);
        let err = import_file(&mut no_vault, "run.gpx", GPX.as_bytes()).unwrap_err();
        assert!(err.contains("not available"), "got: {err}");

        // A backup/restore/relocation in flight.
        {
            let _running = state.vault_flight.try_begin().unwrap();
            let err = import_file(&mut ctx, "run.gpx", GPX.as_bytes()).unwrap_err();
            assert!(err.starts_with(VAULT_BUSY), "got: {err}");
        }

        // LOCKED vault: lock on disk, no key in memory.
        let lock = crate::crypto::VaultLock {
            salt: String::new(),
            verifier: String::new(),
            nonce: String::new(),
            created_at: String::new(),
            scopes: crate::crypto::EncryptionScopes { activities: true, database: false, photos: false },
        };
        crate::crypto::write_vault_lock(&vault, &lock).unwrap();
        let err = import_file(&mut ctx, "run.gpx", GPX.as_bytes()).unwrap_err();
        assert!(err.starts_with(VAULT_LOCKED), "got: {err}");

        assert_eq!(count(&state, "SELECT COUNT(*) FROM activity"), 0);
        assert_eq!(count(&state, "SELECT COUNT(*) FROM raw_file"), 0);
        assert!(raw_files(&vault).is_empty(), "nothing may reach raw/");
        assert!(state.vault_flight.try_begin().is_some(), "the slot is released after a refusal");

        let _ = std::fs::remove_dir_all(&vault);
    }

    #[test]
    fn import_file_encrypts_under_the_activities_scope_only() {
        let key = [9u8; 32];
        let lock_with = |activities: bool| crate::crypto::VaultLock {
            salt: String::new(),
            verifier: String::new(),
            nonce: String::new(),
            created_at: String::new(),
            scopes: crate::crypto::EncryptionScopes { activities, database: false, photos: !activities },
        };

        let vault = fresh_vault("enc");
        crate::crypto::write_vault_lock(&vault, &lock_with(true)).unwrap();
        let state = app_state(&vault, Some(key));
        let r = import_file(&mut import_ctx(&state), "run.gpx", GPX.as_bytes()).unwrap();
        assert_eq!(r.imported, 1);
        let files = raw_files(&vault);
        assert!(files.iter().all(|f| f.ends_with(".enc")), "encrypted: {files:?}");
        let _ = std::fs::remove_dir_all(&vault);

        // Key in memory but the activities scope off: plaintext, or the file
        // would be stranded as ciphertext after the scope is disabled.
        let vault = fresh_vault("scope_off");
        crate::crypto::write_vault_lock(&vault, &lock_with(false)).unwrap();
        let state = app_state(&vault, Some(key));
        let r = import_file(&mut import_ctx(&state), "run.gpx", GPX.as_bytes()).unwrap();
        assert_eq!(r.imported, 1);
        let files = raw_files(&vault);
        assert!(files.iter().all(|f| f.ends_with(".gpx")), "plaintext: {files:?}");
        let _ = std::fs::remove_dir_all(&vault);
    }

    #[test]
    fn rows_as_json_array_builds_and_rejects_bad_json() {
        let ok = rows_as_json_array(&[r#"{"a":1}"#.to_string(), "2".to_string()]).unwrap();
        assert_eq!(ok, r#"[{"a":1},2]"#);
        // A corrupt stored row must error, not panic or emit broken JSON.
        assert!(rows_as_json_array(&["not json".to_string()]).is_err());
    }
}
