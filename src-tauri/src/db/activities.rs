use rusqlite::{params, Connection, OptionalExtension, Result};

use crate::models::activity::{
    Activity, ActivityFilters, ActivityLocation, ActivitySummary, ActivityUpdate, CalDayActivity,
    DaySummary, DeviceStats, SIMULATED_COURSE_SPORTS,
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
     a.duration_s, a.elev_gain_m, a.avg_speed_mps, a.avg_hr, a.location_name, a.source_device, \
     a.gear_id";

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
        source_device: row.get(10)?,
        gear_id: row.get(11)?,
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
/// range, distance, duration, elevation and devices — to `conditions`, binding
/// their values into `params` and advancing `idx`. `prefix` is the column
/// qualifier (`"a."` for aliased queries, `""` otherwise). Sort/limit/offset
/// are NOT handled here. Shared by the list, calendar and map so every view
/// honours the same filters.
pub(crate) fn push_facet_conditions(
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
    if let Some(ref devices) = filters.devices {
        // Raw source_device values; "" stands for "no device" (NULL).
        let none = devices.iter().any(|d| d.is_empty());
        let named: Vec<&String> = devices.iter().filter(|d| !d.is_empty()).collect();
        let mut parts: Vec<String> = Vec::new();
        if !named.is_empty() {
            let placeholders: Vec<String> = named
                .iter()
                .map(|d| {
                    params.push(Box::new((*d).clone()));
                    let p = format!("?{}", *idx);
                    *idx += 1;
                    p
                })
                .collect();
            parts.push(format!("{prefix}source_device IN ({})", placeholders.join(", ")));
        }
        if none {
            // The same set get_detected_devices files under "" (COALESCE):
            // NULL and, should one ever be stored, the empty string.
            parts.push(format!("({prefix}source_device IS NULL OR {prefix}source_device = '')"));
        }
        if !parts.is_empty() {
            conditions.push(format!("({})", parts.join(" OR ")));
        }
    }
    if let Some(ref gear) = filters.gear_ids {
        // Item ids; "" stands for "no gear" (NULL). Same shape as devices.
        let none = gear.iter().any(|g| g.is_empty());
        let named: Vec<&String> = gear.iter().filter(|g| !g.is_empty()).collect();
        let mut parts: Vec<String> = Vec::new();
        if !named.is_empty() {
            let placeholders: Vec<String> = named
                .iter()
                .map(|g| {
                    params.push(Box::new((*g).clone()));
                    let p = format!("?{}", *idx);
                    *idx += 1;
                    p
                })
                .collect();
            parts.push(format!("{prefix}gear_id IN ({})", placeholders.join(", ")));
        }
        if none {
            parts.push(format!("{prefix}gear_id IS NULL"));
        }
        if !parts.is_empty() {
            conditions.push(format!("({})", parts.join(" OR ")));
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

/// The FTP an activity is filed under and the two numbers derived from it
/// — written together, because one without the others is a lie.
pub fn set_power_metrics(
    conn: &Connection,
    id: &str,
    threshold_power_w: f64,
    intensity_factor: f64,
    training_stress_score: f64,
) -> Result<()> {
    conn.execute(
        "UPDATE activity SET threshold_power_w = ?1, intensity_factor = ?2, training_stress_score = ?3 WHERE id = ?4",
        params![threshold_power_w, intensity_factor, training_stress_score, id],
    )?;
    Ok(())
}

/// An earlier activity with an FTP — what a ride's FTP is compared against
/// (#139): its FTP, device and date.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PreviousPower {
    pub activity_id: String,
    pub start_time: String,
    pub threshold_power_w: f64,
    pub source_device: Option<String>,
}

/// How many earlier rides the FTP hint looks back over. The hint compares
/// a ride against the FTP most of these carry, which assumes an FTP that
/// holds still between rides — true when the device's auto-detection is
/// off, as it is meant to be. With auto-detection on, the value drifts
/// almost every ride, no three agree, the majority degenerates to the
/// newest neighbor, and the hint would show all the time.
pub const RECENT_POWER_RIDES: usize = 3;

/// The newest `limit` activities with an FTP that started before `id`,
/// newest first. Ordered by start time, not import time: a rider thinks in
/// ride dates, and an old file imported late belongs in the past. Legs of a
/// multisport activity are excluded by `parent_id IS NULL`; containers
/// carry no power metrics today, so they fall out through
/// `threshold_power_w IS NOT NULL` — should they ever aggregate power,
/// this query would start seeing them.
pub fn recent_power_activities(conn: &Connection, id: &str, limit: usize) -> Result<Vec<PreviousPower>> {
    let mut stmt = conn.prepare(
        "SELECT id, start_time, threshold_power_w, source_device FROM activity
         WHERE threshold_power_w IS NOT NULL AND parent_id IS NULL AND id != ?1
           AND start_time < (SELECT start_time FROM activity WHERE id = ?1)
         ORDER BY start_time DESC LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![id, limit as i64], |r| {
        Ok(PreviousPower {
            activity_id: r.get(0)?,
            start_time: r.get(1)?,
            threshold_power_w: r.get(2)?,
            source_device: r.get(3)?,
        })
    })?;
    rows.collect()
}

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
    // A segment drawn on a simulated course (#198) is kept off every list
    // while its source lives; with the source gone it would come back as
    // an orphan, so it goes along. A real activity's segments stay (the
    // source link turns NULL).
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        &format!(
            "DELETE FROM segment WHERE source_activity_id = ?1 AND NOT {}",
            crate::db::segments::on_real_ground("segment")
        ),
        params![id],
    )?;
    tx.execute("DELETE FROM activity WHERE id = ?1", params![id])?;
    tx.commit()?;
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
    // distance, search, devices, …) are intersected on top so the calendar honours
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

/// File the rides already imported as plain `ride` under the trainer kinds
/// their stored `sub_sport` names (#189: `indoor_ride`, `virtual_ride`).
/// Narrower than [`recompute_sport_types`] on purpose: only a `ride` moves,
/// and only to one of the two new kinds, so a sport the user corrected by
/// hand stays as set. A leg of a merged event stays too — the event was
/// built on it being a ride, and a trainer ride is no leg. The bike that is
/// the default for `ride` becomes the default for both new kinds as well,
/// where none is set: the rides that move used to get it, and so should
/// the ones still to come. Returns the number of activities changed.
pub fn refile_rides_by_sub_sport(conn: &Connection) -> Result<usize> {
    use crate::models::activity::SportType;
    for kind in ["indoor_ride", "virtual_ride"] {
        conn.execute(
            "INSERT OR IGNORE INTO gear_default (sport_type, gear_id)
             SELECT ?1, gear_id FROM gear_default WHERE sport_type = 'ride'",
            params![kind],
        )?;
    }
    let rows: Vec<(String, String)> = {
        let mut stmt = conn.prepare(
            "SELECT id, sub_sport FROM activity
             WHERE sport_type = 'ride' AND sub_sport IS NOT NULL AND parent_id IS NULL",
        )?;
        let mapped = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        let mut v = Vec::new();
        for r in mapped {
            v.push(r?);
        }
        v
    };
    let mut changed = 0usize;
    for (id, sub_sport) in rows {
        // As an import would: the sub_sport under a ride (Garmin's shared
        // `virtual_activity` names a kind only there).
        let kind = SportType::resolve(Some("ride"), Some(&sub_sport));
        if matches!(kind, SportType::IndoorRide | SportType::VirtualRide) {
            conn.execute(
                "UPDATE activity SET sport_type = ?1 WHERE id = ?2",
                params![kind.as_str(), id],
            )?;
            changed += 1;
        }
    }
    Ok(changed)
}

/// File the runs done on a simulator (#192): a `run` whose sub_sport is
/// Garmin's `virtual_activity` becomes a `virtual_run`, and the traces its
/// simulated course left while it was a run go at once — its segment
/// efforts, and with `clear_names` the place name the geocoder gave it
/// (the same rule as a sport change in the edit modal). A leg of a merged
/// event stays a run, as a trainer ride stays a ride. Shoes that are the
/// default for `run` become the default for `virtual_run` where none is
/// set. One transaction: a refiled run is never left with its old traces.
/// Returns the number of activities moved.
///
/// Reach: only a FIT run names its simulator in the sub_sport. A GPX/TCX
/// whose type read "Virtual Run" was filed as a plain `run` with no
/// sub_sport before #192 and stays so — the sport badge moves it. On a
/// vault so old that `sport_types_backfilled_v1` runs first, that pass
/// files these runs (legs included) itself; the startup scrub of #190,
/// whose flag such a vault lacks too, then takes their traces off.
pub fn refile_virtual_runs(conn: &Connection, clear_names: bool) -> Result<usize> {
    use crate::models::activity::SportType;
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT OR IGNORE INTO gear_default (sport_type, gear_id)
         SELECT 'virtual_run', gear_id FROM gear_default WHERE sport_type = 'run'",
        [],
    )?;
    let rows: Vec<(String, String)> = {
        let mut stmt = tx.prepare(
            "SELECT id, sub_sport FROM activity
             WHERE sport_type = 'run' AND sub_sport IS NOT NULL AND parent_id IS NULL",
        )?;
        let mapped = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        let mut v = Vec::new();
        for r in mapped {
            v.push(r?);
        }
        v
    };
    let mut changed = 0usize;
    for (id, sub_sport) in rows {
        // As an import would: the sub_sport under a run.
        if SportType::resolve(Some("run"), Some(&sub_sport)) != SportType::VirtualRun {
            continue;
        }
        tx.execute(
            "UPDATE activity SET sport_type = 'virtual_run' WHERE id = ?1",
            params![id],
        )?;
        scrub_simulated_course(&tx, Some(&id), clear_names)?;
        changed += 1;
    }
    tx.commit()?;
    Ok(changed)
}

/// Take a simulated course's traces of the real world off (#190): every
/// segment effort it earned while it was a ride, and — with `clear_names`,
/// i.e. only while the geocoder is switched on, since a name it could not
/// have written is the user's — the place name without manual coordinates
/// (a picked point writes `start_lat`; a typed name does too when the
/// lookup succeeds, and stays as typed otherwise; "" is the cleared marker
/// and stays). One activity, or every one whose sport has a simulated
/// course. Returns (names cleared, efforts removed). Accepted residue: a
/// name typed while the geocoder was off keeps no mark of its origin, so
/// once the geocoder is on it reads as the geocoder's.
pub fn scrub_simulated_course(
    conn: &Connection,
    only: Option<&str>,
    clear_names: bool,
) -> Result<(usize, usize)> {
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut idx = 1;
    let not_simulated = not_simulated_clause("", &mut params, &mut idx);
    let only_clause = match only {
        Some(id) => {
            params.push(Box::new(id.to_string()));
            format!("AND id = ?{idx}")
        }
        None => String::new(),
    };
    let simulated = format!("NOT ({not_simulated}) {only_clause}");
    let params_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let names = if clear_names {
        conn.execute(
            &format!(
                "UPDATE activity SET location_name = NULL
                 WHERE {simulated} AND start_lat IS NULL AND COALESCE(location_name, '') <> ''"
            ),
            params_refs.as_slice(),
        )?
    } else {
        0
    };
    let efforts = conn.execute(
        &format!("DELETE FROM segment_effort WHERE activity_id IN (SELECT id FROM activity WHERE {simulated})"),
        params_refs.as_slice(),
    )?;
    Ok((names, efforts))
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

/// The activity profile the file was recorded under (ADR 0003, rules).
pub fn set_profile_name(conn: &Connection, id: &str, profile: Option<&str>) -> Result<()> {
    conn.execute(
        "UPDATE activity SET profile_name = ?1 WHERE id = ?2",
        params![profile, id],
    )?;
    Ok(())
}

/// Overwrite the recording device an activity names (the #144 backfill).
pub fn set_source_device(conn: &Connection, id: &str, device: Option<&str>) -> Result<()> {
    conn.execute(
        "UPDATE activity SET source_device = ?1 WHERE id = ?2",
        params![device, id],
    )?;
    Ok(())
}

/// Every distinct recording device in the library with its activity count
/// and last use, most used first. Activities without a device form one
/// group under the empty name, so a filter can offer "No device" with a
/// count. Merged-triathlon legs are left out, as everywhere the library is
/// listed.
pub fn get_detected_devices(conn: &Connection) -> Result<Vec<DeviceStats>> {
    let mut stmt = conn.prepare(
        "SELECT COALESCE(source_device, '') AS dev, COUNT(*) as cnt, MAX(start_time) as last_time
         FROM activity
         WHERE parent_id IS NULL
         GROUP BY dev
         ORDER BY cnt DESC, dev",
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
    // A simulated course has no place on the map of real ones (#190).
    conditions.push(not_simulated_clause("a.", &mut params, &mut idx));
    let where_sql = format!("WHERE {}", conditions.join(" AND "));

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

/// Write the background geocoder's answer — only if the activity still
/// waits for one (#198). The geocoder reads its list, lets the lock go and
/// asks the network; meanwhile the user may have typed a name, picked a
/// point, or turned the activity into a sport whose course is simulated
/// (the edit modal, the startup refile). Any of those wins over the late
/// answer. Returns whether the row took it.
pub fn set_geocoded_name(conn: &Connection, id: &str, name: &str) -> Result<bool> {
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> =
        vec![Box::new(name.to_string()), Box::new(id.to_string())];
    let mut idx = 3;
    let not_simulated = not_simulated_clause("", &mut params, &mut idx);
    let params_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let changed = conn.execute(
        &format!(
            "UPDATE activity SET location_name = ?1
             WHERE id = ?2 AND location_name IS NULL AND {not_simulated}"
        ),
        params_refs.as_slice(),
    )?;
    Ok(changed > 0)
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

/// `<prefix>sport_type NOT IN (?N, …)` over the simulated-course sports
/// (#190), appending their parameters; `idx` moves past them.
fn not_simulated_clause(
    prefix: &str,
    params: &mut Vec<Box<dyn rusqlite::types::ToSql>>,
    idx: &mut usize,
) -> String {
    let marks: Vec<String> = SIMULATED_COURSE_SPORTS
        .iter()
        .map(|s| {
            params.push(Box::new(s.to_string()));
            *idx += 1;
            format!("?{}", *idx - 1)
        })
        .collect();
    format!("{prefix}sport_type NOT IN ({})", marks.join(", "))
}

/// Activities still to be named by the background geocoder: never looked
/// up (`location_name` NULL — "" is the cleared marker). A virtual ride is
/// left out (#190): its start point would name an island in the Pacific.
pub fn get_activities_without_location(conn: &Connection) -> Result<Vec<(String, f64, f64)>> {
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut idx = 1;
    let not_simulated = not_simulated_clause("a.", &mut params, &mut idx);
    let mut stmt = conn.prepare(&format!(
        "SELECT a.id,
                (SELECT lat FROM trackpoint WHERE activity_id = a.id AND lat IS NOT NULL ORDER BY rowid ASC LIMIT 1),
                (SELECT lon FROM trackpoint WHERE activity_id = a.id AND lon IS NOT NULL ORDER BY rowid ASC LIMIT 1)
         FROM activity a
         WHERE a.location_name IS NULL AND {not_simulated}"
    ))?;
    let params_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let rows = stmt.query_map(params_refs.as_slice(), |row| {
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

/// The start_time of the activity closest in time to `unix_ts` — its
/// explicit UTC offset is the best guess for the device's clock at that
/// moment (Monitor files carry no usable offset of their own, ADR 0002).
pub fn start_time_nearest(conn: &Connection, unix_ts: i64) -> Result<Option<String>> {
    // A start_time SQLite cannot read would sort FIRST under ORDER BY (NULL
    // < everything) and hand every Monitor file garbage — skip such rows.
    conn.query_row(
        "SELECT start_time FROM activity
         WHERE strftime('%s', start_time) IS NOT NULL
         ORDER BY ABS(CAST(strftime('%s', start_time) AS INTEGER) - ?1) LIMIT 1",
        params![unix_ts],
        |r| r.get(0),
    )
    .optional()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    /// The hint looks back over the newest EARLIER activities with an FTP —
    /// not later ones, not a leg of a triathlon, not itself — newest first,
    /// capped.
    #[test]
    fn recent_power_activities_are_the_newest_earlier_ones_with_an_ftp() {
        let conn = db::test_db();
        let mut a0 = sample_activity("a0"); a0.start_time = "2026-09-01T08:00:00+00:00".into(); a0.threshold_power_w = Some(235.0);
        let mut a1 = sample_activity("a1"); a1.start_time = "2026-09-05T08:00:00+00:00".into(); a1.threshold_power_w = Some(238.0);
        let mut a = sample_activity("a"); a.start_time = "2026-09-10T08:00:00+00:00".into(); a.threshold_power_w = Some(238.0); a.source_device = Some("fenix 7".into());
        let mut b = sample_activity("b"); b.start_time = "2026-09-12T08:00:00+00:00".into(); b.threshold_power_w = None;
        let mut c = sample_activity("c"); c.start_time = "2026-09-16T08:00:00+00:00".into(); c.threshold_power_w = Some(200.0); c.source_device = Some("Edge 840".into());
        let mut d = sample_activity("d"); d.start_time = "2026-09-18T08:00:00+00:00".into(); d.threshold_power_w = Some(250.0);
        // A leg of a merged triathlon carries an FTP but is not a ride of its own.
        let mut leg = sample_activity("leg"); leg.start_time = "2026-09-11T08:00:00+00:00".into(); leg.threshold_power_w = Some(199.0); leg.parent_id = Some("tri".into());
        let mut tri = sample_activity("tri"); tri.start_time = "2026-09-11T07:00:00+00:00".into(); tri.threshold_power_w = None;
        for x in [&a0, &a1, &a, &b, &c, &d, &tri, &leg] { insert_activity(&conn, x).unwrap(); }
        let recent = recent_power_activities(&conn, "c", 3).unwrap();
        let ids: Vec<&str> = recent.iter().map(|r| r.activity_id.as_str()).collect();
        assert_eq!(ids, ["a", "a1", "a0"], "b has no FTP, d is later, the leg is a leg; newest first, capped at 3");
        assert_eq!(recent[0].threshold_power_w, 238.0);
        assert_eq!(recent[0].source_device.as_deref(), Some("fenix 7"));
        assert!(recent_power_activities(&conn, "a0", 3).unwrap().is_empty(), "nothing earlier");
    }

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

    /// A virtual ride's points are a simulator's (#190): the library map
    /// leaves it out, and so does the geocoding queue — with or without
    /// manual coordinates, with a track or without.
    #[test]
    fn a_virtual_ride_is_on_no_map_and_in_no_geocoding_queue() {
        let conn = db::test_db();
        let mut zwift = sample_activity("zwift");
        zwift.sport_type = "virtual_ride".to_string();
        zwift.start_lat = Some(-11.64);
        zwift.start_lon = Some(166.95);
        insert_activity(&conn, &zwift).unwrap();
        let mut tracked = sample_activity("zwift-track");
        tracked.sport_type = "virtual_ride".to_string();
        insert_activity(&conn, &tracked).unwrap();
        conn.execute(
            "INSERT INTO trackpoint (activity_id, lat, lon) VALUES ('zwift-track', -11.64, 166.95)",
            [],
        )
        .unwrap();
        let mut road = sample_activity("road");
        road.sport_type = "ride".to_string();
        insert_activity(&conn, &road).unwrap();
        conn.execute("INSERT INTO trackpoint (activity_id, lat, lon) VALUES ('road', 36.54, 32.0)", []).unwrap();

        let on_map: Vec<String> = get_activity_start_locations(&conn, &ActivityFilters::default())
            .unwrap()
            .into_iter()
            .map(|l| l.id)
            .collect();
        assert_eq!(on_map, vec!["road"]);
        let queue: Vec<String> = get_activities_without_location(&conn).unwrap().into_iter().map(|(id, _, _)| id).collect();
        assert_eq!(queue, vec!["road"]);
    }

    /// The scrub (#190) takes the geocoder's name and the segment efforts
    /// off a virtual ride, and nothing else: a name the user typed or
    /// picked (it carries start_lat) stays, the cleared marker stays, a
    /// road ride is untouched.
    #[test]
    fn scrub_takes_the_geocoded_name_and_the_efforts_off_a_virtual_ride() {
        let conn = db::test_db();
        let row = |id: &str, sport: &str, name: Option<&str>, lat: Option<f64>| {
            let mut a = sample_activity(id);
            a.sport_type = sport.to_string();
            a.location_name = name.map(str::to_string);
            a.start_lat = lat;
            a.start_lon = lat.map(|_| 166.95);
            insert_activity(&conn, &a).unwrap();
        };
        row("geocoded", "virtual_ride", Some("Solomon Islands"), None);
        row("typed", "virtual_ride", Some("Watopia"), Some(-11.64));
        row("cleared", "virtual_ride", Some(""), None);
        row("unnamed", "virtual_ride", None, None);
        row("road", "ride", Some("Alanya"), None);
        conn.execute("INSERT INTO segment (id, name, sport, source_activity_id, created_at, distance_m, start_lat, start_lon, end_lat, end_lon, min_lat, max_lat, min_lon, max_lon) VALUES ('s', 'S', 'ride', 'road', '', 100, 0, 0, 0, 0, 0, 0, 0, 0)", []).unwrap();
        for id in ["geocoded", "road"] {
            conn.execute(
                "INSERT INTO segment_effort (segment_id, activity_id, start_idx, end_idx, elapsed_s, distance_m) VALUES ('s', ?1, 0, 1, 10, 100)",
                params![id],
            )
            .unwrap();
        }

        // The geocoder off: a name without coordinates is the user's, and
        // only the efforts go.
        assert_eq!(scrub_simulated_course(&conn, None, false).unwrap(), (0, 1));
        let name = |id: &str| -> Option<String> {
            conn.query_row("SELECT location_name FROM activity WHERE id = ?1", params![id], |r| r.get(0)).unwrap()
        };
        assert_eq!(name("geocoded").as_deref(), Some("Solomon Islands"));
        // Re-plant the effort, then the geocoder on: the name goes too.
        conn.execute(
            "INSERT INTO segment_effort (segment_id, activity_id, start_idx, end_idx, elapsed_s, distance_m) VALUES ('s', 'geocoded', 0, 1, 10, 100)",
            [],
        )
        .unwrap();
        assert_eq!(scrub_simulated_course(&conn, None, true).unwrap(), (1, 1));
        assert_eq!(name("geocoded"), None);
        assert_eq!(name("typed").as_deref(), Some("Watopia"));
        assert_eq!(name("cleared").as_deref(), Some(""));
        assert_eq!(name("road").as_deref(), Some("Alanya"));
        let efforts: Vec<String> = {
            let mut st = conn.prepare("SELECT activity_id FROM segment_effort ORDER BY activity_id").unwrap();
            st.query_map([], |r| r.get(0)).unwrap().collect::<Result<_>>().unwrap()
        };
        assert_eq!(efforts, vec!["road"]);
        assert_eq!(scrub_simulated_course(&conn, None, true).unwrap(), (0, 0), "idempotent");

        // One activity: the badge changed it just now.
        row("just-changed", "virtual_ride", Some("Honiara"), None);
        row("other", "virtual_ride", Some("Gizo"), None);
        assert_eq!(scrub_simulated_course(&conn, Some("just-changed"), true).unwrap(), (1, 0));
        assert_eq!(name("just-changed"), None);
        assert_eq!(name("other").as_deref(), Some("Gizo"));
        assert_eq!(scrub_simulated_course(&conn, Some("road"), true).unwrap(), (0, 0), "a road ride is no simulated course");
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
        a4.source_device = None; // no device — its own group under ""
        a4.start_time = "2025-09-01T08:00:00+00:00".to_string();

        insert_activity(&conn, &a1).unwrap();
        insert_activity(&conn, &a2).unwrap();
        insert_activity(&conn, &a3).unwrap();
        insert_activity(&conn, &a4).unwrap();

        let devices = get_detected_devices(&conn).unwrap();
        assert_eq!(devices.len(), 3);
        // Ordered by count DESC, then name — so Garmin first, the empty
        // "no device" group before Wahoo on the tie.
        assert_eq!(devices[0].device_name, "Garmin FR265");
        assert_eq!(devices[0].activity_count, 2);
        assert_eq!(devices[1].device_name, "");
        assert_eq!(devices[1].activity_count, 1);
        assert_eq!(devices[2].device_name, "Wahoo ELEMNT");
        assert_eq!(devices[2].activity_count, 1);
    }

    /// The device facet matches raw source_device values; "" selects the
    /// activities without one; both together OR; an empty list is "all".
    #[test]
    fn get_activities_filters_by_device() {
        let conn = db::test_db();
        let with = |id: &str, dev: Option<&str>| {
            let mut a = sample_activity(id);
            a.source_device = dev.map(str::to_string);
            a
        };
        insert_activity(&conn, &with("fx", Some("Garmin fenix6x"))).unwrap();
        insert_activity(&conn, &with("fx-asia", Some("Garmin fenix6x_asia"))).unwrap();
        insert_activity(&conn, &with("edge", Some("Garmin edge_840"))).unwrap();
        insert_activity(&conn, &with("none", None)).unwrap();
        insert_activity(&conn, &with("blank", Some(""))).unwrap();

        let ids = |devices: Vec<&str>| -> Vec<String> {
            let f = ActivityFilters {
                devices: Some(devices.into_iter().map(str::to_string).collect()),
                sort_dir: Some("asc".into()),
                ..Default::default()
            };
            let mut v: Vec<String> = get_activities(&conn, &f).unwrap().into_iter().map(|a| a.id).collect();
            v.sort();
            v
        };
        assert_eq!(ids(vec!["Garmin fenix6x", "Garmin fenix6x_asia"]), vec!["fx", "fx-asia"]);
        // "" selects what get_detected_devices files under "": NULL and a
        // stored empty string alike.
        assert_eq!(ids(vec![""]), vec!["blank", "none"]);
        assert_eq!(ids(vec!["Garmin edge_840", ""]), vec!["blank", "edge", "none"]);
        assert_eq!(ids(vec![]).len(), 5);
        // The raw string is matched exactly: a label is not a value.
        assert!(ids(vec!["fenix 6X Pro"]).is_empty());
    }

    /// The gear facet matches item ids; "" selects the activities without
    /// gear; both together OR; an empty list is "all".
    #[test]
    fn get_activities_filters_by_gear() {
        let conn = db::test_db();
        for id in ["r1", "r2", "g1", "none"] {
            insert_activity(&conn, &sample_activity(id)).unwrap();
        }
        conn.execute_batch(
            "INSERT INTO gear (id, kind, name) VALUES ('road', 'bike', 'Road'), ('gravel', 'bike', 'Gravel');
             UPDATE activity SET gear_id = 'road' WHERE id IN ('r1', 'r2');
             UPDATE activity SET gear_id = 'gravel' WHERE id = 'g1';",
        )
        .unwrap();
        let ids = |gear: Vec<&str>| -> Vec<String> {
            let f = ActivityFilters {
                gear_ids: Some(gear.into_iter().map(str::to_string).collect()),
                ..Default::default()
            };
            let mut v: Vec<String> = get_activities(&conn, &f).unwrap().into_iter().map(|a| a.id).collect();
            v.sort();
            v
        };
        assert_eq!(ids(vec!["road"]), vec!["r1", "r2"]);
        assert_eq!(ids(vec![""]), vec!["none"]);
        assert_eq!(ids(vec!["gravel", ""]), vec!["g1", "none"]);
        assert_eq!(ids(vec![]).len(), 4);
        assert!(ids(vec!["ghost"]).is_empty());
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

    /// The trainer backfill (#189) moves only a `ride` whose sub_sport
    /// names a trainer, and leaves every other row — a road ride, a ride
    /// the user re-filed by hand, a non-ride with a cycling sub_sport —
    /// exactly as it is.
    #[test]
    fn refile_rides_by_sub_sport_moves_only_trainer_rides() {
        use crate::models::gear::{GearInput, GearKind};
        let conn = db::test_db();
        let row = |id: &str, sport: &str, sub: Option<&str>, parent: Option<&str>| {
            let mut a = sample_activity(id);
            a.sport_type = sport.to_string();
            a.sub_sport = sub.map(str::to_string);
            a.parent_id = parent.map(str::to_string);
            insert_activity(&conn, &a).unwrap();
        };
        row("trainer", "ride", Some("indoor_cycling"), None);
        row("zwift", "ride", Some("virtual_activity"), None);
        row("road", "ride", Some("road"), None);
        row("plain", "ride", None, None);
        row("by-hand", "walk", Some("indoor_cycling"), None);
        row("already", "indoor_ride", Some("indoor_cycling"), None);
        row("tri", "triathlon", None, None);
        row("leg", "ride", Some("indoor_cycling"), Some("tri"));

        // The road bike is the default for ride; a spin bike already claims
        // indoor_ride and must keep it.
        let gear = |name: &str, default_for: &[&str]| GearInput {
            kind: GearKind::Bike,
            name: name.into(),
            brand: None,
            model: None,
            purchased_at: None,
            initial_distance_m: 0.0,
            distance_limit_m: None,
            notes: None,
            default_for: default_for.iter().map(|s| s.to_string()).collect(),
            rules: vec![],
        };
        let road = crate::db::gear::insert(&conn, &gear("Road", &["ride"])).unwrap();
        let spin = crate::db::gear::insert(&conn, &gear("Spin", &["indoor_ride"])).unwrap();

        assert_eq!(refile_rides_by_sub_sport(&conn).unwrap(), 2);
        let default_of = |sport: &str| -> Option<String> {
            conn.query_row("SELECT gear_id FROM gear_default WHERE sport_type = ?1", params![sport], |r| r.get(0))
                .optional()
                .unwrap()
        };
        assert_eq!(default_of("virtual_ride").as_deref(), Some(road.id.as_str()), "ride's bike follows");
        assert_eq!(default_of("indoor_ride").as_deref(), Some(spin.id.as_str()), "a set default stays");
        assert_eq!(default_of("ride").as_deref(), Some(road.id.as_str()));
        let sport = |id: &str| -> String {
            conn.query_row("SELECT sport_type FROM activity WHERE id = ?1", params![id], |r| r.get(0))
                .unwrap()
        };
        assert_eq!(sport("trainer"), "indoor_ride");
        assert_eq!(sport("zwift"), "virtual_ride");
        assert_eq!(sport("road"), "ride");
        assert_eq!(sport("plain"), "ride");
        assert_eq!(sport("by-hand"), "walk");
        assert_eq!(sport("already"), "indoor_ride");
        assert_eq!(sport("leg"), "ride", "a merged event's leg stays a ride");
        assert_eq!(refile_rides_by_sub_sport(&conn).unwrap(), 0, "idempotent");
    }

    /// The virtual-run backfill (#192) moves only a `run` whose sub_sport
    /// is `virtual_activity`, takes the efforts and (geocoder on) the
    /// geocoded name off just those runs, and copies the run shoes over.
    #[test]
    fn refile_virtual_runs_moves_and_scrubs_only_simulator_runs() {
        use crate::models::gear::{GearInput, GearKind};
        let conn = db::test_db();
        let row = |id: &str, sport: &str, sub: Option<&str>, parent: Option<&str>, place: Option<&str>| {
            let mut a = sample_activity(id);
            a.sport_type = sport.to_string();
            a.sub_sport = sub.map(str::to_string);
            a.parent_id = parent.map(str::to_string);
            a.location_name = place.map(str::to_string);
            insert_activity(&conn, &a).unwrap();
        };
        row("zwift-run", "run", Some("virtual_activity"), None, Some("Honiara"));
        row("street", "run", Some("generic"), None, Some("Alanya"));
        row("plain", "run", None, None, None);
        row("treadmill", "treadmill", Some("treadmill"), None, None);
        row("zwift-ride", "virtual_ride", Some("virtual_activity"), None, Some("Typed by hand"));
        row("tri", "triathlon", None, None, None);
        row("leg", "run", Some("virtual_activity"), Some("tri"), None);
        for (seg_activity, n) in [("zwift-run", 0), ("street", 1)] {
            conn.execute(
                "INSERT INTO segment (id, name, sport, source_activity_id, created_at, distance_m, start_lat, start_lon, end_lat, end_lon, min_lat, max_lat, min_lon, max_lon) VALUES (?1, 'S', 'run', ?2, '', 100, 0, 0, 0, 0, 0, 0, 0, 0)",
                params![format!("s{n}"), seg_activity],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO segment_effort (segment_id, activity_id, start_idx, end_idx, elapsed_s, distance_m) VALUES (?1, ?2, 0, 1, 10, 100)",
                params![format!("s{n}"), seg_activity],
            )
            .unwrap();
        }
        let shoes = crate::db::gear::insert(
            &conn,
            &GearInput {
                kind: GearKind::Shoes,
                name: "Daily".into(),
                brand: None,
                model: None,
                purchased_at: None,
                initial_distance_m: 0.0,
                distance_limit_m: None,
                notes: None,
                default_for: vec!["run".into()],
                rules: vec![],
            },
        )
        .unwrap();

        assert_eq!(refile_virtual_runs(&conn, true).unwrap(), 1);

        let sport = |id: &str| -> String {
            conn.query_row("SELECT sport_type FROM activity WHERE id = ?1", params![id], |r| r.get(0)).unwrap()
        };
        let place = |id: &str| -> Option<String> {
            conn.query_row("SELECT location_name FROM activity WHERE id = ?1", params![id], |r| r.get(0)).unwrap()
        };
        let efforts = |id: &str| -> i64 {
            conn.query_row("SELECT COUNT(*) FROM segment_effort WHERE activity_id = ?1", params![id], |r| r.get(0))
                .unwrap()
        };
        assert_eq!(sport("zwift-run"), "virtual_run");
        assert_eq!(place("zwift-run"), None, "the geocoder's name for Watopia goes");
        assert_eq!(efforts("zwift-run"), 0);
        assert_eq!(sport("street"), "run");
        assert_eq!(place("street").as_deref(), Some("Alanya"));
        assert_eq!(efforts("street"), 1, "a real run keeps its efforts");
        assert_eq!(sport("plain"), "run");
        assert_eq!(sport("treadmill"), "treadmill");
        assert_eq!(sport("leg"), "run", "a merged event's leg stays a run");
        assert_eq!(
            place("zwift-ride").as_deref(),
            Some("Typed by hand"),
            "a virtual ride scrubbed before is not looked at again"
        );
        let default_of = |sport: &str| -> Option<String> {
            conn.query_row("SELECT gear_id FROM gear_default WHERE sport_type = ?1", params![sport], |r| r.get(0))
                .optional()
                .unwrap()
        };
        assert_eq!(default_of("virtual_run").as_deref(), Some(shoes.id.as_str()), "run's shoes follow");
        assert_eq!(refile_virtual_runs(&conn, true).unwrap(), 0, "idempotent");
    }

    /// With the geocoder off the name on a refiled run is the user's: it
    /// stays, while the efforts still go.
    #[test]
    fn refile_virtual_runs_keeps_the_name_while_the_geocoder_is_off() {
        let conn = db::test_db();
        let mut a = sample_activity("zr");
        a.sport_type = "run".into();
        a.sub_sport = Some("virtual_activity".into());
        a.location_name = Some("Treadmill at home".into());
        insert_activity(&conn, &a).unwrap();
        assert_eq!(refile_virtual_runs(&conn, false).unwrap(), 1);
        let (sport, place): (String, Option<String>) = conn
            .query_row("SELECT sport_type, location_name FROM activity WHERE id = 'zr'", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(sport, "virtual_run");
        assert_eq!(place.as_deref(), Some("Treadmill at home"));
    }

    /// The geocoder's late answer lands only on a row that still waits for
    /// it (#198): not over a name typed meanwhile, not over the cleared
    /// marker, and never on a sport whose course is simulated.
    #[test]
    fn a_geocoded_name_lands_only_where_one_is_still_wanted() {
        let conn = db::test_db();
        let row = |id: &str, sport: &str, place: Option<&str>| {
            let mut a = sample_activity(id);
            a.sport_type = sport.to_string();
            a.location_name = place.map(str::to_string);
            insert_activity(&conn, &a).unwrap();
        };
        row("waiting", "run", None);
        row("typed", "run", Some("Home loop"));
        row("cleared", "run", Some(""));
        row("zwift", "virtual_ride", None);
        row("zrun", "virtual_run", None);

        assert!(set_geocoded_name(&conn, "waiting", "Alanya").unwrap());
        assert!(!set_geocoded_name(&conn, "typed", "Alanya").unwrap());
        assert!(!set_geocoded_name(&conn, "cleared", "Alanya").unwrap());
        assert!(!set_geocoded_name(&conn, "zwift", "Honiara").unwrap());
        assert!(!set_geocoded_name(&conn, "zrun", "Honiara").unwrap());
        assert!(!set_geocoded_name(&conn, "missing", "Alanya").unwrap());

        let place = |id: &str| -> Option<String> {
            conn.query_row("SELECT location_name FROM activity WHERE id = ?1", params![id], |r| r.get(0)).unwrap()
        };
        assert_eq!(place("waiting").as_deref(), Some("Alanya"));
        assert_eq!(place("typed").as_deref(), Some("Home loop"));
        assert_eq!(place("cleared").as_deref(), Some(""));
        assert_eq!(place("zwift"), None);
        assert_eq!(place("zrun"), None);
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

    #[test]
    fn start_time_nearest_skips_unreadable_rows_and_reads_naive_ones() {
        let conn = crate::db::test_db();
        assert_eq!(start_time_nearest(&conn, 1_788_555_600).unwrap(), None);
        for (id, st) in [
            ("bad", "not a date"),
            ("naive", "2026-09-01T07:00:00"),
            ("plus3", "2026-09-04T07:35:00+03:00"),
        ] {
            conn.execute(
                "INSERT INTO activity (id, start_time, sport_type) VALUES (?1, ?2, 'ride')",
                rusqlite::params![id, st],
            )
            .unwrap();
        }
        // 2026-09-05 00:00 +03:00 → the +03:00 ride the day before, not the
        // unreadable row (which would sort first as NULL) and not the naive one.
        assert_eq!(
            start_time_nearest(&conn, 1_788_555_600).unwrap().as_deref(),
            Some("2026-09-04T07:35:00+03:00")
        );
        // Close to the naive row, SQLite reads it (as UTC) and returns it —
        // the caller's rfc3339 parse then falls back to the machine's zone.
        assert_eq!(
            start_time_nearest(&conn, 1_788_246_000).unwrap().as_deref(),
            Some("2026-09-01T07:00:00")
        );
    }

    /// The list row names the recording device (#147): summaries carry
    /// source_device as stored, null included.
    #[test]
    fn summaries_carry_the_source_device() {
        let conn = db::test_db();
        let mut a = sample_activity("with-dev");
        a.source_device = Some("Garmin fenix6x".into());
        insert_activity(&conn, &a).unwrap();
        let mut none = sample_activity("no-dev");
        none.source_device = None;
        insert_activity(&conn, &none).unwrap();
        let rows = get_activities(&conn, &ActivityFilters::default()).unwrap();
        let dev = |id: &str| rows.iter().find(|r| r.id == id).unwrap().source_device.clone();
        assert_eq!(dev("with-dev").as_deref(), Some("Garmin fenix6x"));
        assert_eq!(dev("no-dev"), None);
    }
}
