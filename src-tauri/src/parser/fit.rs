use chrono::{DateTime, Utc};
use fitparser::{self, profile::MesgNum};

use crate::models::exercise_set::ExerciseSet;
use crate::models::hrv_sample::HrvSample;
use crate::models::lap::Lap;
use crate::models::swim_length::SwimLength;
use crate::models::time_in_zone::TimeInZone;
use crate::models::trackpoint::TrackPoint;
use crate::models::multisport_leg::MultisportLeg;
use crate::parser::{ParsedActivity, SessionMetrics};

/// Test convenience: the app itself always parses in-memory bytes (the import
/// pipeline reads + size-gates files before parsing).
#[cfg(test)]
pub fn parse_fit(path: &str, activity_id: &str) -> Result<ParsedActivity, String> {
    let data =
        std::fs::read(path).map_err(|e| format!("Failed to read FIT file: {}", e))?;
    parse_fit_bytes(&data, activity_id)
}

/// Compose the recording device's display name from a device_info message's
/// fields. An explicit product_name wins; otherwise manufacturer + model
/// ("garmin" + "fenix6x" → "Garmin fenix6x"). The name also becomes the GPX
/// `creator` on export, where services match it against device databases —
/// a bare manufacturer ("garmin") is only the last resort.
fn compose_device_name(
    product_name: Option<String>,
    manufacturer: Option<String>,
    product: Option<String>,
) -> Option<String> {
    if product_name.is_some() {
        return product_name;
    }
    let capitalize = |s: String| {
        let mut chars = s.chars();
        match chars.next() {
            Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
            None => s,
        }
    };
    match (manufacturer, product) {
        (Some(m), Some(p)) => Some(format!("{} {}", capitalize(m), p)),
        (Some(m), None) => Some(capitalize(m)),
        (None, p) => p,
    }
}

/// Decode a FIT left/right balance value to the RIGHT-pedal percentage.
///
/// The raw value carries a flag bit saying "the payload refers to the right
/// pedal": record-level is uint8 (bit 7 = right, bits 0-6 = percent),
/// session/lap-level (`scale100`) is uint16 (bit 15 = right, low bits =
/// percent × 100). Stored raw it reads as nonsense (171 is really "right
/// 43%"). A value without the flag has an UNDEFINED side per the FIT profile
/// (real devices always set it) — dropped rather than guessed. The SDK also
/// decodes a bare flag byte (0x80, no percentage) to the enum string
/// "right"; that never reaches here because `field_to_f64` yields None.
fn decode_lr_balance(raw: f64, scale100: bool) -> Option<f64> {
    let (flag, max_scaled, scale) = if scale100 {
        (f64::from(0x8000), 10_000.0, 100.0)
    } else {
        (f64::from(0x80), 100.0, 1.0)
    };
    let value = raw - flag;
    if value < 0.0 || value > max_scaled {
        return None;
    }
    Some(value / scale)
}

/// Choose which TimeInZone messages feed the activity's zone rows. Garmin
/// writes one message per LAP plus one per SESSION (`reference_mesg` labels
/// each); ingesting all of them duplicated every zone row — invisible today
/// (the UI only reads boundaries, which dedup), but any "time in zones"
/// feature would multiply times by the lap count + 1. Keep session-scoped
/// messages when any exist; unlabeled files keep everything.
fn select_time_in_zones(
    groups: Vec<(Option<String>, Vec<TimeInZone>)>,
) -> Vec<TimeInZone> {
    let has_session = groups
        .iter()
        .any(|(r, _)| r.as_deref() == Some("session"));
    groups
        .into_iter()
        .filter(|(r, _)| !has_session || r.as_deref() == Some("session"))
        .flat_map(|(_, rows)| rows)
        .collect()
}

/// First two elements of a FIT array field, as a pair. Power-phase arrays
/// are [start, end] (degrees, 0° = top dead center); `*_position` arrays
/// are [seated, standing] (the FIT rider_position_type order).
fn field_pair(field: &fitparser::FitDataField) -> (Option<f64>, Option<f64>) {
    let v = field_to_f64_vec(field);
    (v.first().copied(), v.get(1).copied())
}

/// Whether the file recorded a barometer channel (a device_info message with
/// local_device_type == "barometer") — i.e. the altitude stream is barometric
/// rather than GPS-derived. Exports advertise this in the GPX `creator` so
/// services (Strava) trust the <ele> series instead of DEM-correcting it.
pub fn fit_has_barometer(data: &[u8]) -> bool {
    fitparser::from_bytes(data).is_ok_and(|messages| {
        messages.iter().any(|m| {
            m.kind() == MesgNum::DeviceInfo
                && m.fields().iter().any(|f| {
                    f.name() == "local_device_type"
                        && format!("{}", f.value()) == "barometer"
                })
        })
    })
}

/// Parse FIT from in-memory bytes (e.g. decompressed from a .gz import).
pub fn parse_fit_bytes(data: &[u8], activity_id: &str) -> Result<ParsedActivity, String> {
    let messages =
        fitparser::from_bytes(data).map_err(|e| format!("Failed to parse FIT: {}", e))?;

    let mut trackpoints: Vec<TrackPoint> = Vec::new();
    let mut start_time: Option<String> = None;
    // From MesgNum::Sport — the fallback when the file carries no sessions.
    let mut sport_message: Option<String> = None;
    let mut source_device: Option<String> = None;
    // One entry per Session message: (its sport, its metrics). Multisport
    // files (triathlon) carry one session PER LEG — they are resolved after
    // the loop; collapsing them here with last-wins used to file every
    // triathlon under its final leg with the legs' metrics shuffled.
    let mut sessions: Vec<(Option<String>, Option<String>, SessionMetrics)> = Vec::new();
    let mut laps: Vec<Lap> = Vec::new();
    let mut lengths: Vec<SwimLength> = Vec::new();
    let mut sets: Vec<ExerciseSet> = Vec::new();
    // One entry per TimeInZone message: (its reference_mesg, its rows).
    // Resolved after the loop — see select_time_in_zones.
    let mut time_in_zone_groups: Vec<(Option<String>, Vec<TimeInZone>)> = Vec::new();
    let mut hrv_samples: Vec<HrvSample> = Vec::new();

    for msg in &messages {
        match msg.kind() {
            MesgNum::Record => {
                let mut tp = TrackPoint {
                    activity_id: activity_id.to_string(),
                    t: None,
                    lat: None,
                    lon: None,
                    altitude_m: None,
                    speed_mps: None,
                    hr: None,
                    cadence: None,
                    power_w: None,
                    temperature_c: None,
                    vertical_oscillation_mm: None,
                    stance_time_ms: None,
                    stance_time_percent: None,
                    step_length_mm: None,
                    grade_percent: None,
                    left_right_balance: None,
                    left_torque_effectiveness: None,
                    right_torque_effectiveness: None,
                    left_pedal_smoothness: None,
                    right_pedal_smoothness: None,
                };

                for field in msg.fields() {
                    match field.name() {
                        "timestamp" => {
                            if let Some(iso) = value_to_timestamp(field.value()) {
                                if start_time.is_none() {
                                    start_time = Some(iso.clone());
                                }
                                tp.t = Some(iso);
                            }
                        }
                        "position_lat" => {
                            if let Some(v) = field_to_f64(field) {
                                tp.lat = Some(v * (180.0 / 2f64.powi(31)));
                            }
                        }
                        "position_long" => {
                            if let Some(v) = field_to_f64(field) {
                                tp.lon = Some(v * (180.0 / 2f64.powi(31)));
                            }
                        }
                        "altitude" | "enhanced_altitude" => {
                            tp.altitude_m = field_to_f64(field);
                        }
                        "speed" | "enhanced_speed" => {
                            tp.speed_mps = field_to_f64(field);
                        }
                        "heart_rate" => {
                            tp.hr = field_to_f64(field).map(|v| v as i32);
                        }
                        "cadence" => {
                            tp.cadence = field_to_f64(field).map(|v| v as i32);
                        }
                        "power" => {
                            tp.power_w = field_to_f64(field).map(|v| v as i32);
                        }
                        "temperature" => {
                            tp.temperature_c = field_to_f64(field);
                        }
                        "vertical_oscillation" => {
                            tp.vertical_oscillation_mm = field_to_f64(field);
                        }
                        "stance_time" => {
                            tp.stance_time_ms = field_to_f64(field);
                        }
                        "stance_time_percent" => {
                            tp.stance_time_percent = field_to_f64(field);
                        }
                        "step_length" => {
                            tp.step_length_mm = field_to_f64(field);
                        }
                        "grade" => {
                            tp.grade_percent = field_to_f64(field);
                        }
                        "left_right_balance" => {
                            tp.left_right_balance =
                                field_to_f64(field).and_then(|v| decode_lr_balance(v, false));
                        }
                        "left_torque_effectiveness" => {
                            tp.left_torque_effectiveness = field_to_f64(field);
                        }
                        "right_torque_effectiveness" => {
                            tp.right_torque_effectiveness = field_to_f64(field);
                        }
                        "left_pedal_smoothness" => {
                            tp.left_pedal_smoothness = field_to_f64(field);
                        }
                        "right_pedal_smoothness" => {
                            tp.right_pedal_smoothness = field_to_f64(field);
                        }
                        _ => {}
                    }
                }

                trackpoints.push(tp);
            }
            MesgNum::Session => {
                let mut sm = SessionMetrics::default();
                let mut session_sport: Option<String> = None;
                let mut session_start: Option<String> = None;
                // A Session's `timestamp` is when the message was WRITTEN —
                // normally the session's END. Only the fallback when the file
                // carries no start_time; taken first-wins it would window a
                // leg as [end, end+elapsed] and focus an empty slice.
                let mut session_ts_fallback: Option<String> = None;
                for field in msg.fields() {
                    match field.name() {
                        "start_time" => {
                            if let Some(iso) = value_to_timestamp(field.value()) {
                                if start_time.is_none() {
                                    start_time = Some(iso.clone());
                                }
                                session_start = Some(iso);
                            }
                        }
                        "timestamp" => {
                            if let Some(iso) = value_to_timestamp(field.value()) {
                                if start_time.is_none() {
                                    start_time = Some(iso.clone());
                                }
                                if session_ts_fallback.is_none() {
                                    session_ts_fallback = Some(iso);
                                }
                            }
                        }
                        "sport" => {
                            session_sport = Some(format!("{}", field.value()));
                        }
                        "total_distance" => {
                            sm.total_distance_m = field_to_f64(field);
                        }
                        "total_elapsed_time" => {
                            sm.total_elapsed_time_s = field_to_f64(field);
                        }
                        "total_timer_time" => {
                            sm.total_timer_time_s = field_to_f64(field);
                        }
                        "total_ascent" => {
                            sm.total_ascent_m = field_to_f64(field);
                        }
                        "total_descent" => {
                            sm.total_descent_m = field_to_f64(field);
                        }
                        "enhanced_avg_speed" | "avg_speed" => {
                            if sm.avg_speed_mps.is_none() {
                                sm.avg_speed_mps = field_to_f64(field);
                            }
                        }
                        "enhanced_max_speed" | "max_speed" => {
                            if sm.max_speed_mps.is_none() {
                                sm.max_speed_mps = field_to_f64(field);
                            }
                        }
                        "avg_heart_rate" => {
                            sm.avg_hr = field_to_f64(field);
                        }
                        "max_heart_rate" => {
                            sm.max_hr = field_to_f64(field);
                        }
                        "avg_running_cadence" | "avg_cadence" => {
                            if sm.avg_cadence.is_none() {
                                sm.avg_cadence = field_to_f64(field);
                            }
                        }
                        "max_running_cadence" | "max_cadence" => {
                            if sm.max_cadence.is_none() {
                                sm.max_cadence = field_to_f64(field);
                            }
                        }
                        "total_calories" => {
                            sm.total_calories = field_to_f64(field);
                        }
                        "avg_temperature" => {
                            sm.avg_temperature_c = field_to_f64(field);
                        }
                        "max_temperature" => {
                            sm.max_temperature_c = field_to_f64(field);
                        }
                        "avg_power" => {
                            sm.avg_power_w = field_to_f64(field);
                        }
                        "max_power" => {
                            sm.max_power_w = field_to_f64(field);
                        }
                        "normalized_power" => {
                            sm.normalized_power_w = field_to_f64(field);
                        }
                        "total_work" => {
                            sm.total_work_kj = field_to_f64(field).map(|v| v / 1000.0);
                        }
                        "threshold_power" => {
                            sm.threshold_power_w = field_to_f64(field);
                        }
                        "training_stress_score" => {
                            sm.training_stress_score = field_to_f64(field);
                        }
                        "intensity_factor" => {
                            sm.intensity_factor = field_to_f64(field);
                        }
                        "total_training_effect" => {
                            sm.training_effect_aerobic = field_to_f64(field);
                        }
                        "total_anaerobic_training_effect" => {
                            sm.training_effect_anaerobic = field_to_f64(field);
                        }
                        "training_load_peak" => {
                            sm.training_load_peak = field_to_f64(field);
                        }
                        "avg_vertical_oscillation" => {
                            sm.avg_vertical_oscillation_mm = field_to_f64(field);
                        }
                        "avg_stance_time" => {
                            sm.avg_stance_time_ms = field_to_f64(field);
                        }
                        "avg_stance_time_percent" => {
                            sm.avg_stance_time_percent = field_to_f64(field);
                        }
                        "avg_step_length" => {
                            sm.avg_step_length_mm = field_to_f64(field);
                        }
                        "total_strides" => {
                            sm.total_strides = field_to_f64(field).map(|v| v as i64);
                        }
                        "min_heart_rate" => {
                            sm.min_hr = field_to_f64(field);
                        }
                        "total_moving_time" => {
                            sm.moving_time_s = field_to_f64(field);
                        }
                        "sub_sport" => {
                            sm.sub_sport = Some(format!("{}", field.value()));
                        }
                        "enhanced_avg_respiration_rate" | "avg_respiration_rate" => {
                            if sm.avg_respiration_rate.is_none() {
                                sm.avg_respiration_rate = field_to_f64(field);
                            }
                        }
                        "enhanced_max_respiration_rate" | "max_respiration_rate" => {
                            if sm.max_respiration_rate.is_none() {
                                sm.max_respiration_rate = field_to_f64(field);
                            }
                        }
                        "rmssd_hrv" => {
                            sm.hrv_rmssd = field_to_f64(field);
                        }
                        "sdrr_hrv" => {
                            sm.hrv_sdrr = field_to_f64(field);
                        }
                        "end_position_lat" => {
                            if let Some(v) = field_to_f64(field) {
                                sm.end_lat = Some(v * (180.0 / 2f64.powi(31)));
                            }
                        }
                        "end_position_long" => {
                            if let Some(v) = field_to_f64(field) {
                                sm.end_lon = Some(v * (180.0 / 2f64.powi(31)));
                            }
                        }
                        "avg_left_torque_effectiveness" => {
                            sm.avg_left_torque_effectiveness = field_to_f64(field);
                        }
                        "avg_right_torque_effectiveness" => {
                            sm.avg_right_torque_effectiveness = field_to_f64(field);
                        }
                        "avg_left_pedal_smoothness" => {
                            sm.avg_left_pedal_smoothness = field_to_f64(field);
                        }
                        "avg_right_pedal_smoothness" => {
                            sm.avg_right_pedal_smoothness = field_to_f64(field);
                        }
                        "left_right_balance" => {
                            sm.avg_left_right_balance =
                                field_to_f64(field).and_then(|v| decode_lr_balance(v, true));
                        }
                        "avg_left_pco" => {
                            sm.avg_left_pco_mm = field_to_f64(field);
                        }
                        "avg_right_pco" => {
                            sm.avg_right_pco_mm = field_to_f64(field);
                        }
                        "avg_left_power_phase" => {
                            (sm.avg_left_power_phase_start_deg, sm.avg_left_power_phase_end_deg) =
                                field_pair(field);
                        }
                        "avg_left_power_phase_peak" => {
                            (
                                sm.avg_left_power_phase_peak_start_deg,
                                sm.avg_left_power_phase_peak_end_deg,
                            ) = field_pair(field);
                        }
                        "avg_right_power_phase" => {
                            (sm.avg_right_power_phase_start_deg, sm.avg_right_power_phase_end_deg) =
                                field_pair(field);
                        }
                        "avg_right_power_phase_peak" => {
                            (
                                sm.avg_right_power_phase_peak_start_deg,
                                sm.avg_right_power_phase_peak_end_deg,
                            ) = field_pair(field);
                        }
                        "avg_power_position" => {
                            (sm.avg_power_seated_w, sm.avg_power_standing_w) = field_pair(field);
                        }
                        "max_power_position" => {
                            (sm.max_power_seated_w, sm.max_power_standing_w) = field_pair(field);
                        }
                        "avg_cadence_position" => {
                            (sm.avg_cadence_seated, sm.avg_cadence_standing) = field_pair(field);
                        }
                        "max_cadence_position" => {
                            (sm.max_cadence_seated, sm.max_cadence_standing) = field_pair(field);
                        }
                        "time_standing" => {
                            sm.time_standing_s = field_to_f64(field);
                        }
                        "stand_count" => {
                            sm.stand_count = field_to_f64(field).map(|v| v as i64);
                        }
                        _ => {}
                    }
                }
                sessions.push((session_sport, session_start.or(session_ts_fallback), sm));
            }
            MesgNum::Sport => {
                for field in msg.fields() {
                    if field.name() == "sport" {
                        sport_message = Some(format!("{}", field.value()));
                    }
                }
            }
            MesgNum::Lap => {
                let mut lap = Lap {
                    id: None,
                    activity_id: activity_id.to_string(),
                    lap_number: (laps.len() + 1) as i32,
                    start_time: None,
                    total_elapsed_time_s: None,
                    total_timer_time_s: None,
                    total_distance_m: None,
                    avg_speed_mps: None,
                    max_speed_mps: None,
                    avg_hr: None,
                    max_hr: None,
                    avg_cadence: None,
                    max_cadence: None,
                    total_ascent_m: None,
                    total_descent_m: None,
                    total_calories: None,
                    avg_power_w: None,
                    max_power_w: None,
                    normalized_power_w: None,
                    avg_vertical_oscillation_mm: None,
                    avg_stance_time_ms: None,
                    avg_step_length_mm: None,
                };

                for field in msg.fields() {
                    match field.name() {
                        "start_time" | "timestamp" => {
                            if lap.start_time.is_none() {
                                lap.start_time = value_to_timestamp(field.value());
                            }
                        }
                        "total_elapsed_time" => {
                            lap.total_elapsed_time_s = field_to_f64(field);
                        }
                        "total_timer_time" => {
                            lap.total_timer_time_s = field_to_f64(field);
                        }
                        "total_distance" => {
                            lap.total_distance_m = field_to_f64(field);
                        }
                        "enhanced_avg_speed" | "avg_speed" => {
                            if lap.avg_speed_mps.is_none() {
                                lap.avg_speed_mps = field_to_f64(field);
                            }
                        }
                        "enhanced_max_speed" | "max_speed" => {
                            if lap.max_speed_mps.is_none() {
                                lap.max_speed_mps = field_to_f64(field);
                            }
                        }
                        "avg_heart_rate" => {
                            lap.avg_hr = field_to_f64(field);
                        }
                        "max_heart_rate" => {
                            lap.max_hr = field_to_f64(field);
                        }
                        "avg_running_cadence" | "avg_cadence" => {
                            if lap.avg_cadence.is_none() {
                                lap.avg_cadence = field_to_f64(field);
                            }
                        }
                        "max_running_cadence" | "max_cadence" => {
                            if lap.max_cadence.is_none() {
                                lap.max_cadence = field_to_f64(field);
                            }
                        }
                        "total_ascent" => {
                            lap.total_ascent_m = field_to_f64(field);
                        }
                        "total_descent" => {
                            lap.total_descent_m = field_to_f64(field);
                        }
                        "total_calories" => {
                            lap.total_calories = field_to_f64(field);
                        }
                        "avg_power" => {
                            lap.avg_power_w = field_to_f64(field);
                        }
                        "max_power" => {
                            lap.max_power_w = field_to_f64(field);
                        }
                        "normalized_power" => {
                            lap.normalized_power_w = field_to_f64(field);
                        }
                        "avg_vertical_oscillation" => {
                            lap.avg_vertical_oscillation_mm = field_to_f64(field);
                        }
                        "avg_stance_time" => {
                            lap.avg_stance_time_ms = field_to_f64(field);
                        }
                        "avg_step_length" => {
                            lap.avg_step_length_mm = field_to_f64(field);
                        }
                        _ => {}
                    }
                }

                laps.push(lap);
            }
            MesgNum::Length => {
                let mut length = SwimLength {
                    id: None,
                    activity_id: activity_id.to_string(),
                    length_number: (lengths.len() + 1) as i32,
                    start_time: None,
                    total_elapsed_time_s: None,
                    total_timer_time_s: None,
                    avg_speed_mps: None,
                    avg_swimming_cadence: None,
                    swim_stroke: None,
                    total_strokes: None,
                    total_calories: None,
                    length_type: None,
                };

                for field in msg.fields() {
                    match field.name() {
                        "start_time" | "timestamp" => {
                            if length.start_time.is_none() {
                                length.start_time = value_to_timestamp(field.value());
                            }
                        }
                        "total_elapsed_time" => {
                            length.total_elapsed_time_s = field_to_f64(field);
                        }
                        "total_timer_time" => {
                            length.total_timer_time_s = field_to_f64(field);
                        }
                        "avg_speed" => {
                            length.avg_speed_mps = field_to_f64(field);
                        }
                        "avg_swimming_cadence" => {
                            length.avg_swimming_cadence = field_to_f64(field);
                        }
                        "swim_stroke" => {
                            length.swim_stroke = Some(format!("{}", field.value()));
                        }
                        "total_strokes" | "stroke_count" => {
                            length.total_strokes = field_to_f64(field).map(|v| v as i32);
                        }
                        "total_calories" => {
                            length.total_calories = field_to_f64(field);
                        }
                        "length_type" => {
                            length.length_type = Some(format!("{}", field.value()));
                        }
                        _ => {}
                    }
                }

                lengths.push(length);
            }
            MesgNum::Set => {
                let mut set = ExerciseSet {
                    id: None,
                    activity_id: activity_id.to_string(),
                    set_number: (sets.len() + 1) as i32,
                    start_time: None,
                    category: None,
                    category_subtype: None,
                    set_type: None,
                    duration_s: None,
                    repetitions: None,
                    weight_kg: None,
                    wkt_step_index: None,
                };

                for field in msg.fields() {
                    match field.name() {
                        "start_time" | "timestamp" => {
                            if set.start_time.is_none() {
                                set.start_time = value_to_timestamp(field.value());
                            }
                        }
                        "category" => {
                            set.category = Some(format!("{}", field.value()));
                        }
                        "category_subtype" => {
                            set.category_subtype = Some(format!("{}", field.value()));
                        }
                        "set_type" => {
                            set.set_type = Some(format!("{}", field.value()));
                        }
                        "duration" => {
                            set.duration_s = field_to_f64(field);
                        }
                        "repetitions" => {
                            set.repetitions = field_to_f64(field).map(|v| v as i32);
                        }
                        "weight" => {
                            set.weight_kg = field_to_f64(field);
                        }
                        "wkt_step_index" => {
                            set.wkt_step_index = field_to_f64(field).map(|v| v as i32);
                        }
                        _ => {}
                    }
                }

                sets.push(set);
            }
            MesgNum::TimeInZone => {
                let mut reference_mesg: Option<String> = None;
                let mut time_in_zones: Vec<TimeInZone> = Vec::new();
                let mut hr_times: Vec<f64> = Vec::new();
                let mut power_times: Vec<f64> = Vec::new();
                let mut cadence_times: Vec<f64> = Vec::new();
                let mut speed_times: Vec<f64> = Vec::new();
                let mut hr_boundaries: Vec<f64> = Vec::new();
                let mut power_boundaries: Vec<f64> = Vec::new();
                let mut cadence_boundaries: Vec<f64> = Vec::new();
                let mut speed_boundaries: Vec<f64> = Vec::new();

                for field in msg.fields() {
                    match field.name() {
                        "reference_mesg" => {
                            reference_mesg = Some(format!("{}", field.value()));
                        }
                        "time_in_hr_zone" => {
                            hr_times = field_to_f64_vec(field);
                        }
                        "time_in_power_zone" => {
                            power_times = field_to_f64_vec(field);
                        }
                        "time_in_cadence_zone" => {
                            cadence_times = field_to_f64_vec(field);
                        }
                        "time_in_speed_zone" => {
                            speed_times = field_to_f64_vec(field);
                        }
                        "hr_zone_high_boundary" => {
                            hr_boundaries = field_to_f64_vec(field);
                        }
                        "power_zone_high_boundary" => {
                            power_boundaries = field_to_f64_vec(field);
                        }
                        // sic: the FIT SDK profile itself misspells "bondary".
                        "cadence_zone_high_bondary" => {
                            cadence_boundaries = field_to_f64_vec(field);
                        }
                        // Boundaries in m/s (the frontend converts to the
                        // display unit).
                        "speed_zone_high_boundary" => {
                            speed_boundaries = field_to_f64_vec(field);
                        }
                        _ => {}
                    }
                }

                for (i, time) in hr_times.iter().enumerate() {
                    time_in_zones.push(TimeInZone {
                        id: None,
                        activity_id: activity_id.to_string(),
                        zone_type: "hr".to_string(),
                        zone_index: i as i32,
                        time_s: *time,
                        zone_high_boundary: hr_boundaries.get(i).copied(),
                    });
                }

                for (i, time) in power_times.iter().enumerate() {
                    time_in_zones.push(TimeInZone {
                        id: None,
                        activity_id: activity_id.to_string(),
                        zone_type: "power".to_string(),
                        zone_index: i as i32,
                        time_s: *time,
                        zone_high_boundary: power_boundaries.get(i).copied(),
                    });
                }

                for (i, time) in cadence_times.iter().enumerate() {
                    time_in_zones.push(TimeInZone {
                        id: None,
                        activity_id: activity_id.to_string(),
                        zone_type: "cadence".to_string(),
                        zone_index: i as i32,
                        time_s: *time,
                        zone_high_boundary: cadence_boundaries.get(i).copied(),
                    });
                }

                for (i, time) in speed_times.iter().enumerate() {
                    time_in_zones.push(TimeInZone {
                        id: None,
                        activity_id: activity_id.to_string(),
                        zone_type: "speed".to_string(),
                        zone_index: i as i32,
                        time_s: *time,
                        zone_high_boundary: speed_boundaries.get(i).copied(),
                    });
                }

                time_in_zone_groups.push((reference_mesg, time_in_zones));
            }
            MesgNum::Hrv => {
                for field in msg.fields() {
                    if field.name() == "time" {
                        let rr_values = field_to_f64_vec(field);
                        for val in rr_values {
                            // Filter invalid markers (65535 in raw, or ~65.535 in seconds)
                            if val < 65.0 && val > 0.0 {
                                hrv_samples.push(HrvSample {
                                    id: None,
                                    activity_id: activity_id.to_string(),
                                    sample_index: hrv_samples.len() as i32,
                                    rr_interval_ms: val * 1000.0,
                                });
                            }
                        }
                    }
                }
            }
            MesgNum::DeviceInfo => {
                if source_device.is_none() {
                    let mut product_name = None;
                    let mut manufacturer = None;
                    let mut product = None;
                    for field in msg.fields() {
                        let val = format!("{}", field.value());
                        if val.is_empty() || val == "0" {
                            continue;
                        }
                        match field.name() {
                            "product_name" => product_name = Some(val),
                            "manufacturer" => manufacturer = Some(val),
                            // The SDK resolves manufacturer-specific product
                            // enums to model names ("fenix6x", "edge_840").
                            "garmin_product" | "product" => product = Some(val),
                            _ => {}
                        }
                    }
                    source_device = compose_device_name(product_name, manufacturer, product);
                }
            }
            _ => {}
        }
    }

    let legs = sessions_to_legs(activity_id, &sessions);
    let session_sports: Vec<String> =
        sessions.iter().filter_map(|(s, _, _)| s.clone()).collect();
    let sport_type = resolve_sport(&session_sports).or(sport_message);
    let session_metrics = if sessions.is_empty() {
        None
    } else {
        Some(SessionMetrics::aggregate(
            sessions.into_iter().map(|(_, _, m)| m).collect(),
        ))
    };

    Ok(ParsedActivity {
        start_time,
        sport_type,
        title: None,
        source_device,
        trackpoints,
        session_metrics,
        laps,
        lengths,
        sets,
        time_in_zones: select_time_in_zones(time_in_zone_groups),
        hrv_samples,
        legs,
    })
}

/// Per-leg breakdown for multisport files: one MultisportLeg per session,
/// in file order, transitions flagged. Single-session files get NONE — the
/// activity itself is the only "leg" and a one-row table is noise.
fn sessions_to_legs(
    activity_id: &str,
    sessions: &[(Option<String>, Option<String>, SessionMetrics)],
) -> Vec<MultisportLeg> {
    if sessions.len() < 2 {
        return Vec::new();
    }
    sessions
        .iter()
        .enumerate()
        .map(|(i, (sport, start, sm))| {
            let raw = sport.as_deref().unwrap_or("generic");
            let is_transition = raw == "transition";
            let sport_type = if is_transition {
                "transition".to_string()
            } else {
                // Resolve WITH the session's sub_sport, like the activity
                // itself: a swimrun's legs are trail_run and open_water, not
                // generic run/swim (fenix files carry sub per session).
                crate::models::activity::SportType::resolve(Some(raw), sm.sub_sport.as_deref())
                    .as_str()
                    .to_string()
            };
            MultisportLeg {
                id: None,
                activity_id: activity_id.to_string(),
                leg_number: (i + 1) as i32,
                sport_type,
                is_transition,
                start_time: start.clone(),
                total_distance_m: sm.total_distance_m,
                total_timer_time_s: sm.total_timer_time_s,
                total_elapsed_time_s: sm.total_elapsed_time_s,
                avg_speed_mps: sm.avg_speed_mps,
                avg_hr: sm.avg_hr,
                max_hr: sm.max_hr,
                total_ascent_m: sm.total_ascent_m,
                total_calories: sm.total_calories,
                source_activity_id: None,
            }
        })
        .collect()
}

/// The activity's sport from its sessions' sports. A multisport file
/// (triathlon/duathlon) has one session per LEG — two or more distinct
/// non-transition sports mean the activity IS the combination, not whichever
/// leg happened to come last. "multisport" maps to SportType::Triathlon.
fn resolve_sport(session_sports: &[String]) -> Option<String> {
    let distinct: std::collections::HashSet<&str> = session_sports
        .iter()
        .map(|s| s.as_str())
        .filter(|s| *s != "transition")
        .collect();
    if distinct.len() >= 2 {
        return Some("multisport".to_string());
    }
    distinct
        .into_iter()
        .next()
        .map(str::to_string)
        // All-transition would be odd, but any sport beats none.
        .or_else(|| session_sports.first().cloned())
}

fn value_to_timestamp(value: &fitparser::Value) -> Option<String> {
    match value {
        fitparser::Value::Timestamp(dt) => Some(dt.to_rfc3339()),
        fitparser::Value::String(s) => {
            if let Ok(dt) = s.parse::<DateTime<Utc>>() {
                Some(dt.to_rfc3339())
            } else {
                Some(s.clone())
            }
        }
        fitparser::Value::UInt32(secs) => {
            let garmin_epoch = DateTime::parse_from_rfc3339("1989-12-31T00:00:00+00:00")
                .ok()?
                .with_timezone(&Utc);
            let dt = garmin_epoch + chrono::Duration::seconds(*secs as i64);
            Some(dt.to_rfc3339())
        }
        _ => {
            let s = format!("{}", value);
            if !s.is_empty() && s != "0" {
                Some(s)
            } else {
                None
            }
        }
    }
}

fn field_to_f64(field: &fitparser::FitDataField) -> Option<f64> {
    match field.value() {
        fitparser::Value::SInt8(v) => Some(*v as f64),
        fitparser::Value::UInt8(v) => Some(*v as f64),
        fitparser::Value::SInt16(v) => Some(*v as f64),
        fitparser::Value::UInt16(v) => Some(*v as f64),
        fitparser::Value::SInt32(v) => Some(*v as f64),
        fitparser::Value::UInt32(v) => Some(*v as f64),
        fitparser::Value::SInt64(v) => Some(*v as f64),
        fitparser::Value::UInt64(v) => Some(*v as f64),
        fitparser::Value::Float32(v) => Some(*v as f64),
        fitparser::Value::Float64(v) => Some(*v),
        _ => None,
    }
}

fn field_to_f64_vec(field: &fitparser::FitDataField) -> Vec<f64> {
    match field.value() {
        fitparser::Value::Array(vals) => vals
            .iter()
            .filter_map(|v| match v {
                fitparser::Value::Float64(f) => Some(*f),
                fitparser::Value::Float32(f) => Some(*f as f64),
                fitparser::Value::UInt32(v) => Some(*v as f64),
                fitparser::Value::UInt16(v) => Some(*v as f64),
                fitparser::Value::UInt8(v) => Some(*v as f64),
                fitparser::Value::SInt32(v) => Some(*v as f64),
                fitparser::Value::SInt16(v) => Some(*v as f64),
                fitparser::Value::SInt8(v) => Some(*v as f64),
                fitparser::Value::UInt64(v) => Some(*v as f64),
                fitparser::Value::SInt64(v) => Some(*v as f64),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Device naming: an explicit product_name wins outright; otherwise the
    /// name composes as "Manufacturer model" — a Garmin file without
    /// product_name must yield "Garmin fenix6x", not the bare "garmin" that
    /// no device database recognizes.
    #[test]
    fn compose_device_name_prefers_product_name_then_composes() {
        let s = |v: &str| Some(v.to_string());
        assert_eq!(compose_device_name(s("ELEMNT BOLT"), s("wahoo"), s("123")), s("ELEMNT BOLT"));
        assert_eq!(compose_device_name(None, s("garmin"), s("fenix6x")), s("Garmin fenix6x"));
        assert_eq!(compose_device_name(None, s("garmin"), None), s("Garmin"));
        assert_eq!(compose_device_name(None, None, s("fenix6x")), s("fenix6x"));
        assert_eq!(compose_device_name(None, None, None), None);
    }

    /// The per-leg breakdown: multisport sessions become ordered legs with
    /// normalized sports and flagged transitions; single-session files get
    /// none (a one-row table is noise, the activity IS the leg).
    #[test]
    fn sessions_to_legs_builds_ordered_normalized_legs() {
        let sm = |dist: f64| SessionMetrics {
            total_distance_m: Some(dist),
            total_timer_time_s: Some(600.0),
            ..Default::default()
        };
        // The swim leg carries a sub_sport — it must resolve like the
        // activity itself would (open_water, not generic swim).
        let mut ow = sm(1500.0);
        ow.sub_sport = Some("open_water".to_string());
        let sessions = vec![
            (Some("swimming".to_string()), Some("2026-07-01T08:00:00+00:00".to_string()), ow),
            (Some("transition".to_string()), None, sm(0.2)),
            (Some("cycling".to_string()), None, sm(40000.0)),
            (Some("transition".to_string()), None, sm(0.2)),
            (Some("running".to_string()), None, sm(10000.0)),
        ];
        let legs = sessions_to_legs("tri-1", &sessions);
        assert_eq!(legs.len(), 5);
        assert_eq!(legs[0].sport_type, "open_water");
        assert_eq!(legs[0].leg_number, 1);
        assert_eq!(legs[0].start_time.as_deref(), Some("2026-07-01T08:00:00+00:00"));
        assert!(legs[1].is_transition);
        assert_eq!(legs[1].sport_type, "transition");
        assert_eq!(legs[2].sport_type, "ride");
        assert_eq!(legs[4].sport_type, "run");
        assert_eq!(legs[4].total_distance_m, Some(10000.0));

        // Single session → no legs.
        let single = vec![(Some("running".to_string()), None, sm(5000.0))];
        assert!(sessions_to_legs("a", &single).is_empty());
    }

    /// The triathlon fix: one session per leg → the activity is the combo,
    /// not the last leg. Transitions never count as a distinct sport.
    #[test]
    fn resolve_sport_detects_multisport() {
        let tri: Vec<String> = ["swimming", "transition", "cycling", "transition", "running"]
            .iter().map(|s| s.to_string()).collect();
        assert_eq!(resolve_sport(&tri), Some("multisport".to_string()));

        // Single-sport (even split across several sessions) keeps its sport.
        let run: Vec<String> = vec!["running".to_string(), "running".to_string()];
        assert_eq!(resolve_sport(&run), Some("running".to_string()));
        assert_eq!(resolve_sport(&["cycling".to_string()]), Some("cycling".to_string()));

        // Degenerate inputs: no sessions → None; all-transition → transition.
        assert_eq!(resolve_sport(&[]), None);
        assert_eq!(
            resolve_sport(&["transition".to_string()]),
            Some("transition".to_string())
        );
    }

    /// Balance decoding: the flag bit says "payload = right pedal". Raw
    /// record bytes like 171 mean right 43%, session values like 37026 mean
    /// right 42.58% — stored raw they read as impossible percentages.
    #[test]
    fn decode_lr_balance_strips_flag_and_scales() {
        // Record-level uint8: 128 + percent.
        assert_eq!(decode_lr_balance(171.0, false), Some(43.0));
        assert_eq!(decode_lr_balance(128.0, false), Some(0.0));
        assert_eq!(decode_lr_balance(228.0, false), Some(100.0));
        // Flagless side is undefined per the FIT profile → dropped.
        assert_eq!(decode_lr_balance(43.0, false), None);
        // 255 is the uint8 invalid marker (would be "right 127%").
        assert_eq!(decode_lr_balance(255.0, false), None);

        // Session-level uint16: 32768 + percent × 100.
        assert_eq!(decode_lr_balance(37026.0, true), Some(42.58));
        assert_eq!(decode_lr_balance(32768.0, true), Some(0.0));
        assert_eq!(decode_lr_balance(42768.0, true), Some(100.0));
        assert_eq!(decode_lr_balance(4258.0, true), None);
        assert_eq!(decode_lr_balance(65535.0, true), None);
    }

    /// Garmin writes TimeInZone once per lap AND once per session; keeping
    /// all of them duplicated every zone row (times × lap count + 1). Only
    /// session-scoped messages survive when any exist; unlabeled files keep
    /// everything.
    #[test]
    fn select_time_in_zones_prefers_session_scope() {
        let row = |zone_type: &str, time_s: f64| TimeInZone {
            id: None,
            activity_id: "a".to_string(),
            zone_type: zone_type.to_string(),
            zone_index: 0,
            time_s,
            zone_high_boundary: None,
        };
        let groups = vec![
            (Some("lap".to_string()), vec![row("hr", 100.0)]),
            (Some("lap".to_string()), vec![row("hr", 200.0)]),
            (Some("session".to_string()), vec![row("hr", 300.0), row("power", 250.0)]),
        ];
        let selected = select_time_in_zones(groups);
        assert_eq!(selected.len(), 2);
        assert!(selected.iter().all(|z| z.time_s >= 250.0));

        // No session-labeled message (or no labels at all) → keep everything.
        let unlabeled = vec![
            (None, vec![row("hr", 100.0)]),
            (Some("lap".to_string()), vec![row("hr", 200.0)]),
        ];
        assert_eq!(select_time_in_zones(unlabeled).len(), 2);
    }

    /// Cycling Dynamics arrays: power phase = [start, end], *_position =
    /// [seated, standing]. Short/empty/non-array values must yield Nones,
    /// not panic or misalign.
    #[test]
    fn field_pair_extracts_first_two_elements() {
        use fitparser::{FitDataField, Value};
        let arr = |vals: Vec<Value>| {
            FitDataField::new("avg_power_position".into(), 0, Value::Array(vals), "watts".into())
        };

        assert_eq!(
            field_pair(&arr(vec![Value::UInt16(231), Value::UInt16(161)])),
            (Some(231.0), Some(161.0))
        );
        // Longer arrays (some devices append extras): first two only.
        assert_eq!(
            field_pair(&arr(vec![
                Value::Float64(324.8),
                Value::Float64(230.6),
                Value::Float64(1.0)
            ])),
            (Some(324.8), Some(230.6))
        );
        assert_eq!(field_pair(&arr(vec![Value::UInt16(83)])), (Some(83.0), None));
        assert_eq!(field_pair(&arr(vec![])), (None, None));
        // Scalar (non-array) field → no pair.
        let scalar = FitDataField::new("x".into(), 0, Value::UInt16(5), "".into());
        assert_eq!(field_pair(&scalar), (None, None));
    }

    #[test]
    fn parse_fit_invalid_file_returns_error() {
        let result = parse_fit("/nonexistent/path.fit", "test");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Failed to read FIT file"));
    }

    #[test]
    fn parse_fit_invalid_binary_returns_error() {
        let tmp = std::env::temp_dir().join("tv_test_bad.fit");
        std::fs::write(&tmp, b"not a valid FIT file").unwrap();

        let result = parse_fit(tmp.to_str().unwrap(), "test-bad");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Failed to parse FIT"));

        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn value_to_timestamp_uint32_garmin_epoch() {
        // Garmin epoch: 1989-12-31T00:00:00Z
        // 1000000000 seconds after = ~2021
        let val = fitparser::Value::UInt32(1000000000);
        let ts = value_to_timestamp(&val);
        assert!(ts.is_some());
        let s = ts.unwrap();
        assert!(s.contains("2021"));
    }

    #[test]
    fn value_to_timestamp_string_iso() {
        let val = fitparser::Value::String("2025-06-01T08:00:00+00:00".to_string());
        let ts = value_to_timestamp(&val);
        assert!(ts.is_some());
        assert!(ts.unwrap().contains("2025"));
    }

    #[test]
    fn value_to_timestamp_string_noniso() {
        let val = fitparser::Value::String("some-time-string".to_string());
        let ts = value_to_timestamp(&val);
        assert_eq!(ts, Some("some-time-string".to_string()));
    }

    #[test]
    fn value_to_timestamp_returns_none_for_zero() {
        let val = fitparser::Value::SInt8(0);
        let ts = value_to_timestamp(&val);
        // "0" should return None
        assert!(ts.is_none());
    }

    #[test]
    fn value_to_timestamp_non_zero_fallback() {
        let val = fitparser::Value::SInt8(42);
        let ts = value_to_timestamp(&val);
        assert!(ts.is_some());
        assert_eq!(ts.unwrap(), "42");
    }

    #[test]
    fn field_to_f64_vec_extracts_array_values() {
        use fitparser::{FitDataField, Value};

        // Test with Array of Float64 values
        let field = FitDataField::new(
            "time_in_hr_zone".to_string(),
            0,
            Value::Array(vec![
                Value::Float64(10.0),
                Value::Float64(120.5),
                Value::Float64(300.0),
                Value::Float64(180.0),
                Value::Float64(50.0),
            ]),
            "s".to_string(),
        );
        let result = field_to_f64_vec(&field);
        assert_eq!(result.len(), 5);
        assert_eq!(result[0], 10.0);
        assert_eq!(result[1], 120.5);
        assert_eq!(result[4], 50.0);
    }

    #[test]
    fn field_to_f64_vec_handles_uint_array() {
        use fitparser::{FitDataField, Value};

        let field = FitDataField::new(
            "hr_zone_high_boundary".to_string(),
            0,
            Value::Array(vec![
                Value::UInt8(120),
                Value::UInt8(140),
                Value::UInt8(160),
            ]),
            "bpm".to_string(),
        );
        let result = field_to_f64_vec(&field);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0], 120.0);
        assert_eq!(result[1], 140.0);
        assert_eq!(result[2], 160.0);
    }

    #[test]
    fn field_to_f64_vec_returns_empty_for_non_array() {
        use fitparser::{FitDataField, Value};

        let field = FitDataField::new(
            "some_field".to_string(),
            0,
            Value::Float64(42.0),
            "".to_string(),
        );
        let result = field_to_f64_vec(&field);
        assert!(result.is_empty());
    }
}
