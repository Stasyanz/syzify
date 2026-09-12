use std::sync::Mutex;
use std::time::{Duration, Instant};

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

    throttle();
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

/// Forward geocode: text query → (lat, lon, locality) of the best match,
/// where the locality is the settlement the match is in plus its parent
/// ("Mahmutlar, Alanya") — see [`locality_name`] and [`pick_best`].
pub fn forward_geocode(query: &str) -> Result<(f64, f64, String), String> {
    let url = search_url(query);

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .user_agent("Syzify/1.0")
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    throttle();
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

    let first = pick_best(&body, query).ok_or("No results found")?;

    let lat: f64 = first["lat"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .ok_or("Missing lat")?;
    let lon: f64 = first["lon"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .ok_or("Missing lon")?;
    let display = first["display_name"].as_str().unwrap_or(query);
    let class = first["class"].as_str().unwrap_or("");
    let name = locality_from_address(&first["address"]).unwrap_or_else(|| locality_name(display, class));
    Ok((lat, lon, if name.is_empty() { query.to_string() } else { name }))
}

/// The locality from Nominatim's structured `address`: the finest
/// settlement-level part the match sits in, then the next level up the
/// same ladder that reads as a place — "Mahmutlar, Alanya" (village, town),
/// "Austin, Texas" (city, state: "Travis County" in between is a label,
/// not a place, and is skipped by its value), "Brooklyn, New York"
/// (borough, city), "Berlin, Germany" (city and state coincide, so the
/// country stands in), "Moscow, Russia" (a city that Nominatim knows only
/// as a state). None when the object carries nothing on the ladder.
pub(crate) fn locality_from_address(address: &serde_json::Value) -> Option<String> {
    // Fine to coarse. `region` is deliberately absent ("Central Federal
    // District" is not where a ride was), and `county` counts only when its
    // value reads as a place: Turkey's district (ilçe) arrives as county —
    // "Alanya" — while "Travis County" and "Kings County" are labels a
    // rider never files under.
    const LADDER: &[&str] = &[
        "neighbourhood", "quarter", "suburb", "borough", "city_district",
        "hamlet", "village", "town", "city", "municipality",
        "county", "state", "province", "country",
    ];
    const NOT_A_PLACE: &[&str] = &["county", "district", "region", "oblast", "parish"];
    let value = |k: &str| {
        address[k].as_str().map(str::trim).filter(|v| !v.is_empty()).filter(|v| {
            k != "county" || !NOT_A_PLACE.iter().any(|suffix| v.to_lowercase().ends_with(suffix))
        })
    };
    let (i, main) = LADDER.iter().enumerate().find_map(|(i, k)| value(k).map(|v| (i, v)))?;
    if LADDER[i] == "country" {
        return None;
    }
    let parent = LADDER[i + 1..]
        .iter()
        .filter_map(|k| value(k))
        .find(|p| !p.eq_ignore_ascii_case(main));
    Some(match parent {
        Some(p) => format!("{main}, {p}"),
        None => main.to_string(),
    })
}

/// The row a free-text save follows, out of the top few: Nominatim ranks
/// by importance, so a bare "Alanya" comes back with the city of Antalya
/// first and Alanya itself second, and "London" with "Greater London"
/// first and London, Ontario third. Among the rows whose own name carries
/// every word of the query, the highest-ranked settlement or boundary wins
/// (Alanya over Antalya, Greater London over Ontario and over London
/// Bridge), then any such row; failing that, the first settlement or
/// boundary; failing that, the first row as ranked.
pub(crate) fn pick_best<'a>(rows: &'a [serde_json::Value], query: &str) -> Option<&'a serde_json::Value> {
    let wanted = name_words(query);
    let own_name = |row: &serde_json::Value| {
        row["display_name"]
            .as_str()
            .and_then(|d| d.split(',').next())
            .map(name_words)
            .unwrap_or_default()
    };
    // Only rows the caller can use: the list parser skips rows without
    // coordinates, and the free-text path must not pick one and fail.
    let usable = |row: &&serde_json::Value| {
        ["lat", "lon"].iter().all(|k| row[*k].as_str().and_then(|v| v.parse::<f64>().ok()).is_some_and(f64::is_finite))
    };
    let named = |row: &&serde_json::Value| {
        let own = own_name(row);
        !wanted.is_empty() && wanted.iter().all(|w| own.contains(w))
    };
    let settlement = |row: &&serde_json::Value| matches!(row["class"].as_str(), Some("place" | "boundary"));
    // Among the rows named like the query, a settlement before a POI:
    // "London" must not land on London Bridge because it ranks higher.
    rows.iter()
        .filter(usable)
        .find(|row| named(row) && settlement(row))
        .or_else(|| rows.iter().filter(usable).find(named))
        .or_else(|| rows.iter().filter(usable).find(settlement))
        .or_else(|| rows.iter().find(usable))
}

/// The words of a place name, folded for comparison: lowercase, the
/// combining marks that lowercasing leaves behind stripped (Turkish
/// "İstanbul" lowercases to "i" plus a combining dot), and the precomposed
/// letters a Latin keyboard types without their marks folded by a small
/// map — "Kadıköy" and "München" have to equal a typed "kadikoy" and
/// "munchen". Not a general normalizer (no NFD without a crate): the
/// Turkish set plus the common German and French letters.
fn name_words(text: &str) -> Vec<String> {
    let fold = |c: char| match c {
        'ı' => 'i',
        'ö' | 'ó' | 'ò' | 'ô' | 'õ' => 'o',
        'ü' | 'ú' | 'ù' | 'û' => 'u',
        'ç' => 'c',
        'ş' => 's',
        'ğ' => 'g',
        'ä' | 'á' | 'à' | 'â' | 'ã' | 'å' => 'a',
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'í' | 'ì' | 'î' | 'ï' => 'i',
        'ñ' => 'n',
        c => c,
    };
    text.to_lowercase()
        .chars()
        .filter(|c| !('\u{0300}'..='\u{036F}').contains(c))
        .map(fold)
        .collect::<String>()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

/// The locality a free-text save is filed under: the settlement plus its
/// parent, "Mahmutlar, Alanya". Nominatim's display_name runs fine to
/// coarse ("Cebeci Towers, Mahmutlar, Alanya, Antalya, …, Turkey"), so a
/// settlement or boundary hit (class `place`/`boundary`) is its first two
/// segments, and anything inside one skips its own name and takes the two
/// that follow; postcodes (all digits) are not places and are skipped;
/// a repeated segment ("Berlin, Berlin") collapses to one.
pub(crate) fn locality_name(display_name: &str, class: &str) -> String {
    let segments: Vec<&str> = display_name
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.chars().all(|c| c.is_ascii_digit()))
        .collect();
    let settlement = matches!(class, "place" | "boundary");
    let start = if settlement || segments.len() < 2 { 0 } else { 1 };
    let mut out: Vec<&str> = Vec::new();
    for seg in segments.iter().skip(start).take(2) {
        if !out.contains(seg) {
            out.push(seg);
        }
    }
    out.join(", ")
}

/// One hit of a location search: what the suggestion list shows and what
/// picking it writes — the short name (the first part of Nominatim's
/// display_name; #131 first asked for the full display_name, but the short
/// one is what forward_geocode has always stored and what every existing
/// row holds, so the library stays uniform) plus a short context line so
/// two "Mahmutlar"s can be told apart.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct LocationHit {
    /// What the list shows and a pick stores: the place itself for a
    /// settlement, "Cebeci 7, Mahmutlar" for something inside one.
    pub name: String,
    /// The first segment of display_name alone — the hit's identity for
    /// dedup, independent of how `name` is composed.
    #[serde(skip)]
    pub place: String,
    pub detail: String,
    pub lat: f64,
    pub lon: f64,
    /// Nominatim's `type` (city, village, suburb, …), for the UI to hint at.
    pub kind: String,
}

/// How many hits one search asks for.
pub const SEARCH_LIMIT: usize = 5;

/// Nominatim's usage policy: at most one request per second, for the app as
/// a whole — every path to it (reverse, forward, search) takes its turn at
/// this one slot, so a search typed during the import backfill, or a Save
/// right after a search, still keeps the spacing.
const MIN_REQUEST_INTERVAL: Duration = Duration::from_secs(1);
static NEXT_REQUEST_AT: Mutex<Option<Instant>> = Mutex::new(None);

/// How long a request issued at `now` must wait so that it lands at least
/// `min` after the previously scheduled one (`last`).
pub(crate) fn throttle_delay(last: Option<Instant>, now: Instant, min: Duration) -> Duration {
    match last {
        Some(l) if now < l + min => (l + min) - now,
        _ => Duration::ZERO,
    }
}

/// Reserve the next send slot and sleep until it — concurrent callers queue
/// one behind another rather than all firing at once.
fn throttle() {
    let wait = {
        let mut slot = NEXT_REQUEST_AT.lock().unwrap_or_else(|p| p.into_inner());
        let now = Instant::now();
        let d = throttle_delay(*slot, now, MIN_REQUEST_INTERVAL);
        *slot = Some(now + d);
        d
    };
    if !wait.is_zero() {
        std::thread::sleep(wait);
    }
}

/// Location search: text → up to SEARCH_LIMIT hits for the user to pick
/// from. Network and HTTP failures are `Err`; "nothing matched" is `Ok(vec![])`.
pub fn search_locations(query: &str) -> Result<Vec<LocationHit>, String> {
    let url = search_url(query);

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .user_agent("Syzify/1.0")
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    throttle();
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

    Ok(parse_search_hits(&body))
}

/// The search request for `query`: SEARCH_LIMIT hits, JSON, English names,
/// with the structured `address` object a free-text save is filed by.
pub(crate) fn search_url(query: &str) -> String {
    format!(
        "https://nominatim.openstreetmap.org/search?q={}&format=json&limit={}&accept-language=en&addressdetails=1",
        urlencoded(query),
        SEARCH_LIMIT
    )
}

/// Two hits with the same first segment within a couple of kilometers are
/// one place returned twice (as a boundary and as a node, or under two
/// classes, which would compose `name` differently); namesakes far apart
/// are exactly what the list is for, and both stay.
fn same_place(a: &LocationHit, b: &LocationHit) -> bool {
    a.place == b.place && (a.lat - b.lat).abs() < 0.02 && (a.lon - b.lon).abs() < 0.02
}

/// Nominatim search rows → hits, in order, skipping rows without usable
/// coordinates or a name, and collapsing rows that are one place twice.
pub(crate) fn parse_search_hits(body: &[serde_json::Value]) -> Vec<LocationHit> {
    let coord = |v: &serde_json::Value| v.as_str().and_then(|s| s.parse::<f64>().ok()).filter(|f| f.is_finite());
    let mut out: Vec<LocationHit> = Vec::new();
    for row in body {
        let (Some(lat), Some(lon)) = (coord(&row["lat"]), coord(&row["lon"])) else {
            continue;
        };
        let mut parts = row["display_name"]
            .as_str()
            .unwrap_or("")
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let Some(place) = parts.next() else { continue };
        let mut rest: Vec<&str> = parts.collect();
        // A settlement or an administrative area (Nominatim class `place`
        // or `boundary`) is its own name; anything inside one — a street,
        // a housing complex, a mosque — carries the place it is in, so a
        // library row does not read "Cebeci 7" with no town in sight.
        let settlement = matches!(row["class"].as_str().unwrap_or(""), "place" | "boundary");
        let name = if settlement || rest.is_empty() {
            place.to_string()
        } else {
            format!("{place}, {}", rest.remove(0))
        };
        // The nearest and the widest of the rest: "Alanya, Türkiye", not
        // the whole administrative chain with its postcode.
        let detail = match rest.as_slice() {
            [] => String::new(),
            [only] => (*only).to_string(),
            [near, .., last] => format!("{near}, {last}"),
        };
        let hit = LocationHit {
            name,
            place: place.to_string(),
            detail,
            lat,
            lon,
            kind: row["type"].as_str().unwrap_or("").to_string(),
        };
        if !out.iter().any(|h| same_place(h, &hit)) {
            out.push(hit);
        }
    }
    out
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
    fn search_hits_keep_order_short_names_and_context_and_drop_junk() {
        let body: Vec<serde_json::Value> = serde_json::from_str(
            r#"[
              {"lat":"36.4899","lon":"32.0938","class":"place","type":"suburb",
               "display_name":"Mahmutlar, Alanya, Antalya, Mediterranean Region, 07460, Türkiye"},
              {"lat":"36.4899","lon":"32.0938","class":"boundary","type":"administrative",
               "display_name":"Mahmutlar, Alanya, Antalya, Mediterranean Region, 07460, Türkiye"},
              {"lat":"36.4905","lon":"32.0941","class":"landuse","type":"residential",
               "display_name":"Mahmutlar, Alanya, Antalya, Mediterranean Region, 07460, Türkiye"},
              {"lat":"38.9","lon":"30.5","class":"place","type":"village",
               "display_name":"Mahmutlar, Alanya, Antalya, Mediterranean Region, 07460, Türkiye"},
              {"lat":"40.1","lon":"29.2","class":"place","type":"village","display_name":"Mahmutlar, Bursa, Türkiye"},
              {"lat":"36.48","lon":"32.10","class":"landuse","type":"residential",
               "display_name":"Cebeci 7, Mahmutlar, Alanya, Antalya, Mediterranean Region, Türkiye"},
              {"lat":"36.47","lon":"32.11","class":"highway","type":"tertiary",
               "display_name":"Cebeci Street, Kestel"},
              {"lat":"x","lon":"1.0","display_name":"Nowhere, Land"},
              {"lat":"1.0","lon":"2.0","display_name":"  ,  "},
              {"lat":"3.0","lon":"4.0","class":"amenity","type":"cafe","display_name":"Lonely"}
            ]"#,
        )
        .unwrap();
        let hits = parse_search_hits(&body);
        // The same Mahmutlar three times — place, boundary, and a landuse
        // row that would compose its name differently — collapses to one;
        // the bad lat and the empty name are dropped.
        assert_eq!(hits.len(), 6);
        assert_eq!(hits[0].name, "Mahmutlar");
        assert_eq!(hits[0].place, "Mahmutlar");
        assert_eq!(hits[0].detail, "Alanya, Türkiye");
        assert_eq!(hits[0].kind, "suburb");
        assert_eq!((hits[0].lat, hits[0].lon), (36.4899, 32.0938));
        // Same name and context line, 300 km away: a namesake, kept.
        assert_eq!((hits[1].lat, hits[1].lon), (38.9, 30.5));
        assert_eq!(hits[2].detail, "Bursa, Türkiye", "two parts: both kept");
        // Inside a settlement: the name carries the place, the context line
        // continues from there; identity stays the bare first segment.
        assert_eq!(hits[3].name, "Cebeci 7, Mahmutlar");
        assert_eq!(hits[3].place, "Cebeci 7");
        assert_eq!(hits[3].detail, "Alanya, Türkiye");
        assert_eq!(hits[4].name, "Cebeci Street, Kestel");
        assert_eq!(hits[4].detail, "");
        assert_eq!(hits[5].name, "Lonely", "nothing to add when there is no rest");
        assert_eq!(hits[5].detail, "");
        assert!(parse_search_hits(&[]).is_empty());
    }

    #[test]
    fn pick_best_prefers_the_row_named_like_the_query_then_a_settlement() {
        let rows: Vec<serde_json::Value> = serde_json::from_str(
            r#"[
              {"lat":"36.9","lon":"30.7","class":"place","type":"city","display_name":"Antalya, Muratpaşa, Antalya, Turkey"},
              {"lat":"36.5","lon":"32.0","class":"boundary","type":"administrative","display_name":"Alanya, Antalya, Turkey"}
            ]"#,
        )
        .unwrap();
        assert_eq!(pick_best(&rows, " alanya ").unwrap()["display_name"], "Alanya, Antalya, Turkey");
        assert_eq!(pick_best(&rows, "Antalya").unwrap()["display_name"], "Antalya, Muratpaşa, Antalya, Turkey");
        // Nominatim's London: the capital answers as "Greater London" and
        // ranks first; the row literally named "London" is in Ontario.
        let london: Vec<serde_json::Value> = serde_json::from_str(
            r#"[
              {"lat":"51.5","lon":"-0.1","class":"boundary","display_name":"Greater London, England, United Kingdom"},
              {"lat":"51.51","lon":"-0.09","class":"boundary","display_name":"City of London, Greater London, England, United Kingdom"},
              {"lat":"42.98","lon":"-81.25","class":"boundary","display_name":"London, Southwestern Ontario, Ontario, Canada"}
            ]"#,
        )
        .unwrap();
        assert_eq!(pick_best(&london, "London").unwrap()["display_name"], "Greater London, England, United Kingdom");
        // A POI named like the query, ranked above the settlement, does not win.
        let bridge: Vec<serde_json::Value> = serde_json::from_str(
            r#"[
              {"lat":"51.508","lon":"-0.088","class":"man_made","display_name":"London Bridge, Southwark, London"},
              {"lat":"51.5","lon":"-0.1","class":"boundary","display_name":"Greater London, England, United Kingdom"}
            ]"#,
        )
        .unwrap();
        assert_eq!(pick_best(&bridge, "London").unwrap()["class"], "boundary");
        assert_eq!(pick_best(&bridge, "London Bridge").unwrap()["class"], "man_made");
        assert_eq!(pick_best(&london, "city of london").unwrap()["display_name"].as_str().unwrap().starts_with("City of London"), true);
        // A name carrying every word of the query wins even as a POI; with
        // no such row, the first settlement/boundary beats a POI ranked
        // above it.
        let mixed: Vec<serde_json::Value> = serde_json::from_str(
            r#"[
              {"lat":"36.5","lon":"32.1","class":"amenity","type":"cafe","display_name":"Cafe Alanya, Kestel, Alanya"},
              {"lat":"36.5","lon":"32.0","class":"boundary","type":"administrative","display_name":"Alanya, Antalya, Turkey"}
            ]"#,
        )
        .unwrap();
        assert_eq!(pick_best(&mixed, "alanya cafe").unwrap()["class"], "amenity");
        assert_eq!(pick_best(&mixed, "alanya centre").unwrap()["class"], "boundary");
        // Turkish capitals: "İstanbul" and "Kızılcahamam" match what a Latin
        // keyboard types; a row without coordinates is never picked.
        let turkish: Vec<serde_json::Value> = serde_json::from_str(
            r#"[
              {"class":"place","display_name":"İstanbul, Türkiye"},
              {"lat":"41.0","lon":"29.0","class":"place","display_name":"İstanbul, Marmara Region, Türkiye"},
              {"lat":"40.5","lon":"32.6","class":"place","display_name":"Kızılcahamam, Ankara, Türkiye"}
            ]"#,
        )
        .unwrap();
        assert_eq!(pick_best(&turkish, "istanbul").unwrap()["lat"], "41.0");
        assert_eq!(pick_best(&turkish, "KIZILCAHAMAM").unwrap()["lat"], "40.5");
        assert_eq!(name_words("İstanbul-Kadıköy"), ["istanbul", "kadikoy"]);
        assert_eq!(name_words("München, Çeşme"), ["munchen", "cesme"]);
        let no_coords: Vec<serde_json::Value> =
            serde_json::from_str(r#"[{"class":"place","display_name":"Nowhere"}]"#).unwrap();
        assert!(pick_best(&no_coords, "Nowhere").is_none());
        // Only POIs: the first as ranked.
        let pois: Vec<serde_json::Value> = serde_json::from_str(
            r#"[{"lat":"36.48","lon":"32.10","class":"landuse","display_name":"Cebeci Towers, Mahmutlar"},{"lat":"36.48","lon":"32.11","class":"landuse","display_name":"Cebeci 7, Mahmutlar"}]"#,
        )
        .unwrap();
        assert_eq!(pick_best(&pois, "Mahmutlar cebeci 6").unwrap()["display_name"], "Cebeci Towers, Mahmutlar");
        assert!(pick_best(&[], "x").is_none());
    }

    #[test]
    fn locality_from_address_reads_the_settlement_and_a_parent_that_is_a_place() {
        let loc = |json: &str| locality_from_address(&serde_json::from_str::<serde_json::Value>(json).unwrap());
        // Real Nominatim objects, trimmed.
        assert_eq!(
            loc(r#"{"residential":"Cebeci Towers","village":"Mahmutlar","town":"Alanya","province":"Antalya","region":"Mediterranean Region","country":"Turkey"}"#).as_deref(),
            Some("Mahmutlar, Alanya")
        );
        assert_eq!(
            loc(r#"{"city":"Austin","county":"Travis County","state":"Texas","country":"United States"}"#).as_deref(),
            Some("Austin, Texas")
        );
        assert_eq!(
            loc(r#"{"borough":"Brooklyn","city":"New York","state":"New York","country":"United States"}"#).as_deref(),
            Some("Brooklyn, New York")
        );
        assert_eq!(loc(r#"{"city":"Berlin","state":"Berlin","country":"Germany"}"#).as_deref(), Some("Berlin, Germany"));
        assert_eq!(
            loc(r#"{"state":"Moscow","region":"Central Federal District","country":"Russia"}"#).as_deref(),
            Some("Moscow, Russia")
        );
        // A shop on a street: the road is not on the ladder, the suburb and town are.
        assert_eq!(
            loc(r#"{"shop":"bakery","road":"Cebeci Street","suburb":"Kestel","town":"Alanya","country":"Türkiye"}"#).as_deref(),
            Some("Kestel, Alanya")
        );
        // Turkey's district arrives as `county`, and it is a place; a US
        // "Travis County" is not (asserted above).
        assert_eq!(loc(r#"{"village":"Ürünlü","county":"Alanya","state":"Antalya"}"#).as_deref(), Some("Ürünlü, Alanya"));
        assert_eq!(loc(r#"{"hamlet":"Nook","village":"Bigger","state":"S"}"#).as_deref(), Some("Nook, Bigger"));
        assert_eq!(loc(r#"{"city":"Monaco","country":"Monaco"}"#).as_deref(), Some("Monaco"));
        assert_eq!(loc(r#"{"country":"Türkiye"}"#), None, "a country alone is not a locality");
        assert_eq!(loc(r#"{"city":"  ","country":"X"}"#), None);
        assert_eq!(loc(r#"{}"#), None);
    }

    #[test]
    fn locality_name_is_the_settlement_and_its_parent() {
        let chain = "Cebeci Towers, Mahmutlar, Alanya, Antalya, Mediterranean Region, 07460, Türkiye";
        assert_eq!(locality_name(chain, "landuse"), "Mahmutlar, Alanya");
        assert_eq!(locality_name("Mahmutlar, Alanya, Antalya, Türkiye", "place"), "Mahmutlar, Alanya");
        assert_eq!(locality_name("Alanya, Antalya, Türkiye", "boundary"), "Alanya, Antalya");
        // A postcode is not a place; a repeated name collapses.
        assert_eq!(locality_name("Cebeci Street, 07450, Kestel, Alanya", "highway"), "Kestel, Alanya");
        assert_eq!(locality_name("Berlin, Berlin, Germany", "place"), "Berlin");
        // Too little to skip anything: keep what there is.
        assert_eq!(locality_name("Lonely", "amenity"), "Lonely");
        assert_eq!(locality_name("  ,  ", "place"), "");
    }

    #[test]
    fn search_url_carries_the_limit_and_the_encoded_query() {
        assert_eq!(
            search_url("Mahmutlar, Cebeci 7"),
            "https://nominatim.openstreetmap.org/search?q=Mahmutlar%2C+Cebeci+7&format=json&limit=5&accept-language=en&addressdetails=1"
        );
        assert!(search_url("München & Co").contains("q=M%C3%BCnchen+%26+Co&"));
    }

    #[test]
    fn throttle_delay_spaces_requests_a_second_apart() {
        let min = Duration::from_secs(1);
        let t0 = Instant::now();
        assert_eq!(throttle_delay(None, t0, min), Duration::ZERO);
        assert_eq!(throttle_delay(Some(t0), t0 + Duration::from_millis(300), min), Duration::from_millis(700));
        assert_eq!(throttle_delay(Some(t0), t0 + Duration::from_secs(2), min), Duration::ZERO);
        assert_eq!(throttle_delay(Some(t0), t0 + min, min), Duration::ZERO);
    }

    #[test]
    fn urlencoded_basic() {
        assert_eq!(urlencoded("Moscow"), "Moscow");
        assert_eq!(urlencoded("New York"), "New+York");
        assert_eq!(urlencoded("München"), "M%C3%BCnchen");
    }
}

