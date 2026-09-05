//! Daily training load from heart-rate zones (ADR 0002): the recovery
//! index needs a load figure for every day, and the device's TSS exists
//! for a handful of power rides only, while HR zones exist for almost
//! every activity.
//!
//! hrTSS = Σ hours_in_zone × w² × 100, with w the intensity-factor
//! equivalent of each Garmin bucket (0 = below Z1 … 6 = above max). On the
//! five rides that carry a device TSS the two agree within ~15%.

use std::collections::BTreeMap;

use rusqlite::{Connection, Result};

/// Intensity-factor equivalent per HR bucket (below-Z1, Z1…Z5, above max).
const ZONE_IF: [f64; 7] = [0.3, 0.5, 0.65, 0.8, 0.95, 1.1, 1.15];

/// hrTSS per local calendar day ("YYYY-MM-DD"), summed over the day's
/// activities.
///
/// The day is the activity's own local date. start_time is stored as the
/// device wrote it: with an offset (`…+03:00`) or naive local, where the
/// first ten characters ARE the local date and `date()` would shift it to
/// UTC first; a few imports carry a UTC `…Z` timestamp, for which the
/// machine's zone is the best guess available (`timezone_offset` is empty
/// everywhere).
///
/// Per activity the MAX seconds per zone index is taken, so the per-lap
/// duplicate rows older imports still hold do not double count. Rows
/// longer than the activity itself are corrupt (imports before the
/// zone dedup fix left some with days' worth of seconds) and are dropped
/// before the MAX — otherwise the MAX would pick exactly those.
pub fn daily_hrtss(conn: &Connection) -> Result<BTreeMap<String, f64>> {
    let mut stmt = conn.prepare(
        "SELECT CASE WHEN a.start_time LIKE '%Z'
                     THEN date(a.start_time, 'localtime')
                     ELSE substr(a.start_time, 1, 10) END AS day,
                z.zone_index, SUM(z.secs)
         FROM (
           SELECT z.activity_id, z.zone_index, MAX(z.time_s) AS secs
           FROM time_in_zone z JOIN activity a ON a.id = z.activity_id
           WHERE z.zone_type = 'hr' AND z.time_s > 0
             AND (a.duration_s IS NULL OR z.time_s <= a.duration_s)
           GROUP BY z.activity_id, z.zone_index
         ) z
         JOIN activity a ON a.id = z.activity_id
         GROUP BY day, z.zone_index",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, f64>(2)?))
    })?;
    let mut out: BTreeMap<String, f64> = BTreeMap::new();
    for row in rows {
        let (day, zone, secs) = row?;
        let w = ZONE_IF[zone.clamp(0, 6) as usize];
        *out.entry(day).or_insert(0.0) += secs / 3600.0 * w * w * 100.0;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn activity(conn: &Connection, id: &str, start: &str) {
        activity_of(conn, id, start, Some(7200.0));
    }

    fn activity_of(conn: &Connection, id: &str, start: &str, duration_s: Option<f64>) {
        conn.execute(
            "INSERT INTO activity (id, start_time, sport_type, duration_s)
             VALUES (?1, ?2, 'ride', ?3)",
            rusqlite::params![id, start, duration_s],
        )
        .unwrap();
    }

    fn zone(conn: &Connection, activity: &str, index: i64, secs: f64) {
        conn.execute(
            "INSERT INTO time_in_zone
               (activity_id, zone_type, zone_index, time_s, zone_high_boundary)
             VALUES (?1, 'hr', ?2, ?3, NULL)",
            rusqlite::params![activity, index, secs],
        )
        .unwrap();
    }

    #[test]
    fn weights_seconds_by_zone_and_sums_a_day() {
        let conn = db::test_db();
        activity(&conn, "a", "2026-07-25T06:35:00+03:00");
        // An hour at Z4 (0.95² = 0.9025 → 90.25) + 30 min at Z2 (0.65² → 21.1).
        zone(&conn, "a", 4, 3600.0);
        zone(&conn, "a", 2, 1800.0);
        let daily = daily_hrtss(&conn).unwrap();
        let tss = daily["2026-07-25"];
        assert!((tss - (90.25 + 21.125)).abs() < 0.01, "{tss}");
    }

    #[test]
    fn lap_duplicates_do_not_double_count_but_two_activities_add_up() {
        let conn = db::test_db();
        activity(&conn, "a", "2026-07-25T06:35:00+03:00");
        // Session row + two lap rows of the same zone: MAX, not SUM.
        zone(&conn, "a", 3, 3600.0);
        zone(&conn, "a", 3, 1800.0);
        zone(&conn, "a", 3, 1800.0);
        activity(&conn, "b", "2026-07-25T18:00:00+03:00");
        zone(&conn, "b", 3, 3600.0);
        let daily = daily_hrtss(&conn).unwrap();
        let tss = daily["2026-07-25"];
        assert!((tss - 2.0 * 64.0).abs() < 0.01, "{tss}");
    }

    #[test]
    fn rows_longer_than_the_activity_are_corrupt_and_dropped() {
        let conn = db::test_db();
        // A 1987 s run whose session zones add up (68+114+349+1318+139), plus
        // a leftover row of 133988 s in zone 1 from a pre-dedup import.
        activity_of(&conn, "a", "2026-07-08T07:00:00+03:00", Some(1987.0));
        for (z, s) in [(1, 68.0), (2, 114.0), (3, 349.0), (4, 1318.0), (5, 139.0)] {
            zone(&conn, "a", z, s);
        }
        zone(&conn, "a", 1, 133_988.0);
        let daily = daily_hrtss(&conn).unwrap();
        let expect = (68.0 * 0.25 + 114.0 * 0.4225 + 349.0 * 0.64 + 1318.0 * 0.9025 + 139.0 * 1.21)
            / 36.0;
        let tss = daily["2026-07-08"];
        assert!((tss - expect).abs() < 0.01, "{tss} vs {expect}");
        // Without a duration nothing can be judged: the rows are kept as-is.
        activity_of(&conn, "b", "2026-07-09T07:00:00+03:00", None);
        zone(&conn, "b", 1, 3600.0);
        assert!((daily_hrtss(&conn).unwrap()["2026-07-09"] - 25.0).abs() < 0.01);
    }

    #[test]
    fn a_utc_timestamp_falls_back_to_the_machines_local_date() {
        let conn = db::test_db();
        // Noon UTC is the same calendar day in every zone from −12 to +11.
        activity(&conn, "a", "2026-07-26T12:00:00Z");
        zone(&conn, "a", 1, 3600.0);
        let daily = daily_hrtss(&conn).unwrap();
        assert!(daily.contains_key("2026-07-26"), "{daily:?}");
    }

    #[test]
    fn the_day_is_the_activitys_own_local_date() {
        let conn = db::test_db();
        // 01:00 +03:00 is 22:00 UTC of the day before — still the 26th here.
        activity(&conn, "a", "2026-07-26T01:00:00+03:00");
        zone(&conn, "a", 1, 3600.0);
        let daily = daily_hrtss(&conn).unwrap();
        assert!(daily.contains_key("2026-07-26"));
        assert!(!daily.contains_key("2026-07-25"));
    }

    #[test]
    fn days_without_hr_zones_are_absent_and_the_bucket_index_is_clamped() {
        let conn = db::test_db();
        activity(&conn, "a", "2026-07-25T06:35:00+03:00");
        activity(&conn, "b", "2026-07-26T06:35:00+03:00");
        zone(&conn, "b", 9, 3600.0); // an index beyond the palette → "above max"
        let daily = daily_hrtss(&conn).unwrap();
        assert!(!daily.contains_key("2026-07-25"));
        assert!((daily["2026-07-26"] - 1.15 * 1.15 * 100.0).abs() < 0.01);
    }
}
