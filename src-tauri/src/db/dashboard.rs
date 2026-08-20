use std::collections::HashMap;

use rusqlite::{params, params_from_iter, Connection, Result};

use crate::models::dashboard::{
    DashboardData, DistancePb, PersonalRecord, Records, SportBucket, SportEntry, SportRecords,
    SportShare, VolumeBucket, WeekTotals,
};

/// Ultramarathon threshold (m) — runs at least this long count as an ultra.
const ULTRA_MIN_M: f64 = 50000.0;

/// How close a GPS-less race's summary distance must be to a standard distance
/// to count as a record for it (metres). Keeps a long run from masquerading as a
/// short-distance PB while still catching races logged without a track.
const DISTANCE_PB_TOLERANCE_M: f64 = 500.0;
/// How far UNDER the target a GPS-less race may be logged and still count —
/// RELATIVE (2%), so a rounded certified result qualifies (a "42.10 km"
/// marathon is 0.2% short → in) while a genuinely shorter run does not (a
/// 4.6 km parkrun is 8% short of 5 km → out). An absolute floor here would
/// either reject the marathon (at 50 m) or admit the parkrun (at 500 m).
const DISTANCE_PB_UNDER_FRACTION: f64 = 0.02;

/// Most sports shown in the "By sport" donut.
const SPORT_DONUT_MAX: usize = 5;

/// Whole-percent shares of `counts` that sum to exactly 100 (largest-remainder
/// method), so rounded slices never add up to 99 or 101.
fn integer_shares(counts: &[i64]) -> Vec<i64> {
    let total: i64 = counts.iter().sum();
    if total <= 0 {
        return vec![0; counts.len()];
    }
    let raw: Vec<f64> = counts
        .iter()
        .map(|&c| c as f64 / total as f64 * 100.0)
        .collect();
    let mut shares: Vec<i64> = raw.iter().map(|r| r.floor() as i64).collect();
    let mut left = 100 - shares.iter().sum::<i64>();
    // Hand the leftover points to the largest fractional parts first.
    let mut order: Vec<usize> = (0..raw.len()).collect();
    order.sort_by(|&a, &b| {
        let fa = raw[a] - raw[a].floor();
        let fb = raw[b] - raw[b].floor();
        fb.partial_cmp(&fa).unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut k = 0;
    while left > 0 && k < order.len() {
        shares[order[k]] += 1;
        left -= 1;
        k += 1;
    }
    shares
}

/// The last-7-days sport split: the busiest sports (capped at `SPORT_DONUT_MAX`)
/// with integer shares summing to 100. Aggregated from the same `week_volume`
/// buckets the Volume chart uses, so the windows match exactly.
fn week_sport_distribution(week_volume: &[VolumeBucket]) -> Vec<SportShare> {
    let mut counts: HashMap<String, i64> = HashMap::new();
    for b in week_volume {
        for (sport, sb) in &b.by_sport {
            *counts.entry(sport.clone()).or_insert(0) += sb.activities;
        }
    }
    let mut ranked: Vec<(String, i64)> = counts.into_iter().filter(|(_, n)| *n > 0).collect();
    // Busiest first; ties broken by sport name for a stable order.
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    ranked.truncate(SPORT_DONUT_MAX);

    let shares = integer_shares(&ranked.iter().map(|(_, n)| *n).collect::<Vec<_>>());
    ranked
        .into_iter()
        .zip(shares)
        .map(|((sport_type, activities), share_pct)| SportShare {
            sport_type,
            activities,
            share_pct,
        })
        .collect()
}

fn is_running(sport: &str) -> bool {
    // One source of truth with the best-effort engine, which computes the
    // pace-based distance PBs these records display.
    crate::db::best_efforts::RUNNING_SPORTS.contains(&sport)
}

/// Water sports: any recorded "elevation gain" is GPS/pressure noise from the
/// watch losing fix in the water, not an achievement to rank.
fn is_water(sport: &str) -> bool {
    matches!(sport, "swim" | "open_water")
}

/// Running records, longest first: an "Ultra" row for the longest run beyond a
/// marathon (full distance + time), then best-effort splits for marathon /
/// half / 10 km / 5 km (fastest time to cover that distance within any run).
fn distance_pbs(conn: &Connection, sport: &str) -> Result<Vec<DistancePb>> {
    let mut out = Vec::new();

    // Ultra = the longest single run (≥ 50 km), shown as distance + total time.
    {
        let mut stmt = conn.prepare(
            "SELECT id, title, date(start_time), distance_m, duration_s
             FROM activity
             WHERE sport_type = ?1 AND distance_m >= ?2 AND duration_s IS NOT NULL
                   AND duration_s > 0
             ORDER BY distance_m DESC LIMIT 1",
        )?;
        let mut rows = stmt.query(params![sport, ULTRA_MIN_M])?;
        if let Some(row) = rows.next()? {
            out.push(DistancePb {
                label: "Ultra".to_string(),
                activity_id: row.get(0)?,
                title: row.get(1)?,
                date: row.get(2)?,
                duration_s: row.get(4)?,
                distance_m: row.get(3)?,
            });
        }
    }

    // Standard distances: the fastest time to cover that distance (longest
    // first). Normally this is a best-effort split from a GPS track, but a race
    // logged as a summary only (no track → no split) can still set the record
    // from its total distance/time when that distance is essentially the bucket.
    for (label, dist) in [
        ("Marathon", 42195.0),
        ("Half marathon", 21097.0),
        ("10 km", 10000.0),
        ("5 km", 5000.0),
    ] {
        // Candidate from a GPS best-effort split (covers exactly `dist`).
        let mut best: Option<(f64, String, Option<String>, String, f64)> =
            crate::db::best_efforts::fastest_for_distance(conn, sport, dist)?
                .map(|(id, title, date, dur)| (dur, id, title, date, dist));

        // Candidate from a GPS-less race whose summary distance ≈ `dist`; wins
        // only if it is faster than the best split (a tie keeps the split).
        if let Some((id, title, date, adist, dur)) = whole_activity_pb(conn, sport, dist)? {
            if best.as_ref().is_none_or(|b| dur < b.0) {
                best = Some((dur, id, title, date, adist));
            }
        }

        if let Some((duration_s, activity_id, title, date, distance_m)) = best {
            out.push(DistancePb {
                label: label.to_string(),
                activity_id,
                title,
                date,
                duration_s,
                distance_m,
            });
        }
    }

    Ok(out)
}

/// Fastest full-activity time whose total distance falls in [lower, upper].
/// Returns (id, title, date, distance_m, duration_s) of the fastest qualifier.
#[allow(clippy::type_complexity)]
fn fastest_in_distance_band(
    conn: &Connection,
    sport: &str,
    lower: f64,
    upper: f64,
) -> Result<Option<(String, Option<String>, String, f64, f64)>> {
    let mut stmt = conn.prepare(
        "SELECT id, title, date(start_time), distance_m, duration_s
         FROM activity
         WHERE sport_type = ?1 AND distance_m IS NOT NULL
               AND duration_s IS NOT NULL AND duration_s > 0
               AND distance_m >= ?2 AND distance_m <= ?3
         ORDER BY duration_s ASC LIMIT 1",
    )?;
    let mut rows = stmt.query(params![sport, lower, upper])?;
    match rows.next()? {
        Some(row) => Ok(Some((
            row.get(0)?,
            row.get(1)?,
            row.get(2)?,
            row.get(3)?,
            row.get(4)?,
        ))),
        None => Ok(None),
    }
}

/// Fastest full-activity time for a race at (about) `target` metres. A race
/// logged without a GPS track has no best-effort split, so its summary
/// distance/time stand in — but the total distance must be from `target − 2%`
/// up to `target + 500 m`. The window is ASYMMETRIC on purpose: a run genuinely
/// SHORTER than the distance can't hold that distance's record (a 4.6 km run's
/// time is not a 5 km PB), while a rounded-or-slightly-longer race still counts.
#[allow(clippy::type_complexity)]
fn whole_activity_pb(
    conn: &Connection,
    sport: &str,
    target: f64,
) -> Result<Option<(String, Option<String>, String, f64, f64)>> {
    fastest_in_distance_band(
        conn,
        sport,
        target * (1.0 - DISTANCE_PB_UNDER_FRACTION),
        target + DISTANCE_PB_TOLERANCE_M,
    )
}

/// Triathlon race classes: label + nominal total distance. The PB bands are
/// WIDE (±20%) unlike running's — real courses drift a lot around the nominal
/// (a "sprint" spans ~20–30 km total), and the classes are far enough apart
/// that ±20% bands can't overlap.
const TRIATHLON_CLASSES: [(&str, f64); 4] = [
    ("Full distance", 226_000.0),
    ("Half distance", 113_000.0),
    ("Olympic", 51_500.0),
    ("Sprint", 25_750.0),
];
const TRIATHLON_BAND_FRACTION: f64 = 0.2;

/// Triathlon PBs: the fastest race in each distance class (longest first).
fn triathlon_pbs(conn: &Connection) -> Result<Vec<DistancePb>> {
    let mut out = Vec::new();
    for (label, target) in TRIATHLON_CLASSES {
        let lower = target * (1.0 - TRIATHLON_BAND_FRACTION);
        let upper = target * (1.0 + TRIATHLON_BAND_FRACTION);
        if let Some((id, title, date, dist, dur)) =
            fastest_in_distance_band(conn, "triathlon", lower, upper)?
        {
            out.push(DistancePb {
                label: label.to_string(),
                activity_id: id,
                title,
                date,
                duration_s: dur,
                distance_m: dist,
            });
        }
    }
    Ok(out)
}

/// Convert a period string to a SQL datetime cutoff
fn period_to_cutoff(period: &str) -> Option<String> {
    match period {
        "1w" => Some("datetime('now', '-7 days')".to_string()),
        "4w" => Some("datetime('now', '-28 days')".to_string()),
        "3m" => Some("datetime('now', '-3 months')".to_string()),
        "12m" => Some("datetime('now', '-12 months')".to_string()),
        "all" => None,
        _ => None,
    }
}

pub fn get_dashboard_data(conn: &Connection, period: &str, sport_type: Option<&str>) -> Result<DashboardData> {
    let cutoff = period_to_cutoff(period);
    // `cutoff` is a closed-set SQL expression (never user input), so it is
    // embedded directly; `sport_type` is user-controlled (reachable from
    // plugins via host_query) and is always bound as a parameter.
    let mut conditions: Vec<String> = Vec::new();
    // Merged legs never count in dashboard aggregates (see push_facet_conditions).
    conditions.push("parent_id IS NULL".to_string());
    if let Some(expr) = &cutoff {
        conditions.push(format!("start_time >= {}", expr));
    }
    let mut main_params: Vec<String> = Vec::new();
    if let Some(sport) = sport_type {
        main_params.push(sport.to_string());
        conditions.push(format!("sport_type = ?{}", main_params.len()));
    }
    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    // Query 1: Summary totals
    let summary_sql = format!(
        "SELECT COUNT(*), COALESCE(SUM(distance_m),0), COALESCE(SUM(duration_s),0),
                COALESCE(SUM(elev_gain_m),0), AVG(CASE WHEN avg_hr IS NOT NULL THEN avg_hr END)
         FROM activity {}",
        where_clause
    );
    let (total_activities, total_distance_m, total_duration_s, total_elev_gain_m, avg_hr) = conn
        .query_row(&summary_sql, params_from_iter(&main_params), |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, f64>(1)?,
                row.get::<_, f64>(2)?,
                row.get::<_, f64>(3)?,
                row.get::<_, Option<f64>>(4)?,
            ))
        })?;

    // Week totals — drives the "THIS WEEK" tiles, independent of the selected
    // period. The window is the current CALENDAR week from Monday 00:00 local
    // (`weekday 0` advances to the next Sunday incl. today; −6 days lands on
    // that week's Monday), not a rolling 7 days — the tiles say "this week".
    // Respects the sport filter when one is set.
    // Compare on the LOCAL date so an offset/Z-stored timestamp near the Monday
    // boundary lands in the same week as the volume chart (which groups by
    // date(start_time,'localtime')). A raw string compare of start_time would
    // put a Monday-01:00+03:00 run (stored '…T22:00:00Z') on the wrong side.
    let mut week_conditions = vec![
        "date(start_time, 'localtime') >= date('now', 'localtime', 'weekday 0', '-6 days')"
            .to_string(),
        "parent_id IS NULL".to_string(),
    ];
    let mut week_params: Vec<String> = Vec::new();
    if let Some(sport) = sport_type {
        week_params.push(sport.to_string());
        week_conditions.push(format!("sport_type = ?{}", week_params.len()));
    }
    let week_sql = format!(
        "SELECT COUNT(*), COALESCE(SUM(distance_m),0), COALESCE(SUM(duration_s),0),
                COALESCE(SUM(elev_gain_m),0),
                AVG(CASE WHEN avg_hr IS NOT NULL THEN avg_hr END)
         FROM activity WHERE {}",
        week_conditions.join(" AND ")
    );
    let week = conn.query_row(&week_sql, params_from_iter(&week_params), |row| {
        Ok(WeekTotals {
            activities: row.get(0)?,
            distance_m: row.get(1)?,
            duration_s: row.get(2)?,
            elev_gain_m: row.get(3)?,
            avg_hr: row.get(4)?,
        })
    })?;

    // Daily volume for the last 7 days (by sport) — the dashboard's 7-bar chart.
    // Both the window and the day bucket use date(start_time,'localtime') so the
    // chart, its window, and the THIS-WEEK tiles all key on the SAME local date
    // and never contradict each other; the frontend then picks its 7 local keys.
    // (Caveat: timestamps are a mix — FIT/GPX carry an offset and localize
    // correctly, while a naive Runkeeper import has none and 'localtime' treats
    // it as UTC. That's a storage-normalization gap, not a per-view disagreement.)
    let mut wv_conditions =
        vec!["date(start_time, 'localtime') >= date('now', 'localtime', '-6 days')".to_string()];
    wv_conditions.push("parent_id IS NULL".to_string());
    let mut wv_params: Vec<String> = Vec::new();
    if let Some(sport) = sport_type {
        wv_params.push(sport.to_string());
        wv_conditions.push(format!("sport_type = ?{}", wv_params.len()));
    }
    let wv_sql = format!(
        "SELECT date(start_time, 'localtime') as day, sport_type,
                COALESCE(SUM(distance_m),0), COALESCE(SUM(duration_s),0), COUNT(*)
         FROM activity WHERE {}
         GROUP BY day, sport_type
         ORDER BY day ASC",
        wv_conditions.join(" AND ")
    );
    let mut wv_stmt = conn.prepare(&wv_sql)?;
    let wv_rows = wv_stmt.query_map(params_from_iter(&wv_params), |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, f64>(2)?,
            row.get::<_, f64>(3)?,
            row.get::<_, i64>(4)?,
        ))
    })?;
    let mut wv_map: HashMap<String, VolumeBucket> = HashMap::new();
    let mut wv_order: Vec<String> = Vec::new();
    for row in wv_rows {
        let (day, sport_type, distance_m, duration_s, activities) = row?;
        let entry = wv_map.entry(day.clone()).or_insert_with(|| {
            wv_order.push(day.clone());
            VolumeBucket {
                label: day.clone(),
                start_date: day,
                distance_m: 0.0,
                duration_s: 0.0,
                activities: 0,
                by_sport: HashMap::new(),
            }
        });
        entry.distance_m += distance_m;
        entry.duration_s += duration_s;
        entry.activities += activities;
        entry.by_sport.insert(
            sport_type,
            SportBucket {
                distance_m,
                duration_s,
                activities,
            },
        );
    }
    let week_volume: Vec<VolumeBucket> = wv_order
        .into_iter()
        .filter_map(|k| wv_map.remove(&k))
        .collect();

    // Query 2: Volume buckets (weekly)
    let volume_sql = format!(
        "SELECT strftime('%Y-W%W', start_time) as bucket,
                MIN(date(start_time, 'weekday 1', '-6 days')) as start_date,
                sport_type,
                COALESCE(SUM(distance_m),0),
                COALESCE(SUM(duration_s),0),
                COUNT(*)
         FROM activity {}
         GROUP BY bucket, sport_type
         ORDER BY bucket ASC",
        where_clause
    );
    let mut stmt = conn.prepare(&volume_sql)?;
    let rows = stmt.query_map(params_from_iter(&main_params), |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, f64>(3)?,
            row.get::<_, f64>(4)?,
            row.get::<_, i64>(5)?,
        ))
    })?;

    // Group by bucket
    let mut bucket_map: HashMap<String, VolumeBucket> = HashMap::new();
    let mut bucket_order: Vec<String> = Vec::new();
    for row in rows {
        let (bucket, start_date, sport_type, distance_m, duration_s, activities) = row?;
        let entry = bucket_map.entry(bucket.clone()).or_insert_with(|| {
            bucket_order.push(bucket.clone());
            VolumeBucket {
                label: bucket,
                start_date: start_date.clone(),
                distance_m: 0.0,
                duration_s: 0.0,
                activities: 0,
                by_sport: HashMap::new(),
            }
        });
        entry.distance_m += distance_m;
        entry.duration_s += duration_s;
        entry.activities += activities;
        entry.by_sport.insert(
            sport_type,
            SportBucket {
                distance_m,
                duration_s,
                activities,
            },
        );
    }
    let volume_buckets: Vec<VolumeBucket> = bucket_order
        .into_iter()
        .filter_map(|k| bucket_map.remove(&k))
        .collect();

    // Query 3: Sport distribution
    let sport_sql = format!(
        "SELECT sport_type, COUNT(*), COALESCE(SUM(distance_m),0), COALESCE(SUM(duration_s),0)
         FROM activity {}
         GROUP BY sport_type
         ORDER BY COUNT(*) DESC",
        where_clause
    );
    let mut stmt = conn.prepare(&sport_sql)?;
    let sport_rows = stmt.query_map(params_from_iter(&main_params), |row| {
        Ok(SportEntry {
            sport_type: row.get(0)?,
            activities: row.get(1)?,
            distance_m: row.get(2)?,
            duration_s: row.get(3)?,
        })
    })?;
    let mut sport_distribution = Vec::new();
    for row in sport_rows {
        sport_distribution.push(row?);
    }

    // Query 4: Personal records — all-time, grouped by sport, for the 5
    // most-frequent sports present in the library (best ever per sport).
    let top_sports: Vec<(String, i64)> = {
        let mut stmt = conn.prepare(
            "SELECT sport_type, COUNT(*) FROM activity
             GROUP BY sport_type ORDER BY COUNT(*) DESC LIMIT 5",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
        let mut v = Vec::new();
        for r in rows {
            v.push(r?);
        }
        v
    };
    let mut records_by_sport = Vec::new();
    for (sport, count) in top_sports {
        let records = Records {
            longest_distance: get_record(conn, &sport, "distance_m")?,
            longest_duration: get_record(conn, &sport, "duration_s")?,
            highest_elevation: if is_water(&sport) {
                None
            } else {
                get_record(conn, &sport, "elev_gain_m")?
            },
            fastest_speed: get_record(conn, &sport, "avg_speed_mps")?,
            heaviest_set: get_heaviest_set(conn, &sport)?,
        };
        let pbs = if is_running(&sport) {
            distance_pbs(conn, &sport)?
        } else if sport == "triathlon" {
            triathlon_pbs(conn)?
        } else {
            Vec::new()
        };
        records_by_sport.push(SportRecords {
            sport_type: sport,
            activity_count: count,
            records,
            distance_pbs: pbs,
        });
    }

    let week_sport_distribution = week_sport_distribution(&week_volume);

    Ok(DashboardData {
        total_activities,
        total_distance_m,
        total_duration_s,
        total_elev_gain_m,
        avg_hr,
        week,
        week_volume,
        volume_buckets,
        sport_distribution,
        week_sport_distribution,
        records_by_sport,
    })
}

/// Heaviest single weighted set for a sport (e.g. strength), with the activity
/// it belongs to. Returns None when no weight data exists.
fn get_heaviest_set(conn: &Connection, sport: &str) -> Result<Option<PersonalRecord>> {
    let mut stmt = conn.prepare(
        "SELECT a.id, a.title, date(a.start_time), es.weight_kg
         FROM exercise_set es JOIN activity a ON a.id = es.activity_id
         WHERE a.sport_type = ?1 AND es.weight_kg IS NOT NULL AND es.weight_kg > 0
         ORDER BY es.weight_kg DESC LIMIT 1",
    )?;
    let mut rows = stmt.query([sport])?;
    match rows.next()? {
        Some(row) => Ok(Some(PersonalRecord {
            activity_id: row.get(0)?,
            title: row.get(1)?,
            date: row.get(2)?,
            value: row.get(3)?,
        })),
        None => Ok(None),
    }
}

/// Best activity for a sport by `column` (largest value). `column` is always an
/// internal constant (never user input); `sport` is bound as a parameter.
fn get_record(conn: &Connection, sport: &str, column: &str) -> Result<Option<PersonalRecord>> {
    let sql = format!(
        "SELECT id, title, date(start_time), {col}
         FROM activity
         WHERE sport_type = ?1 AND {col} IS NOT NULL
         ORDER BY {col} DESC
         LIMIT 1",
        col = column,
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([sport])?;
    match rows.next()? {
        Some(row) => Ok(Some(PersonalRecord {
            activity_id: row.get(0)?,
            title: row.get(1)?,
            date: row.get(2)?,
            value: row.get(3)?,
        })),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::models::activity::Activity;
    use crate::models::exercise_set::ExerciseSet;

    fn sample_activity(id: &str, sport: &str, distance: f64, duration: f64, elev: f64) -> Activity {
        Activity {
            id: id.to_string(),
            start_time: "2026-02-01T08:00:00".to_string(),
            timezone_offset: None,
            sport_type: sport.to_string(),
            title: Some(format!("{} activity", sport)),
            notes: None,
            distance_m: Some(distance),
            duration_s: Some(duration),
            elev_gain_m: Some(elev),
            elev_loss_m: Some(elev * 0.9),
            avg_speed_mps: Some(distance / duration),
            max_speed_mps: Some(distance / duration * 1.2),
            avg_hr: Some(150.0),
            max_hr: Some(175.0),
            avg_cadence: Some(85.0),
            calories: None,
            avg_temperature_c: None,
            max_temperature_c: None,
            source_device: Some("Test Device".to_string()),
            location_name: None,
            start_lat: None,
            start_lon: None,
            avg_power_w: None,
            max_power_w: None,
            normalized_power_w: None,
            total_work_kj: None,
            threshold_power_w: None,
            training_stress_score: None,
            intensity_factor: None,
            training_effect_aerobic: None,
            training_effect_anaerobic: None,
            training_load_peak: None,
            avg_vertical_oscillation_mm: None,
            avg_stance_time_ms: None,
            avg_stance_time_percent: None,
            avg_step_length_mm: None,
            total_strides: None,
            min_hr: None, moving_time_s: None, sub_sport: None,
            avg_respiration_rate: None, max_respiration_rate: None,
            hrv_rmssd: None, hrv_sdrr: None, end_lat: None, end_lon: None,
            avg_left_torque_effectiveness: None, avg_right_torque_effectiveness: None,
            avg_left_pedal_smoothness: None, avg_right_pedal_smoothness: None,
            avg_left_right_balance: None,
            ..Default::default()
        }
    }

    #[test]
    fn dashboard_empty_db() {
        let conn = db::test_db();
        let data = get_dashboard_data(&conn, "all", None).unwrap();
        assert_eq!(data.total_activities, 0);
        assert_eq!(data.total_distance_m, 0.0);
        assert!(data.volume_buckets.is_empty());
        assert!(data.sport_distribution.is_empty());
        assert!(data.records_by_sport.is_empty());
    }

    #[test]
    fn dashboard_with_activities() {
        let conn = db::test_db();
        let a1 = sample_activity("d1", "run", 5000.0, 1800.0, 100.0);
        let mut a2 = sample_activity("d2", "ride", 20000.0, 3600.0, 200.0);
        a2.start_time = "2026-02-08T10:00:00".to_string();

        db::activities::insert_activity(&conn, &a1).unwrap();
        db::activities::insert_activity(&conn, &a2).unwrap();

        let data = get_dashboard_data(&conn, "all", None).unwrap();
        assert_eq!(data.total_activities, 2);
        assert!((data.total_distance_m - 25000.0).abs() < 0.1);
        assert!((data.total_duration_s - 5400.0).abs() < 0.1);
        assert_eq!(data.sport_distribution.len(), 2);

        // Records grouped by sport: the ride record belongs to d2.
        let ride = data
            .records_by_sport
            .iter()
            .find(|s| s.sport_type == "ride")
            .unwrap();
        let dist_rec = ride.records.longest_distance.as_ref().unwrap();
        assert_eq!(dist_rec.activity_id, "d2");
        assert!((dist_rec.value - 20000.0).abs() < 0.1);
    }

    #[test]
    fn dashboard_week_avg_hr() {
        let conn = db::test_db();
        // An activity from today drives the "this week" tiles. Local time, to
        // match the week window's `date('now','localtime',…)` cutoff (UTC
        // would flake for a few hours around Monday midnight).
        let now = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();
        let mut a = sample_activity("wk", "run", 5000.0, 1800.0, 50.0);
        a.start_time = now;
        a.avg_hr = Some(140.0);
        db::activities::insert_activity(&conn, &a).unwrap();

        let data = get_dashboard_data(&conn, "all", None).unwrap();
        assert_eq!(data.week.activities, 1);
        let hr = data.week.avg_hr.expect("week avg hr present");
        assert!((hr - 140.0).abs() < 0.5, "week avg hr ~140, got {hr}");
    }

    /// week_volume buckets by LOCAL date, so a UTC-Z activity lands on the bar
    /// the frontend's local grid expects (not the UTC date, which can be the
    /// previous/next day and fall off the chart while still in the tiles).
    #[test]
    fn week_volume_buckets_by_local_date() {
        let conn = db::test_db();
        // Today at noon local, expressed as a UTC-Z instant — date() (UTC) and
        // date(...,'localtime') can disagree by a day for large offsets; the
        // bucket key must be the LOCAL date so it matches the chart grid.
        let now = chrono::Local::now();
        let local_day = now.format("%Y-%m-%d").to_string();
        let utc_instant = now
            .with_timezone(&chrono::Utc)
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();
        let mut a = sample_activity("z", "ride", 12000.0, 3600.0, 100.0);
        a.start_time = utc_instant;
        db::activities::insert_activity(&conn, &a).unwrap();

        let data = get_dashboard_data(&conn, "all", None).unwrap();
        let bucket = data
            .week_volume
            .iter()
            .find(|b| b.start_date == local_day)
            .expect("activity bucketed on its local date");
        assert!((bucket.distance_m - 12000.0).abs() < 0.1);
    }

    /// "This week" is the current calendar week from Monday (local), not a
    /// rolling 7 days: last Sunday's activity must stay out of the tiles.
    /// Timestamps carry the machine's local offset (like real FIT/GPX data), so
    /// the query's date(start_time,'localtime') resolves them to the intended
    /// local day regardless of the runner's zone.
    #[test]
    fn dashboard_week_starts_on_monday() {
        use chrono::{Datelike, Local, TimeZone};
        let conn = db::test_db();
        let today = Local::now().date_naive();
        let monday = today - chrono::Days::new(today.weekday().num_days_from_monday() as u64);
        let last_sunday = monday - chrono::Days::new(1);

        // Build local-zoned RFC3339 (with offset) so date(...,'localtime') is a
        // round-trip to the same local date on any test machine.
        let local_rfc = |d: chrono::NaiveDate, h: u32| {
            Local
                .from_local_datetime(&d.and_hms_opt(h, 0, 0).unwrap())
                .unwrap()
                .to_rfc3339()
        };

        let mut in_week = sample_activity("in", "run", 5000.0, 1800.0, 10.0);
        in_week.start_time = local_rfc(monday, 6);
        let mut out_of_week = sample_activity("out", "run", 7000.0, 2400.0, 10.0);
        out_of_week.start_time = local_rfc(last_sunday, 23);
        db::activities::insert_activity(&conn, &in_week).unwrap();
        db::activities::insert_activity(&conn, &out_of_week).unwrap();

        let data = get_dashboard_data(&conn, "all", None).unwrap();
        assert_eq!(data.week.activities, 1, "only Monday's activity is in the week");
        assert!((data.week.distance_m - 5000.0).abs() < 0.1);
    }

    /// Swim "elevation gain" is GPS/pressure noise — no elevation record for
    /// water sports, while distance/duration records stay.
    #[test]
    fn dashboard_no_elevation_record_for_water_sports() {
        let conn = db::test_db();
        // 15 m of "gain" recorded by a confused watch during a swim.
        let swim = sample_activity("sw", "swim", 4000.0, 4440.0, 15.0);
        let open = sample_activity("ow", "open_water", 2000.0, 2400.0, 8.0);
        db::activities::insert_activity(&conn, &swim).unwrap();
        db::activities::insert_activity(&conn, &open).unwrap();

        let data = get_dashboard_data(&conn, "all", None).unwrap();
        for sport in ["swim", "open_water"] {
            let rec = data
                .records_by_sport
                .iter()
                .find(|s| s.sport_type == sport)
                .unwrap();
            assert!(rec.records.highest_elevation.is_none(), "{sport} must have no elevation record");
            assert!(rec.records.longest_distance.is_some());
            assert!(rec.records.longest_duration.is_some());
        }
    }

    #[test]
    fn dashboard_records_are_all_time() {
        let conn = db::test_db();
        let mut old = sample_activity("old", "run", 42000.0, 14400.0, 500.0);
        old.start_time = "2020-01-01T08:00:00".to_string();
        let recent = sample_activity("recent", "run", 5000.0, 1800.0, 50.0);

        db::activities::insert_activity(&conn, &old).unwrap();
        db::activities::insert_activity(&conn, &recent).unwrap();

        // A short period excludes both from the totals, but records are all-time.
        let data = get_dashboard_data(&conn, "4w", None).unwrap();
        let run = data
            .records_by_sport
            .iter()
            .find(|s| s.sport_type == "run")
            .unwrap();
        let dist = run.records.longest_distance.as_ref().unwrap();
        assert_eq!(dist.activity_id, "old");
        assert!((dist.value - 42000.0).abs() < 0.1);
    }

    #[test]
    fn dashboard_strength_heaviest_set() {
        let conn = db::test_db();
        let a = sample_activity("s1", "strength", 0.0, 3600.0, 0.0);
        db::activities::insert_activity(&conn, &a).unwrap();
        let mk = |n: i32, w: f64| ExerciseSet {
            id: None,
            activity_id: "s1".to_string(),
            set_number: n,
            start_time: None,
            category: None,
            category_subtype: None,
            set_type: None,
            duration_s: None,
            repetitions: Some(5),
            weight_kg: Some(w),
            wkt_step_index: None,
        };
        db::exercise_sets::insert_exercise_sets(&conn, &[mk(1, 60.0), mk(2, 80.0), mk(3, 72.5)])
            .unwrap();

        let data = get_dashboard_data(&conn, "all", None).unwrap();
        let strength = data
            .records_by_sport
            .iter()
            .find(|s| s.sport_type == "strength")
            .unwrap();
        let hs = strength.records.heaviest_set.as_ref().unwrap();
        assert_eq!(hs.activity_id, "s1");
        assert!((hs.value - 80.0).abs() < 0.1);
        // A non-strength sport has no weighted sets.
        assert!(data
            .records_by_sport
            .iter()
            .all(|s| s.sport_type != "run" || s.records.heaviest_set.is_none()));
    }

    #[test]
    fn integer_shares_sum_to_100() {
        // 5/4/3/3/2 of 17 → naive rounding is 29/24/18/18/12 = 101.
        let s = integer_shares(&[5, 4, 3, 3, 2]);
        assert_eq!(s.iter().sum::<i64>(), 100);
        assert_eq!(s[0], 29); // busiest keeps its floor
        // Single sport gets the whole pie; empty input stays zero.
        assert_eq!(integer_shares(&[7]), vec![100]);
        assert_eq!(integer_shares(&[0, 0]), vec![0, 0]);
        assert_eq!(integer_shares(&[]), Vec::<i64>::new());
    }

    #[test]
    fn week_sport_distribution_caps_at_five_and_sums_to_100() {
        let conn = db::test_db();
        // Six sports within the last 7 days, run busiest.
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string();
        let mk = |id: &str, sport: &str| {
            let mut a = sample_activity(id, sport, 5000.0, 1800.0, 0.0);
            a.start_time = now.clone();
            db::activities::insert_activity(&conn, &a).unwrap();
        };
        for (i, sport) in ["run", "run", "run", "ride", "ride", "swim", "hike", "walk", "paddle"]
            .iter()
            .enumerate()
        {
            mk(&format!("w{i}"), sport);
        }

        let data = get_dashboard_data(&conn, "all", None).unwrap();
        let dist = &data.week_sport_distribution;
        assert_eq!(dist.len(), 5, "capped at 5 sports");
        assert_eq!(dist[0].sport_type, "run"); // busiest first
        assert_eq!(dist[0].activities, 3);
        assert_eq!(dist.iter().map(|s| s.share_pct).sum::<i64>(), 100);
    }

    #[test]
    fn dashboard_filters_by_sport() {
        let conn = db::test_db();
        db::activities::insert_activity(&conn, &sample_activity("r", "run", 5000.0, 1800.0, 0.0))
            .unwrap();
        db::activities::insert_activity(&conn, &sample_activity("b", "ride", 20000.0, 3600.0, 0.0))
            .unwrap();

        let data = get_dashboard_data(&conn, "all", Some("run")).unwrap();
        assert_eq!(data.total_activities, 1);
        assert!((data.total_distance_m - 5000.0).abs() < 0.1);
        // Distribution and records collapse to the filtered sport only.
        assert_eq!(data.sport_distribution.len(), 1);
        assert_eq!(data.sport_distribution[0].sport_type, "run");
    }

    #[test]
    fn dashboard_sport_filter_is_injection_safe() {
        let conn = db::test_db();
        db::activities::insert_activity(&conn, &sample_activity("a", "run", 5000.0, 1800.0, 0.0))
            .unwrap();

        // A malicious sport_type must be treated as a literal value, not SQL.
        let evil = "run'; DROP TABLE activity; --";
        let data = get_dashboard_data(&conn, "all", Some(evil)).unwrap();
        assert_eq!(data.total_activities, 0); // matches nothing, no error

        // The table is intact and the real row is still queryable.
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM activity", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    /// The hybrid multisport model: a merged triathlon's legs count in
    /// their own sport's records, and the container's total time competes in
    /// the triathlon distance-class PBs.
    #[test]
    fn multisport_legs_and_container_both_count_in_records() {
        let conn = db::test_db();

        // A standalone run and a FASTER run leg of a merged triathlon.
        let standalone = sample_activity("solo-run", "run", 5000.0, 1700.0, 10.0);
        db::activities::insert_activity(&conn, &standalone).unwrap();
        let mut leg = sample_activity("tri-run-leg", "run", 5000.0, 1601.0, 5.0);
        leg.parent_id = Some("tri-1".to_string());
        // container first for the FK
        let mut container = sample_activity("tri-1", "triathlon", 25750.0, 5776.0, 50.0);
        container.parent_id = None;
        db::activities::insert_activity(&conn, &container).unwrap();
        conn.execute(
            "UPDATE activity SET parent_id='tri-1' WHERE id='tri-run-leg'",
            [],
        )
        .ok(); // set after insert below
        db::activities::insert_activity(&conn, &leg).unwrap();
        conn.execute("UPDATE activity SET parent_id='tri-1' WHERE id='tri-run-leg'", []).unwrap();
        // A second, slower sprint triathlon.
        let slow = sample_activity("tri-2", "triathlon", 26000.0, 6100.0, 60.0);
        db::activities::insert_activity(&conn, &slow).unwrap();

        let data = get_dashboard_data(&conn, "all", None).unwrap();

        // Run records: the leg (1601 s) beats the standalone (1700 s).
        let run = data.records_by_sport.iter().find(|r| r.sport_type == "run").unwrap();
        let five = run.distance_pbs.iter().find(|p| p.label == "5 km").unwrap();
        assert_eq!(five.activity_id, "tri-run-leg");
        assert_eq!(five.duration_s, 1601.0);

        // Triathlon class PBs: the faster sprint holds the record.
        let tri = data
            .records_by_sport
            .iter()
            .find(|r| r.sport_type == "triathlon")
            .expect("triathlon records present");
        let sprint = tri.distance_pbs.iter().find(|p| p.label == "Sprint").unwrap();
        assert_eq!(sprint.activity_id, "tri-1");
        assert_eq!(sprint.duration_s, 5776.0);

        // But the WEEK/summary aggregates still exclude the leg: total run
        // distance counts solo-run once, the leg's 5 km lives inside tri-1.
        // (3 top-level activities: solo + 2 tris; the leg is hidden.)
        assert_eq!(data.total_activities, 3);
    }

    #[test]
    fn distance_pbs_from_best_efforts_and_longest_run() {
        let conn = db::test_db();
        db::activities::insert_activity(&conn, &sample_activity("r1", "run", 12000.0, 3600.0, 0.0)).unwrap();
        db::activities::insert_activity(&conn, &sample_activity("r2", "run", 11000.0, 3300.0, 0.0)).unwrap();
        // best-effort splits (distance_m, duration_s)
        db::best_efforts::set_best_efforts(&conn, "r1", &[(5000.0, 1500.0), (10000.0, 3100.0)]).unwrap();
        db::best_efforts::set_best_efforts(&conn, "r2", &[(5000.0, 1400.0), (10000.0, 3200.0)]).unwrap();
        // a real ultra (≥ 50 km)
        db::activities::insert_activity(&conn, &sample_activity("u1", "run", 55000.0, 18000.0, 0.0)).unwrap();

        let data = get_dashboard_data(&conn, "all", None).unwrap();
        let run = data.records_by_sport.iter().find(|s| s.sport_type == "run").unwrap();
        let labels: Vec<&str> = run.distance_pbs.iter().map(|p| p.label.as_str()).collect();
        // Ultra (longest run) + the two best-effort splits, longest first.
        assert_eq!(labels, vec!["Ultra", "10 km", "5 km"]);
        // Best 5 km is the faster split (r2).
        let five = run.distance_pbs.iter().find(|p| p.label == "5 km").unwrap();
        assert_eq!(five.activity_id, "r2");
        assert!((five.duration_s - 1400.0).abs() < 0.1);
        // Best 10 km is r1 (3100 < 3200).
        let ten = run.distance_pbs.iter().find(|p| p.label == "10 km").unwrap();
        assert_eq!(ten.activity_id, "r1");
        // Ultra is the 55 km run shown with its full time.
        let ultra = run.distance_pbs.iter().find(|p| p.label == "Ultra").unwrap();
        assert_eq!(ultra.activity_id, "u1");
        assert!((ultra.duration_s - 18000.0).abs() < 0.1);
    }

    #[test]
    fn distance_pb_falls_back_to_gpsless_race_summary() {
        let conn = db::test_db();
        // A marathon logged as a summary only (no trackpoints → no best-effort
        // split), like a race imported without GPS.
        let mut m = sample_activity("m1", "run", 42200.0, 16103.0, 0.0);
        m.start_time = "2014-11-16T13:30:00".to_string();
        db::activities::insert_activity(&conn, &m).unwrap();
        // A slightly-too-long run must NOT count toward the marathon record.
        db::activities::insert_activity(&conn, &sample_activity("long", "run", 43000.0, 17000.0, 0.0))
            .unwrap();

        let data = get_dashboard_data(&conn, "all", None).unwrap();
        let run = data.records_by_sport.iter().find(|s| s.sport_type == "run").unwrap();

        // The summary marathon sets the Marathon PB with its full time.
        let marathon = run
            .distance_pbs
            .iter()
            .find(|p| p.label == "Marathon")
            .expect("marathon PB from race summary");
        assert_eq!(marathon.activity_id, "m1");
        assert!((marathon.duration_s - 16103.0).abs() < 0.1);
        // A 42 km run must not masquerade as a 5 km / 10 km / half record.
        assert!(run
            .distance_pbs
            .iter()
            .all(|p| matches!(p.label.as_str(), "Marathon" | "Ultra")));
    }

    /// A run genuinely SHORTER than a standard distance can't hold that
    /// distance's PB — a fast 4.6 km parkrun is not a 5 km record.
    #[test]
    fn distance_pb_rejects_a_shorter_run() {
        let conn = db::test_db();
        // 4.6 km in 19:00 — faster than any real 5 km, but 400 m short.
        db::activities::insert_activity(
            &conn,
            &sample_activity("short", "run", 4600.0, 1140.0, 0.0),
        )
        .unwrap();
        // A legit 5 km race summary (a touch long) in 21:00.
        db::activities::insert_activity(
            &conn,
            &sample_activity("fivek", "run", 5010.0, 1260.0, 0.0),
        )
        .unwrap();

        let data = get_dashboard_data(&conn, "all", None).unwrap();
        let run = data.records_by_sport.iter().find(|s| s.sport_type == "run").unwrap();
        let five = run.distance_pbs.iter().find(|p| p.label == "5 km").expect("5 km PB");
        // The 5.01 km race holds it, NOT the shorter 4.6 km run.
        assert_eq!(five.activity_id, "fivek");
        assert!((five.duration_s - 1260.0).abs() < 0.1);
    }

    /// A certified distance rounded a hair short (42.10 km marathon, 0.2% under)
    /// still counts — the under-window is relative (2%), not a tight 50 m floor.
    #[test]
    fn distance_pb_accepts_a_rounded_certified_distance() {
        let conn = db::test_db();
        db::activities::insert_activity(
            &conn,
            &sample_activity("rounded", "run", 42100.0, 12600.0, 0.0),
        )
        .unwrap();

        let data = get_dashboard_data(&conn, "all", None).unwrap();
        let run = data.records_by_sport.iter().find(|s| s.sport_type == "run").unwrap();
        let marathon =
            run.distance_pbs.iter().find(|p| p.label == "Marathon").expect("marathon PB");
        assert_eq!(marathon.activity_id, "rounded");
    }

    #[test]
    fn distance_pb_prefers_faster_gps_split_over_summary() {
        let conn = db::test_db();
        // A 10 km race logged as a summary (no track) in 50 min.
        db::activities::insert_activity(&conn, &sample_activity("race", "run", 10050.0, 3000.0, 0.0))
            .unwrap();
        // A faster 10 km split from a GPS run.
        db::activities::insert_activity(&conn, &sample_activity("gps", "run", 12000.0, 3600.0, 0.0))
            .unwrap();
        db::best_efforts::set_best_efforts(&conn, "gps", &[(10000.0, 2700.0)]).unwrap();

        let data = get_dashboard_data(&conn, "all", None).unwrap();
        let run = data.records_by_sport.iter().find(|s| s.sport_type == "run").unwrap();
        let ten = run.distance_pbs.iter().find(|p| p.label == "10 km").unwrap();
        // The 45 min GPS split beats the 50 min summary race.
        assert_eq!(ten.activity_id, "gps");
        assert!((ten.duration_s - 2700.0).abs() < 0.1);
    }
}
