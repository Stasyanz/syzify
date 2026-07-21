use std::time::Duration;

/// Reverse geocode coordinates to a city/town name using Nominatim.
/// Returns the most relevant locality name (city > town > village > county).
///
/// `Ok(None)` means Nominatim answered but knows no name for these
/// coordinates (open sea, wilderness) — a definitive result the caller should
/// record so the same point isn't re-sent forever. `Err` is transient
/// (network/HTTP) and worth retrying later.
pub fn reverse_geocode(lat: f64, lon: f64) -> Result<Option<String>, String> {
    let url = format!(
        "https://nominatim.openstreetmap.org/reverse?lat={}&lon={}&format=json&zoom=10&accept-language=en",
        lat, lon
    );

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .user_agent("Syzify/1.0")
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let resp = client
        .get(&url)
        .send()
        .map_err(|e| format!("Nominatim request failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("Nominatim returned status {}", resp.status()));
    }

    let body: serde_json::Value = resp
        .json()
        .map_err(|e| format!("Failed to parse Nominatim response: {}", e))?;

    Ok(extract_location_name(&body))
}

fn extract_location_name(body: &serde_json::Value) -> Option<String> {
    let address = &body["address"];

    // Pick the most relevant locality name
    for key in &["city", "town", "village", "municipality", "county", "state"] {
        if let Some(name) = address[*key].as_str() {
            return Some(name.to_string());
        }
    }

    // Fallback: first part of display_name
    if let Some(display) = body["display_name"].as_str() {
        if let Some(first) = display.split(',').next() {
            return Some(first.trim().to_string());
        }
    }

    None
}

/// Forward geocode: text query → (lat, lon, display_name).
pub fn forward_geocode(query: &str) -> Result<(f64, f64, String), String> {
    let url = format!(
        "https://nominatim.openstreetmap.org/search?q={}&format=json&limit=1&accept-language=en",
        urlencoded(query)
    );

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .user_agent("Syzify/1.0")
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let resp = client
        .get(&url)
        .send()
        .map_err(|e| format!("Nominatim request failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("Nominatim returned status {}", resp.status()));
    }

    let body: Vec<serde_json::Value> = resp
        .json()
        .map_err(|e| format!("Failed to parse Nominatim response: {}", e))?;

    let first = body.first().ok_or("No results found")?;

    let lat: f64 = first["lat"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .ok_or("Missing lat")?;
    let lon: f64 = first["lon"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .ok_or("Missing lon")?;
    let display = first["display_name"]
        .as_str()
        .unwrap_or(query)
        .to_string();

    // Extract short name (city-level) from display_name
    let short_name = display
        .split(',')
        .next()
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| query.to_string());

    Ok((lat, lon, short_name))
}

fn urlencoded(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                String::from(b as char)
            }
            b' ' => "+".to_string(),
            _ => format!("%{:02X}", b),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_city_from_response() {
        let body: serde_json::Value = serde_json::json!({
            "address": {
                "city": "Moscow",
                "state": "Moscow",
                "country": "Russia"
            },
            "display_name": "Moscow, Russia"
        });
        assert_eq!(extract_location_name(&body).as_deref(), Some("Moscow"));
    }

    #[test]
    fn extract_town_when_no_city() {
        let body: serde_json::Value = serde_json::json!({
            "address": {
                "town": "Obninsk",
                "state": "Kaluga Oblast",
                "country": "Russia"
            }
        });
        assert_eq!(extract_location_name(&body).as_deref(), Some("Obninsk"));
    }

    #[test]
    fn fallback_to_display_name() {
        let body: serde_json::Value = serde_json::json!({
            "address": {},
            "display_name": "Some Place, Some Region, Some Country"
        });
        assert_eq!(extract_location_name(&body).as_deref(), Some("Some Place"));
    }

    #[test]
    fn no_location_found() {
        let body: serde_json::Value = serde_json::json!({
            "address": {}
        });
        assert!(extract_location_name(&body).is_none());
    }

    #[test]
    fn urlencoded_basic() {
        assert_eq!(urlencoded("Moscow"), "Moscow");
        assert_eq!(urlencoded("New York"), "New+York");
        assert_eq!(urlencoded("München"), "M%C3%BCnchen");
    }
}
