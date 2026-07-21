// SPDX-License-Identifier: MIT-0
// (Example code — copy freely into your plugin; see examples/plugins/LICENSE.)

//! Reference Syzify plugin (phase 2): a dashboard widget showing training
//! consistency over the last 4 weeks. Demonstrates a capability-gated host call
//! (`host_query` with `read:dashboard`) and returning a declarative ViewSpec.
//!
//! Build: `cargo build --release --target wasm32-unknown-unknown`
//! then copy `target/wasm32-unknown-unknown/release/consistency_widget.wasm`
//! to `plugin.wasm` next to `plugin.json`.

use extism_pdk::*;

#[host_fn]
extern "ExtismHost" {
    fn host_query(req: String) -> String;
}

#[plugin_fn]
pub fn dashboard_widget(_input: String) -> FnResult<String> {
    // Ask the host for 4-week dashboard totals (requires read:dashboard).
    let resp = unsafe { host_query(r#"{"kind":"dashboard","period":"4w"}"#.to_string())? };
    let d: serde_json::Value = serde_json::from_str(&resp)?;

    let activities = d["total_activities"].as_i64().unwrap_or(0);
    let distance_km = d["total_distance_m"].as_f64().unwrap_or(0.0) / 1000.0;
    let per_week = activities as f64 / 4.0;

    let spec = serde_json::json!({
        "title": "Consistency · last 4 weeks",
        "elements": [
            { "type": "stat_grid", "stats": [
                { "label": "Activities", "value": activities.to_string() },
                { "label": "Per week", "value": format!("{:.1}", per_week) },
                { "label": "Distance", "value": format!("{:.0} km", distance_km) }
            ]}
        ]
    });
    Ok(serde_json::to_string(&spec)?)
}

#[plugin_fn]
pub fn activity_detail_panel(input: String) -> FnResult<String> {
    // The host passes {"activity_id": "..."} as context.
    let ctx: serde_json::Value = serde_json::from_str(&input)?;
    let id = ctx["activity_id"].as_str().unwrap_or_default();

    // Fetch the activity (requires read:activities).
    let req = serde_json::json!({ "kind": "activity", "id": id }).to_string();
    let resp = unsafe { host_query(req)? };
    let a: serde_json::Value = serde_json::from_str(&resp)?;

    let distance_km = a["distance_m"].as_f64().unwrap_or(0.0) / 1000.0;
    let duration_s = a["duration_s"].as_f64().unwrap_or(0.0);
    let pace = if distance_km > 0.0 { duration_s / 60.0 / distance_km } else { 0.0 };

    let spec = serde_json::json!({
        "title": "Example insight",
        "elements": [
            { "type": "stat", "label": "Avg pace", "value": format!("{:.2} min/km", pace) },
            { "type": "stat", "label": "Distance", "value": format!("{:.1} km", distance_km) }
        ]
    });
    Ok(serde_json::to_string(&spec)?)
}