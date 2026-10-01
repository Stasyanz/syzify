// SPDX-License-Identifier: MIT-0
// (Example code — copy freely into your plugin; see examples/plugins/LICENSE.)

//! Reference Syzify plugin: a dashboard widget that lists the Garage
//! (`host_query {"kind":"gear"}`, needs `read:activities`) and puts an
//! activity on one of its items (`host_set_activity_gear`, needs
//! `gear:write`). That is what a sync plugin does with the gear a service
//! assigned: look the item up by name, then set it on the activity it just
//! imported. A refusal — a multisport activity, a retired item, an unknown
//! id — fails the call with the same words the user would read in the app.
//!
//! Build: `cargo build --release --target wasm32-unknown-unknown`
//! then copy `target/wasm32-unknown-unknown/release/gear_demo.wasm` to `plugin.wasm`.

use extism_pdk::*;
use serde_json::{json, Value};

#[host_fn]
extern "ExtismHost" {
    fn host_query(request: String) -> String;
    fn host_set_activity_gear(request: String) -> String;
}

#[plugin_fn]
pub fn dashboard_widget(input: String) -> FnResult<String> {
    let ctx: Value = serde_json::from_str(&input).unwrap_or_else(|_| json!({}));
    let action = ctx["action"].as_str().unwrap_or("");

    let registry = unsafe { host_query(json!({ "kind": "gear" }).to_string())? };
    let items: Vec<Value> = serde_json::from_str(&registry)?;

    let mut elements = vec![json!({
        "type": "text",
        "text": if items.is_empty() { "The Garage is empty.".to_string() } else { format!("{} item(s) in the Garage.", items.len()) }
    })];
    for item in &items {
        let km = item["stats"]["distance_m"].as_f64().unwrap_or(0.0) / 1000.0;
        elements.push(json!({
            "type": "text",
            "text": format!("{} ({}) · {:.1} km · {} activities", item["name"].as_str().unwrap_or("?"), item["kind"].as_str().unwrap_or("?"), km, item["stats"]["activities"])
        }));
    }

    if action == "assign" {
        let activity_id = ctx["values"]["activity_id"].as_str().unwrap_or("").to_string();
        let gear_name = ctx["values"]["gear"].as_str().unwrap_or("").trim().to_string();
        // Look the item up by name (names are not unique: the first match
        // wins). Only an EXPLICITLY empty name takes the activity off; a
        // name the Garage does not have writes nothing — a sync plugin
        // must never erase the user's or the rules' pick on a miss.
        let gear_id = if gear_name.is_empty() {
            None
        } else {
            match items
                .iter()
                .find(|i| i["name"].as_str() == Some(gear_name.as_str()))
                .and_then(|i| i["id"].as_str().map(str::to_string))
            {
                Some(id) => Some(id),
                None => {
                    elements.push(json!({ "type": "divider" }));
                    elements.push(json!({ "type": "notice", "text": format!("No gear named \"{gear_name}\" in the Garage — nothing changed."), "level": "warning" }));
                    return finish(ctx, elements);
                }
            }
        };
        let answer = unsafe {
            host_set_activity_gear(json!({ "activity_id": activity_id, "gear_id": gear_id }).to_string())?
        };
        let r: Value = serde_json::from_str(&answer)?;
        elements.push(json!({ "type": "divider" }));
        elements.push(json!({ "type": "stat_grid", "stats": [
            { "label": "Activity", "value": r["activity_id"].as_str().unwrap_or("?") },
            { "label": "Gear", "value": r["gear_id"].as_str().unwrap_or("none") }
        ]}));
    }
    finish(ctx, elements)
}

fn finish(ctx: Value, mut elements: Vec<Value>) -> FnResult<String> {
    elements.push(json!({ "type": "input", "id": "activity_id", "label": "Activity id", "value": ctx["values"]["activity_id"].as_str().unwrap_or("") }));
    elements.push(json!({ "type": "input", "id": "gear", "label": "Gear name (empty = none)", "value": ctx["values"]["gear"].as_str().unwrap_or("") }));
    elements.push(json!({ "type": "button", "label": "Assign", "action": "assign" }));

    Ok(json!({ "title": "Gear demo", "elements": elements }).to_string())
}
