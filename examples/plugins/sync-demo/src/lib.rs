// SPDX-License-Identifier: MIT-0
// (Example code — copy freely into your plugin; see examples/plugins/LICENSE.)

//! Reference Syzify plugin: a `sync.source` page — the shape of a sync plugin
//! without a network. It "syncs" three steps, one per invocation, through the
//! **continue loop**: a view that carries `"continue": "sync"` tells the host
//! to show it and call the plugin again with `{action: "sync", values}` right
//! away. The cursor lives in the plugin's own key/value store (`data:own`),
//! so every round is an ordinary invocation inside the sandbox's budget and a
//! stopped loop (the host's Stop button, a closed page) resumes where it was.
//!
//! The initial render is the status only: no work, no request — the host
//! re-renders pages on its own. The work starts behind the **Sync now**
//! button. A real sync plugin does one download + `host_import_file` per
//! round (an activity, a day of wellness) and keeps its cursor the same way.
//!
//! Build: `cargo build --release --target wasm32-unknown-unknown`
//! then copy `target/wasm32-unknown-unknown/release/sync_demo.wasm` to `plugin.wasm`.

use extism_pdk::*;
use serde_json::{json, Value};

#[host_fn]
extern "ExtismHost" {
    fn host_kv_get(key: String) -> String;
    fn host_kv_set(request: String) -> String;
}

const STEPS: u32 = 3;

fn cursor() -> u32 {
    unsafe { host_kv_get("cursor".to_string()) }
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

fn set_cursor(n: u32) -> FnResult<()> {
    unsafe { host_kv_set(json!({ "key": "cursor", "value": n.to_string() }).to_string()) }?;
    Ok(())
}

#[plugin_fn]
pub fn sync_source(input: String) -> FnResult<String> {
    let ctx: Value = serde_json::from_str(&input).unwrap_or_else(|_| json!({}));
    let done = cursor();
    let spec = match ctx["action"].as_str().unwrap_or("") {
        // One step per invocation, then ask for the next round — until the
        // last step, which answers with the plain status (no `continue`).
        "sync" if done < STEPS => {
            let n = done + 1;
            set_cursor(n)?;
            if n < STEPS {
                json!({
                    "title": "Sync demo",
                    "continue": "sync",
                    "elements": [
                        { "type": "text", "text": format!("Syncing step {n} of {STEPS}…") },
                        { "type": "stat_grid", "stats": [{ "label": "Synced", "value": n.to_string() }] }
                    ]
                })
            } else {
                status(n)
            }
        }
        "reset" => {
            set_cursor(0)?;
            status(0)
        }
        _ => status(done),
    };
    Ok(spec.to_string())
}

fn status(done: u32) -> Value {
    let mut elements = vec![json!({
        "type": "text",
        "text": if done >= STEPS { "Everything is synced.".to_string() } else { format!("{done} of {STEPS} steps synced.") }
    })];
    if done < STEPS {
        elements.push(json!({ "type": "button", "label": "Sync now", "action": "sync" }));
    }
    elements.push(json!({ "type": "button", "label": "Reset", "action": "reset" }));
    json!({ "title": "Sync demo", "elements": elements })
}
