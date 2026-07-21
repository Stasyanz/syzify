//! Import a Runkeeper `cardioActivities.csv` export.
//!
//! Unlike GPX/FIT/TCX (one file = one activity), this is a single CSV listing
//! many activities. Rows that reference a GPX file are skipped — those are
//! imported from the GPX itself (with the real track). The rest (swimming,
//! manually-logged, indoor — no GPS) become GPS-less activities, so they aren't
//! lost. Re-running is safe (content dedup).

use rusqlite::Connection;
use uuid::Uuid;

use crate::db;
use crate::import::dedup;
use crate::import::pipeline::{FailedFile, ImportResult};
use crate::models::activity::{default_activity_title, Activity, SportType};

pub fn import_runkeeper_csv(conn: &Connection, csv_path: &str) -> Result<ImportResult, String> {
    // Read as bytes and decode lossily — a non-UTF8 byte (old/Latin-1 exports)
    // must not abort the import. Strip a leading UTF-8 BOM so the first header
    // ("Date") still matches.
    let raw = std::fs::read(csv_path).map_err(|e| format!("Failed to read CSV: {e}"))?;
    let content = String::from_utf8_lossy(&raw);
    let content = content.strip_prefix('\u{FEFF}').unwrap_or(&content);
    let (rows, unterminated_quote) = parse_csv(content);
    let mut records = rows.into_iter();

    let header = records.next().ok_or("empty CSV")?;
    let col = |name: &str| header.iter().position(|h| h.trim() == name);
    let c_date = col("Date").ok_or("CSV missing a 'Date' column — not a Runkeeper export?")?;
    let c_type = col("Type").ok_or("CSV missing a 'Type' column — not a Runkeeper export?")?;
    let (c_route, c_dist, c_dur, c_speed, c_cal, c_climb, c_hr, c_notes, c_gpx) = (
        col("Route Name"),
        col("Distance (km)"),
        col("Duration"),
        col("Average Speed (km/h)"),
        col("Calories Burned"),
        col("Climb (m)"),
        col("Average Heart Rate (bpm)"),
        col("Notes"),
        col("GPX File"),
    );

    let mut result = ImportResult { imported: 0, skipped: 0, failed: Vec::new() };

    for row in records {
        if row.iter().all(|f| f.trim().is_empty()) {
            continue; // blank line
        }
        let get = |i: Option<usize>| i.and_then(|idx| row.get(idx)).map(|s| s.trim()).unwrap_or("");

        // Rows with a GPX file are imported from the GPX (with the track) — skip.
        if !get(c_gpx).is_empty() {
            result.skipped += 1;
            continue;
        }

        let date = get(Some(c_date));
        let Some(start_time) = parse_rk_date(date) else {
            result.failed.push(FailedFile {
                path: format!("row dated {date:?}"),
                reason: "unrecognized date format".to_string(),
            });
            continue;
        };

        let sport = SportType::from_str(get(Some(c_type)));
        let distance_m = parse_f64(get(c_dist)).map(|km| km * 1000.0);
        let duration_s = parse_duration(get(c_dur));

        // require_sport=true: a CSV row must not dedup against a different sport
        // at a similar distance/time (avoids silent loss of distinct activities).
        if dedup::is_content_duplicate(conn, &start_time, sport.as_str(), distance_m, duration_s, true)
            .map_err(|e| e.to_string())?
        {
            result.skipped += 1;
            continue;
        }

        let opt = |s: &str| (!s.is_empty()).then(|| s.to_string());
        let title = opt(get(c_route)).or_else(|| Some(default_activity_title(&sport, &start_time)));
        let activity = Activity {
            id: Uuid::new_v4().to_string(),
            start_time,
            sport_type: sport.as_str().to_string(),
            title,
            notes: opt(get(c_notes)),
            distance_m,
            duration_s,
            elev_gain_m: parse_f64(get(c_climb)),
            avg_speed_mps: parse_f64(get(c_speed)).map(|kmh| kmh / 3.6),
            avg_hr: parse_f64(get(c_hr)),
            calories: parse_f64(get(c_cal)),
            source_device: Some("Runkeeper (CSV)".to_string()),
            ..Default::default()
        };
        db::activities::insert_activity(conn, &activity).map_err(|e| e.to_string())?;
        result.imported += 1;
    }

    // Surface a truncated/corrupt CSV instead of silently dropping the tail.
    if unterminated_quote {
        result.failed.push(FailedFile {
            path: csv_path.to_string(),
            reason: "CSV ended inside a quoted field — file may be truncated".to_string(),
        });
    }

    Ok(result)
}

/// RFC4180-ish CSV reader: handles quoted fields with embedded commas, quotes
/// (`""`) and newlines. Returns the records plus whether the input ended inside
/// an unterminated quote (truncated/corrupt file).
fn parse_csv(content: &str) -> (Vec<Vec<String>>, bool) {
    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut chars = content.chars().peekable();

    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(c);
            }
        } else {
            match c {
                '"' => in_quotes = true,
                ',' => record.push(std::mem::take(&mut field)),
                '\r' => {}
                '\n' => {
                    record.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut record));
                }
                _ => field.push(c),
            }
        }
    }
    if !field.is_empty() || !record.is_empty() {
        record.push(field);
        records.push(record);
    }
    (records, in_quotes)
}

/// Parse a non-negative finite metric; rejects empty, NaN/Inf and negatives
/// (all Runkeeper metrics — distance, speed, calories, climb, HR — are ≥ 0).
fn parse_f64(s: &str) -> Option<f64> {
    let s = s.trim();
    if s.is_empty() {
        None
    } else {
        s.parse::<f64>().ok().filter(|v| v.is_finite() && *v >= 0.0)
    }
}

/// "27:37" (m:s) or "1:01:10" (h:m:s) → seconds (finite, non-negative).
fn parse_duration(s: &str) -> Option<f64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let nums: Option<Vec<f64>> = s.split(':').map(|p| p.trim().parse::<f64>().ok()).collect();
    let secs = match nums?.as_slice() {
        [sec] => *sec,
        [m, sec] => m * 60.0 + sec,
        [h, m, sec] => h * 3600.0 + m * 60.0 + sec,
        _ => return None,
    };
    (secs.is_finite() && secs >= 0.0).then_some(secs)
}

/// "YYYY-MM-DD HH:MM:SS" (Runkeeper local time) → "YYYY-MM-DDTHH:MM:SS".
///
/// NOTE: this is *local* time with no offset, whereas GPX trackpoints are UTC.
/// Content dedup compares timestamps naively, so a CSV row and a GPX of the same
/// workout could miss each other by the user's UTC offset. In practice this is
/// harmless here: rows that reference a GPX file are skipped entirely (only
/// GPS-less rows are imported from the CSV), so there is no GPX counterpart to
/// dedup against.
fn parse_rk_date(s: &str) -> Option<String> {
    let s = s.trim();
    let b = s.as_bytes();
    if s.len() == 19 && b[4] == b'-' && b[7] == b'-' && b[10] == b' ' && b[13] == b':' {
        Some(s.replacen(' ', "T", 1))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    const CSV: &str = "Date,Type,Route Name,Distance (km),Duration,Average Pace,Average Speed (km/h),Calories Burned,Climb (m),Average Heart Rate (bpm),Notes,GPX File\n\
2015-05-30 16:11:56,Running,,5.40,27:37,5:07,11.72,546.0,83.66,140,,2015-05-30-1611.gpx\n\
2015-05-28 07:00:00,Swimming,Pool,1.50,0:45:00,,2.0,400.0,0.0,,\"Felt, great\",\n\
2015-05-27 19:00:00,Cycling,,26.05,1:01:10,2:21,25.55,755.0,374.82,130,Evening,\n";

    #[test]
    fn imports_gpsless_rows_and_skips_gpx_rows() {
        let conn = db::test_db();
        let dir = std::env::temp_dir().join(format!("rk_csv_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cardioActivities.csv");
        std::fs::write(&path, CSV).unwrap();

        let r = import_runkeeper_csv(&conn, path.to_str().unwrap()).unwrap();
        assert_eq!(r.imported, 2, "swimming + cycling (no GPX)");
        assert_eq!(r.skipped, 1, "running row has a GPX file");
        assert!(r.failed.is_empty());

        // Re-run → all deduped.
        let r2 = import_runkeeper_csv(&conn, path.to_str().unwrap()).unwrap();
        assert_eq!(r2.imported, 0);

        let swims = conn
            .query_row(
                "SELECT distance_m, calories, notes, sport_type FROM activity WHERE sport_type='swim'",
                [],
                |row| Ok((
                    row.get::<_, f64>(0)?,
                    row.get::<_, f64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                )),
            )
            .unwrap();
        assert_eq!(swims.0, 1500.0); // 1.5 km
        assert_eq!(swims.1, 400.0);
        assert_eq!(swims.2, "Felt, great"); // quoted comma preserved
        assert_eq!(swims.3, "swim");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn unterminated_quote_is_reported() {
        let conn = db::test_db();
        let dir = std::env::temp_dir().join(format!("rk_q_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("c.csv");
        // Notes field opens a quote that is never closed → truncated tail.
        std::fs::write(
            &path,
            "Date,Type,Route Name,Distance (km),Duration,Average Pace,Average Speed (km/h),Calories Burned,Climb (m),Average Heart Rate (bpm),Notes,GPX File\n\
2015-01-02 07:00:00,Swimming,,1.0,30:00,,2.0,300,0,,\"oops never closed\n",
        )
        .unwrap();

        let r = import_runkeeper_csv(&conn, path.to_str().unwrap()).unwrap();
        assert!(
            r.failed.iter().any(|f| f.reason.contains("quoted field")),
            "truncated CSV must be reported: {r:?}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn handles_bom_and_non_utf8() {
        let conn = db::test_db();
        let dir = std::env::temp_dir().join(format!("rk_bom_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("c.csv");
        // Leading UTF-8 BOM + an invalid byte (0xFF) inside a Notes field.
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(
            b"Date,Type,Route Name,Distance (km),Duration,Average Pace,Average Speed (km/h),Calories Burned,Climb (m),Average Heart Rate (bpm),Notes,GPX File\n\
2015-01-02 07:00:00,Swimming,,1.0,30:00,,2.0,300,0,,",
        );
        bytes.push(0xFF); // invalid UTF-8 in Notes
        bytes.extend_from_slice(b",\n");
        std::fs::write(&path, &bytes).unwrap();

        // Must not error on the BOM/invalid byte, and must import the row.
        let r = import_runkeeper_csv(&conn, path.to_str().unwrap()).unwrap();
        assert_eq!(r.imported, 1, "{r:?}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn duration_and_date_parsing() {
        assert_eq!(parse_duration("27:37"), Some(27.0 * 60.0 + 37.0));
        assert_eq!(parse_duration("1:01:10"), Some(3670.0));
        assert_eq!(parse_duration(""), None);
        // Reject non-finite / negative (L3/L5).
        assert_eq!(parse_duration("inf:00"), None);
        assert_eq!(parse_duration("-5:00"), None);
        assert_eq!(parse_f64("-5"), None);
        assert_eq!(parse_f64("inf"), None);
        assert_eq!(parse_f64("5.4"), Some(5.4));
        assert_eq!(parse_rk_date("2015-05-30 16:11:56").as_deref(), Some("2015-05-30T16:11:56"));
        assert_eq!(parse_rk_date("nope"), None);
    }
}
