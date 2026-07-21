//! Capability-gated host functions exposed to plugin WASM modules.
//!
//! Every function checks the calling plugin's granted permissions before
//! touching data, and reads/writes go through the `db/` layer. Plugins get no
//! ambient authority: no DOM, no IPC, no filesystem, and no network (Extism
//! `allowed_hosts` is left empty — network brokering is a later phase).

use extism::{host_fn, UserData};
use serde::Deserialize;

use crate::db;
use crate::models::activity::ActivityFilters;
use crate::models::plugin::Permission;
use crate::state::Db;

/// Per-invocation context handed to host functions via Extism `UserData`.
/// Holds the DB handle directly (not the Tauri `AppHandle`), so the host layer
/// is decoupled from Tauri and constructible in tests.
#[derive(Clone)]
pub struct PluginCtx {
    pub db: Db,
    pub plugin_id: String,
    pub permissions: Vec<Permission>,
}

impl PluginCtx {
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
    fn rows_as_json_array_builds_and_rejects_bad_json() {
        let ok = rows_as_json_array(&[r#"{"a":1}"#.to_string(), "2".to_string()]).unwrap();
        assert_eq!(ok, r#"[{"a":1},2]"#);
        // A corrupt stored row must error, not panic or emit broken JSON.
        assert!(rows_as_json_array(&["not json".to_string()]).is_err());
    }
}
