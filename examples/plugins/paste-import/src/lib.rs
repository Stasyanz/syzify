// SPDX-License-Identifier: MIT-0
// (Example code — copy freely into your plugin; see examples/plugins/LICENSE.)

//! Reference Syzify plugin: a dashboard widget that imports a pasted GPX/TCX
//! file into the vault through `host_import_file` (needs `import:files`).
//! It is the smallest possible sync plugin — get bytes, hand them to the
//! host one file at a time, show the outcome. A real sync plugin fetches
//! the bytes from its declared `net:host=` instead of a text box, and loops
//! over the files through the action loop (one host call per file keeps
//! each invocation inside the sandbox's time budget).
//!
//! The host runs its own import pipeline on the bytes: the format comes
//! from the name's extension, a file already in the vault is skipped by
//! hash, a Garmin Monitor file lands as monitoring (its day is recomputed
//! once the whole invocation is over, not per call). The answer is the same
//! `ImportResult` JSON the app's drop import gets (`imported`, `skipped`,
//! `failed: [{path, reason}]`, `monitoring_files`). A file the pipeline
//! refuses is a `failed` entry, not an error; an error (the call fails) means
//! the plugin's own mistake or an unavailable vault — missing permission, a
//! bad name, an oversized file, a locked vault.
//!
//! Build: `cargo build --release --target wasm32-unknown-unknown`
//! then copy `target/wasm32-unknown-unknown/release/paste_import.wasm` to `plugin.wasm`.

use extism_pdk::*;
use serde_json::{json, Value};

#[host_fn]
extern "ExtismHost" {
    fn host_import_file(name: String, bytes: Vec<u8>) -> String;
}

#[plugin_fn]
pub fn dashboard_widget(input: String) -> FnResult<String> {
    let ctx: Value = serde_json::from_str(&input).unwrap_or_else(|_| json!({}));
    let action = ctx["action"].as_str().unwrap_or("");
    let name = ctx["values"]["name"].as_str().unwrap_or("pasted.gpx").to_string();
    let content = ctx["values"]["content"].as_str().unwrap_or("").to_string();

    let mut outcome = Vec::new();
    if action == "import" {
        let answer = unsafe { host_import_file(name.clone(), content.clone().into_bytes())? };
        let r: Value = serde_json::from_str(&answer)?;
        let count = |key: &str| r[key].as_u64().unwrap_or(0).to_string();
        let failed = r["failed"].as_array().cloned().unwrap_or_default();
        outcome.push(json!({ "type": "divider" }));
        outcome.push(json!({ "type": "stat_grid", "stats": [
            { "label": "Imported", "value": count("imported") },
            { "label": "Skipped", "value": count("skipped") },
            { "label": "Monitoring", "value": count("monitoring_files") },
            { "label": "Failed", "value": failed.len().to_string() }
        ]}));
        for f in &failed {
            let text = format!(
                "{}: {}",
                f["path"].as_str().unwrap_or("?"),
                f["reason"].as_str().unwrap_or("?")
            );
            outcome.push(json!({ "type": "text", "text": text }));
        }
    }

    // The pasted text is cleared once handed over; the name stays for the next file.
    let shown_content = if action == "import" { String::new() } else { content };
    let mut elements = vec![
        json!({ "type": "text", "text": "Paste a GPX or TCX file and import it into the vault." }),
        json!({ "type": "input", "id": "name", "label": "File name", "value": name }),
        json!({ "type": "input", "id": "content", "label": "File contents", "value": shown_content }),
        json!({ "type": "button", "label": "Import", "action": "import" }),
    ];
    elements.extend(outcome);

    let spec = json!({ "title": "Paste import", "elements": elements });
    Ok(serde_json::to_string(&spec)?)
}
