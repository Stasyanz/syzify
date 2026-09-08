// SPDX-License-Identifier: MIT-0
// (Example code — copy freely into your plugin; see examples/plugins/LICENSE.)

//! Reference Syzify plugin: a `route.planner` page that sends one request
//! through `host_http` and shows what came back — status, final URL,
//! headers, body. Handy to see what an API answers before writing a sync
//! plugin, and to watch the allow-list refuse a host the manifest does not
//! declare (edit the URL to another host: the call fails, nothing is sent).
//!
//! `host_http` is the only way to the network. It takes the request as JSON
//! (`{url, method, headers: [[name, value], …]}`) plus the body bytes and
//! returns the body of the final response; `host_http_meta()` right after
//! gives that response's `{status, url, headers}` — headers as pairs, so a
//! repeated `Set-Cookie` survives. The host follows redirects itself,
//! checks every hop against the plugin's `net:host=` hosts, and keeps a
//! cookie jar for the length of one invocation, so a login flow of several
//! requests just works inside one action.
//!
//! A refused host, a bad request or a failed transport does not come back
//! as an error value: the host call fails and the whole invocation aborts
//! (the host shows an error card). The `Result` a host function returns
//! covers only the decoding of its answer.
//!
//! The `probe` export returns the raw answer as JSON; the app's tests drive
//! it (it is not a contribution point).
//!
//! Build: `cargo build --release --target wasm32-unknown-unknown`
//! then copy `target/wasm32-unknown-unknown/release/net_probe.wasm` to `plugin.wasm`.

use extism_pdk::*;
use serde_json::{json, Value};

const DEFAULT_URL: &str = "https://api.open-meteo.com/v1/forecast\
?latitude=52.52&longitude=13.41&current=temperature_2m";

#[host_fn]
extern "ExtismHost" {
    fn host_http(request: String, body: Vec<u8>) -> Vec<u8>;
    fn host_http_meta() -> String;
    fn host_secret_set(request: String) -> String;
    fn host_secret_get(key: String) -> String;
}

/// One request through the host: the final response's meta and body.
fn fetch(
    url: &str,
    method: &str,
    headers: &[(String, String)],
    body: Vec<u8>,
) -> Result<(Value, Vec<u8>), String> {
    let request = json!({ "url": url, "method": method, "headers": headers }).to_string();
    let body = unsafe { host_http(request, body) }.map_err(|e| e.to_string())?;
    let meta = unsafe { host_http_meta() }.map_err(|e| e.to_string())?;
    let meta: Value = serde_json::from_str(&meta).map_err(|e| e.to_string())?;
    Ok((meta, body))
}

/// `[[name, value], …]` → pairs; anything else → no headers.
fn header_pairs(v: &Value) -> Vec<(String, String)> {
    v.as_array()
        .map(|pairs| {
            pairs
                .iter()
                .filter_map(|p| Some((p[0].as_str()?.to_string(), p[1].as_str()?.to_string())))
                .collect()
        })
        .unwrap_or_default()
}

/// Raw probe for tests: `{url, method?, headers?: [[k, v]…], body?}` →
/// `{status, url, headers, body}`.
#[plugin_fn]
pub fn probe(input: String) -> FnResult<String> {
    let v: Value = serde_json::from_str(&input)?;
    let (meta, body) = fetch(
        v["url"].as_str().unwrap_or(""),
        v["method"].as_str().unwrap_or("GET"),
        &header_pairs(&v["headers"]),
        v["body"].as_str().unwrap_or("").as_bytes().to_vec(),
    )
    .map_err(|e| Error::msg(e))?;
    Ok(json!({
        "status": meta["status"],
        "url": meta["url"],
        "headers": meta["headers"],
        "body": String::from_utf8_lossy(&body),
    })
    .to_string())
}

/// For tests: store a secret (`{"key", "value"}`; an empty value deletes) —
/// what a login flow does with the tokens it obtained (needs `data:secret`).
#[plugin_fn]
pub fn secret_set(input: String) -> FnResult<String> {
    unsafe { host_secret_set(input) }?;
    Ok("ok".to_string())
}

/// For tests: read a secret back (an empty string when there is none).
#[plugin_fn]
pub fn secret_get(input: String) -> FnResult<String> {
    Ok(unsafe { host_secret_get(input) }?)
}

/// For tests: the meta without a request — an error on a fresh invocation.
#[plugin_fn]
pub fn meta_only(_input: String) -> FnResult<String> {
    Ok(unsafe { host_http_meta() }?)
}

#[plugin_fn]
pub fn route_planner(input: String) -> FnResult<String> {
    let ctx: Value = serde_json::from_str(&input).unwrap_or_else(|_| json!({}));
    let url = ctx["values"]["url"].as_str().unwrap_or(DEFAULT_URL);
    let mut elements = vec![
        json!({ "type": "text", "text": "Send a GET through host_http and see what comes back. Only the host declared in plugin.json is reachable." }),
        json!({ "type": "input", "id": "url", "label": "URL", "value": url }),
        json!({ "type": "button", "label": "Fetch", "action": "fetch" }),
    ];
    // Network only behind the button: the host re-renders pages on its own.
    if ctx["action"].as_str() == Some("fetch") {
        match fetch(url, "GET", &[], Vec::new()) {
            Ok((meta, body)) => {
                let headers = header_pairs(&meta["headers"])
                    .iter()
                    .map(|(k, v)| format!("{k}: {v}"))
                    .collect::<Vec<_>>()
                    .join("\n");
                let text = String::from_utf8_lossy(&body);
                let preview: String = text.chars().take(2000).collect();
                elements.push(json!({ "type": "stat_grid", "stats": [
                    { "label": "Status", "value": meta["status"].to_string() },
                    { "label": "Bytes", "value": body.len().to_string() },
                ] }));
                elements.push(json!({ "type": "text", "text": format!("Final URL: {}", meta["url"].as_str().unwrap_or("")) }));
                elements.push(json!({ "type": "text", "text": headers }));
                elements.push(json!({ "type": "text", "text": preview }));
            }
            Err(e) => elements.push(json!({ "type": "text", "text": format!("Request failed: {e}") })),
        }
    }
    Ok(json!({ "title": "Network probe", "elements": elements }).to_string())
}
