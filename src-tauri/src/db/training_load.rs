//! Daily training load from heart-rate zones (ADR 0002): the recovery
//! index needs a load figure for every day, and the device's TSS exists
//! for a handful of power rides only, while HR zones exist for almost
//! every activity.
//!
//! hrTSS = Σ hours_in_zone × w² × 100, with w the intensity-factor
//! equivalent of each Garmin bucket (0 = below Z1 … 6 = above max). On the
//! five rides that carry a device TSS the two agree within ~15%.
//!
//! An activity whose file carries no HR zones (Zwift, Syzify's own indoor
//! rides) but has a heart-rate stream gets its time in zone from the
//! stream, with the zone boundaries the athlete's device last recorded
//! (#294). Worked out on every read, never stored, so stored HR rows are
//! always a device's; the Time in Zones card gets the same buckets from the
//! same tops ([`hr_zones_from_stream`], #298), fitted to the timer — they
//! differ from the load by the short stops. A ride is often recorded
//! twice (Zwift and the bike computer, Syzify and Zwift): stream time that
//! another recording already counts is left out. The stream counts elapsed
//! time (a stop up to 30 s holds), the device its timer: within ~1 % on
//! the rides that have both.

use std::collections::BTreeMap;

use rusqlite::{params, Connection, Result};

use crate::models::activity::SportType;
use crate::models::time_in_zone::TimeInZone;

/// Intensity-factor equivalent per HR bucket (below-Z1, Z1…Z5, above max).
const ZONE_IF: [f64; 7] = [0.3, 0.5, 0.65, 0.8, 0.95, 1.1, 1.15];

/// A sample's heart rate holds until the next one, unless they are further
/// apart than this: a pause or a drop, not riding.
const MAX_GAP_S: f64 = 30.0;

/// An activity's local calendar day (see [`daily_hrtss`]).
const DAY_SQL: &str = "CASE WHEN a.start_time LIKE '%Z'
                            THEN date(a.start_time, 'localtime')
                            ELSE substr(a.start_time, 1, 10) END";

/// A device HR zone row `z` of activity `a` that counts: rows longer than
/// the activity are corrupt (see [`daily_hrtss`]). An activity with one
/// has its load from them, never from its stream.
const COUNTED_ZONE: &str = "z.zone_type = 'hr' AND z.time_s > 0
                            AND (a.duration_s IS NULL OR z.time_s <= a.duration_s)";

fn hrtss(zone: usize, secs: f64) -> f64 {
    let w = ZONE_IF[zone.min(ZONE_IF.len() - 1)];
    secs / 3600.0 * w * w * 100.0
}

/// hrTSS per local calendar day ("YYYY-MM-DD"), summed over the day's
/// activities: from the device's HR zones, else from the HR stream.
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
    let mut stmt = conn.prepare(&format!(
        "SELECT {DAY_SQL} AS day, z.zone_index, SUM(z.secs)
         FROM (
           SELECT z.activity_id, z.zone_index, MAX(z.time_s) AS secs
           FROM time_in_zone z JOIN activity a ON a.id = z.activity_id
           WHERE {COUNTED_ZONE}
           GROUP BY z.activity_id, z.zone_index
         ) z
         JOIN activity a ON a.id = z.activity_id
         GROUP BY day, z.zone_index"
    ))?;
    let rows = stmt.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, f64>(2)?))
    })?;
    let mut out: BTreeMap<String, f64> = BTreeMap::new();
    for row in rows {
        let (day, zone, secs) = row?;
        *out.entry(day).or_insert(0.0) += hrtss(zone.max(0) as usize, secs);
    }
    for (day, zones) in stream_zone_seconds(conn)? {
        let tss: f64 = zones.iter().enumerate().map(|(zone, &secs)| hrtss(zone, secs)).sum();
        *out.entry(day).or_insert(0.0) += tss;
    }
    Ok(out)
}

/// A timestamp as epoch seconds: with an offset, or naive (read as UTC:
/// differenced, or compared with times of the same kind; a naive time
/// next to an offset one is off by the offset — every stored time with a
/// heart rate in the vault this was written on has one).
fn instant(text: &str) -> Option<f64> {
    if let Ok(t) = chrono::DateTime::parse_from_rfc3339(text) {
        return Some(t.timestamp_millis() as f64 / 1000.0);
    }
    chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S%.f")
        .ok()
        .map(|t| t.and_utc().timestamp_millis() as f64 / 1000.0)
}

/// The HR zone boundaries a device recorded with one activity.
struct Bounds {
    at: f64,
    sport: String,
    /// The top of each zone from bucket 0 up, rising.
    high: Vec<f64>,
}

/// One activity's boundary rows as they come, before they are judged.
struct Recorded {
    id: String,
    start: String,
    sport: String,
    zones: Vec<(i64, f64)>,
}

/// Every activity's recorded HR zone boundaries that make a usable set:
/// a top for each bucket but the last (below Z1, Z1…Z5), in order, each
/// above the one below. Another count would shift every bucket's weight.
fn recorded_bounds(conn: &Connection) -> Result<Vec<Bounds>> {
    let mut stmt = conn.prepare(
        "SELECT a.id, a.start_time, a.sport_type, z.zone_index, MAX(z.zone_high_boundary)
         FROM time_in_zone z JOIN activity a ON a.id = z.activity_id
         WHERE z.zone_type = 'hr' AND z.zone_high_boundary IS NOT NULL
         GROUP BY a.id, z.zone_index
         ORDER BY a.id, z.zone_index",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, i64>(3)?,
            r.get::<_, f64>(4)?,
        ))
    })?;
    let mut sets: Vec<Recorded> = Vec::new();
    for row in rows {
        let (id, start, sport, zone, high) = row?;
        match sets.last_mut() {
            Some(last) if last.id == id => last.zones.push((zone, high)),
            _ => sets.push(Recorded { id, start, sport, zones: vec![(zone, high)] }),
        }
    }
    Ok(sets
        .into_iter()
        .filter_map(|Recorded { start, sport, zones, .. }| {
            let whole = zones.len() == ZONE_IF.len() - 1
                && zones.iter().enumerate().all(|(i, &(zone, _))| zone == i as i64);
            let rising = zones.windows(2).all(|w| w[1].1 > w[0].1);
            let high: Vec<f64> = zones.into_iter().map(|(_, h)| h).collect();
            (whole && rising).then_some(Bounds { at: instant(&start)?, sport, high })
        })
        .collect())
}

/// The boundaries to read an activity of `sport` at `at` with: the last
/// recorded at or before it in its family of sports, else in any sport.
/// None before it: not counted (a rider who had Zwift before a watch with
/// zones gets no load for those early rides).
fn bounds_for<'a>(all: &'a [Bounds], sport: &str, at: f64) -> Option<&'a [f64]> {
    let family = SportType::family(sport);
    let kin = |b: &Bounds| if family.is_empty() { b.sport == sport } else { family.contains(&b.sport.as_str()) };
    let latest = |pick: &dyn Fn(&Bounds) -> bool| {
        all.iter().filter(|b| b.at <= at && pick(b)).max_by(|x, y| x.at.total_cmp(&y.at))
    };
    latest(&kin).or_else(|| latest(&|_| true)).map(|b| b.high.as_slice())
}

/// The bucket of `hr` under zone tops `high`: 0 below the first top, one
/// more per top reached, the last bucket above them all. A heart rate on a
/// top is in the zone above, as the devices count it (checked on the
/// vault's rides that have both: most agree within 1 %, short ones 5 %).
fn bucket(hr: f64, high: &[f64]) -> usize {
    high.iter().position(|&top| hr < top).unwrap_or(high.len())
}

/// Seconds per bucket of a stream of (time, heart rate) in time order:
/// each sample's heart rate holds until the next sample, unless they are
/// [`MAX_GAP_S`] apart, or the sample falls in a span another recording
/// already counts (`covered`, [from, to)).
fn zones_of(stream: &[(f64, Option<f64>)], high: &[f64], covered: &[(f64, f64)]) -> [f64; 7] {
    let mut zones = [0.0; 7];
    for pair in stream.windows(2) {
        let ((t0, hr), (t1, _)) = (pair[0], pair[1]);
        let dt = t1 - t0;
        let counted = covered.iter().any(|&(from, to)| t0 >= from && t0 < to);
        if let Some(hr) = hr.filter(|&hr| hr > 0.0 && dt > 0.0 && dt <= MAX_GAP_S && !counted) {
            zones[bucket(hr, high).min(zones.len() - 1)] += dt;
        }
    }
    zones
}

/// HR zones worked out from an activity's own stream (`t` in seconds, `hr`
/// as the detail page has them — its times read like [`instant`]'s for
/// every stored kind but a naive one), as rows shaped like a device's, for
/// the Time in Zones card and the charts of a ride or a run with none of
/// its own (#298): the same tops and buckets as its load. Empty for any
/// other sport (a swim's wrist heart rate on a runner's tops says little),
/// with no heart rate, or with no tops at or before it. Never stored, so
/// stored HR rows stay the device's.
///
/// The card reads the zones against the timer (`duration_s`), a device's
/// own way; the stream holds a heart rate through short stops, so the
/// seconds are scaled down to the timer when they run past it. The load
/// keeps the unscaled seconds: the two differ by those stops.
pub(crate) fn hr_zones_from_stream(
    conn: &Connection,
    activity_id: &str,
    start: &str,
    sport: &str,
    duration_s: Option<f64>,
    t: &[Option<f64>],
    hr: &[Option<i32>],
) -> Result<Vec<TimeInZone>> {
    let has_hr = hr.iter().any(|h| h.is_some_and(|h| h > 0));
    if !has_hr || SportType::family(sport).is_empty() {
        return Ok(Vec::new());
    }
    let Some(at) = instant(start) else { return Ok(Vec::new()) };
    let bounds = recorded_bounds(conn)?;
    let Some(high) = bounds_for(&bounds, sport, at) else { return Ok(Vec::new()) };
    let mut stream: Vec<(f64, Option<f64>)> =
        t.iter().zip(hr).filter_map(|(t, hr)| t.map(|t| (t, hr.map(f64::from)))).collect();
    stream.sort_by(|x, y| x.0.total_cmp(&y.0));
    let mut zones = zones_of(&stream, high, &[]);
    let total: f64 = zones.iter().sum();
    if total == 0.0 {
        return Ok(Vec::new());
    }
    if let Some(timer) = duration_s.filter(|&d| d > 0.0 && d < total) {
        // `total × timer / total` can come out a hair past the timer, and
        // the card drops a zone longer than the ride.
        let scale = timer / total;
        zones.iter_mut().for_each(|secs| *secs = (*secs * scale).min(timer));
    }
    Ok(zones
        .iter()
        .enumerate()
        .map(|(i, &secs)| TimeInZone {
            id: None,
            activity_id: activity_id.to_string(),
            zone_type: "hr".to_string(),
            zone_index: i as i32,
            time_s: secs,
            zone_high_boundary: high.get(i).copied(),
        })
        .collect())
}

/// The spans of the activities whose device zones count, on `day` or a
/// day either side: from the start to the last sample (else to the start
/// plus the duration).
fn device_spans(conn: &Connection, day: &str) -> Result<Vec<(f64, f64)>> {
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT a.start_time, a.duration_s, (SELECT MAX(t.t) FROM trackpoint t WHERE t.activity_id = a.id)
         FROM activity a
         WHERE {DAY_SQL} BETWEEN date(?1, '-1 day') AND date(?1, '+1 day')
           AND EXISTS (SELECT 1 FROM time_in_zone z WHERE z.activity_id = a.id AND {COUNTED_ZONE})"
    ))?;
    let rows = stmt.query_map(params![day], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, Option<f64>>(1)?, r.get::<_, Option<String>>(2)?))
    })?;
    let mut spans = Vec::new();
    for row in rows {
        let (start, duration, last) = row?;
        let Some(from) = instant(&start) else { continue };
        let to = last.as_deref().and_then(instant).unwrap_or(from + duration.unwrap_or(0.0));
        spans.push((from, to));
    }
    Ok(spans)
}

/// Seconds per HR bucket, by day, of every activity with no HR zones of
/// its own but a heart-rate stream and boundaries to read it with — less
/// the time another recording counts: a device's, or an earlier stream's.
fn stream_zone_seconds(conn: &Connection) -> Result<Vec<(String, [f64; 7])>> {
    let bounds = recorded_bounds(conn)?;
    let mut stmt = conn.prepare(&format!(
        "SELECT a.id, {DAY_SQL}, a.start_time, a.sport_type FROM activity a
         WHERE NOT EXISTS (SELECT 1 FROM time_in_zone z WHERE z.activity_id = a.id AND {COUNTED_ZONE})
           AND EXISTS (SELECT 1 FROM trackpoint t WHERE t.activity_id = a.id AND t.hr > 0)"
    ))?;
    let mut activities = stmt
        .query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?))
        })?
        .filter_map(|row| row.map(|(id, day, start, sport)| instant(&start).map(|at| (at, id, day, sport))).transpose())
        .collect::<Result<Vec<_>>>()?;
    // Earliest first: of two streams of one ride, the first counts.
    activities.sort_by(|x, y| x.0.total_cmp(&y.0));
    let mut points = conn.prepare("SELECT t, hr FROM trackpoint WHERE activity_id = ?1")?;
    let mut streamed: Vec<(f64, f64)> = Vec::new();
    let mut out = Vec::new();
    for (at, id, day, sport) in activities {
        let Some(high) = bounds_for(&bounds, &sport, at) else { continue };
        let mut stream = points
            .query_map(params![id], |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, Option<f64>>(1)?)))?
            .filter_map(|row| row.map(|(t, hr)| t.as_deref().and_then(instant).map(|t| (t, hr))).transpose())
            .collect::<Result<Vec<_>>>()?;
        stream.sort_by(|x, y| x.0.total_cmp(&y.0));
        let (Some(&(first, _)), Some(&(last, _))) = (stream.first(), stream.last()) else { continue };
        let mut covered = device_spans(conn, &day)?;
        covered.extend_from_slice(&streamed);
        // Only what meets this stream: the earlier streams pile up.
        covered.retain(|&(from, to)| to > first && from <= last);
        let zones = zones_of(&stream, high, &covered);
        // The whole span, samples without a heart rate too: a strap that
        // left one recording midway leaves those minutes uncounted by both.
        streamed.push((first, last));
        if zones.iter().any(|&secs| secs > 0.0) {
            out.push((day, zones));
        }
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

    const TOPS: [f64; 6] = [100.0, 120.0, 140.0, 160.0, 180.0, 190.0];

    fn activity_sport(conn: &Connection, id: &str, start: &str, sport: &str) {
        conn.execute(
            "INSERT INTO activity (id, start_time, sport_type, duration_s) VALUES (?1, ?2, ?3, 7200)",
            rusqlite::params![id, start, sport],
        )
        .unwrap();
    }

    /// The device's zones with these tops (the last bucket has none).
    fn zones_with_tops(conn: &Connection, activity: &str, tops: &[f64], secs: &[f64]) {
        for (index, &s) in secs.iter().enumerate() {
            conn.execute(
                "INSERT INTO time_in_zone (activity_id, zone_type, zone_index, time_s, zone_high_boundary)
                 VALUES (?1, 'hr', ?2, ?3, ?4)",
                rusqlite::params![activity, index as i64, s, tops.get(index)],
            )
            .unwrap();
        }
    }

    /// One sample a second at `hr` for `secs` seconds from `start`.
    fn stream(conn: &Connection, activity: &str, start: &str, hr: i64, secs: i64) {
        let t0 = chrono::DateTime::parse_from_rfc3339(start).unwrap();
        for s in 0..=secs {
            let t = (t0 + chrono::Duration::seconds(s)).to_rfc3339();
            conn.execute(
                "INSERT INTO trackpoint (activity_id, t, hr) VALUES (?1, ?2, ?3)",
                rusqlite::params![activity, t, hr],
            )
            .unwrap();
        }
    }

    #[test]
    fn a_ride_without_device_zones_counts_from_its_heart_rate() {
        let conn = db::test_db();
        // A device ride: its zones count, its stream does not count again.
        activity_sport(&conn, "garmin", "2026-11-01T09:00:00+03:00", "ride");
        zones_with_tops(&conn, "garmin", &TOPS, &[0.0, 0.0, 0.0, 3600.0, 0.0, 0.0, 0.0]);
        stream(&conn, "garmin", "2026-11-01T09:00:00+03:00", 150, 3600);
        // A Zwift ride: an hour at 150 is Z3 under the device's tops (0.8² → 64).
        activity_sport(&conn, "zwift", "2026-11-12T18:00:00+03:00", "virtual_ride");
        stream(&conn, "zwift", "2026-11-12T18:00:00+03:00", 150, 3600);
        conn.execute("INSERT INTO trackpoint (activity_id, t, hr) VALUES ('zwift', 'garbage', 150), ('zwift', NULL, 150)", [])
            .unwrap();
        // No boundaries of its own kind: any sport's are taken.
        activity_sport(&conn, "swim", "2026-11-13T08:00:00+03:00", "swim");
        stream(&conn, "swim", "2026-11-13T08:00:00+03:00", 170, 1800);
        // Before any boundaries, or at a time that does not read: not counted.
        activity_sport(&conn, "early", "2026-01-01T08:00:00+03:00", "ride");
        stream(&conn, "early", "2026-01-01T08:00:00+03:00", 150, 3600);
        activity_sport(&conn, "odd", "someday", "ride");
        stream(&conn, "odd", "2026-11-14T08:00:00+03:00", 150, 3600);

        let daily = daily_hrtss(&conn).unwrap();
        assert!((daily["2026-11-01"] - 64.0).abs() < 0.01, "counted once: {}", daily["2026-11-01"]);
        assert!((daily["2026-11-12"] - 64.0).abs() < 0.01, "{}", daily["2026-11-12"]);
        assert!((daily["2026-11-13"] - 0.95 * 0.95 * 50.0).abs() < 0.01, "{}", daily["2026-11-13"]);
        assert!(!daily.contains_key("2026-01-01"));
        assert!(!daily.contains_key("someday") && daily.len() == 3, "{daily:?}");
    }

    fn bounds(at: f64, sport: &str, first: f64) -> Bounds {
        Bounds { at, sport: sport.into(), high: vec![first, first + 20.0] }
    }

    #[test]
    fn the_boundaries_are_the_last_before_in_the_family_else_any() {
        let all = [bounds(10.0, "ride", 100.0), bounds(20.0, "run", 90.0), bounds(30.0, "strength", 80.0), bounds(40.0, "ride", 110.0)];
        assert_eq!(bounds_for(&all, "virtual_ride", 35.0), Some(&[100.0, 120.0][..]), "the family, though a run is later");
        assert_eq!(bounds_for(&all, "ride", 40.0), Some(&[110.0, 130.0][..]), "at the same time counts");
        assert_eq!(bounds_for(&all, "strength", 45.0), Some(&[80.0, 100.0][..]), "a sport alone: its own");
        assert_eq!(bounds_for(&all, "swim", 25.0), Some(&[90.0, 110.0][..]), "none of its kind: the last of any");
        assert_eq!(bounds_for(&all, "ride", 5.0), None, "nothing before");
    }

    #[test]
    fn only_whole_rising_boundary_sets_are_taken() {
        let conn = db::test_db();
        activity_sport(&conn, "good", "2026-11-01T09:00:00+03:00", "ride");
        zones_with_tops(&conn, "good", &TOPS, &[0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0]);
        // A lap row repeating a zone: one top per zone all the same.
        zones_with_tops(&conn, "good", &TOPS[..1], &[5.0]);
        activity_sport(&conn, "naive", "2026-10-01T09:00:00", "strength");
        zones_with_tops(&conn, "naive", &TOPS, &[1.0; 6]);
        activity_sport(&conn, "falling", "2026-11-02T09:00:00+03:00", "ride");
        zones_with_tops(&conn, "falling", &[100.0, 120.0, 140.0, 130.0, 180.0, 190.0], &[1.0; 6]);
        activity_sport(&conn, "level", "2026-11-02T10:00:00+03:00", "ride");
        zones_with_tops(&conn, "level", &[100.0, 120.0, 140.0, 140.0, 180.0, 190.0], &[1.0; 6]);
        // Five tops (Z1…Z5 only): every bucket would slide one down.
        activity_sport(&conn, "five", "2026-11-02T11:00:00+03:00", "ride");
        zones_with_tops(&conn, "five", &TOPS[..5], &[1.0; 5]);
        // Six tops but bucket 1 missing.
        activity_sport(&conn, "holed", "2026-11-03T09:00:00+03:00", "ride");
        zones_with_tops(&conn, "holed", &[100.0, 110.0, 120.0, 140.0, 160.0, 180.0, 190.0], &[1.0; 7]);
        conn.execute("DELETE FROM time_in_zone WHERE activity_id = 'holed' AND zone_index = 1", []).unwrap();
        activity_sport(&conn, "timeless", "someday", "ride");
        zones_with_tops(&conn, "timeless", &TOPS, &[1.0; 6]);

        let mut got: Vec<(String, Vec<f64>)> =
            recorded_bounds(&conn).unwrap().into_iter().map(|b| (b.sport, b.high)).collect();
        got.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(got, vec![("ride".to_string(), TOPS.to_vec()), ("strength".to_string(), TOPS.to_vec())]);
    }

    #[test]
    fn a_stream_s_seconds_go_to_the_zone_of_each_sample() {
        let stream = [
            (0.0, Some(150.0)), // Z3
            (1.0, Some(160.0)), // on a top: the zone above, Z4
            (2.0, None),        // no heart rate
            (3.0, Some(0.0)),   // none either
            (4.0, Some(200.0)), // above every top
            (5.0, Some(99.0)),  // 30 s to the next: still riding
            (35.0, Some(99.0)), // 31 s to the next: a pause, not counted
            (66.0, Some(150.0)),
            (67.0, Some(150.0)),
            (67.0, Some(150.0)), // the same instant twice
            (60.0, Some(150.0)), // back in time
        ];
        assert_eq!(zones_of(&stream, &TOPS, &[]), [30.0, 0.0, 0.0, 2.0, 1.0, 0.0, 1.0]);
    }

    #[test]
    fn a_span_another_recording_counts_is_left_out() {
        let stream: Vec<(f64, Option<f64>)> = (0..4).map(|t| (t as f64, Some(150.0))).collect();
        // [1, 2): the sample at 1 is counted elsewhere, the one at 2 is not.
        assert_eq!(zones_of(&stream, &TOPS, &[(1.0, 2.0)])[3], 2.0);
    }

    #[test]
    fn one_ride_recorded_twice_counts_once() {
        let conn = db::test_db();
        activity_sport(&conn, "zones", "2026-11-01T09:00:00+03:00", "ride");
        zones_with_tops(&conn, "zones", &TOPS, &[0.0, 0.0, 0.0, 60.0, 0.0, 0.0, 0.0]);
        // The bike computer: an hour in Z3 by its own zones (64).
        activity_sport(&conn, "edge", "2026-11-12T18:00:00+03:00", "ride");
        zones_with_tops(&conn, "edge", &TOPS, &[0.0, 0.0, 0.0, 3600.0, 0.0, 0.0, 0.0]);
        stream(&conn, "edge", "2026-11-12T18:00:00+03:00", 150, 3600);
        // Zwift over the same hour: nothing more.
        activity_sport(&conn, "zwift", "2026-11-12T17:59:50+03:00", "virtual_ride");
        stream(&conn, "zwift", "2026-11-12T17:59:50+03:00", 150, 3600);
        let day = daily_hrtss(&conn).unwrap()["2026-11-12"];
        assert!((day - 64.0 - hrtss(3, 10.0)).abs() < 0.01, "only Zwift's first 10 s: {day}");

        // Zwift for an hour, the watch only for its last 15 minutes (with
        // no samples stored: its span is its duration): 45 Zwift minutes.
        let conn = db::test_db();
        activity_sport(&conn, "zones", "2026-11-01T09:00:00+03:00", "ride");
        zones_with_tops(&conn, "zones", &TOPS, &[0.0, 0.0, 0.0, 60.0, 0.0, 0.0, 0.0]);
        activity_sport(&conn, "zwift", "2026-11-13T18:00:00+03:00", "virtual_ride");
        stream(&conn, "zwift", "2026-11-13T18:00:00+03:00", 150, 3600);
        conn.execute(
            "INSERT INTO activity (id, start_time, sport_type, duration_s) VALUES ('watch', '2026-11-13T18:45:00+03:00', 'ride', 900)",
            [],
        )
        .unwrap();
        zones_with_tops(&conn, "watch", &TOPS, &[0.0, 0.0, 0.0, 900.0, 0.0, 0.0, 0.0]);
        let day = daily_hrtss(&conn).unwrap()["2026-11-13"];
        assert!((day - hrtss(3, 900.0) - hrtss(3, 2700.0)).abs() < 0.01, "{day}");

        // Syzify and Zwift, both streams only, over one half hour: once;
        // a device ride whose start does not read counts its own minute
        // and hides nothing.
        let conn = db::test_db();
        activity_sport(&conn, "zones", "2026-11-01T09:00:00+03:00", "ride");
        zones_with_tops(&conn, "zones", &TOPS, &[0.0, 0.0, 0.0, 60.0, 0.0, 0.0, 0.0]);
        activity_sport(&conn, "syzify", "2026-11-14T18:00:00+03:00", "indoor_ride");
        stream(&conn, "syzify", "2026-11-14T18:00:00+03:00", 150, 1800);
        activity_sport(&conn, "zwift", "2026-11-14T18:00:05+03:00", "virtual_ride");
        stream(&conn, "zwift", "2026-11-14T18:00:05+03:00", 150, 1800);
        activity_sport(&conn, "unread", "2026-11-14Tjunk", "ride");
        zones_with_tops(&conn, "unread", &TOPS, &[0.0, 0.0, 0.0, 60.0, 0.0, 0.0, 0.0]);
        let day = daily_hrtss(&conn).unwrap()["2026-11-14"];
        assert!((day - hrtss(3, 1805.0) - hrtss(3, 60.0)).abs() < 0.01, "{day}");
    }

    #[test]
    fn device_zones_that_do_not_count_leave_the_stream_to_count() {
        let conn = db::test_db();
        activity_sport(&conn, "zones", "2026-11-01T09:00:00+03:00", "ride");
        zones_with_tops(&conn, "zones", &TOPS, &[0.0, 0.0, 0.0, 60.0, 0.0, 0.0, 0.0]);
        // Only zero rows, or only rows longer than the ride (corrupt): the
        // stream counts. A stream with no heart rate counts nothing.
        activity_sport(&conn, "zeros", "2026-11-10T09:00:00+03:00", "ride");
        zones_with_tops(&conn, "zeros", &[], &[0.0; 7]);
        stream(&conn, "zeros", "2026-11-10T09:00:00+03:00", 150, 3600);
        activity_sport(&conn, "corrupt", "2026-11-11T09:00:00+03:00", "ride");
        zones_with_tops(&conn, "corrupt", &[], &[0.0, 99_999.0]);
        stream(&conn, "corrupt", "2026-11-11T09:00:00+03:00", 150, 3600);
        activity_sport(&conn, "flat", "2026-11-12T09:00:00+03:00", "ride");
        stream(&conn, "flat", "2026-11-12T09:00:00+03:00", 0, 3600);
        // A stream wholly inside a device ride that began the day before
        // (across midnight): no seconds, so no day.
        activity_sport(&conn, "edge", "2026-11-20T23:30:00+03:00", "ride");
        zones_with_tops(&conn, "edge", &TOPS, &[0.0, 0.0, 0.0, 7200.0, 0.0, 0.0, 0.0]);
        stream(&conn, "edge", "2026-11-20T23:30:00+03:00", 150, 7200);
        activity_sport(&conn, "inside", "2026-11-21T00:00:00+03:00", "virtual_ride");
        stream(&conn, "inside", "2026-11-21T00:00:00+03:00", 150, 3600);
        // A stream whose times do not read: nothing to count.
        activity_sport(&conn, "timeless", "2026-11-22T09:00:00+03:00", "ride");
        conn.execute("INSERT INTO trackpoint (activity_id, t, hr) VALUES ('timeless', 'x', 150)", []).unwrap();
        let daily = daily_hrtss(&conn).unwrap();
        assert!((daily["2026-11-10"] - 64.0).abs() < 0.01, "{daily:?}");
        assert!((daily["2026-11-11"] - 64.0).abs() < 0.01, "{daily:?}");
        assert!(!daily.contains_key("2026-11-12"), "{daily:?}");
        assert!(!daily.contains_key("2026-11-21") && !daily.contains_key("2026-11-22"), "{daily:?}");
    }

    #[test]
    fn hr_zones_for_the_card_come_from_the_stream_or_not_at_all() {
        let conn = db::test_db();
        activity_sport(&conn, "zones", "2026-11-01T09:00:00+03:00", "ride");
        zones_with_tops(&conn, "zones", &TOPS, &[0.0, 0.0, 0.0, 60.0, 0.0, 0.0, 0.0]);
        // Times in seconds (only their differences count), as the detail
        // page has them; a sample with no time is left out, one out of
        // order is put in its place.
        let start = "2026-11-12T18:00:00+03:00";
        let t = [Some(100.0), Some(102.0), None, Some(101.0), Some(103.0)];
        let hr = [Some(150), Some(170), Some(150), Some(150), None];
        let zones = |sport: &str, duration: Option<f64>| {
            hr_zones_from_stream(&conn, "zwift", start, sport, duration, &t, &hr).unwrap()
        };
        let rows = zones("virtual_ride", Some(3.0));
        let seconds: Vec<f64> = rows.iter().map(|z| z.time_s).collect();
        assert_eq!(seconds, [0.0, 0.0, 0.0, 2.0, 1.0, 0.0, 0.0]);
        assert!(rows.iter().all(|z| z.activity_id == "zwift" && z.zone_type == "hr" && z.id.is_none()));
        let tops: Vec<Option<f64>> = rows.iter().map(|z| z.zone_high_boundary).collect();
        assert_eq!(tops[..6], TOPS.map(Some));
        assert_eq!(tops[6], None);
        assert_eq!(zones("run", None).len(), 7, "a run, no timer: as counted");
        // A swim, no tops before it, a start that does not read, no heart
        // rate: none.
        assert!(zones("swim", None).is_empty());
        let none = |start: &str, hr: &[Option<i32>]| {
            hr_zones_from_stream(&conn, "x", start, "ride", None, &t, hr).unwrap().is_empty()
        };
        assert!(none("2026-10-01T09:00:00+03:00", &hr));
        assert!(none("someday", &hr));
        assert!(none(start, &[None, Some(0), None, Some(0), None]));
        assert!(none(start, &[None, None, None, None, Some(150)]), "the last sample alone: no seconds");
    }

    /// Like Syzify's ride of 11.11: 1 Hz with a stop the timer left out.
    /// The heart rate held through the stop would run the zones past the
    /// timer, and the card drops a zone longer than the ride: scaled down.
    #[test]
    fn hr_zones_for_the_card_fit_the_timer() {
        let conn = db::test_db();
        activity_sport(&conn, "zones", "2026-11-01T09:00:00+03:00", "ride");
        zones_with_tops(&conn, "zones", &TOPS, &[0.0, 0.0, 0.0, 60.0, 0.0, 0.0, 0.0]);
        let mut t: Vec<Option<f64>> = (0..60).map(|s| Some(s as f64)).collect();
        t.extend((0..74).map(|s| Some(88.0 + s as f64)));
        let hr: Vec<Option<i32>> = (0..134).map(|i| Some(if i < 14 { 110 } else { 125 })).collect();
        let rows = hr_zones_from_stream(&conn, "syzify", "2026-11-11T19:08:57+03:00", "indoor_ride", Some(134.0), &t, &hr)
            .unwrap();
        let total: f64 = rows.iter().map(|z| z.time_s).sum();
        assert!((total - 134.0).abs() < 1e-9, "{total}");
        assert!(rows.iter().all(|z| z.time_s <= 134.0));
        assert!(rows[2].time_s > rows[1].time_s, "Z2 is the ride's: {rows:?}");

        // All in one zone, 43 s held over a 14 s timer: 43 × (14 / 43) is
        // 14.000000000000002 in floating point — never past the timer.
        let mut t: Vec<Option<f64>> = (0..=13).map(|s| Some(s as f64)).collect();
        t.push(Some(43.0));
        let hr = vec![Some(125); t.len()];
        let rows = hr_zones_from_stream(&conn, "short", "2026-11-11T19:08:57+03:00", "indoor_ride", Some(14.0), &t, &hr)
            .unwrap();
        assert_eq!(rows[2].time_s, 14.0, "{rows:?}");
        assert!(rows.iter().all(|z| z.time_s <= 14.0));
    }

    #[test]
    fn times_read_with_an_offset_or_naive() {
        assert_eq!(instant("2026-11-12T18:00:00+03:00"), Some(1_794_495_600.0));
        assert_eq!(instant("2026-11-12T15:00:00Z"), Some(1_794_495_600.0));
        assert_eq!(instant("2026-11-12T15:00:00"), Some(1_794_495_600.0));
        assert_eq!(instant("2014-05-20T04:53:35.500000000Z"), Some(1_400_561_615.5));
        assert_eq!(instant("someday"), None);
    }
}
