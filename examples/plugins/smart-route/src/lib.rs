// SPDX-License-Identifier: MIT-0
// (Example code — copy freely into your plugin; see examples/plugins/LICENSE.)

//! Reference Syzify plugin (phase 2): an interactive `route.planner` page.
//!
//! It shows a form (distance) and, on **Plan**, fetches current weather and
//! returns a suggested loop drawn as a `map` overlay. Demonstrates the action
//! loop (the host re-invokes with `{action, values}`), a `map` element, and
//! **brokered network access** — the manifest declares
//! `net:host=api.open-meteo.com`; the host wires exactly that into the sandbox's
//! allow-list. A non-200 response degrades gracefully; a *blocked* host aborts
//! the call (the host shows an error card), so a plugin can never reach a host
//! the user didn't approve.
//!
//! Build: `cargo build --release --target wasm32-unknown-unknown`
//! then copy `target/wasm32-unknown-unknown/release/smart_route.wasm` to `plugin.wasm`.

use extism_pdk::*;
use serde_json::{json, Value};

const LAT: f64 = 52.52;
const LON: f64 = 13.41;
const FORECAST_URL: &str = "https://api.open-meteo.com/v1/forecast\
?latitude=52.52&longitude=13.41&current=temperature_2m,wind_speed_10m";

#[plugin_fn]
pub fn route_planner(input: String) -> FnResult<String> {
    let ctx: Value = serde_json::from_str(&input).unwrap_or_else(|_| json!({}));
    let action = ctx["action"].as_str().unwrap_or("");
    let distance_km = ctx["values"]["distance"]
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(8.0);

    // Initial view (and any non-plan action): just the form. No network here, so
    // the page always opens even offline.
    if action != "plan" {
        let spec = json!({
            "title": "Smart route · Berlin",
            "elements": [
                { "type": "text", "text": "Enter a distance and plan a loop based on current weather." },
                { "type": "input", "id": "distance", "label": "Distance (km)", "value": "8", "input_type": "number" },
                { "type": "button", "label": "Plan route", "action": "plan" }
            ]
        });
        return Ok(serde_json::to_string(&spec)?);
    }

    // Plan: fetch weather (needs the allowed host) and draw a loop.
    let (temp, wind) = fetch_weather().unwrap_or((f64::NAN, f64::NAN));
    let mut stats = vec![json!({ "label": "Distance", "value": format!("{distance_km:.0} km") })];
    if temp.is_finite() {
        stats.push(json!({ "label": "Temperature", "value": format!("{temp:.0} °C") }));
    }
    if wind.is_finite() {
        stats.push(json!({ "label": "Wind", "value": format!("{wind:.0} km/h") }));
    }

    let spec = json!({
        "title": "Planned route",
        "elements": [
            { "type": "input", "id": "distance", "label": "Distance (km)", "value": format!("{distance_km:.0}"), "input_type": "number" },
            { "type": "button", "label": "Re-plan", "action": "plan" },
            { "type": "stat_grid", "stats": stats },
            { "type": "map", "points": loop_points(distance_km), "label": "Suggested loop" }
        ]
    });
    Ok(serde_json::to_string(&spec)?)
}

/// A crude diamond loop around the center, sized so its perimeter ≈ distance.
fn loop_points(distance_km: f64) -> Vec<[f64; 2]> {
    let r = (distance_km / 4.0) / 111.0; // ~km per degree latitude
    vec![
        [LAT + r, LON],
        [LAT, LON + r],
        [LAT - r, LON],
        [LAT, LON - r],
        [LAT + r, LON],
    ]
}

fn fetch_weather() -> Result<(f64, f64), String> {
    let req = HttpRequest::new(FORECAST_URL).with_method("GET");
    let res = http::request::<()>(&req, None).map_err(|e| e.to_string())?;
    if res.status_code() != 200 {
        return Err(format!("status {}", res.status_code()));
    }
    let v: Value = serde_json::from_slice(&res.body()).map_err(|e| e.to_string())?;
    let temp = v["current"]["temperature_2m"].as_f64().ok_or("no temperature")?;
    let wind = v["current"]["wind_speed_10m"].as_f64().ok_or("no wind")?;
    Ok((temp, wind))
}