use rusqlite::{params, Connection, OptionalExtension, Result};

use crate::models::activity::{
    Activity, ActivityFilters, ActivityLocation, ActivitySummary, ActivityUpdate, CalDayActivity,
    DaySummary, DeviceStats,
};

pub fn insert_activity(conn: &Connection, activity: &Activity) -> Result<()> {
    conn.execute(
        "INSERT INTO activity (id, start_time, timezone_offset, sport_type, title, notes,
         distance_m, duration_s, elev_gain_m, elev_loss_m, avg_speed_mps, max_speed_mps,
         avg_hr, max_hr, avg_cadence, calories, avg_temperature_c, max_temperature_c,
         source_device, location_name, start_lat, start_lon,
         avg_power_w, max_power_w, normalized_power_w, total_work_kj, threshold_power_w,
         training_stress_score, intensity_factor, training_effect_aerobic, training_effect_anaerobic, training_load_peak,
         avg_vertical_oscillation_mm, avg_stance_time_ms, avg_stance_time_percent, avg_step_length_mm, total_strides,
         min_hr, moving_time_s, sub_sport, avg_respiration_rate, max_respiration_rate, hrv_rmssd, hrv_sdrr, end_lat, end_lon,
         avg_left_torque_effectiveness, avg_right_torque_effectiveness, avg_left_pedal_smoothness, avg_right_pedal_smoothness, avg_left_right_balance,
         avg_left_pco_mm, avg_right_pco_mm,
         avg_left_power_phase_start_deg, avg_left_power_phase_end_deg, avg_left_power_phase_peak_start_deg, avg_left_power_phase_peak_end_deg,
         avg_right_power_phase_start_deg, avg_right_power_phase_end_deg, avg_right_power_phase_peak_start_deg, avg_right_power_phase_peak_end_deg,
         avg_power_seated_w, avg_power_standing_w, max_power_seated_w, max_power_standing_w,
         avg_cadence_seated, avg_cadence_standing, max_cadence_seated, max_cadence_standing,
         time_standing_s, stand_count, parent_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31, ?32, ?33, ?34, ?35, ?36, ?37, ?38, ?39, ?40, ?41, ?42, ?43, ?44, ?45, ?46, ?47, ?48, ?49, ?50, ?51, ?52, ?53, ?54, ?55, ?56, ?57, ?58, ?59, ?60, ?61, ?62, ?63, ?64, ?65, ?66, ?67, ?68, ?69, ?70, ?71, ?72)",
        params![
            activity.id,
            activity.start_time,
            activity.timezone_offset,
            activity.sport_type,
            activity.title,
            activity.notes,
            activity.distance_m,
            activity.duration_s,
            activity.elev_gain_m,
            activity.elev_loss_m,
            activity.avg_speed_mps,
            activity.max_speed_mps,
            activity.avg_hr,
            activity.max_hr,
            activity.avg_cadence,
            activity.calories,
            activity.avg_temperature_c,
            activity.max_temperature_c,
            activity.source_device,
            activity.location_name,
            activity.start_lat,
            activity.start_lon,
            activity.avg_power_w,
            activity.max_power_w,
            activity.normalized_power_w,
            activity.total_work_kj,
            activity.threshold_power_w,
            activity.training_stress_score,
            activity.intensity_factor,
            activity.training_effect_aerobic,
            activity.training_effect_anaerobic,
            activity.training_load_peak,
            activity.avg_vertical_oscillation_mm,
            activity.avg_stance_time_ms,
            activity.avg_stance_time_percent,
            activity.avg_step_length_mm,
            activity.total_strides,
            activity.min_hr,
            activity.moving_time_s,
            activity.sub_sport,
            activity.avg_respiration_rate,
            activity.max_respiration_rate,
            activity.hrv_rmssd,
            activity.hrv_sdrr,
            activity.end_lat,
            activity.end_lon,
            activity.avg_left_torque_effectiveness,
            activity.avg_right_torque_effectiveness,
            activity.avg_left_pedal_smoothness,
            activity.avg_right_pedal_smoothness,
            activity.avg_left_right_balance,
            activity.avg_left_pco_mm,
            activity.avg_right_pco_mm,
            activity.avg_left_power_phase_start_deg,
            activity.avg_left_power_phase_end_deg,
            activity.avg_left_power_phase_peak_start_deg,
            activity.avg_left_power_phase_peak_end_deg,
            activity.avg_right_power_phase_start_deg,
            activity.avg_right_power_phase_end_deg,
            activity.avg_right_power_phase_peak_start_deg,
            activity.avg_right_power_phase_peak_end_deg,
            activity.avg_power_seated_w,
            activity.avg_power_standing_w,
            activity.max_power_seated_w,
            activity.max_power_standing_w,
            activity.avg_cadence_seated,
            activity.avg_cadence_standing,
            activity.max_cadence_seated,
            activity.max_cadence_standing,
            activity.time_standing_s,
            activity.stand_count,
            activity.parent_id,
        ],
    )?;
    Ok(())
}

/// Canonical column list for a full [`Activity`], in the exact order
/// [`row_to_activity`] reads. Any query mapped with `row_to_activity` MUST
/// select these columns in this order.
const ACTIVITY_COLUMNS: &str = "id, start_time, timezone_offset, sport_type, title, notes, \
     distance_m, duration_s, elev_gain_m, elev_loss_m, avg_speed_mps, max_speed_mps, \
     avg_hr, max_hr, avg_cadence, calories, avg_temperature_c, max_temperature_c, \
     source_device, location_name, start_lat, start_lon, \
     avg_power_w, max_power_w, normalized_power_w, total_work_kj, threshold_power_w, \
     training_stress_score, intensity_factor, training_effect_aerobic, training_effect_anaerobic, training_load_peak, \
     avg_vertical_oscillation_mm, avg_stance_time_ms, avg_stance_time_percent, avg_step_length_mm, total_strides, \
     min_hr, moving_time_s, sub_sport, avg_respiration_rate, max_respiration_rate, hrv_rmssd, hrv_sdrr, end_lat, end_lon, \
     avg_left_torque_effectiveness, avg_right_torque_effectiveness, avg_left_pedal_smoothness, avg_right_pedal_smoothness, avg_left_right_balance, \
     avg_left_pco_mm, avg_right_pco_mm, \
     avg_left_power_phase_start_deg, avg_left_power_phase_end_deg, avg_left_power_phase_peak_start_deg, avg_left_power_phase_peak_end_deg, \
     avg_right_power_phase_start_deg, avg_right_power_phase_end_deg, avg_right_power_phase_peak_start_deg, avg_right_power_phase_peak_end_deg, \
     avg_power_seated_w, avg_power_standing_w, max_power_seated_w, max_power_standing_w, \
     avg_cadence_seated, avg_cadence_standing, max_cadence_seated, max_cadence_standing, \
     time_standing_s, stand_count, \
     created_at, updated_at, parent_id";

/// Canonical column list for an [`ActivitySummary`] (aliased `a`), in the order
/// [`row_to_summary`] reads.
const SUMMARY_COLUMNS: &str = "a.id, a.start_time, a.sport_type, a.title, a.distance_m, \
     a.duration_s, a.elev_gain_m, a.avg_speed_mps, a.avg_hr, a.location_name";

/// Build a full [`Activity`] from a row selected with [`ACTIVITY_COLUMNS`].
fn row_to_activity(row: &rusqlite::Row) -> Result<Activity> {
    Ok(Activity {
        id: row.get(0)?,
        start_time: row.get(1)?,
        timezone_offset: row.get(2)?,
        sport_type: row.get(3)?,
        title: row.get(4)?,
        notes: row.get(5)?,
        distance_m: row.get(6)?,
        duration_s: row.get(7)?,
        elev_gain_m: row.get(8)?,
        elev_loss_m: row.get(9)?,
        avg_speed_mps: row.get(10)?,
        max_speed_mps: row.get(11)?,
        avg_hr: row.get(12)?,
        max_hr: row.get(13)?,
        avg_cadence: row.get(14)?,
        calories: row.get(15)?,
        avg_temperature_c: row.get(16)?,
        max_temperature_c: row.get(17)?,
        source_device: row.get(18)?,
        location_name: row.get(19)?,
        start_lat: row.get(20)?,
        start_lon: row.get(21)?,
        avg_power_w: row.get(22)?,
        max_power_w: row.get(23)?,
        normalized_power_w: row.get(24)?,
        total_work_kj: row.get(25)?,
        threshold_power_w: row.get(26)?,
        training_stress_score: row.get(27)?,
        intensity_factor: row.get(28)?,
        training_effect_aerobic: row.get(29)?,
        training_effect_anaerobic: row.get(30)?,
        training_load_peak: row.get(31)?,
        avg_vertical_oscillation_mm: row.get(32)?,
        avg_stance_time_ms: row.get(33)?,
        avg_stance_time_percent: row.get(34)?,
        avg_step_length_mm: row.get(35)?,
        total_strides: row.get(36)?,
        min_hr: row.get(37)?,
        moving_time_s: row.get(38)?,
        sub_sport: row.get(39)?,
        avg_respiration_rate: row.get(40)?,
        max_respiration_rate: row.get(41)?,
        hrv_rmssd: row.get(42)?,
        hrv_sdrr: row.get(43)?,
        end_lat: row.get(44)?,
        end_lon: row.get(45)?,
        avg_left_torque_effectiveness: row.get(46)?,
        avg_right_torque_effectiveness: row.get(47)?,
        avg_left_pedal_smoothness: row.get(48)?,
        avg_right_pedal_smoothness: row.get(49)?,
        avg_left_right_balance: row.get(50)?,
        avg_left_pco_mm: row.get(51)?,
        avg_right_pco_mm: row.get(52)?,
        avg_left_power_phase_start_deg: row.get(53)?,
        avg_left_power_phase_end_deg: row.get(54)?,
        avg_left_power_phase_peak_start_deg: row.get(55)?,
        avg_left_power_phase_peak_end_deg: row.get(56)?,
        avg_right_power_phase_start_deg: row.get(57)?,
        avg_right_power_phase_end_deg: row.get(58)?,
        avg_right_power_phase_peak_start_deg: row.get(59)?,
        avg_right_power_phase_peak_end_deg: row.get(60)?,
        avg_power_seated_w: row.get(61)?,
        avg_power_standing_w: row.get(62)?,
        max_power_seated_w: row.get(63)?,
        max_power_standing_w: row.get(64)?,
        avg_cadence_seated: row.get(65)?,
        avg_cadence_standing: row.get(66)?,
        max_cadence_seated: row.get(67)?,
        max_cadence_standing: row.get(68)?,
        time_standing_s: row.get(69)?,
        stand_count: row.get(70)?,
        created_at: row.get(71)?,
        updated_at: row.get(72)?,
        parent_id: row.get(73)?,
    })
}

/// Build an [`ActivitySummary`] from a row selected with [`SUMMARY_COLUMNS`].
/// `tags` are populated separately by the caller.
fn row_to_summary(row: &rusqlite::Row) -> Result<ActivitySummary> {
    Ok(ActivitySummary {
        id: row.get(0)?,
        start_time: row.get(1)?,
        sport_type: row.get(2)?,
        title: row.get(3)?,
        distance_m: row.get(4)?,
        duration_s: row.get(5)?,
        elev_gain_m: row.get(6)?,
        avg_speed_mps: row.get(7)?,
        avg_hr: row.get(8)?,
        location_name: row.get(9)?,
        tags: Vec::new(),
    })
}

pub fn get_activity_by_id(conn: &Connection, id: &str) -> Result<Option<Activity>> {
    let mut stmt = conn.prepare(&format!("SELECT {ACTIVITY_COLUMNS} FROM activity WHERE id = ?1"))?;

    let mut rows = stmt.query(params![id])?;
    match rows.next()? {
        Some(row) => Ok(Some(row_to_activity(row)?)),
        None => Ok(None),
    }
}

/// Escape LIKE wildcards so a user-typed `%`, `_` or `\` matches literally
/// (paired with `ESCAPE '\'` in the query).
fn escape_like(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Free-text search fragment for `?{idx}`, matching the term anywhere in
/// title / notes / location_name. `prefix` is the table alias (`"a."` or `""`).
/// Returns the SQL condition and the bound `%term%` value, or `None` when the
/// term is blank (so it doesn't filter anything out).
fn search_condition(term: Option<&str>, idx: usize, prefix: &str) -> Option<(String, String)> {
    let term = term.map(str::trim).filter(|t| !t.is_empty())?;
    let cond = format!(
        "({p}title LIKE ?{i} ESCAPE '\\' OR {p}notes LIKE ?{i} ESCAPE '\\' OR {p}location_name LIKE ?{i} ESCAPE '\\')",
        p = prefix,
        i = idx
    );
    Some((cond, format!("%{}%", escape_like(term))))
}

/// Append all faceted WHERE conditions from `filters` — search, sport, date
/// range, distance, duration, elevation and tags — to `conditions`, binding
/// their values into `params` and advancing `idx`. `prefix` is the column
/// qualifier (`"a."` for aliased queries, `""` otherwise). Sort/limit/offset
/// are NOT handled here. Shared by the list, calendar and map so every view
/// honours the same filters.
fn push_facet_conditions(
    filters: &ActivityFilters,
    prefix: &str,
    conditions: &mut Vec<String>,
    params: &mut Vec<Box<dyn rusqlite::types::ToSql>>,
    idx: &mut usize,
) {
    // Free-text search: match the term anywhere in title, notes or location.
    // SQLite's LIKE is case-insensitive for ASCII; `%term%` does a substring
    // match. Blank/whitespace-only terms are ignored so they don't filter out
    // everything.
    if let Some((cond, like)) = search_condition(filters.search.as_deref(), *idx, prefix) {
        conditions.push(cond);
        params.push(Box::new(like));
        *idx += 1;
    }
    if let Some(ref sports) = filters.sport_types {
        if !sports.is_empty() {
            let placeholders: Vec<String> = sports
                .iter()
                .map(|s| {
                    params.push(Box::new(s.clone()));
                    let p = format!("?{}", *idx);
                    *idx += 1;
                    p
                })
                .collect();
            conditions.push(format!(
                "{prefix}sport_type IN ({})",
                placeholders.join(", ")
            ));
        }
    }
    if let Some(ref from) = filters.date_from {
        conditions.push(format!("{prefix}start_time >= ?{idx}"));
        params.push(Box::new(from.clone()));
        *idx += 1;
    }
    if let Some(ref to) = filters.date_to {
        // `to` is a bare 'YYYY-MM-DD' but start_time is a full timestamp, so a
        // plain `<= to` string compare drops the whole end day. Bound by the
        // start of the NEXT day instead — includes any time on `to`, any offset.
        conditions.push(format!("{prefix}start_time < date(?{idx}, '+1 day')"));
        params.push(Box::new(to.clone()));
        *idx += 1;
    }
    if let Some(min) = filters.distance_min {
        conditions.push(format!("{prefix}distance_m >= ?{idx}"));
        params.push(Box::new(min));
        *idx += 1;
    }
    if let Some(max) = filters.distance_max {
        conditions.push(format!("{prefix}distance_m <= ?{idx}"));
        params.push(Box::new(max));
        *idx += 1;
    }
    if let Some(min) = filters.duration_min {
        conditions.push(format!("{prefix}duration_s >= ?{idx}"));
        params.push(Box::new(min));
        *idx += 1;
    }
    if let Some(max) = filters.duration_max {
        conditions.push(format!("{prefix}duration_s <= ?{idx}"));
        params.push(Box::new(max));
        *idx += 1;
    }
    if let Some(min) = filters.elev_gain_min {
        conditions.push(format!("{prefix}elev_gain_m >= ?{idx}"));
        params.push(Box::new(min));
        *idx += 1;
    }
    if let Some(max) = filters.elev_gain_max {
        conditions.push(format!("{prefix}elev_gain_m <= ?{idx}"));
        params.push(Box::new(max));
        *idx += 1;
    }
    if let Some(ref ids) = filters.tag_ids {
        if !ids.is_empty() {
            let placeholders: Vec<String> = ids
                .iter()
                .map(|id| {
                    params.push(Box::new(*id));
                    let p = format!("?{}", *idx);
                    *idx += 1;
                    p
                })
                .collect();
            conditions.push(format!(
                "{prefix}id IN (SELECT activity_id FROM activity_tag WHERE tag_id IN ({}))",
                placeholders.join(", ")
            ));
        }
    }
    if let Some(has_gps) = filters.has_gps {
        // "Has GPS" = the activity owns a route: at least one trackpoint with
        // a latitude. start_lat alone deliberately doesn't count — manual
        // location entry can geocode a start point onto an indoor workout.
        // trackpoint has its own `id` column, so the outer id must be
        // qualified even in unaliased queries (calendar passes prefix "").
        let outer = if prefix.is_empty() { "activity." } else { prefix };
        let exists = format!(
            "EXISTS (SELECT 1 FROM trackpoint WHERE activity_id = {outer}id AND lat IS NOT NULL)"
        );
        conditions.push(if has_gps { exists } else { format!("NOT {exists}") });
    }

    // Merged-triathlon legs are hidden from every list, calendar and map that
    // runs through here — they'd double-count against their container and
    // clutter the library. They stay reachable only by direct id (the Legs
    // card links to them). Containers themselves have parent_id NULL.
    conditions.push(format!("{prefix}parent_id IS NULL"));
}

pub fn get_activities(conn: &Connection, filters: &ActivityFilters) -> Result<Vec<ActivitySummary>> {
    let mut sql = format!("SELECT {SUMMARY_COLUMNS} FROM activity a");

    let mut conditions: Vec<String> = Vec::new();
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut param_idx = 1;

    push_facet_conditions(filters, "a.", &mut conditions, &mut param_values, &mut param_idx);

    if !conditions.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&conditions.join(" AND "));
    }

    let sort_col = match filters.sort_by.as_deref() {
        Some("distance") => "a.distance_m",
        Some("duration") => "a.duration_s",
        Some("elevation") => "a.elev_gain_m",
        _ => "a.start_time",
    };
    let sort_dir = match filters.sort_dir.as_deref() {
        Some("asc") => "ASC",
        _ => "DESC",
    };
    sql.push_str(&format!(" ORDER BY {} {}", sort_col, sort_dir));

    let limit = filters.limit.unwrap_or(100);
    let offset = filters.offset.unwrap_or(0);
    sql.push_str(&format!(" LIMIT {} OFFSET {}", limit, offset));

    let params_refs: Vec<&dyn rusqlite::types::ToSql> = param_values.iter().map(|p| p.as_ref()).collect();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_refs.as_slice(), row_to_summary)?;

    let mut activities = Vec::new();
    for row in rows {
        activities.push(row?);
    }
    Ok(activities)
}

/// Records this activity holds within its sport (all-time maxes): longest
/// distance, highest elevation gain, longest duration, fastest avg speed.
/// Returns one badge per metric where the activity is the sport's best.
pub fn get_record_badges(conn: &Connection, id: &str) -> Result<Vec<crate::models::activity::RecordBadge>> {
    use crate::models::activity::RecordBadge;

    // The activity's own sport + metrics.
    let row = conn
        .query_row(
            "SELECT sport_type, distance_m, duration_s, elev_gain_m, avg_speed_mps
             FROM activity WHERE id = ?1",
            params![id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, Option<f64>>(1)?,
                    r.get::<_, Option<f64>>(2)?,
                    r.get::<_, Option<f64>>(3)?,
                    r.get::<_, Option<f64>>(4)?,
                ))
            },
        )
        .optional()?;
    let Some((sport, dist, dur, elev, speed)) = row else {
        return Ok(Vec::new());
    };

    // Per-sport maxes + how many activities the sport has. A lone activity
    // trivially equals its own max on every metric, so "all-time record" is
    // meaningless until there are at least two to compare — no badges then.
    let (count, mdist, mdur, melev, mspeed) = conn.query_row(
        "SELECT COUNT(*), MAX(distance_m), MAX(duration_s), MAX(elev_gain_m), MAX(avg_speed_mps)
         FROM activity WHERE sport_type = ?1",
        params![sport],
        |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Option<f64>>(1)?,
                r.get::<_, Option<f64>>(2)?,
                r.get::<_, Option<f64>>(3)?,
                r.get::<_, Option<f64>>(4)?,
            ))
        },
    )?;
    if count < 2 {
        return Ok(Vec::new());
    }

    // A metric earns a badge when the activity has a positive value equal to
    // the sport's max (ties all qualify).
    let is_best = |v: Option<f64>, m: Option<f64>| match (v, m) {
        (Some(v), Some(m)) => v > 0.0 && (v - m).abs() < 1e-6,
        _ => false,
    };

    let mut badges = Vec::new();
    for (kind, v, m) in [
        ("distance", dist, mdist),
        ("elevation", elev, melev),
        ("duration", dur, mdur),
        ("pace", speed, mspeed),
    ] {
        if is_best(v, m) {
            badges.push(RecordBadge { kind: kind.to_string(), all_time: true });
        }
    }
    Ok(badges)
}

/// Title length cap in CHARS — mirrors MAX_TITLE_LENGTH in src/lib/types.ts.
const MAX_TITLE_CHARS: usize = 100;

pub fn update_activity(conn: &Connection, id: &str, updates: &ActivityUpdate) -> Result<()> {
    let mut sets: Vec<String> = Vec::new();
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut idx = 1;

    if let Some(ref title) = updates.title {
        sets.push(format!("title = ?{}", idx));
        // Backstop for the UI cap — an overlong title (old data, other
        // callers) is truncated here too. chars(), not bytes: a UTF-8
        // boundary slice would panic.
        param_values.push(Box::new(title.chars().take(MAX_TITLE_CHARS).collect::<String>()));
        idx += 1;
    }
    if let Some(ref notes) = updates.notes {
        sets.push(format!("notes = ?{}", idx));
        param_values.push(Box::new(notes.clone()));
        idx += 1;
    }
    if let Some(ref sport) = updates.sport_type {
        sets.push(format!("sport_type = ?{}", idx));
        param_values.push(Box::new(sport.clone()));
        idx += 1;
    }
    if let Some(ref loc) = updates.location_name {
        sets.push(format!("location_name = ?{}", idx));
        param_values.push(Box::new(loc.clone()));
        idx += 1;
    }
    if let Some(lat) = updates.start_lat {
        sets.push(format!("start_lat = ?{}", idx));
        param_values.push(Box::new(lat));
        idx += 1;
    }
    if let Some(lon) = updates.start_lon {
        sets.push(format!("start_lon = ?{}", idx));
        param_values.push(Box::new(lon));
        idx += 1;
    }

    if sets.is_empty() {
        return Ok(());
    }

    sets.push("updated_at = datetime('now')".to_string());
    let sql = format!("UPDATE activity SET {} WHERE id = ?{}", sets.join(", "), idx);
    param_values.push(Box::new(id.to_string()));

    let params_refs: Vec<&dyn rusqlite::types::ToSql> = param_values.iter().map(|p| p.as_ref()).collect();
    conn.execute(&sql, params_refs.as_slice())?;
    Ok(())
}

pub fn delete_activity(conn: &Connection, id: &str) -> Result<()> {
    // A merged leg must leave through unmerge, not deletion: the container's
    // leg row would keep a dangling source_activity_id (no FK there) and the
    // container's aggregate would silently include a vanished activity.
    let is_leg: bool = conn.query_row(
        "SELECT parent_id IS NOT NULL FROM activity WHERE id = ?1",
        params![id],
        |r| r.get(0),
    )?;
    if is_leg {
        return Err(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(1),
            Some("activity is part of a multisport — unmerge it first".into()),
        ));
    }
    conn.execute("DELETE FROM activity WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn get_calendar_data(
    conn: &Connection,
    year: i32,
    month: u32,
    filters: &ActivityFilters,
) -> Result<Vec<DaySummary>> {
    let date_from = format!("{:04}-{:02}-01", year, month);
    let date_to = if month == 12 {
        format!("{:04}-01-01", year + 1)
    } else {
        format!("{:04}-{:02}-01", year, month + 1)
    };

    // The displayed month bounds the query; the remaining facets (sport,
    // distance, search, tags, …) are intersected on top so the calendar honours
    // the same filters as the list. The drawer's own date range, if set, is
    // applied too — it just narrows within the month.
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> =
        vec![Box::new(date_from), Box::new(date_to)];
    let mut conditions: Vec<String> =
        vec!["start_time >= ?1".into(), "start_time < ?2".into()];
    let mut idx = 3;
    push_facet_conditions(filters, "", &mut conditions, &mut params, &mut idx);
    let where_sql = conditions.join(" AND ");

    let sql = format!(
        "SELECT date(start_time) as day, id, sport_type, title, distance_m, duration_s, elev_gain_m
         FROM activity
         WHERE {where_sql}
         ORDER BY day, start_time"
    );
    let mut stmt = conn.prepare(&sql)?;

    struct Row {
        day: String,
        act: CalDayActivity,
        elev_gain_m: Option<f64>,
    }
    let params_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let rows = stmt.query_map(params_refs.as_slice(), |row| {
        Ok(Row {
            day: row.get(0)?,
            act: CalDayActivity {
                id: row.get(1)?,
                sport_type: row.get(2)?,
                title: row.get(3)?,
                distance_m: row.get(4)?,
                duration_s: row.get(5)?,
            },
            elev_gain_m: row.get(6)?,
        })
    })?;

    // Group consecutive rows by day (query is ordered by day), building the
    // per-activity list and deriving the aggregate fields from it.
    let mut days: Vec<DaySummary> = Vec::new();
    for row in rows {
        let Row { day, act, elev_gain_m } = row?;
        if days.last().map(|d| d.date != day).unwrap_or(true) {
            days.push(DaySummary {
                date: day,
                activity_count: 0,
                total_distance_m: 0.0,
                total_duration_s: 0.0,
                total_elev_gain_m: 0.0,
                sport_types: Vec::new(),
                activities: Vec::new(),
            });
        }
        let d = days.last_mut().unwrap();
        d.activity_count += 1;
        d.total_distance_m += act.distance_m.unwrap_or(0.0);
        d.total_duration_s += act.duration_s.unwrap_or(0.0);
        d.total_elev_gain_m += elev_gain_m.unwrap_or(0.0);
        if !d.sport_types.contains(&act.sport_type) {
            d.sport_types.push(act.sport_type.clone());
        }
        d.activities.push(act);
    }
    Ok(days)
}

/// Re-normalize `sport_type` for already-imported activities using the current
/// mapping. The original raw sport string isn't stored, so we feed the existing
/// `sport_type` plus the stored `sub_sport` through `SportType::resolve` — this
/// upgrades activities whose finer type lives in `sub_sport` (e.g. swim +
/// open_water → open_water, other + yoga → yoga). Returns the number changed.
pub fn recompute_sport_types(conn: &Connection) -> Result<usize> {
    let rows: Vec<(String, String, Option<String>)> = {
        let mut stmt = conn.prepare("SELECT id, sport_type, sub_sport FROM activity")?;
        let mapped = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        let mut v = Vec::new();
        for r in mapped {
            v.push(r?);
        }
        v
    };

    let mut changed = 0usize;
    for (id, sport_type, sub_sport) in rows {
        let new = crate::models::activity::SportType::resolve(
            Some(&sport_type),
            sub_sport.as_deref(),
        )
        .as_str();
        if new != sport_type {
            conn.execute(
                "UPDATE activity SET sport_type = ?1 WHERE id = ?2",
                params![new, id],
            )?;
            changed += 1;
        }
    }
    Ok(changed)
}

/// Distinct sport types actually present among imported activities, most-used
/// first. Drives the filter chips so only relevant sports are shown.
pub fn get_used_sport_types(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT sport_type FROM activity WHERE parent_id IS NULL \
         GROUP BY sport_type ORDER BY COUNT(*) DESC",
    )?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// Years of the earliest and latest activity (by start_time, ISO strings so
/// the first four chars are the year), or None for an empty library. Feeds
/// the date-picker's year dropdown.
pub fn get_activity_year_range(conn: &Connection) -> Result<Option<(i32, i32)>> {
    conn.query_row(
        "SELECT CAST(substr(MIN(start_time), 1, 4) AS INTEGER),
                CAST(substr(MAX(start_time), 1, 4) AS INTEGER)
         FROM activity",
        [],
        |row| {
            let min: Option<i32> = row.get(0)?;
            let max: Option<i32> = row.get(1)?;
            Ok(min.zip(max))
        },
    )
}

pub fn get_detected_devices(conn: &Connection) -> Result<Vec<DeviceStats>> {
    let mut stmt = conn.prepare(
        "SELECT source_device, COUNT(*) as cnt, MAX(start_time) as last_time
         FROM activity
         WHERE source_device IS NOT NULL
         GROUP BY source_device
         ORDER BY cnt DESC",
    )?;

    let rows = stmt.query_map([], |row| {
        Ok(DeviceStats {
            device_name: row.get(0)?,
            activity_count: row.get(1)?,
            last_activity: row.get(2)?,
        })
    })?;

    let mut devices = Vec::new();
    for row in rows {
        devices.push(row?);
    }
    Ok(devices)
}

/// Returns the IDs of the adjacent activities (by start_time DESC, id DESC order).
/// `prev_id` = newer activity (above in list), `next_id` = older activity (below in list).
pub fn get_adjacent_activity_ids(
    conn: &Connection,
    id: &str,
) -> Result<(Option<String>, Option<String>)> {
    // Get the start_time of the current activity
    let start_time: String = conn.query_row(
        "SELECT start_time FROM activity WHERE id = ?1",
        params![id],
        |row| row.get(0),
    )?;

    // Previous (newer) — tie-break by id to handle same-timestamp activities
    let prev_id: Option<String> = conn
        .query_row(
            "SELECT id FROM activity
             WHERE parent_id IS NULL AND ((start_time > ?1) OR (start_time = ?1 AND id > ?2))
             ORDER BY start_time ASC, id ASC LIMIT 1",
            params![start_time, id],
            |row| row.get(0),
        )
        .ok();

    // Next (older) — tie-break by id
    let next_id: Option<String> = conn
        .query_row(
            "SELECT id FROM activity
             WHERE parent_id IS NULL AND ((start_time < ?1) OR (start_time = ?1 AND id < ?2))
             ORDER BY start_time DESC, id DESC LIMIT 1",
            params![start_time, id],
            |row| row.get(0),
        )
        .ok();

    Ok((prev_id, next_id))
}

pub fn get_activity_start_locations(
    conn: &Connection,
    filters: &ActivityFilters,
) -> Result<Vec<ActivityLocation>> {
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut conditions: Vec<String> = Vec::new();
    let mut idx = 1;
    push_facet_conditions(filters, "a.", &mut conditions, &mut params, &mut idx);
    let where_sql = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let sql = format!(
        "SELECT a.id, a.start_time, a.sport_type, a.title, a.distance_m, a.duration_s,
                COALESCE(a.start_lat, (SELECT lat FROM trackpoint WHERE activity_id = a.id AND lat IS NOT NULL ORDER BY rowid ASC LIMIT 1)) as start_lat,
                COALESCE(a.start_lon, (SELECT lon FROM trackpoint WHERE activity_id = a.id AND lon IS NOT NULL ORDER BY rowid ASC LIMIT 1)) as start_lon
         FROM activity a
         {where_sql}
         ORDER BY a.start_time DESC"
    );
    let mut stmt = conn.prepare(&sql)?;

    let params_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let rows = stmt.query_map(params_refs.as_slice(), |row| {
        let lat: Option<f64> = row.get(6)?;
        let lon: Option<f64> = row.get(7)?;
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, Option<f64>>(4)?,
            row.get::<_, Option<f64>>(5)?,
            lat,
            lon,
        ))
    })?;

    let mut locations = Vec::new();
    for row in rows {
        let (id, start_time, sport_type, title, distance_m, duration_s, lat, lon) = row?;
        if let (Some(lat), Some(lon)) = (lat, lon) {
            locations.push(ActivityLocation {
                id,
                start_time,
                sport_type,
                title,
                distance_m,
                duration_s,
                lat,
                lon,
            });
        }
    }
    Ok(locations)
}

pub fn set_location_name(conn: &Connection, id: &str, name: &str) -> Result<()> {
    conn.execute(
        "UPDATE activity SET location_name = ?1 WHERE id = ?2",
        params![name, id],
    )?;
    Ok(())
}

/// Clear an activity's location name and manual start coordinates.
///
/// The name becomes the empty-string marker, NOT NULL: NULL means "never
/// looked up" and gets re-sent by the next background geocoding pass — which
/// would resurrect the very name the user just erased. "" means "resolved:
/// nothing to show" and is skipped (see get_activities_without_location).
pub fn clear_location(conn: &Connection, id: &str) -> Result<()> {
    conn.execute(
        "UPDATE activity SET location_name = '', start_lat = NULL, start_lon = NULL, \
         updated_at = datetime('now') WHERE id = ?1",
        params![id],
    )?;
    Ok(())
}

pub fn get_activities_without_location(conn: &Connection) -> Result<Vec<(String, f64, f64)>> {
    let mut stmt = conn.prepare(
        "SELECT a.id,
                (SELECT lat FROM trackpoint WHERE activity_id = a.id AND lat IS NOT NULL ORDER BY rowid ASC LIMIT 1),
                (SELECT lon FROM trackpoint WHERE activity_id = a.id AND lon IS NOT NULL ORDER BY rowid ASC LIMIT 1)
         FROM activity a
         WHERE a.location_name IS NULL",
    )?;
    let rows = stmt.query_map([], |row| {
        let id: String = row.get(0)?;
        let lat: Option<f64> = row.get(1)?;
        let lon: Option<f64> = row.get(2)?;
        Ok((id, lat, lon))
    })?;

    let mut result = Vec::new();
    for row in rows {
        let (id, lat, lon) = row?;
        if let (Some(lat), Some(lon)) = (lat, lon) {
            result.push((id, lat, lon));
        }
    }
    Ok(result)
}

pub fn search_activities(conn: &Connection, query: &str) -> Result<Vec<ActivitySummary>> {
    // escape_like + ESCAPE, same as the filter path: a typed `%`/`_` must
    // match literally, not act as a wildcard.
    let pattern = format!("%{}%", escape_like(query));
    let mut stmt = conn.prepare(&format!(
        "SELECT {SUMMARY_COLUMNS} FROM activity a
         WHERE a.parent_id IS NULL
           AND (a.title LIKE ?1 ESCAPE '\\' OR a.notes LIKE ?1 ESCAPE '\\'
            OR a.source_device LIKE ?1 ESCAPE '\\' OR a.location_name LIKE ?1 ESCAPE '\\')
         ORDER BY a.start_time DESC
         LIMIT 100"
    ))?;

    let rows = stmt.query_map(params![pattern], row_to_summary)?;

    let mut activities = Vec::new();
    for row in rows {
        activities.push(row?);
    }
    Ok(activities)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn sample_activity(id: &str) -> Activity {
        Activity {
            id: id.to_string(),
            start_time: "2025-06-01T08:00:00+00:00".to_string(),
            timezone_offset: None,
            sport_type: "run".to_string(),
            title: Some("Morning Run".to_string()),
            notes: None,
            distance_m: Some(5000.0),
            duration_s: Some(1800.0),
            elev_gain_m: Some(50.0),
            elev_loss_m: Some(45.0),
            avg_speed_mps: Some(2.78),
            max_speed_mps: Some(3.5),
            avg_hr: Some(150.0),
            max_hr: Some(175.0),
            avg_cadence: Some(85.0),
            calories: Some(350.0),
            avg_temperature_c: None,
            max_temperature_c: None,
            source_device: Some("Garmin FR265".to_string()),
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
            avg_vertical_oscillation_mm: None, avg_stance_time_ms: None, avg_stance_time_percent: None,
            avg_step_length_mm: None, total_strides: None,
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
    fn insert_and_get_activity() {
        let conn = db::test_db();
        let a = sample_activity("test-1");
        insert_activity(&conn, &a).unwrap();

        let loaded = get_activity_by_id(&conn, "test-1").unwrap().unwrap();
        assert_eq!(loaded.id, "test-1");
        assert_eq!(loaded.sport_type, "run");
        assert_eq!(loaded.distance_m, Some(5000.0));
        assert_eq!(loaded.title, Some("Morning Run".to_string()));
        assert_eq!(loaded.calories, Some(350.0));
    }

    /// Cycling Dynamics columns roundtrip — insert placeholders and
    /// row_to_activity indices march in lockstep, so a shifted index would
    /// surface here as a wrong or missing value.
    #[test]
    fn cycling_dynamics_roundtrip() {
        let conn = db::test_db();
        let a = Activity {
            avg_left_pco_mm: Some(0.0),
            avg_right_pco_mm: Some(9.0),
            avg_left_power_phase_start_deg: Some(324.8),
            avg_left_power_phase_end_deg: Some(230.6),
            avg_left_power_phase_peak_start_deg: Some(70.3),
            avg_left_power_phase_peak_end_deg: Some(125.2),
            avg_right_power_phase_start_deg: Some(355.8),
            avg_right_power_phase_end_deg: Some(208.1),
            avg_right_power_phase_peak_start_deg: Some(68.9),
            avg_right_power_phase_peak_end_deg: Some(113.9),
            avg_power_seated_w: Some(231.0),
            avg_power_standing_w: Some(161.0),
            max_power_seated_w: Some(1013.0),
            max_power_standing_w: Some(956.0),
            avg_cadence_seated: Some(83.0),
            avg_cadence_standing: Some(55.0),
            max_cadence_seated: Some(111.0),
            max_cadence_standing: Some(105.0),
            time_standing_s: Some(1156.9),
            stand_count: Some(90),
            ..sample_activity("dyn-1")
        };
        insert_activity(&conn, &a).unwrap();

        let loaded = get_activity_by_id(&conn, "dyn-1").unwrap().unwrap();
        assert_eq!(loaded.avg_right_pco_mm, Some(9.0));
        assert_eq!(loaded.avg_left_power_phase_start_deg, Some(324.8));
        assert_eq!(loaded.avg_right_power_phase_peak_end_deg, Some(113.9));
        assert_eq!(loaded.avg_power_standing_w, Some(161.0));
        assert_eq!(loaded.max_power_seated_w, Some(1013.0));
        assert_eq!(loaded.avg_cadence_standing, Some(55.0));
        assert_eq!(loaded.time_standing_s, Some(1156.9));
        assert_eq!(loaded.stand_count, Some(90));
        // The tail columns after the new block must not have shifted.
        assert_eq!(loaded.parent_id, None);
        assert!(!loaded.created_at.is_empty());
    }

    #[test]
    fn get_nonexistent_activity_returns_none() {
        let conn = db::test_db();
        assert!(get_activity_by_id(&conn, "nope").unwrap().is_none());
    }

    /// The GPS facet: has_gps=true keeps only activities owning a trackpoint
    /// with a latitude; false keeps the rest. A lat-less trackpoint (indoor
    /// session with HR samples) and a geocoded start_lat both count as NO
    /// track — only real coordinates make a route.
    #[test]
    fn filter_by_gps_track_presence() {
        let conn = db::test_db();
        let mut with_gps = sample_activity("with-gps");
        with_gps.start_lat = None;
        insert_activity(&conn, &with_gps).unwrap();
        let mut indoor = sample_activity("indoor");
        // Geocoded start point without a track must still read as "no GPS".
        indoor.start_lat = Some(52.5);
        indoor.start_lon = Some(13.4);
        insert_activity(&conn, &indoor).unwrap();

        conn.execute(
            "INSERT INTO trackpoint (activity_id, lat, lon) VALUES ('with-gps', 52.5, 13.4)",
            [],
        )
        .unwrap();
        // Indoor sessions can still have trackpoints (HR/cadence), just no lat.
        conn.execute(
            "INSERT INTO trackpoint (activity_id, hr) VALUES ('indoor', 140)",
            [],
        )
        .unwrap();

        let with_filter = ActivityFilters { has_gps: Some(true), ..Default::default() };
        let ids: Vec<String> = get_activities(&conn, &with_filter)
            .unwrap()
            .into_iter()
            .map(|a| a.id)
            .collect();
        assert_eq!(ids, vec!["with-gps".to_string()]);

        let without_filter = ActivityFilters { has_gps: Some(false), ..Default::default() };
        let ids: Vec<String> = get_activities(&conn, &without_filter)
            .unwrap()
            .into_iter()
            .map(|a| a.id)
            .collect();
        assert_eq!(ids, vec!["indoor".to_string()]);

        // The calendar shares the facet through the prefix-less query path.
        let days = get_calendar_data(&conn, 2025, 6, &without_filter).unwrap();
        assert_eq!(days.len(), 1);
        assert_eq!(days[0].activities[0].id, "indoor");
    }

    /// The date-picker's year dropdown spans the first..last activity years;
    /// an empty library yields None (the frontend falls back to today).
    #[test]
    fn year_range_spans_first_to_last_activity() {
        let conn = db::test_db();
        assert_eq!(get_activity_year_range(&conn).unwrap(), None);

        let mut old = sample_activity("old");
        old.start_time = "2019-03-15T08:00:00+00:00".to_string();
        insert_activity(&conn, &old).unwrap();
        let mut recent = sample_activity("recent");
        recent.start_time = "2026-07-01T08:00:00+00:00".to_string();
        insert_activity(&conn, &recent).unwrap();

        assert_eq!(get_activity_year_range(&conn).unwrap(), Some((2019, 2026)));
    }

    /// The DB-layer backstop for the UI title cap: an overlong title is
    /// truncated to 100 CHARS (multi-byte safe), not stored verbatim.
    #[test]
    fn update_activity_truncates_overlong_title() {
        let conn = db::test_db();
        insert_activity(&conn, &sample_activity("test-cap")).unwrap();

        // Cyrillic (2 bytes/char) proves the cut counts chars, not bytes.
        let long: String = "ы".repeat(150);
        let upd = ActivityUpdate { title: Some(long), ..Default::default() };
        update_activity(&conn, "test-cap", &upd).unwrap();

        let loaded = get_activity_by_id(&conn, "test-cap").unwrap().unwrap();
        let stored = loaded.title.unwrap();
        assert_eq!(stored.chars().count(), 100);
        assert_eq!(stored, "ы".repeat(100));
    }

    #[test]
    fn update_activity_fields() {
        let conn = db::test_db();
        insert_activity(&conn, &sample_activity("test-u")).unwrap();

        let upd = ActivityUpdate {
            title: Some("Evening Run".to_string()),
            notes: Some("Felt great".to_string()),
            sport_type: None,
            location_name: None,
            start_lat: None,
            start_lon: None,
        };
        update_activity(&conn, "test-u", &upd).unwrap();

        let loaded = get_activity_by_id(&conn, "test-u").unwrap().unwrap();
        assert_eq!(loaded.title, Some("Evening Run".to_string()));
        assert_eq!(loaded.notes, Some("Felt great".to_string()));
        assert_eq!(loaded.sport_type, "run"); // unchanged
    }

    #[test]
    fn update_activity_location_fields() {
        let conn = db::test_db();
        insert_activity(&conn, &sample_activity("test-loc")).unwrap();

        let upd = ActivityUpdate {
            title: None,
            notes: None,
            sport_type: None,
            location_name: Some("Moscow".to_string()),
            start_lat: Some(55.75),
            start_lon: Some(37.62),
        };
        update_activity(&conn, "test-loc", &upd).unwrap();

        let loaded = get_activity_by_id(&conn, "test-loc").unwrap().unwrap();
        assert_eq!(loaded.location_name, Some("Moscow".to_string()));
        assert_eq!(loaded.start_lat, Some(55.75));
        assert_eq!(loaded.start_lon, Some(37.62));
    }

    #[test]
    fn clear_location_marks_name_and_nulls_coords() {
        let conn = db::test_db();
        insert_activity(&conn, &sample_activity("test-clr")).unwrap();
        let upd = ActivityUpdate {
            title: None,
            notes: None,
            sport_type: None,
            location_name: Some("Moscow".to_string()),
            start_lat: Some(55.75),
            start_lon: Some(37.62),
        };
        update_activity(&conn, "test-clr", &upd).unwrap();

        clear_location(&conn, "test-clr").unwrap();

        let loaded = get_activity_by_id(&conn, "test-clr").unwrap().unwrap();
        // "" (not NULL): NULL would put the activity back into the background
        // geocoding queue, resurrecting the name the user just erased.
        assert_eq!(loaded.location_name.as_deref(), Some(""));
        assert_eq!(loaded.start_lat, None);
        assert_eq!(loaded.start_lon, None);

        // A trackpoint gives the activity coordinates — even so, the cleared
        // marker keeps it OUT of the geocoding queue.
        conn.execute(
            "INSERT INTO trackpoint (activity_id, lat, lon) VALUES ('test-clr', 55.75, 37.62)",
            [],
        )
        .unwrap();
        let queue = get_activities_without_location(&conn).unwrap();
        assert!(
            !queue.iter().any(|(id, _, _)| id == "test-clr"),
            "cleared activity must not be re-geocoded"
        );
    }

    #[test]
    fn update_activity_location_text_only() {
        let conn = db::test_db();
        insert_activity(&conn, &sample_activity("test-loc2")).unwrap();

        let upd = ActivityUpdate {
            title: None,
            notes: None,
            sport_type: None,
            location_name: Some("Some Gym".to_string()),
            start_lat: None,
            start_lon: None,
        };
        update_activity(&conn, "test-loc2", &upd).unwrap();

        let loaded = get_activity_by_id(&conn, "test-loc2").unwrap().unwrap();
        assert_eq!(loaded.location_name, Some("Some Gym".to_string()));
        assert_eq!(loaded.start_lat, None);
        assert_eq!(loaded.start_lon, None);
    }

    #[test]
    fn start_locations_includes_manual_coords() {
        let conn = db::test_db();

        let mut a = sample_activity("manual-loc");
        a.start_lat = Some(48.85);
        a.start_lon = Some(2.35);
        a.location_name = Some("Paris".to_string());
        insert_activity(&conn, &a).unwrap();

        let locations = get_activity_start_locations(&conn, &ActivityFilters::default()).unwrap();
        assert_eq!(locations.len(), 1);
        assert!((locations[0].lat - 48.85).abs() < 0.001);
        assert!((locations[0].lon - 2.35).abs() < 0.001);
    }

    #[test]
    fn insert_and_get_with_coords() {
        let conn = db::test_db();
        let mut a = sample_activity("coords-1");
        a.start_lat = Some(40.71);
        a.start_lon = Some(-74.01);
        insert_activity(&conn, &a).unwrap();

        let loaded = get_activity_by_id(&conn, "coords-1").unwrap().unwrap();
        assert_eq!(loaded.start_lat, Some(40.71));
        assert_eq!(loaded.start_lon, Some(-74.01));
    }

    #[test]
    fn delete_activity_removes_it() {
        let conn = db::test_db();
        insert_activity(&conn, &sample_activity("test-d")).unwrap();
        delete_activity(&conn, "test-d").unwrap();
        assert!(get_activity_by_id(&conn, "test-d").unwrap().is_none());
    }

    /// A typed `%`/`_` in the search box matches literally, not as a LIKE
    /// wildcard (same escape_like + ESCAPE treatment as the filter path).
    #[test]
    fn search_treats_like_wildcards_literally() {
        let conn = db::test_db();
        let mut literal = sample_activity("s-lit");
        literal.title = Some("100% effort".to_string());
        let mut decoy = sample_activity("s-dec");
        decoy.title = Some("100x effort".to_string());
        insert_activity(&conn, &literal).unwrap();
        insert_activity(&conn, &decoy).unwrap();

        let hits = search_activities(&conn, "100%").unwrap();
        assert_eq!(hits.len(), 1, "'%' must not act as a wildcard");
        assert_eq!(hits[0].id, "s-lit");

        // Plain substring search still works.
        assert_eq!(search_activities(&conn, "effort").unwrap().len(), 2);
    }

    #[test]
    fn get_activities_with_filters() {
        let conn = db::test_db();

        let mut a1 = sample_activity("a1");
        a1.sport_type = "run".to_string();
        a1.distance_m = Some(3000.0);

        let mut a2 = sample_activity("a2");
        a2.sport_type = "ride".to_string();
        a2.distance_m = Some(20000.0);
        a2.start_time = "2025-07-01T10:00:00+00:00".to_string();

        insert_activity(&conn, &a1).unwrap();
        insert_activity(&conn, &a2).unwrap();

        // Filter by sport type
        let filters = ActivityFilters {
            sport_types: Some(vec!["run".to_string()]),
            ..Default::default()
        };
        let results = get_activities(&conn, &filters).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].sport_type, "run");

        // Several sports match ANY of them; an empty list means "all".
        let filters = ActivityFilters {
            sport_types: Some(vec!["run".to_string(), "ride".to_string()]),
            ..Default::default()
        };
        assert_eq!(get_activities(&conn, &filters).unwrap().len(), 2);
        let filters = ActivityFilters {
            sport_types: Some(Vec::new()),
            ..Default::default()
        };
        assert_eq!(get_activities(&conn, &filters).unwrap().len(), 2);

        // Filter by distance range
        let filters = ActivityFilters {
            distance_min: Some(10000.0),
            ..Default::default()
        };
        let results = get_activities(&conn, &filters).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "a2");

        // Filter by duration range
        let filters = ActivityFilters {
            duration_min: Some(3000.0),
            ..Default::default()
        };
        let results = get_activities(&conn, &filters).unwrap();
        // a1: 1800s, a2: 1800s (default) — neither >= 3000
        assert_eq!(results.len(), 0);

        let filters = ActivityFilters {
            duration_max: Some(1800.0),
            ..Default::default()
        };
        let results = get_activities(&conn, &filters).unwrap();
        assert_eq!(results.len(), 2);

        // Filter by elevation gain range
        let filters = ActivityFilters {
            elev_gain_min: Some(100.0),
            ..Default::default()
        };
        let results = get_activities(&conn, &filters).unwrap();
        // both have elev_gain_m = 50 (default) — neither >= 100
        assert_eq!(results.len(), 0);

        let filters = ActivityFilters {
            elev_gain_max: Some(50.0),
            ..Default::default()
        };
        let results = get_activities(&conn, &filters).unwrap();
        assert_eq!(results.len(), 2);

        // Combined filters: duration + elevation
        let filters = ActivityFilters {
            duration_min: Some(1000.0),
            elev_gain_max: Some(60.0),
            ..Default::default()
        };
        let results = get_activities(&conn, &filters).unwrap();
        assert_eq!(results.len(), 2);

        // No filters — get all
        let all = get_activities(&conn, &ActivityFilters::default()).unwrap();
        assert_eq!(all.len(), 2);
    }

    /// The date_to bound is inclusive of the whole end day — a timestamp at any
    /// time on that date (any offset) must pass, not just midnight.
    #[test]
    fn date_to_filter_includes_the_end_day() {
        let conn = db::test_db();
        let mut a = sample_activity("late");
        a.start_time = "2026-07-05T23:30:00+03:00".to_string();
        let mut b = sample_activity("next");
        b.start_time = "2026-07-06T00:10:00+00:00".to_string();
        insert_activity(&conn, &a).unwrap();
        insert_activity(&conn, &b).unwrap();

        let filters = ActivityFilters {
            date_from: Some("2026-07-01".to_string()),
            date_to: Some("2026-07-05".to_string()),
            ..Default::default()
        };
        let results = get_activities(&conn, &filters).unwrap();
        assert_eq!(results.len(), 1, "only the July-5 activity is in range");
        assert_eq!(results[0].id, "late");
    }

    #[test]
    fn avg_speed_backfill_fills_only_missing() {
        let conn = db::test_db();
        // Missing avg speed, but has distance + duration → derivable.
        let mut a = sample_activity("fill");
        a.avg_speed_mps = None;
        a.distance_m = Some(10000.0);
        a.duration_s = Some(2000.0);
        insert_activity(&conn, &a).unwrap();
        // Already has avg speed → must be left untouched.
        let mut b = sample_activity("keep");
        b.avg_speed_mps = Some(3.0);
        insert_activity(&conn, &b).unwrap();

        // Same statement as migration 022.
        conn.execute(
            "UPDATE activity SET avg_speed_mps = distance_m / duration_s
             WHERE avg_speed_mps IS NULL AND distance_m > 0 AND duration_s > 0",
            [],
        )
        .unwrap();

        let filled = get_activity_by_id(&conn, "fill").unwrap().unwrap();
        assert!((filled.avg_speed_mps.unwrap() - 5.0).abs() < 1e-6);
        let kept = get_activity_by_id(&conn, "keep").unwrap().unwrap();
        assert!((kept.avg_speed_mps.unwrap() - 3.0).abs() < 1e-6);
    }

    #[test]
    fn record_badges_flag_sport_bests() {
        let conn = db::test_db();

        // Two runs: r2 is longer + climbs more; r1 is longer in time.
        let mut r1 = sample_activity("r1");
        r1.distance_m = Some(5000.0);
        r1.duration_s = Some(3600.0);
        r1.elev_gain_m = Some(50.0);
        r1.avg_speed_mps = Some(2.0);
        let mut r2 = sample_activity("r2");
        r2.distance_m = Some(12000.0);
        r2.duration_s = Some(1800.0);
        r2.elev_gain_m = Some(300.0);
        r2.avg_speed_mps = Some(4.0);
        insert_activity(&conn, &r1).unwrap();
        insert_activity(&conn, &r2).unwrap();

        let kinds = |id: &str| {
            let mut k: Vec<String> =
                get_record_badges(&conn, id).unwrap().into_iter().map(|b| b.kind).collect();
            k.sort();
            k
        };

        // r2 holds distance, elevation and pace (speed); r1 holds duration.
        assert_eq!(kinds("r2"), vec!["distance", "elevation", "pace"]);
        assert_eq!(kinds("r1"), vec!["duration"]);

        // A sport with a single activity earns NO badges — a lone activity is
        // trivially its own max on every metric, so "all-time record" is
        // meaningless until there's something to compare against.
        let mut s = sample_activity("s1");
        s.sport_type = "swim".to_string();
        insert_activity(&conn, &s).unwrap();
        assert_eq!(get_record_badges(&conn, "s1").unwrap().len(), 0);

        // Add a second swim → the genuine bests now earn badges.
        let mut s2 = sample_activity("s2");
        s2.sport_type = "swim".to_string();
        s2.distance_m = Some((s.distance_m.unwrap_or(0.0)) + 1000.0);
        insert_activity(&conn, &s2).unwrap();
        assert!(get_record_badges(&conn, "s2").unwrap().iter().any(|b| b.kind == "distance"));
    }

    #[test]
    fn get_activities_free_text_search() {
        let conn = db::test_db();

        let mut a1 = sample_activity("s1");
        a1.title = Some("Hill repeats".to_string());
        a1.notes = Some("felt strong".to_string());
        a1.location_name = Some("Boulder, CO".to_string());

        let mut a2 = sample_activity("s2");
        a2.title = Some("Easy jog".to_string());
        a2.notes = None;
        a2.location_name = Some("Portland".to_string());

        insert_activity(&conn, &a1).unwrap();
        insert_activity(&conn, &a2).unwrap();

        let search = |q: &str| {
            get_activities(
                &conn,
                &ActivityFilters { search: Some(q.to_string()), ..Default::default() },
            )
            .unwrap()
        };

        // Title match (case-insensitive ASCII).
        let r = search("hill");
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].id, "s1");

        // Notes match.
        assert_eq!(search("strong").len(), 1);

        // Location match.
        assert_eq!(search("portland").len(), 1);

        // Blank / whitespace term is ignored (returns everything).
        assert_eq!(search("   ").len(), 2);

        // No match.
        assert_eq!(search("zzz").len(), 0);

        // A literal '%' must not act as a wildcard (it's escaped).
        assert_eq!(search("%").len(), 0);
    }

    #[test]
    fn get_detected_devices_counts() {
        let conn = db::test_db();

        let mut a1 = sample_activity("dev-1");
        a1.source_device = Some("Garmin FR265".to_string());

        let mut a2 = sample_activity("dev-2");
        a2.source_device = Some("Garmin FR265".to_string());
        a2.start_time = "2025-07-01T08:00:00+00:00".to_string();

        let mut a3 = sample_activity("dev-3");
        a3.source_device = Some("Wahoo ELEMNT".to_string());
        a3.start_time = "2025-08-01T08:00:00+00:00".to_string();

        let mut a4 = sample_activity("dev-4");
        a4.source_device = None; // no device — should be excluded
        a4.start_time = "2025-09-01T08:00:00+00:00".to_string();

        insert_activity(&conn, &a1).unwrap();
        insert_activity(&conn, &a2).unwrap();
        insert_activity(&conn, &a3).unwrap();
        insert_activity(&conn, &a4).unwrap();

        let devices = get_detected_devices(&conn).unwrap();
        assert_eq!(devices.len(), 2);
        // Ordered by count DESC, so Garmin first
        assert_eq!(devices[0].device_name, "Garmin FR265");
        assert_eq!(devices[0].activity_count, 2);
        assert_eq!(devices[1].device_name, "Wahoo ELEMNT");
        assert_eq!(devices[1].activity_count, 1);
    }

    #[test]
    fn get_detected_devices_empty() {
        let conn = db::test_db();
        let devices = get_detected_devices(&conn).unwrap();
        assert!(devices.is_empty());
    }

    #[test]
    fn search_finds_by_title_and_device() {
        let conn = db::test_db();
        insert_activity(&conn, &sample_activity("s1")).unwrap();

        let results = search_activities(&conn, "Morning").unwrap();
        assert_eq!(results.len(), 1);

        let results = search_activities(&conn, "Garmin").unwrap();
        assert_eq!(results.len(), 1);

        let results = search_activities(&conn, "nonexistent").unwrap();
        assert!(results.is_empty());
    }

    /// Merged legs are hidden from every list — search included. A leg found
    /// by title would open a page that double-counts its container.
    #[test]
    fn search_excludes_merged_legs() {
        let conn = db::test_db();
        conn.execute(
            "INSERT INTO activity (id, start_time, title) VALUES ('tri', '2025-06-01T08:00:00', 'Tri')",
            [],
        )
        .unwrap();
        let mut a = sample_activity("leg-1");
        a.parent_id = Some("tri".to_string());
        insert_activity(&conn, &a).unwrap();

        let results = search_activities(&conn, "Morning").unwrap();
        assert!(results.is_empty(), "the adopted leg must not surface in search");
    }

    #[test]
    fn calendar_data_groups_by_day() {
        let conn = db::test_db();

        let mut a1 = sample_activity("c1");
        a1.start_time = "2025-06-01T08:00:00".to_string();
        a1.distance_m = Some(5000.0);
        a1.duration_s = Some(1800.0);
        a1.elev_gain_m = Some(120.0);

        let mut a2 = sample_activity("c2");
        a2.start_time = "2025-06-01T18:00:00".to_string();
        a2.sport_type = "ride".to_string();
        a2.distance_m = Some(20000.0);
        a2.duration_s = Some(3600.0);
        a2.elev_gain_m = Some(350.0);

        let mut a3 = sample_activity("c3");
        a3.start_time = "2025-06-15T10:00:00".to_string();
        a3.distance_m = Some(10000.0);
        a3.duration_s = Some(3000.0);
        a3.elev_gain_m = None;

        insert_activity(&conn, &a1).unwrap();
        insert_activity(&conn, &a2).unwrap();
        insert_activity(&conn, &a3).unwrap();

        let days = get_calendar_data(&conn, 2025, 6, &ActivityFilters::default()).unwrap();
        assert_eq!(days.len(), 2); // June 1 and June 15

        let day1 = &days[0];
        assert_eq!(day1.date, "2025-06-01");
        assert_eq!(day1.activity_count, 2);
        assert!((day1.total_distance_m - 25000.0).abs() < 0.1);
        assert!((day1.total_duration_s - 5400.0).abs() < 0.1);
        assert!((day1.total_elev_gain_m - 470.0).abs() < 0.1);
        assert!(day1.sport_types.contains(&"run".to_string()));
        assert!(day1.sport_types.contains(&"ride".to_string()));

        let day15 = &days[1];
        assert_eq!(day15.date, "2025-06-15");
        assert_eq!(day15.activity_count, 1);
        // No stored elevation reads as 0, not a poisoned NaN/None sum.
        assert_eq!(day15.total_elev_gain_m, 0.0);

        // Different month returns empty
        let empty = get_calendar_data(&conn, 2025, 7, &ActivityFilters::default()).unwrap();
        assert!(empty.is_empty());
    }

    #[test]
    fn calendar_data_respects_search() {
        let conn = db::test_db();

        let mut a1 = sample_activity("c1");
        a1.start_time = "2025-06-01T08:00:00".to_string();
        a1.title = Some("Hill repeats".to_string());

        let mut a2 = sample_activity("c2");
        a2.start_time = "2025-06-02T08:00:00".to_string();
        a2.title = Some("Easy jog".to_string());

        insert_activity(&conn, &a1).unwrap();
        insert_activity(&conn, &a2).unwrap();

        let with_search = |q: &str| ActivityFilters { search: Some(q.to_string()), ..Default::default() };

        // Without search: both days present.
        assert_eq!(get_calendar_data(&conn, 2025, 6, &ActivityFilters::default()).unwrap().len(), 2);

        // With search: only the matching day, and only the matching activity.
        let days = get_calendar_data(&conn, 2025, 6, &with_search("hill")).unwrap();
        assert_eq!(days.len(), 1);
        assert_eq!(days[0].date, "2025-06-01");
        assert_eq!(days[0].activity_count, 1);

        // Blank search is ignored (both days back).
        assert_eq!(get_calendar_data(&conn, 2025, 6, &with_search("  ")).unwrap().len(), 2);
    }

    #[test]
    fn calendar_data_respects_facet_filters() {
        let conn = db::test_db();

        let mut run = sample_activity("c1");
        run.start_time = "2025-06-01T08:00:00".to_string();
        run.sport_type = "run".to_string();

        let mut ride = sample_activity("c2");
        ride.start_time = "2025-06-02T08:00:00".to_string();
        ride.sport_type = "ride".to_string();

        insert_activity(&conn, &run).unwrap();
        insert_activity(&conn, &ride).unwrap();

        // Sport filter narrows the calendar to matching days only.
        let only_ride = ActivityFilters {
            sport_types: Some(vec!["ride".to_string()]),
            ..Default::default()
        };
        let days = get_calendar_data(&conn, 2025, 6, &only_ride).unwrap();
        assert_eq!(days.len(), 1);
        assert_eq!(days[0].date, "2025-06-02");
        assert!(days[0].sport_types.contains(&"ride".to_string()));
    }

    #[test]
    fn recompute_sport_types_upgrades_from_sub_sport() {
        let conn = db::test_db();

        // swim + open_water sub_sport → open_water
        let mut a1 = sample_activity("r1");
        a1.sport_type = "swim".to_string();
        a1.sub_sport = Some("open_water".to_string());

        // other + yoga sub_sport → yoga
        let mut a2 = sample_activity("r2");
        a2.sport_type = "other".to_string();
        a2.sub_sport = Some("yoga".to_string());

        // run with no sub_sport → unchanged
        let mut a3 = sample_activity("r3");
        a3.sport_type = "run".to_string();
        a3.sub_sport = None;

        insert_activity(&conn, &a1).unwrap();
        insert_activity(&conn, &a2).unwrap();
        insert_activity(&conn, &a3).unwrap();

        let changed = recompute_sport_types(&conn).unwrap();
        assert_eq!(changed, 2);

        let sport = |id: &str| -> String {
            conn.query_row("SELECT sport_type FROM activity WHERE id = ?1", params![id], |r| r.get(0))
                .unwrap()
        };
        assert_eq!(sport("r1"), "open_water");
        assert_eq!(sport("r2"), "yoga");
        assert_eq!(sport("r3"), "run");

        // Idempotent: a second pass changes nothing.
        assert_eq!(recompute_sport_types(&conn).unwrap(), 0);
    }

    #[test]
    fn used_sport_types_returns_only_present() {
        let conn = db::test_db();
        assert!(get_used_sport_types(&conn).unwrap().is_empty());

        let mut a1 = sample_activity("u1");
        a1.sport_type = "run".to_string();
        let mut a2 = sample_activity("u2");
        a2.sport_type = "run".to_string();
        let mut a3 = sample_activity("u3");
        a3.sport_type = "swim".to_string();
        insert_activity(&conn, &a1).unwrap();
        insert_activity(&conn, &a2).unwrap();
        insert_activity(&conn, &a3).unwrap();

        let used = get_used_sport_types(&conn).unwrap();
        assert_eq!(used.len(), 2);
        assert_eq!(used[0], "run"); // most-used first
        assert!(used.contains(&"swim".to_string()));
        assert!(!used.contains(&"ride".to_string()));
    }

    #[test]
    fn get_start_locations_with_and_without_gps() {
        use crate::models::trackpoint::TrackPoint;

        let conn = db::test_db();

        // Activity with GPS trackpoints
        let mut a1 = sample_activity("loc-gps");
        a1.sport_type = "ride".to_string();
        a1.title = Some("Outdoor Ride".to_string());
        a1.distance_m = Some(20000.0);
        a1.duration_s = Some(3600.0);
        insert_activity(&conn, &a1).unwrap();

        db::trackpoints::insert_trackpoints(
            &conn,
            &[
                TrackPoint {
                    activity_id: "loc-gps".to_string(),
                    t: Some("0".to_string()),
                    lat: Some(55.75),
                    lon: Some(37.62),
                    altitude_m: Some(150.0),
                    speed_mps: Some(5.0),
                    hr: None,
                    cadence: None,
                    power_w: None,
                    temperature_c: None,
                    vertical_oscillation_mm: None, stance_time_ms: None, stance_time_percent: None, step_length_mm: None, grade_percent: None,
                    left_right_balance: None, left_torque_effectiveness: None, right_torque_effectiveness: None,
                    left_pedal_smoothness: None, right_pedal_smoothness: None,
                },
                TrackPoint {
                    activity_id: "loc-gps".to_string(),
                    t: Some("10".to_string()),
                    lat: Some(55.76),
                    lon: Some(37.63),
                    altitude_m: Some(155.0),
                    speed_mps: Some(5.2),
                    hr: None,
                    cadence: None,
                    power_w: None,
                    temperature_c: None,
                    vertical_oscillation_mm: None, stance_time_ms: None, stance_time_percent: None, step_length_mm: None, grade_percent: None,
                    left_right_balance: None, left_torque_effectiveness: None, right_torque_effectiveness: None,
                    left_pedal_smoothness: None, right_pedal_smoothness: None,
                },
            ],
        )
        .unwrap();

        // Indoor activity (no GPS)
        let mut a2 = sample_activity("loc-indoor");
        a2.sport_type = "strength".to_string();
        a2.title = Some("Gym Session".to_string());
        a2.start_time = "2025-07-01T10:00:00+00:00".to_string();
        insert_activity(&conn, &a2).unwrap();

        db::trackpoints::insert_trackpoints(
            &conn,
            &[TrackPoint {
                activity_id: "loc-indoor".to_string(),
                t: Some("0".to_string()),
                lat: None,
                lon: None,
                altitude_m: None,
                speed_mps: None,
                hr: Some(120),
                cadence: None,
                power_w: None,
                temperature_c: None,
                vertical_oscillation_mm: None, stance_time_ms: None, stance_time_percent: None, step_length_mm: None, grade_percent: None,
                left_right_balance: None, left_torque_effectiveness: None, right_torque_effectiveness: None,
                left_pedal_smoothness: None, right_pedal_smoothness: None,
            }],
        )
        .unwrap();

        // Activity with no trackpoints at all
        let mut a3 = sample_activity("loc-empty");
        a3.start_time = "2025-08-01T10:00:00+00:00".to_string();
        insert_activity(&conn, &a3).unwrap();

        let locations = get_activity_start_locations(&conn, &ActivityFilters::default()).unwrap();

        // Only the GPS activity should appear
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].id, "loc-gps");
        assert_eq!(locations[0].sport_type, "ride");
        assert_eq!(locations[0].title, Some("Outdoor Ride".to_string()));
        assert_eq!(locations[0].distance_m, Some(20000.0));
        assert_eq!(locations[0].duration_s, Some(3600.0));
        assert!((locations[0].lat - 55.75).abs() < 0.001);
        assert!((locations[0].lon - 37.62).abs() < 0.001);
    }

    #[test]
    fn get_start_locations_empty_db() {
        let conn = db::test_db();
        let locations = get_activity_start_locations(&conn, &ActivityFilters::default()).unwrap();
        assert!(locations.is_empty());
    }

    #[test]
    fn start_locations_respect_search() {
        let conn = db::test_db();

        let mut a1 = sample_activity("m1");
        a1.title = Some("Hill repeats".to_string());
        a1.start_lat = Some(55.75);
        a1.start_lon = Some(37.62);

        let mut a2 = sample_activity("m2");
        a2.title = Some("Easy jog".to_string());
        a2.sport_type = "ride".to_string();
        a2.start_lat = Some(40.71);
        a2.start_lon = Some(-74.01);

        insert_activity(&conn, &a1).unwrap(); // m1 = run
        insert_activity(&conn, &a2).unwrap(); // m2 = ride

        let with_search = |q: &str| ActivityFilters { search: Some(q.to_string()), ..Default::default() };
        let with_sport = |s: &str| ActivityFilters { sport_types: Some(vec![s.to_string()]), ..Default::default() };

        // No filters: both pins.
        assert_eq!(get_activity_start_locations(&conn, &ActivityFilters::default()).unwrap().len(), 2);

        // Search narrows to the matching activity.
        let hits = get_activity_start_locations(&conn, &with_search("hill")).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "m1");

        // Blank search is ignored.
        assert_eq!(get_activity_start_locations(&conn, &with_search(" ")).unwrap().len(), 2);

        // Sport filter keeps only the matching pin (positive cases)…
        let runs = get_activity_start_locations(&conn, &with_sport("run")).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].id, "m1");
        let rides = get_activity_start_locations(&conn, &with_sport("ride")).unwrap();
        assert_eq!(rides.len(), 1);
        assert_eq!(rides[0].id, "m2");

        // …and drops everything when no activity matches.
        assert_eq!(get_activity_start_locations(&conn, &with_sport("swim")).unwrap().len(), 0);
    }
}
