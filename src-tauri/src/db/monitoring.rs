//! Garmin monitoring storage (ADR 0002): the raw samples of a parsed
//! Monitor file, and the per-local-day aggregates computed from them.
//!
//! Aggregation rules that are NOT obvious (all learned from real files):
//! - activity counters are running day totals → MAX per (day, activity
//!   type), summed over types; a total stamped at local midnight closes
//!   the previous day (`parser::monitoring::local_day_of_total`);
//! - moderate/vigorous minutes → MAX of the running totals, and only
//!   without any total the SUM of the one-minute increments;
//! - a day's time zone is the offset its files were read under, recorded
//!   at store time so a later recompute uses the same clock; when a
//!   confirmed offset replaces a guess, the neighbouring days are marked
//!   stale too, or their windows would overlap the moved one for an hour;
//! - two devices (or a file without a serial) never add up: the day's
//!   counters are the MAX across devices within an activity type;
//! - `active_calories` / `active_time_s` are the day's WHOLE active energy
//!   and time — Garmin's `generic` bucket absorbs recorded workouts
//!   (a 5000 kcal ride day shows as generic 5305 kcal) — so the recovery
//!   index must never add activity calories on top;
//! - per-minute `intensity` marks are decoded but NOT stored in v1.

use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Result};

use crate::models::monitoring::MonitoringDay;
use crate::parser::monitoring::{
    local_day_of_sample, local_day_of_total, ParsedMonitoring, Sample,
};

const DAY: i64 = 86_400;
/// The night window, local hours.
const NIGHT_END_H: i64 = 7;
/// Column list of monitoring_day, the single source of truth for reads.
const DAY_COLUMNS: &str = "date, tz_offset_s, tz_confirmed, night_samples, night_hr_min, \
    night_hr_p10, night_hr_median, night_stress_avg, day_stress_avg, resp_night_avg, \
    spo2_night_avg, rhr_garmin, rhr_garmin_7d, steps, distance_m, active_calories, \
    active_time_s, moderate_min, vigorous_min, computed_at";

/// Readings stamped before this are a corrupt unroll, not data (Garmin's
/// epoch is 1989; the first wearable with these files is a 2010s device).
const MIN_TS: i64 = 946_684_800; // 2000-01-01T00:00:00Z

/// What a store touched: the local days (as days since the Unix epoch)
/// whose aggregates are now stale, and how many rows of the four sample
/// series were written (totals, minutes and RHR rows not counted).
#[derive(Debug, Default, PartialEq)]
pub struct Stored {
    pub days: BTreeSet<i64>,
    pub samples: usize,
}

/// Days since the Unix epoch → "YYYY-MM-DD".
pub fn date_of(day: i64) -> Option<String> {
    DateTime::<Utc>::from_timestamp(day * DAY, 0).map(|d| d.format("%Y-%m-%d").to_string())
}

/// "YYYY-MM-DD" → days since the Unix epoch.
pub fn day_of(date: &str) -> Option<i64> {
    let d = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    Some(d.and_hms_opt(0, 0, 0)?.and_utc().timestamp().div_euclid(DAY))
}

/// Store a parsed Monitor file. Idempotent: the same file twice changes
/// nothing; an overlapping later file overwrites per timestamp. The days
/// it touches get (or keep) a monitoring_day row carrying the offset the
/// file was read under — the aggregates are left to [`recompute_days`], so
/// a batch of files recomputes each day once.
pub fn store(
    conn: &Connection,
    parsed: &ParsedMonitoring,
    raw_file_id: Option<&str>,
) -> Result<Stored> {
    let tx = conn.unchecked_transaction()?;
    let serial = parsed.device_serial.clone().unwrap_or_default();
    let tz = parsed.tz_offset_s;
    let mut out = Stored::default();

    let mut samples = tx.prepare(
        "INSERT OR REPLACE INTO monitoring_sample
         (device_serial, kind, ts, value, confidence, raw_file_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )?;
    let series: [(&str, &Vec<Sample>); 4] = [
        ("hr", &parsed.hr),
        ("stress", &parsed.stress),
        ("respiration", &parsed.respiration),
        ("spo2", &parsed.spo2),
    ];
    for (kind, list) in series {
        for s in list.iter().filter(|s| s.ts >= MIN_TS) {
            samples.execute(params![serial, kind, s.ts, s.value, s.confidence, raw_file_id])?;
            out.days.insert(local_day_of_sample(s.ts, tz));
            out.samples += 1;
        }
    }
    drop(samples);

    let mut totals = tx.prepare(
        "INSERT OR REPLACE INTO monitoring_total
         (device_serial, activity_type, ts, steps, distance_m, active_calories, active_time_s,
          raw_file_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )?;
    for t in parsed.totals.iter().filter(|t| t.ts >= MIN_TS) {
        let ty = t.activity_type.clone().unwrap_or_else(|| "generic".to_string());
        totals.execute(params![
            serial,
            ty,
            t.ts,
            t.steps,
            t.distance_m,
            t.active_calories,
            t.active_time_s,
            raw_file_id
        ])?;
        out.days.insert(local_day_of_total(t.ts, tz));
    }
    drop(totals);

    let mut minutes = tx.prepare(
        "INSERT OR REPLACE INTO monitoring_active_minutes
         (device_serial, ts, moderate_total, vigorous_total, moderate_inc, vigorous_inc,
          raw_file_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )?;
    for m in parsed.active_minutes.iter().filter(|m| m.ts >= MIN_TS) {
        minutes.execute(params![
            serial,
            m.ts,
            m.moderate_total,
            m.vigorous_total,
            m.moderate_inc,
            m.vigorous_inc,
            raw_file_id
        ])?;
        out.days.insert(local_day_of_total(m.ts, tz));
    }
    drop(minutes);

    let mut rhr = tx.prepare(
        "INSERT OR REPLACE INTO monitoring_rhr (device_serial, ts, current_day, seven_day)
         VALUES (?1, ?2, ?3, ?4)",
    )?;
    for r in parsed.rhr.iter().filter(|r| r.ts >= MIN_TS) {
        rhr.execute(params![serial, r.ts, r.current_day, r.seven_day])?;
        out.days.insert(local_day_of_total(r.ts, tz));
    }
    drop(rhr);

    // The file's span, for delete_range (rows lose their raw_file_id to a
    // later overlapping file; the span does not).
    if let (Some(id), Some(first), Some(last)) = (raw_file_id, parsed.first_ts, parsed.last_ts) {
        // A span with a corrupt end is no span: recorded as none rather
        // than inverted, or the file could never be matched for deletion.
        if first >= MIN_TS && last >= first {
            tx.execute(
                "INSERT OR REPLACE INTO monitoring_raw_file (raw_file_id, first_ts, last_ts)
                 VALUES (?1, ?2, ?3)",
                params![id, first, last],
            )?;
        }
    }

    // The day row carries the clock the aggregates must use. A confirmed
    // offset beats an unconfirmed one already there; otherwise the first
    // import's offset stays (no flapping between files of one day). A day
    // whose clock moved drags its neighbours into the recompute: their
    // windows share the moved midnight.
    let mut day_row = tx.prepare(
        "INSERT INTO monitoring_day (date, tz_offset_s, tz_confirmed) VALUES (?1, ?2, ?3)
         ON CONFLICT(date) DO UPDATE SET
           tz_offset_s = CASE WHEN excluded.tz_confirmed = 1 AND tz_confirmed = 0
                              THEN excluded.tz_offset_s ELSE tz_offset_s END,
           tz_confirmed = MAX(tz_confirmed, excluded.tz_confirmed)",
    )?;
    let mut moved: Vec<i64> = Vec::new();
    for day in &out.days {
        let Some(date) = date_of(*day) else { continue };
        let before: Option<i32> = tx
            .query_row(
                "SELECT tz_offset_s FROM monitoring_day WHERE date = ?1",
                params![date],
                |r| r.get(0),
            )
            .optional()?;
        day_row.execute(params![date, tz, parsed.tz_confirmed as i64])?;
        let after: i32 = tx.query_row(
            "SELECT tz_offset_s FROM monitoring_day WHERE date = ?1",
            params![date],
            |r| r.get(0),
        )?;
        if before.is_some_and(|b| b != after) {
            moved.push(*day);
        }
    }
    drop(day_row);
    for day in moved {
        for neighbour in [day - 1, day + 1] {
            if let Some(date) = date_of(neighbour) {
                let exists: i64 = tx.query_row(
                    "SELECT COUNT(*) FROM monitoring_day WHERE date = ?1",
                    params![date],
                    |r| r.get(0),
                )?;
                if exists > 0 {
                    out.days.insert(neighbour);
                }
            }
        }
    }

    tx.commit()?;
    Ok(out)
}

/// Recompute the aggregates of the given local days (days since the Unix
/// epoch) from the stored samples, using each day's recorded offset.
/// Returns how many day rows were written.
pub fn recompute_days(conn: &Connection, days: &[i64]) -> Result<usize> {
    let mut written = 0;
    for day in days {
        let Some(date) = date_of(*day) else { continue };
        let tz: Option<i32> = conn
            .query_row(
                "SELECT tz_offset_s FROM monitoring_day WHERE date = ?1",
                params![date],
                |r| r.get(0),
            )
            .optional()?;
        let Some(tz) = tz else { continue };
        recompute_day(conn, *day, &date, tz)?;
        written += 1;
    }
    Ok(written)
}

/// Days whose aggregates were never written (`computed_at` IS NULL): an
/// import that stored samples but crashed before its batch recompute
/// leaves such rows, and the version flag alone would never pick them up.
pub fn pending_days(conn: &Connection) -> Result<Vec<i64>> {
    day_list(conn, "SELECT date FROM monitoring_day WHERE computed_at IS NULL")
}

/// Every day that has a row — for the formula-version recompute.
pub fn all_days(conn: &Connection) -> Result<Vec<i64>> {
    day_list(conn, "SELECT date FROM monitoring_day")
}

fn day_list(conn: &Connection, sql: &str) -> Result<Vec<i64>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    Ok(rows.filter_map(|r| r.ok().and_then(|d| day_of(&d))).collect())
}

/// Recompute the pending days — the startup safety net.
pub fn recompute_pending(conn: &Connection) -> Result<usize> {
    let days = pending_days(conn)?;
    recompute_days(conn, &days)
}

/// Recompute every day that has a row — the formula-version hook.
pub fn recompute_all(conn: &Connection) -> Result<usize> {
    let days = all_days(conn)?;
    recompute_days(conn, &days)
}

fn recompute_day(conn: &Connection, day: i64, date: &str, tz: i32) -> Result<()> {
    let midnight = day * DAY - i64::from(tz);
    let night_end = midnight + NIGHT_END_H * 3600;
    // The day ends where the NEXT day begins by ITS offset, so the windows
    // of two days tile even when the clock moved between them (an hour
    // shared by both windows would be counted twice).
    let next_tz: Option<i32> = date_of(day + 1)
        .map(|d| {
            conn.query_row(
                "SELECT tz_offset_s FROM monitoring_day WHERE date = ?1",
                params![d],
                |r| r.get(0),
            )
            .optional()
        })
        .transpose()?
        .flatten();
    let next_midnight = (day + 1) * DAY - i64::from(next_tz.unwrap_or(tz));
    // Totals stamped up to 3 min past midnight close the day before
    // (local_day_of_total): shift the window by that slack.
    let totals_from = midnight + 181;
    let totals_to = next_midnight + 181;

    let night_hr = sorted_values(conn, "hr", midnight, night_end)?;
    let n = night_hr.len();
    let (hr_min, hr_p10, hr_median) = if n == 0 {
        (None, None, None)
    } else {
        // Nearest-rank 10th percentile: for n < 10 it is the minimum.
        (Some(night_hr[0]), Some(night_hr[(n - 1) / 10]), Some(median(&night_hr)))
    };
    let night_stress = avg_value(conn, "stress", midnight, night_end)?;
    let day_stress = avg_value(conn, "stress", night_end, next_midnight)?;
    let night_resp = avg_value(conn, "respiration", midnight, night_end)?;
    let night_spo2 = avg_value(conn, "spo2", midnight, night_end)?;

    // Garmin's RHR estimate is a day-so-far figure like the totals — the
    // one written at the midnight cut is the previous day's.
    let rhr: Option<(Option<i64>, Option<i64>)> = conn
        .query_row(
            "SELECT current_day, seven_day FROM monitoring_rhr
             WHERE ts >= ?1 AND ts < ?2 ORDER BY ts DESC LIMIT 1",
            params![totals_from, totals_to],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let (rhr_day, rhr_7d) = rhr.unwrap_or((None, None));

    // MAX per activity type (across devices — a second watch or a file
    // without a serial must not add up), then summed over the types of the
    // day. Garmin's `generic` bucket carries no steps but the whole day's
    // active calories and time, recorded workouts included.
    let (steps, distance, calories, active_time): (
        Option<f64>,
        Option<f64>,
        Option<f64>,
        Option<f64>,
    ) = conn.query_row(
        "SELECT SUM(s), SUM(d), SUM(c), SUM(a) FROM (
           SELECT MAX(steps) AS s, MAX(distance_m) AS d, MAX(active_calories) AS c,
                  MAX(active_time_s) AS a
           FROM monitoring_total WHERE ts >= ?1 AND ts < ?2
           GROUP BY activity_type)",
        params![totals_from, totals_to],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )?;

    let (mod_total, vig_total, mod_inc, vig_inc): (
        Option<f64>,
        Option<f64>,
        Option<f64>,
        Option<f64>,
    ) = conn.query_row(
        "SELECT MAX(moderate_total), MAX(vigorous_total), SUM(moderate_inc), SUM(vigorous_inc)
         FROM monitoring_active_minutes WHERE ts >= ?1 AND ts < ?2",
        params![totals_from, totals_to],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )?;
    let moderate = mod_total.or(mod_inc);
    let vigorous = vig_total.or(vig_inc);

    conn.execute(
        "UPDATE monitoring_day SET
           night_samples = ?2, night_hr_min = ?3, night_hr_p10 = ?4, night_hr_median = ?5,
           night_stress_avg = ?6, day_stress_avg = ?7, resp_night_avg = ?8,
           spo2_night_avg = ?9, rhr_garmin = ?10, rhr_garmin_7d = ?11, steps = ?12,
           distance_m = ?13, active_calories = ?14, active_time_s = ?15, moderate_min = ?16,
           vigorous_min = ?17, computed_at = datetime('now')
         WHERE date = ?1",
        params![
            date,
            n as i64,
            hr_min,
            hr_p10,
            hr_median,
            night_stress,
            day_stress,
            night_resp,
            night_spo2,
            rhr_day,
            rhr_7d,
            steps,
            distance,
            calories,
            active_time,
            moderate,
            vigorous
        ],
    )?;
    Ok(())
}

fn sorted_values(conn: &Connection, kind: &str, from: i64, to: i64) -> Result<Vec<f64>> {
    let mut stmt = conn.prepare(
        "SELECT value FROM monitoring_sample WHERE kind = ?1 AND ts >= ?2 AND ts < ?3
         ORDER BY value",
    )?;
    let rows = stmt.query_map(params![kind, from, to], |r| r.get::<_, f64>(0))?;
    rows.collect()
}

fn avg_value(conn: &Connection, kind: &str, from: i64, to: i64) -> Result<Option<f64>> {
    conn.query_row(
        "SELECT AVG(value) FROM monitoring_sample WHERE kind = ?1 AND ts >= ?2 AND ts < ?3",
        params![kind, from, to],
        |r| r.get(0),
    )
}

fn median(sorted: &[f64]) -> f64 {
    let n = sorted.len();
    if n % 2 == 1 {
        sorted[n / 2]
    } else {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    }
}

/// Day rows in a local date range (inclusive), oldest first.
pub fn get_days(conn: &Connection, from: &str, to: &str) -> Result<Vec<MonitoringDay>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {DAY_COLUMNS} FROM monitoring_day WHERE date >= ?1 AND date <= ?2 ORDER BY date"
    ))?;
    let rows = stmt.query_map(params![from, to], row_to_day)?;
    rows.collect()
}

fn row_to_day(row: &rusqlite::Row) -> Result<MonitoringDay> {
    Ok(MonitoringDay {
        date: row.get(0)?,
        tz_offset_s: row.get(1)?,
        tz_confirmed: row.get::<_, i64>(2)? != 0,
        night_samples: row.get(3)?,
        night_hr_min: row.get(4)?,
        night_hr_p10: row.get(5)?,
        night_hr_median: row.get(6)?,
        night_stress_avg: row.get(7)?,
        day_stress_avg: row.get(8)?,
        resp_night_avg: row.get(9)?,
        spo2_night_avg: row.get(10)?,
        rhr_garmin: row.get(11)?,
        rhr_garmin_7d: row.get(12)?,
        steps: row.get(13)?,
        distance_m: row.get(14)?,
        active_calories: row.get(15)?,
        active_time_s: row.get(16)?,
        moderate_min: row.get(17)?,
        vigorous_min: row.get(18)?,
        computed_at: row.get(19)?,
    })
}

/// Delete every stored reading whose local day falls in the date range
/// (inclusive) and the day rows themselves. Returns the ids of the
/// monitoring raw files that touched the deleted window and now overlap NO
/// remaining day — the caller removes those files and their raw_file rows,
/// so the hashes stop blocking a re-import (stage 7 wires the UI). The
/// span table, not the per-row raw_file_id, decides: a file fully
/// overwritten by a later overlapping one has no rows left pointing at it.
/// A file that still covers a stored day is NOT named yet: it stays, its
/// readings in the range are gone, and its hash keeps a re-import of that
/// same file from restoring them — once its last day is deleted (in one
/// range or several) it is named.
pub fn delete_range(conn: &Connection, from: &str, to: &str) -> Result<Vec<String>> {
    let (Some(from_day), Some(to_day)) = (day_of(from), day_of(to)) else {
        return Ok(Vec::new());
    };
    if to_day < from_day {
        return Ok(Vec::new());
    }
    let tx = conn.unchecked_transaction()?;
    let mut raw_ids: BTreeSet<String> = BTreeSet::new();
    let mut tz_of = tx.prepare("SELECT tz_offset_s FROM monitoring_day WHERE date = ?1")?;
    // Files that touched the window, minus those a remaining day still
    // overlaps. A day (local midnight … next midnight, from its own offset)
    // overlaps a file that reaches more than the 3-minute closing slack
    // into it: a file cut at midnight ends with that day's closing rows
    // and does not belong to the next day.
    let mut files = tx.prepare(
        "SELECT f.raw_file_id FROM monitoring_raw_file f
         WHERE f.last_ts >= ?1 AND f.first_ts < ?2
           AND NOT EXISTS (
             SELECT 1 FROM monitoring_day d
             WHERE ((julianday(d.date) - 2440587.5) * 86400 - d.tz_offset_s) < f.last_ts - 181
               AND ((julianday(d.date) - 2440587.5) * 86400 - d.tz_offset_s + 86400)
                   > f.first_ts)",
    )?;
    let mut del_sample = tx.prepare("DELETE FROM monitoring_sample WHERE ts >= ?1 AND ts < ?2")?;
    let mut del_total = tx.prepare("DELETE FROM monitoring_total WHERE ts >= ?1 AND ts < ?2")?;
    let mut del_minutes =
        tx.prepare("DELETE FROM monitoring_active_minutes WHERE ts >= ?1 AND ts < ?2")?;
    let mut del_rhr = tx.prepare("DELETE FROM monitoring_rhr WHERE ts >= ?1 AND ts < ?2")?;
    let mut del_day = tx.prepare("DELETE FROM monitoring_day WHERE date = ?1")?;
    let mut range: Option<(i64, i64)> = None;
    for day in from_day..=to_day {
        let Some(date) = date_of(day) else { continue };
        let tz: Option<i32> = tz_of.query_row(params![date], |r| r.get(0)).optional()?;
        let Some(tz) = tz else { continue };
        let midnight = day * DAY - i64::from(tz);
        let next = midnight + DAY;
        range = Some(range.map_or((midnight, next), |(a, b)| (a.min(midnight), b.max(next))));
        del_sample.execute(params![midnight, next])?;
        del_total.execute(params![midnight + 181, next + 181])?;
        del_minutes.execute(params![midnight + 181, next + 181])?;
        del_rhr.execute(params![midnight + 181, next + 181])?;
        del_day.execute(params![date])?;
    }
    // Judged AFTER the day rows are gone, against the full deleted window
    // (the closing rows of its last day sit up to 3 min past midnight).
    if let Some((from_ts, to_ts)) = range {
        for id in files.query_map(params![from_ts, to_ts + 181], |r| r.get::<_, String>(0))? {
            raw_ids.insert(id?);
        }
    }
    drop((tz_of, files, del_sample, del_total, del_minutes, del_rhr, del_day));
    tx.commit()?;
    Ok(raw_ids.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::parser::monitoring::{ActiveMinutes, ActivityTotal, RhrReading};

    // 2026-09-04T21:00:00Z = local midnight of 2026-09-05 at +03:00.
    const MIDNIGHT: i64 = 1_788_555_600;
    const PLUS3: i32 = 3 * 3600;

    fn sample(ts: i64, value: f64) -> Sample {
        Sample { ts, value, confidence: None }
    }

    fn parsed() -> ParsedMonitoring {
        ParsedMonitoring {
            device_serial: Some("dev1".into()),
            device_product: Some("fenix6x".into()),
            tz_offset_s: PLUS3,
            tz_confirmed: true,
            first_ts: Some(MIDNIGHT),
            last_ts: Some(MIDNIGHT + DAY),
            hr: (0..10).map(|i| sample(MIDNIGHT + i * 600, 50.0 + i as f64)).collect(),
            stress: vec![
                sample(MIDNIGHT + 600, 10.0),
                sample(MIDNIGHT + 1200, 20.0),
                sample(MIDNIGHT + 8 * 3600, 40.0),
            ],
            respiration: vec![sample(MIDNIGHT + 300, 15.0), sample(MIDNIGHT + 900, 17.0)],
            spo2: vec![Sample { ts: MIDNIGHT + 300, value: 96.0, confidence: Some(12) }],
            rhr: vec![RhrReading {
                ts: MIDNIGHT + 10 * 3600,
                current_day: Some(49),
                seven_day: Some(62),
            }],
            totals: vec![
                ActivityTotal {
                    ts: MIDNIGHT + 8 * 3600,
                    activity_type: Some("walking".into()),
                    steps: Some(300.0),
                    distance_m: Some(250.0),
                    active_calories: Some(10.0),
                    active_time_s: Some(600.0),
                },
                ActivityTotal {
                    ts: MIDNIGHT + 20 * 3600,
                    activity_type: Some("walking".into()),
                    steps: Some(4000.0),
                    distance_m: Some(3000.0),
                    active_calories: Some(150.0),
                    active_time_s: Some(3600.0),
                },
                ActivityTotal {
                    ts: MIDNIGHT + 20 * 3600,
                    activity_type: Some("running".into()),
                    steps: Some(1000.0),
                    distance_m: Some(1200.0),
                    active_calories: Some(80.0),
                    active_time_s: Some(500.0),
                },
                // The closing total at next midnight belongs to THIS day.
                ActivityTotal {
                    ts: MIDNIGHT + DAY,
                    activity_type: Some("walking".into()),
                    steps: Some(4123.0),
                    distance_m: Some(3077.0),
                    active_calories: Some(160.0),
                    active_time_s: Some(3700.0),
                },
            ],
            intensity: Vec::new(),
            active_minutes: vec![
                ActiveMinutes {
                    ts: MIDNIGHT + 9 * 3600,
                    moderate_total: Some(2.0),
                    vigorous_total: Some(4.0),
                    moderate_inc: Some(1.0),
                    vigorous_inc: Some(3.0),
                },
                ActiveMinutes {
                    ts: MIDNIGHT + 10 * 3600,
                    moderate_total: Some(5.0),
                    vigorous_total: Some(4.0),
                    moderate_inc: Some(3.0),
                    vigorous_inc: None,
                },
            ],
        }
    }

    fn count(conn: &Connection, sql: &str) -> i64 {
        conn.query_row(sql, [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn dates_round_trip() {
        assert_eq!(date_of(0).as_deref(), Some("1970-01-01"));
        assert_eq!(day_of("2026-09-05"), Some(20_701));
        assert_eq!(date_of(20_701).as_deref(), Some("2026-09-05"));
        assert_eq!(day_of("nope"), None);
    }

    #[test]
    fn store_is_idempotent_and_reports_the_touched_days() {
        let conn = db::test_db();
        let first = store(&conn, &parsed(), None).unwrap();
        assert_eq!(first.samples, 10 + 3 + 2 + 1);
        // Sep 5 (samples, totals) and the closing total at next midnight is
        // still Sep 5; nothing lands on Sep 6.
        assert_eq!(first.days, BTreeSet::from([day_of("2026-09-05").unwrap()]));
        let second = store(&conn, &parsed(), None).unwrap();
        assert_eq!(second, first);
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM monitoring_sample"), 16);
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM monitoring_total"), 4);
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM monitoring_day"), 1);
    }

    #[test]
    fn a_later_overlapping_file_overwrites_per_timestamp() {
        let conn = db::test_db();
        store(&conn, &parsed(), None).unwrap();
        let mut later = parsed();
        later.hr = vec![sample(MIDNIGHT, 99.0)];
        store(&conn, &later, None).unwrap();
        let v: f64 = conn
            .query_row(
                "SELECT value FROM monitoring_sample WHERE kind='hr' AND ts=?1",
                params![MIDNIGHT],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(v, 99.0);
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM monitoring_sample WHERE kind='hr'"), 10);
    }

    #[test]
    fn recompute_aggregates_the_night_the_day_and_the_running_totals() {
        let conn = db::test_db();
        let stored = store(&conn, &parsed(), None).unwrap();
        let days: Vec<i64> = stored.days.iter().copied().collect();
        assert_eq!(recompute_days(&conn, &days).unwrap(), 1);
        let day = &get_days(&conn, "2026-09-05", "2026-09-05").unwrap()[0];
        assert_eq!(day.tz_offset_s, PLUS3);
        assert!(day.tz_confirmed);
        // Night = 00:00–07:00: HR samples at 0,10,…,90 min → all 10 (50…59).
        assert_eq!(day.night_samples, 10);
        assert_eq!(day.night_hr_min, Some(50.0));
        // Nearest-rank p10 of 10 samples is the first one.
        assert_eq!(day.night_hr_p10, Some(50.0));
        assert_eq!(day.night_hr_median, Some(54.5));
        assert_eq!(day.night_stress_avg, Some(15.0));
        assert_eq!(day.day_stress_avg, Some(40.0));
        assert_eq!(day.resp_night_avg, Some(16.0));
        assert_eq!(day.spo2_night_avg, Some(96.0));
        assert_eq!((day.rhr_garmin, day.rhr_garmin_7d), (Some(49), Some(62)));
        // Running totals: walking MAX 4123 (the closing total at midnight)
        // + running MAX 1000 — never the 300+4000+1000+4123 a sum would give.
        assert_eq!(day.steps, Some(5123.0));
        assert_eq!(day.distance_m, Some(3077.0 + 1200.0));
        assert_eq!(day.active_calories, Some(160.0 + 80.0));
        assert_eq!(day.active_time_s, Some(3700.0 + 500.0));
        // Minutes: MAX of the running totals, not the summed increments.
        assert_eq!((day.moderate_min, day.vigorous_min), (Some(5.0), Some(4.0)));
        assert!(day.computed_at.is_some());
    }

    #[test]
    fn minutes_fall_back_to_summed_increments_without_totals() {
        let conn = db::test_db();
        let mut p = parsed();
        p.active_minutes = vec![
            ActiveMinutes {
                ts: MIDNIGHT + 9 * 3600,
                moderate_inc: Some(1.0),
                vigorous_inc: Some(2.0),
                ..Default::default()
            },
            ActiveMinutes {
                ts: MIDNIGHT + 10 * 3600,
                moderate_inc: Some(1.0),
                ..Default::default()
            },
        ];
        let stored = store(&conn, &p, None).unwrap();
        recompute_days(&conn, &stored.days.iter().copied().collect::<Vec<_>>()).unwrap();
        let day = &get_days(&conn, "2026-09-05", "2026-09-05").unwrap()[0];
        assert_eq!((day.moderate_min, day.vigorous_min), (Some(2.0), Some(2.0)));
    }

    #[test]
    fn a_confirmed_offset_replaces_an_unconfirmed_one_but_not_the_reverse() {
        let conn = db::test_db();
        let mut guess = parsed();
        guess.tz_offset_s = 7200;
        guess.tz_confirmed = false;
        store(&conn, &guess, None).unwrap();
        // Under +02:00 the 00:00 +03:00 samples fall on Sep 4 23:00 → Sep 4
        // AND Sep 5 rows exist; both carry the guess.
        let days = get_days(&conn, "2026-09-04", "2026-09-05").unwrap();
        assert!(days.iter().all(|d| d.tz_offset_s == 7200 && !d.tz_confirmed));
        store(&conn, &parsed(), None).unwrap();
        let sep5 = &get_days(&conn, "2026-09-05", "2026-09-05").unwrap()[0];
        assert_eq!((sep5.tz_offset_s, sep5.tz_confirmed), (PLUS3, true));
        // A later unconfirmed guess does not flap it back.
        store(&conn, &guess, None).unwrap();
        let sep5 = &get_days(&conn, "2026-09-05", "2026-09-05").unwrap()[0];
        assert_eq!((sep5.tz_offset_s, sep5.tz_confirmed), (PLUS3, true));
    }

    #[test]
    fn a_second_device_never_adds_to_the_first() {
        let conn = db::test_db();
        store(&conn, &parsed(), None).unwrap();
        let mut other = parsed();
        other.device_serial = None; // a file without a serial → the "" bucket
        other.hr.clear();
        other.totals = vec![ActivityTotal {
            ts: MIDNIGHT + 21 * 3600,
            activity_type: Some("walking".into()),
            steps: Some(4100.0),
            distance_m: Some(3000.0),
            active_calories: Some(155.0),
            active_time_s: Some(3650.0),
        }];
        other.active_minutes = vec![ActiveMinutes {
            ts: MIDNIGHT + 11 * 3600,
            moderate_total: Some(6.0),
            ..Default::default()
        }];
        let stored = store(&conn, &other, None).unwrap();
        recompute_days(&conn, &stored.days.iter().copied().collect::<Vec<_>>()).unwrap();
        let day = &get_days(&conn, "2026-09-05", "2026-09-05").unwrap()[0];
        // walking = MAX over both devices (4123), not 4123 + 4100.
        assert_eq!(day.steps, Some(4123.0 + 1000.0));
        assert_eq!(day.moderate_min, Some(6.0));
    }

    #[test]
    fn generic_bucket_carries_the_whole_days_active_energy() {
        // A ride day: `generic` absorbs the workout's calories and time while
        // carrying no steps — the day sums generic + walking, and stage 5
        // must not add the activity's calories on top.
        let conn = db::test_db();
        let mut p = parsed();
        p.totals = vec![
            ActivityTotal {
                ts: MIDNIGHT + 20 * 3600,
                activity_type: Some("generic".into()),
                steps: Some(0.0),
                distance_m: Some(0.0),
                active_calories: Some(5305.0),
                active_time_s: Some(31_294.0),
            },
            ActivityTotal {
                ts: MIDNIGHT + 20 * 3600,
                activity_type: Some("walking".into()),
                steps: Some(3507.0),
                distance_m: Some(2800.0),
                active_calories: Some(324.0),
                active_time_s: Some(3507.0),
            },
        ];
        let stored = store(&conn, &p, None).unwrap();
        recompute_days(&conn, &stored.days.iter().copied().collect::<Vec<_>>()).unwrap();
        let day = &get_days(&conn, "2026-09-05", "2026-09-05").unwrap()[0];
        assert_eq!(day.steps, Some(3507.0));
        assert_eq!(day.active_calories, Some(5629.0));
        assert_eq!(day.active_time_s, Some(34_801.0));
    }

    #[test]
    fn a_moved_clock_marks_the_neighbours_stale_and_the_samples_change_day() {
        let conn = db::test_db();
        // First import under a +02:00 guess: the 00:00 +03:00 samples read
        // as 23:00 of Sep 4, so Sep 4 gets the night samples.
        let mut guess = parsed();
        guess.tz_offset_s = 7200;
        guess.tz_confirmed = false;
        let first = store(&conn, &guess, None).unwrap();
        recompute_days(&conn, &first.days.iter().copied().collect::<Vec<_>>()).unwrap();
        let sep4 = &get_days(&conn, "2026-09-04", "2026-09-04").unwrap()[0];
        assert_eq!(sep4.night_samples, 0);
        // Then the confirmed +03:00 file: Sep 5 moves its clock and Sep 4 is
        // reported stale too; after the recompute the night belongs to Sep 5.
        let second = store(&conn, &parsed(), None).unwrap();
        assert!(second.days.contains(&day_of("2026-09-04").unwrap()));
        recompute_days(&conn, &second.days.iter().copied().collect::<Vec<_>>()).unwrap();
        let days = get_days(&conn, "2026-09-04", "2026-09-05").unwrap();
        assert_eq!(days[0].night_samples, 0);
        assert_eq!(days[1].night_samples, 10);
        assert_eq!(days[1].tz_offset_s, PLUS3);
        // Sep 4 (still +02:00) now ends where Sep 5 begins by +03:00: the
        // stress samples of Sep 5's first hour are not Sep 4's evening too.
        assert_eq!(days[0].day_stress_avg, None);
        assert_eq!(days[1].night_stress_avg, Some(15.0));
    }

    #[test]
    fn recompute_pending_picks_up_days_never_aggregated() {
        let conn = db::test_db();
        store(&conn, &parsed(), None).unwrap();
        assert_eq!(recompute_pending(&conn).unwrap(), 1);
        // Nothing pending afterwards.
        assert_eq!(recompute_pending(&conn).unwrap(), 0);
    }

    #[test]
    fn recompute_all_covers_every_day_row_and_an_unknown_day_is_skipped() {
        let conn = db::test_db();
        let stored = store(&conn, &parsed(), None).unwrap();
        assert_eq!(recompute_all(&conn).unwrap(), stored.days.len());
        assert_eq!(recompute_days(&conn, &[day_of("1999-01-01").unwrap()]).unwrap(), 0);
        let day = &get_days(&conn, "2026-09-05", "2026-09-05").unwrap()[0];
        assert!(day.computed_at.is_some());
    }

    #[test]
    fn delete_range_removes_the_days_and_names_the_raw_files_to_drop() {
        let conn = db::test_db();
        // raw_file rows so the FK is satisfied.
        for id in ["rf-a", "rf-b"] {
            conn.execute(
                "INSERT INTO raw_file (id, path_in_vault, format, hash_sha256, kind)
                 VALUES (?1, ?1, 'fit', ?1, 'monitoring')",
                params![id],
            )
            .unwrap();
        }
        store(&conn, &parsed(), Some("rf-a")).unwrap();
        let mut next = parsed();
        next.hr = vec![sample(MIDNIGHT + DAY + 600, 55.0)];
        next.stress.clear();
        next.respiration.clear();
        next.spo2.clear();
        next.rhr.clear();
        next.totals.clear();
        next.active_minutes.clear();
        next.first_ts = Some(MIDNIGHT + DAY);
        next.last_ts = Some(MIDNIGHT + 2 * DAY);
        store(&conn, &next, Some("rf-b")).unwrap();
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM monitoring_day"), 2);

        let dropped = delete_range(&conn, "2026-09-05", "2026-09-05").unwrap();
        assert_eq!(dropped, vec!["rf-a".to_string()]);
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM monitoring_day"), 1);
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM monitoring_sample"), 1);
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM monitoring_total"), 0);
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM monitoring_rhr"), 0);
        // A range with no day rows is a no-op, and so is an inverted one.
        assert!(delete_range(&conn, "2020-01-01", "2020-01-02").unwrap().is_empty());
        assert!(delete_range(&conn, "2026-09-06", "2026-09-05").unwrap().is_empty());
    }

    #[test]
    fn delete_range_names_a_file_even_when_a_later_file_overwrote_all_its_rows() {
        let conn = db::test_db();
        for id in ["rf-daily", "rf-sync"] {
            conn.execute(
                "INSERT INTO raw_file (id, path_in_vault, format, hash_sha256, kind)
                 VALUES (?1, ?1, 'fit', ?1, 'monitoring')",
                params![id],
            )
            .unwrap();
        }
        let mut daily = parsed();
        daily.hr = vec![sample(MIDNIGHT + 600, 50.0)];
        daily.stress.clear();
        daily.respiration.clear();
        daily.spo2.clear();
        daily.totals.clear();
        daily.active_minutes.clear();
        daily.rhr.clear();
        daily.first_ts = Some(MIDNIGHT);
        daily.last_ts = Some(MIDNIGHT + 3600);
        store(&conn, &daily, Some("rf-daily")).unwrap();
        // The sync file covers the same reading and overwrites its row.
        let mut sync = daily.clone();
        sync.hr = vec![sample(MIDNIGHT + 600, 51.0)];
        sync.last_ts = Some(MIDNIGHT + 7200);
        store(&conn, &sync, Some("rf-sync")).unwrap();
        let referenced: String = conn
            .query_row("SELECT raw_file_id FROM monitoring_sample", [], |r| r.get(0))
            .unwrap();
        assert_eq!(referenced, "rf-sync");
        let dropped = delete_range(&conn, "2026-09-05", "2026-09-05").unwrap();
        assert_eq!(dropped, vec!["rf-daily".to_string(), "rf-sync".to_string()]);
    }

    #[test]
    fn delete_range_keeps_a_file_that_also_covers_days_outside_the_range() {
        let conn = db::test_db();
        conn.execute(
            "INSERT INTO raw_file (id, path_in_vault, format, hash_sha256, kind)
             VALUES ('rf-3d', 'rf-3d', 'fit', 'rf-3d', 'monitoring')",
            [],
        )
        .unwrap();
        let mut three_days = parsed();
        three_days.hr = vec![sample(MIDNIGHT + 600, 50.0), sample(MIDNIGHT + 2 * DAY + 600, 52.0)];
        three_days.first_ts = Some(MIDNIGHT);
        three_days.last_ts = Some(MIDNIGHT + 2 * DAY + 600);
        store(&conn, &three_days, Some("rf-3d")).unwrap();
        // Deleting one of its days must not name the file (Sep 7 remains)…
        assert!(delete_range(&conn, "2026-09-05", "2026-09-05").unwrap().is_empty());
        assert!(delete_range(&conn, "2026-09-06", "2026-09-06").unwrap().is_empty());
        // …deleting its last stored day does, even day by day.
        assert_eq!(
            delete_range(&conn, "2026-09-07", "2026-09-07").unwrap(),
            vec!["rf-3d".to_string()]
        );
    }

    #[test]
    fn a_corrupt_span_end_records_no_span() {
        let conn = db::test_db();
        conn.execute(
            "INSERT INTO raw_file (id, path_in_vault, format, hash_sha256, kind)
             VALUES ('rf-bad', 'rf-bad', 'fit', 'rf-bad', 'monitoring')",
            [],
        )
        .unwrap();
        let mut p = parsed();
        p.last_ts = Some(12_345);
        store(&conn, &p, Some("rf-bad")).unwrap();
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM monitoring_raw_file"), 0);
    }

    #[test]
    fn corrupt_timestamps_are_not_stored() {
        let conn = db::test_db();
        let mut p = parsed();
        p.hr.push(sample(12_345, 60.0)); // 1970 — a broken unroll
        let stored = store(&conn, &p, None).unwrap();
        assert_eq!(stored.samples, 16);
        assert!(get_days(&conn, "1970-01-01", "1970-01-02").unwrap().is_empty());
    }

    #[test]
    fn p10_is_the_minimum_for_fewer_than_ten_samples_and_nearest_rank_beyond() {
        let conn = db::test_db();
        let mut p = parsed();
        p.hr = (0..9).map(|i| sample(MIDNIGHT + i * 600, 60.0 + i as f64)).collect();
        let stored = store(&conn, &p, None).unwrap();
        recompute_days(&conn, &stored.days.iter().copied().collect::<Vec<_>>()).unwrap();
        let day = &get_days(&conn, "2026-09-05", "2026-09-05").unwrap()[0];
        assert_eq!(day.night_hr_p10, Some(60.0));
        // Ten samples → still the first (nearest rank 1 of 10); twenty → the second.
        let mut p = parsed();
        p.hr = (0..20).map(|i| sample(MIDNIGHT + i * 600, 60.0 + i as f64)).collect();
        let stored = store(&conn, &p, None).unwrap();
        recompute_days(&conn, &stored.days.iter().copied().collect::<Vec<_>>()).unwrap();
        let day = &get_days(&conn, "2026-09-05", "2026-09-05").unwrap()[0];
        assert_eq!(day.night_hr_p10, Some(61.0));
    }

    #[test]
    fn the_night_ends_at_seven_sharp() {
        let conn = db::test_db();
        let mut p = parsed();
        // 06:59:59 is night, 07:00:00 is day.
        p.hr = vec![sample(MIDNIGHT + 7 * 3600 - 1, 70.0), sample(MIDNIGHT + 7 * 3600, 90.0)];
        let stored = store(&conn, &p, None).unwrap();
        recompute_days(&conn, &stored.days.iter().copied().collect::<Vec<_>>()).unwrap();
        let day = &get_days(&conn, "2026-09-05", "2026-09-05").unwrap()[0];
        assert_eq!((day.night_samples, day.night_hr_median), (1, Some(70.0)));
    }

    #[test]
    fn a_day_without_a_night_still_aggregates_the_rest() {
        let conn = db::test_db();
        let mut off = parsed();
        off.hr = vec![sample(MIDNIGHT + 12 * 3600, 80.0)];
        off.stress = vec![sample(MIDNIGHT + 12 * 3600, 30.0)];
        off.respiration.clear();
        off.spo2.clear();
        let stored = store(&conn, &off, None).unwrap();
        recompute_days(&conn, &stored.days.iter().copied().collect::<Vec<_>>()).unwrap();
        let day = &get_days(&conn, "2026-09-05", "2026-09-05").unwrap()[0];
        assert_eq!(day.night_samples, 0);
        assert_eq!(day.night_hr_median, None);
        assert_eq!(day.day_stress_avg, Some(30.0));
        assert_eq!(day.steps, Some(5123.0));
    }

    #[test]
    fn raw_file_kind_defaults_to_activity_and_survives_activity_deletion() {
        let conn = db::test_db();
        conn.execute(
            "INSERT INTO raw_file (id, path_in_vault, format, hash_sha256)
             VALUES ('rf-old', 'raw/x.fit', 'fit', 'h1')",
            [],
        )
        .unwrap();
        let kind: String = conn
            .query_row("SELECT kind FROM raw_file WHERE id='rf-old'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(kind, "activity");
        conn.execute(
            "INSERT INTO raw_file (id, path_in_vault, format, hash_sha256, kind)
             VALUES ('rf-mon', 'raw/m.fit', 'fit', 'h2', 'monitoring')",
            [],
        )
        .unwrap();
        // delete_for_activity scopes by activity_id — a monitoring file has none.
        db::raw_files::delete_for_activity(&conn, "any-activity").unwrap();
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM raw_file WHERE kind='monitoring'"), 1);
    }
}
