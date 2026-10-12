use chrono::{DateTime, Utc};
use fitparser::{self, profile::MesgNum};

use crate::models::exercise_set::ExerciseSet;
use crate::models::hrv_sample::HrvSample;
use crate::models::lap::Lap;
use crate::models::swim_length::SwimLength;
use crate::models::time_in_zone::TimeInZone;
use crate::models::trackpoint::TrackPoint;
use crate::models::multisport_leg::MultisportLeg;
use crate::parser::{ParsedActivity, SensorInfo, SessionMetrics};

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

/// The device-identifying fields of a `file_id` or `device_info` message.
/// Empty and zero values are dropped: "0" is the SDK's "unknown" product.
#[derive(Default, Debug, Clone, PartialEq)]
struct DeviceFields {
    product_name: Option<String>,
    manufacturer: Option<String>,
    product: Option<String>,
}

impl DeviceFields {
    fn of(msg: &fitparser::FitDataRecord) -> Self {
        let mut d = DeviceFields::default();
        for field in msg.fields() {
            let val = format!("{}", field.value());
            if val.is_empty() || val == "0" {
                continue;
            }
            match field.name() {
                "product_name" => d.product_name = Some(val),
                "manufacturer" => d.manufacturer = Some(val),
                // The SDK resolves manufacturer-specific product enums to
                // model names ("fenix6x", "edge_840"); unknown ids stay numeric.
                "garmin_product" | "product" => d.product = Some(val),
                _ => {}
            }
        }
        d
    }

    fn is_empty(&self) -> bool {
        self.product_name.is_none() && self.manufacturer.is_none() && self.product.is_none()
    }

    /// Fill what `self` lacks from `other` — the two messages describe the
    /// same device, they just carry different subsets of its fields.
    fn or(self, other: DeviceFields) -> DeviceFields {
        DeviceFields {
            product_name: self.product_name.or(other.product_name),
            manufacturer: self.manufacturer.or(other.manufacturer),
            product: self.product.or(other.product),
        }
    }

    fn name(self) -> Option<String> {
        compose_device_name(self.product_name, self.manufacturer, self.product)
    }
}

/// The device that recorded the file, from the decoded messages.
///
/// `file_id` names the file's creator and is always present; the
/// `device_info` with `device_index` = creator (index 0) is the same device
/// with more fields (Wahoo puts `product_name` there, not in file_id), so the
/// two are merged. Only when neither says anything does the FIRST
/// `device_info` count — that message used to be the sole source and it can
/// be a sensor: a paired peripheral listed before the watch turned a fenix
/// ride into "Garmin 1620" (#144).
pub fn source_device_of(messages: &[fitparser::FitDataRecord]) -> Option<String> {
    let mut file_id: Option<DeviceFields> = None;
    let mut creator: Option<DeviceFields> = None;
    let mut first: Option<DeviceFields> = None;
    for msg in messages {
        match msg.kind() {
            MesgNum::FileId if file_id.is_none() => file_id = Some(DeviceFields::of(msg)),
            MesgNum::DeviceInfo => {
                let fields = DeviceFields::of(msg);
                let is_creator = msg.fields().iter().any(|f| {
                    f.name() == "device_index" && {
                        let v = format!("{}", f.value());
                        v == "creator" || v == "0"
                    }
                });
                // A device_info can carry only a timestamp and an index (a
                // sensor's battery report): such a message names nothing
                // and must not take the slot from a later one that does.
                if fields.is_empty() {
                    continue;
                }
                if is_creator && creator.is_none() {
                    creator = Some(fields.clone());
                }
                if first.is_none() {
                    first = Some(fields);
                }
            }
            _ => {}
        }
    }
    let own = file_id.unwrap_or_default().or(creator.unwrap_or_default());
    if !own.is_empty() {
        return own.name();
    }
    first.and_then(DeviceFields::name)
}

/// `source_device_of` over raw file bytes — for the one-time backfill that
/// re-reads stored FIT files (#144). Undecodable bytes give None.
pub fn source_device_of_bytes(data: &[u8]) -> Option<String> {
    fitparser::from_bytes(data)
        .ok()
        .and_then(|messages| source_device_of(&messages))
}

/// The activity profile the device recorded under: the `name` of the
/// `sport` message ("ROAD" on an Edge, "Bike" on a fenix). Garmin users
/// keep one profile per bike, so it is the key gear rules match on.
pub fn profile_name_of(messages: &[fitparser::FitDataRecord]) -> Option<String> {
    messages.iter().find(|m| m.kind() == MesgNum::Sport).and_then(|m| {
        m.fields()
            .iter()
            .find(|f| f.name() == "name")
            .map(|f| format!("{}", f.value()).trim().to_string())
            .filter(|n| !n.is_empty())
    })
}

/// The paired sensors the file names with a serial number: every
/// `device_info` that is not the recording device itself, one entry per
/// serial — a sensor reports its battery several times over a ride, and
/// the later reports fill in what the first left out. The recording
/// device is known by `source_type` local or the creator index, and by
/// its serial: some watches (a vivoactive) also list themselves in a
/// `device_info` with neither marker, so any serial that `file_id` or the
/// creator row carries is the watch, not a sensor. The product field is
/// manufacturer-specific (`garmin_product`, `favero_product`, …); the SDK
/// resolves known ones to names ("assioma_duo"), unknown ones stay numeric.
pub fn sensors_of(messages: &[fitparser::FitDataRecord]) -> Vec<SensorInfo> {
    let field = |msg: &fitparser::FitDataRecord, name: &str| -> Option<String> {
        msg.fields()
            .iter()
            .find(|f| f.name() == name)
            .map(|f| format!("{}", f.value()))
            .filter(|v| !v.is_empty() && v != "0")
    };
    // The watch's own serials: file_id's, and every device_info marked
    // local or creator.
    let mut own: Vec<String> = Vec::new();
    for msg in messages {
        let is_own = match msg.kind() {
            MesgNum::FileId => true,
            MesgNum::DeviceInfo => {
                field(msg, "source_type").as_deref() == Some("local")
                    || field(msg, "device_index").as_deref() == Some("creator")
            }
            _ => false,
        };
        if is_own {
            if let Some(serial) = field(msg, "serial_number") {
                own.push(serial);
            }
        }
    }
    let mut out: Vec<SensorInfo> = Vec::new();
    for msg in messages.iter().filter(|m| m.kind() == MesgNum::DeviceInfo) {
        let mut serial = None;
        let mut device_type = None;
        let mut manufacturer = None;
        let mut product = None;
        let mut local = false;
        for f in msg.fields() {
            let val = format!("{}", f.value());
            if val.is_empty() || val == "0" {
                continue;
            }
            match f.name() {
                "serial_number" => serial = Some(val),
                "antplus_device_type" | "ble_device_type" | "ant_device_type" | "device_type" => {
                    device_type = Some(val)
                }
                "manufacturer" => manufacturer = Some(val),
                "source_type" if val == "local" => local = true,
                "device_index" if val == "creator" => local = true,
                n if n == "product" || n.ends_with("_product") => product = Some(val),
                _ => {}
            }
        }
        let Some(serial) = serial else { continue };
        if local || own.contains(&serial) {
            continue;
        }
        match out.iter_mut().find(|s| s.serial == serial) {
            Some(seen) => {
                // A later report fills what the first left out.
                if seen.device_type.is_none() {
                    seen.device_type = device_type;
                }
                if seen.manufacturer.is_none() {
                    seen.manufacturer = manufacturer;
                }
                if seen.product.is_none() {
                    seen.product = product;
                }
            }
            None => out.push(SensorInfo { serial, device_type, manufacturer, product }),
        }
    }
    out
}

/// Both gear keys over raw file bytes — for the one-time backfill that
/// re-reads stored FIT files (#167). Undecodable bytes give None.
pub fn gear_keys_of_bytes(data: &[u8]) -> Option<(Option<String>, Vec<SensorInfo>)> {
    let messages = fitparser::from_bytes(data).ok()?;
    Some((profile_name_of(&messages), sensors_of(&messages)))
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
/// each); ingesting all of them duplicated every zone row. Vaults imported
/// before this selection still hold those duplicates: the chart code keys
/// buckets by `zone_index` (chartZones.ts `deviceZoneRanges`) so they stay
/// harmless there, but any "time in zones" feature would multiply times by
/// the lap count + 1. Keep session-scoped messages when any exist;
/// unlabeled files keep everything.
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

/// The sets of a strength activity, numbered from 1 in file order. The one
/// place sets are numbered: the import stores them under these numbers and
/// the backfill (#213) finds them again by the same.
pub fn exercise_sets_of(
    messages: &[fitparser::FitDataRecord],
    activity_id: &str,
) -> Vec<ExerciseSet> {
    messages
        .iter()
        .filter(|m| m.kind() == MesgNum::Set)
        .enumerate()
        .map(|(i, m)| exercise_set_of(m, activity_id, i as i32 + 1))
        .collect()
}

/// `exercise_sets_of` over raw file bytes — for the one-time backfill that
/// re-reads stored FIT files (#213). Undecodable bytes give None.
pub fn exercise_sets_of_bytes(data: &[u8], activity_id: &str) -> Option<Vec<ExerciseSet>> {
    fitparser::from_bytes(data)
        .ok()
        .map(|messages| exercise_sets_of(&messages, activity_id))
}

fn exercise_set_of(
    msg: &fitparser::FitDataRecord,
    activity_id: &str,
    set_number: i32,
) -> ExerciseSet {
    let mut set = ExerciseSet {
        id: None,
        activity_id: activity_id.to_string(),
        set_number,
        start_time: None,
        category: None,
        category_subtype: None,
        set_type: None,
        duration_s: None,
        repetitions: None,
        weight_kg: None,
        wkt_step_index: None,
    };
    let (mut category, mut subtype) = (None, None);

    for field in msg.fields() {
        match field.name() {
            "start_time" | "timestamp" => {
                if set.start_time.is_none() {
                    set.start_time = value_to_timestamp(field.value());
                }
            }
            "category" => category = Some(field.value()),
            "category_subtype" => subtype = Some(field.value()),
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

    (set.category, set.category_subtype) = exercise_of(category, subtype);
    set
}

/// FIT's "no value" for a uint16. fitparser 0.7 drops a field that holds it
/// — a whole array when one element is it — so this guards a decoder that
/// would hand the element through instead.
const NO_VALUE: i64 = 0xFFFF;
/// What a watch pads the exercise arrays with: the profile's `unknown`.
const UNKNOWN_EXERCISE: i64 = 0xFFFE;

/// The exercise a set names, as profile names: `("lateral_raise",
/// "seated_lateral_raise")`.
///
/// `category` and `category_subtype` are parallel arrays in the FIT profile,
/// and the decoder names an enum only when it arrives as a single value —
/// the elements of an array stay raw numbers, which were stored as their
/// debug dump, `[UInt16(14), UInt16(14), UInt16(65534)]` (#213). A watch
/// pads the arrays with "unknown"; the first entry that names an exercise is
/// taken as the set's, and the subtype is read at the same position, since
/// what a subtype number means depends on the category beside it.
fn exercise_of(
    category: Option<&fitparser::Value>,
    subtype: Option<&fitparser::Value>,
) -> (Option<String>, Option<String>) {
    let named = category
        .map(elements)
        .unwrap_or_default()
        .into_iter()
        .enumerate()
        .find_map(|(at, value)| category_name(value).map(|name| (at, name)));
    let Some((at, category)) = named else {
        return (None, None);
    };
    let subtype = subtype
        .map(elements)
        .and_then(|values| values.into_iter().nth(at))
        .and_then(|value| subtype_name(&category, value));
    (Some(category), subtype)
}

/// A field's values: the elements of an array, or the single value itself.
fn elements(value: &fitparser::Value) -> Vec<&fitparser::Value> {
    match value {
        fitparser::Value::Array(values) => values.iter().collect(),
        single => vec![single],
    }
}

/// The category one element names; None for padding. A category the profile
/// has no name for keeps its number, so nothing a file says is dropped.
fn category_name(value: &fitparser::Value) -> Option<String> {
    use fitparser::profile::field_types::ExerciseCategory;
    use std::convert::TryInto;

    let name = match value {
        fitparser::Value::String(name) => name.clone(),
        other => {
            let code: i64 = other.try_into().ok()?;
            if code == NO_VALUE {
                return None;
            }
            ExerciseCategory::from(u16::try_from(code).ok()?).to_string()
        }
    };
    (name != "unknown").then_some(name)
}

/// The exercise within `category` one subtype element names; None for
/// padding. A number that category's table does not list is kept as is.
fn subtype_name(category: &str, value: &fitparser::Value) -> Option<String> {
    use fitparser::profile::get_field_variant_as_string;
    use std::convert::TryInto;

    if let fitparser::Value::String(name) = value {
        return Some(name.clone());
    }
    let code: i64 = value.try_into().ok()?;
    if code == NO_VALUE || code == UNKNOWN_EXERCISE {
        return None;
    }
    Some(match exercise_names_of(category) {
        Some(table) if table.is_named_variant(code) => get_field_variant_as_string(table, code),
        _ => code.to_string(),
    })
}

/// The profile's table of exercise names for a category.
fn exercise_names_of(category: &str) -> Option<fitparser::profile::FieldDataType> {
    use fitparser::profile::FieldDataType as T;

    Some(match category {
        "bench_press" => T::BenchPressExerciseName,
        "calf_raise" => T::CalfRaiseExerciseName,
        "cardio" => T::CardioExerciseName,
        "carry" => T::CarryExerciseName,
        "chop" => T::ChopExerciseName,
        "core" => T::CoreExerciseName,
        "crunch" => T::CrunchExerciseName,
        "curl" => T::CurlExerciseName,
        "deadlift" => T::DeadliftExerciseName,
        "flye" => T::FlyeExerciseName,
        "hip_raise" => T::HipRaiseExerciseName,
        "hip_stability" => T::HipStabilityExerciseName,
        "hip_swing" => T::HipSwingExerciseName,
        "hyperextension" => T::HyperextensionExerciseName,
        "lateral_raise" => T::LateralRaiseExerciseName,
        "leg_curl" => T::LegCurlExerciseName,
        "leg_raise" => T::LegRaiseExerciseName,
        "lunge" => T::LungeExerciseName,
        "olympic_lift" => T::OlympicLiftExerciseName,
        "plank" => T::PlankExerciseName,
        "plyo" => T::PlyoExerciseName,
        "pull_up" => T::PullUpExerciseName,
        "push_up" => T::PushUpExerciseName,
        "row" => T::RowExerciseName,
        "shoulder_press" => T::ShoulderPressExerciseName,
        "shoulder_stability" => T::ShoulderStabilityExerciseName,
        "shrug" => T::ShrugExerciseName,
        "sit_up" => T::SitUpExerciseName,
        "squat" => T::SquatExerciseName,
        "total_body" => T::TotalBodyExerciseName,
        "triceps_extension" => T::TricepsExtensionExerciseName,
        "warm_up" => T::WarmUpExerciseName,
        "run" => T::RunExerciseName,
        _ => return None,
    })
}

/// Parse FIT from in-memory bytes (e.g. decompressed from a .gz import).
pub fn parse_fit_bytes(data: &[u8], activity_id: &str) -> Result<ParsedActivity, String> {
    let messages =
        fitparser::from_bytes(data).map_err(|e| format!("Failed to parse FIT: {}", e))?;
    parse_fit_records(&messages, activity_id)
}

/// Build the activity from already-decoded FIT messages — the import
/// pipeline decodes a file once to route it by `file_id` (activity vs
/// Garmin monitoring, ADR 0002) and hands the records on from there.
pub fn parse_fit_records(
    messages: &[fitparser::FitDataRecord],
    activity_id: &str,
) -> Result<ParsedActivity, String> {
    let mut trackpoints: Vec<TrackPoint> = Vec::new();
    let mut start_time: Option<String> = None;
    // From MesgNum::Sport — the fallback when the file carries no sessions.
    let mut sport_message: Option<String> = None;
    // One entry per Session message: (its sport, its metrics). Multisport
    // files (triathlon) carry one session PER LEG — they are resolved after
    // the loop; collapsing them here with last-wins used to file every
    // triathlon under its final leg with the legs' metrics shuffled.
    let mut sessions: Vec<(Option<String>, Option<String>, SessionMetrics)> = Vec::new();
    let mut laps: Vec<Lap> = Vec::new();
    let mut lengths: Vec<SwimLength> = Vec::new();
    let sets = exercise_sets_of(messages, activity_id);
    // One entry per TimeInZone message: (its reference_mesg, its rows).
    // Resolved after the loop — see select_time_in_zones.
    let mut time_in_zone_groups: Vec<(Option<String>, Vec<TimeInZone>)> = Vec::new();
    let mut hrv_samples: Vec<HrvSample> = Vec::new();

    for msg in messages {
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
            _ => {}
        }
    }

    let source_device = source_device_of(messages);
    let profile_name = profile_name_of(messages);
    let sensors = sensors_of(messages);

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
        profile_name,
        sensors,
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

    fn set_msg(fields: Vec<(&str, fitparser::Value)>) -> fitparser::FitDataRecord {
        let mut msg = fitparser::FitDataRecord::new(MesgNum::Set);
        for (i, (name, value)) in fields.into_iter().enumerate() {
            msg.push(fitparser::FitDataField::new(name.to_string(), i as u8, value, String::new()));
        }
        msg
    }

    fn codes(values: &[u16]) -> fitparser::Value {
        fitparser::Value::Array(values.iter().map(|v| fitparser::Value::UInt16(*v)).collect())
    }

    fn exercise(set: &ExerciseSet) -> (Option<&str>, Option<&str>) {
        (set.category.as_deref(), set.category_subtype.as_deref())
    }

    /// #213: the shape a fenix writes — the category as an array padded with
    /// "unknown", which the decoder leaves as raw numbers. It was stored as
    /// `[UInt16(14), UInt16(14), UInt16(65534)]`.
    #[test]
    fn a_set_names_its_exercise_from_the_category_array() {
        use fitparser::Value;
        // The real file: a category array and no subtype at all.
        let sets = exercise_sets_of(
            &[set_msg(vec![
                ("category", codes(&[14, 14, 65534])),
                ("set_type", Value::String("active".into())),
                ("repetitions", Value::UInt16(53)),
                ("duration", Value::Float64(2113.0)),
            ])],
            "act",
        );
        assert_eq!(exercise(&sets[0]), (Some("lateral_raise"), None));
        assert_eq!(sets[0].activity_id, "act");
        assert_eq!((sets[0].set_number, sets[0].repetitions, sets[0].duration_s), (1, Some(53), Some(2113.0)));
        assert_eq!(sets[0].set_type.as_deref(), Some("active"));

        // With a subtype: named from the category's own table.
        let both = exercise_sets_of(
            &[set_msg(vec![
                ("category", codes(&[14, 14, 65534])),
                ("category_subtype", codes(&[24, 24, 65534])),
            ])],
            "act",
        );
        assert_eq!(exercise(&both[0]), (Some("lateral_raise"), Some("seated_lateral_raise")));
    }

    /// The subtype is read at the position of the category it belongs to:
    /// the same number names a different exercise in another category's
    /// table, so reading the first subtype beside the second category would
    /// name the wrong exercise.
    #[test]
    fn a_subtype_is_read_beside_its_own_category() {
        use fitparser::Value;
        let one = |fields| exercise_sets_of(&[set_msg(fields)], "a").remove(0);

        // Padding first: the exercise is the second entry, and so is its subtype.
        let padded = one(vec![("category", codes(&[65534, 0])), ("category_subtype", codes(&[24, 1]))]);
        assert_eq!(exercise(&padded), (Some("bench_press"), Some("barbell_bench_press")));
        // FIT's own "no value" must not be named an exercise either.
        // (fitparser 0.7 never hands this shape over — it drops such a
        // field whole — so only hand-built messages reach it.)
        let invalid = one(vec![("category", codes(&[65535, 14])), ("category_subtype", codes(&[65535, 24]))]);
        assert_eq!(exercise(&invalid), (Some("lateral_raise"), Some("seated_lateral_raise")));
        // A subtype that is padding names nothing; the category still stands.
        let no_sub = one(vec![("category", codes(&[14])), ("category_subtype", codes(&[65534]))]);
        assert_eq!(exercise(&no_sub), (Some("lateral_raise"), None));
        let short_sub = one(vec![("category", codes(&[65534, 14])), ("category_subtype", codes(&[24]))]);
        assert_eq!(exercise(&short_sub), (Some("lateral_raise"), None), "no subtype at the category's position");

        // Nothing but padding, or no category at all: the set names no
        // exercise — and a subtype without a category means nothing.
        let empty = one(vec![("category", codes(&[65534, 65535])), ("category_subtype", codes(&[24]))]);
        assert_eq!(exercise(&empty), (None, None));
        let bare = one(vec![("category_subtype", codes(&[24])), ("repetitions", Value::UInt16(8))]);
        assert_eq!(exercise(&bare), (None, None));
        assert_eq!(bare.repetitions, Some(8));
    }

    /// A single value arrives already named by the decoder (a one-element
    /// array is decoded as a scalar), and a number the profile has no name
    /// for is kept rather than dropped.
    #[test]
    fn single_values_and_unlisted_numbers_are_kept() {
        use fitparser::Value;
        let one = |fields| exercise_sets_of(&[set_msg(fields)], "a").remove(0);

        let scalar = one(vec![
            ("category", Value::String("curl".into())),
            ("category_subtype", Value::UInt16(0)),
        ]);
        assert_eq!(scalar.category.as_deref(), Some("curl"));
        assert_eq!(
            scalar.category_subtype,
            Some(fitparser::profile::get_field_variant_as_string(
                fitparser::profile::FieldDataType::CurlExerciseName,
                0
            ))
        );
        let numeric = one(vec![("category", Value::UInt16(14)), ("category_subtype", Value::UInt16(24))]);
        assert_eq!(exercise(&numeric), (Some("lateral_raise"), Some("seated_lateral_raise")));
        let unknown_scalar = one(vec![("category", Value::String("unknown".into()))]);
        assert_eq!(exercise(&unknown_scalar), (None, None));

        // A category newer than the bundled profile, and a subtype its
        // table does not list.
        let newer = one(vec![("category", codes(&[999])), ("category_subtype", codes(&[7]))]);
        assert_eq!(exercise(&newer), (Some("999"), Some("7")));
        let unlisted = one(vec![("category", codes(&[14])), ("category_subtype", codes(&[60000]))]);
        assert_eq!(exercise(&unlisted), (Some("lateral_raise"), Some("60000")));
    }

    /// Every category the profile names has its OWN table of exercises: a
    /// typo in `exercise_names_of` would silently store numbers instead of
    /// names, and two arms swapped would name the wrong exercise.
    #[test]
    fn every_named_category_has_its_own_exercise_table() {
        use fitparser::profile::field_types::ExerciseCategory;
        let named: Vec<String> = (0u16..=64)
            .filter(|c| ExerciseCategory::is_named_variant(*c as i64))
            .map(|c| ExerciseCategory::from(c).to_string())
            .collect();
        assert_eq!(named.len(), 33, "the profile's categories, bench_press … run");
        for name in &named {
            // "bench_press" → "BenchPressExerciseName", the profile's naming.
            let expected: String = name
                .split('_')
                .map(|w| w[..1].to_uppercase() + &w[1..])
                .chain(["ExerciseName".to_string()])
                .collect();
            let table = exercise_names_of(name).unwrap_or_else(|| panic!("no exercise table for {name}"));
            assert_eq!(format!("{table:?}"), expected);
        }
        assert!(exercise_names_of("unknown").is_none());
        assert!(exercise_names_of("999").is_none());
    }

    /// The same through the real decoder, on the bytes of a file: what a
    /// fenix writes is a uint16[3], and it is the decoder — not a hand-built
    /// message — that leaves the array's elements unnamed.
    #[test]
    fn the_sets_of_a_strength_file_are_named_from_its_bytes() {
        use crate::parser::fit_builder::strength_fixture;
        let bytes = strength_fixture(&[
            ([14, 14, 65534], None),
            ([65534, 65534, 65534], None),
            ([0, 65534, 65534], Some([1, 65534, 65534])),
        ]);
        // What the old parser stored for the first set — the bug itself.
        let messages = fitparser::from_bytes(&bytes).unwrap();
        let raw = messages
            .iter()
            .find(|m| m.kind() == MesgNum::Set)
            .and_then(|m| m.fields().iter().find(|f| f.name() == "category"))
            .map(|f| format!("{}", f.value()));
        assert_eq!(raw.as_deref(), Some("[UInt16(14), UInt16(14), UInt16(65534)]"));

        let sets = exercise_sets_of_bytes(&bytes, "act").unwrap();
        assert_eq!(
            sets.iter().map(|s| (s.set_number, s.repetitions, exercise(s))).collect::<Vec<_>>(),
            vec![
                (1, Some(10), (Some("lateral_raise"), None)),
                (2, Some(11), (None, None)),
                (3, Some(12), (Some("bench_press"), Some("barbell_bench_press"))),
            ]
        );
        // The import stores exactly these.
        let parsed = parse_fit_bytes(&bytes, "act").unwrap();
        assert_eq!(
            parsed.sets.iter().map(|s| (s.set_number, exercise(s))).collect::<Vec<_>>(),
            sets.iter().map(|s| (s.set_number, exercise(s))).collect::<Vec<_>>()
        );
    }

    /// Sets are numbered in file order, other messages do not take a number,
    /// and the import path stores the same names the backfill re-derives.
    #[test]
    fn sets_are_numbered_in_file_order_on_both_paths() {
        let msgs = vec![
            set_msg(vec![("category", codes(&[0, 65534]))]),
            device_msg(MesgNum::Sport, &[("sport", "training"), ("name", "Strength")]),
            set_msg(vec![("category", codes(&[65534]))]),
            set_msg(vec![("category", codes(&[7, 65534])), ("category_subtype", codes(&[0, 65534]))]),
        ];
        let sets = exercise_sets_of(&msgs, "a");
        assert_eq!(sets.iter().map(|s| s.set_number).collect::<Vec<_>>(), vec![1, 2, 3]);
        assert_eq!(
            sets.iter().map(|s| s.category.as_deref()).collect::<Vec<_>>(),
            vec![Some("bench_press"), None, Some("curl")]
        );

        let parsed = parse_fit_records(&msgs, "a").unwrap();
        assert_eq!(
            parsed.sets.iter().map(|s| (s.set_number, s.category.clone(), s.category_subtype.clone())).collect::<Vec<_>>(),
            sets.iter().map(|s| (s.set_number, s.category.clone(), s.category_subtype.clone())).collect::<Vec<_>>()
        );
        assert!(parsed.sets.iter().all(|s| !s.category.as_deref().unwrap_or("").starts_with('[')));

        assert!(exercise_sets_of_bytes(b"not a fit file", "a").is_none());
    }

    fn device_msg(kind: MesgNum, fields: &[(&str, &str)]) -> fitparser::FitDataRecord {
        use fitparser::{FitDataField, Value};
        let mut msg = fitparser::FitDataRecord::new(kind);
        for (i, (name, val)) in fields.iter().enumerate() {
            msg.push(FitDataField::new(
                (*name).to_string(),
                i as u8,
                Value::String((*val).to_string()),
                String::new(),
            ));
        }
        msg
    }

    /// The gear keys (#167): the profile from `sport.name`, trimmed, and
    /// every non-local device_info with a serial, once per serial.
    #[test]
    fn gear_keys_are_the_profile_name_and_the_paired_sensors() {
        let msgs = vec![
            device_msg(MesgNum::FileId, &[("manufacturer", "garmin"), ("garmin_product", "edge_830")]),
            device_msg(MesgNum::DeviceInfo, &[("device_index", "creator"), ("manufacturer", "garmin"), ("serial_number", "3424354665"), ("garmin_product", "edge_830"), ("source_type", "local")]),
            device_msg(MesgNum::DeviceInfo, &[("device_index", "1"), ("local_device_type", "barometer"), ("source_type", "local")]),
            device_msg(MesgNum::DeviceInfo, &[("device_index", "3"), ("antplus_device_type", "heart_rate"), ("manufacturer", "garmin"), ("serial_number", "3384016267"), ("garmin_product", "OHR"), ("source_type", "antplus")]),
            device_msg(MesgNum::DeviceInfo, &[("device_index", "4"), ("antplus_device_type", "bike_power"), ("manufacturer", "favero_electronics"), ("serial_number", "3632674300"), ("favero_product", "assioma_duo"), ("source_type", "antplus")]),
            // The pedals report again mid-ride: still one sensor, and a
            // bare first report is completed by the fuller one.
            device_msg(MesgNum::DeviceInfo, &[("device_index", "5"), ("serial_number", "3945193103"), ("source_type", "antplus")]),
            device_msg(MesgNum::DeviceInfo, &[("device_index", "5"), ("antplus_device_type", "heart_rate"), ("manufacturer", "garmin"), ("serial_number", "3945193103"), ("garmin_product", "hrm_tri"), ("source_type", "antplus")]),
            device_msg(MesgNum::DeviceInfo, &[("device_index", "4"), ("antplus_device_type", "bike_power"), ("manufacturer", "favero_electronics"), ("serial_number", "3632674300"), ("favero_product", "assioma_duo"), ("source_type", "antplus")]),
            // A battery report without a serial names nothing.
            device_msg(MesgNum::DeviceInfo, &[("device_index", "4"), ("battery_status", "ok")]),
            // The watch listing itself with neither marker (a vivoactive
            // does): its serial is the creator's, so it is no sensor.
            device_msg(MesgNum::DeviceInfo, &[("manufacturer", "garmin"), ("serial_number", "3424354665"), ("garmin_product", "edge_830")]),
            device_msg(MesgNum::Sport, &[("sport", "cycling"), ("sub_sport", "road"), ("name", " ROAD ")]),
        ];
        assert_eq!(profile_name_of(&msgs).as_deref(), Some("ROAD"));
        let sensors = sensors_of(&msgs);
        assert_eq!(sensors.iter().map(|s| s.serial.as_str()).collect::<Vec<_>>(), vec!["3384016267", "3632674300", "3945193103"]);
        assert_eq!(sensors[0].device_type.as_deref(), Some("heart_rate"));
        assert_eq!(sensors[1].manufacturer.as_deref(), Some("favero_electronics"));
        assert_eq!(sensors[1].product.as_deref(), Some("assioma_duo"));
        assert_eq!(sensors[1].device_type.as_deref(), Some("bike_power"));
        assert_eq!(sensors[2].product.as_deref(), Some("hrm_tri"), "filled in by the later report");
        assert_eq!(sensors[2].device_type.as_deref(), Some("heart_rate"));
        // The watch's serial only in file_id: still excluded.
        let by_file_id = vec![
            device_msg(MesgNum::FileId, &[("manufacturer", "garmin"), ("serial_number", "3314747279"), ("garmin_product", "vivoactive4")]),
            device_msg(MesgNum::DeviceInfo, &[("manufacturer", "garmin"), ("serial_number", "3314747279"), ("garmin_product", "vivoactive4")]),
            device_msg(MesgNum::DeviceInfo, &[("device_type", "7"), ("serial_number", "555"), ("source_type", "bluetooth_low_energy")]),
        ];
        let s2 = sensors_of(&by_file_id);
        assert_eq!(s2.len(), 1);
        assert_eq!(s2[0].serial, "555");
        assert_eq!(s2[0].device_type.as_deref(), Some("7"));
        // No sport message, no sensors: nothing.
        let bare = vec![device_msg(MesgNum::FileId, &[("manufacturer", "garmin")])];
        assert_eq!(profile_name_of(&bare), None);
        assert!(sensors_of(&bare).is_empty());
        // An empty name is no profile.
        let unnamed = vec![device_msg(MesgNum::Sport, &[("sport", "cycling"), ("name", "  ")])];
        assert_eq!(profile_name_of(&unnamed), None);
        assert_eq!(gear_keys_of_bytes(b"not a fit file"), None);
    }

    /// #144: a paired sensor listed before the watch must not become the
    /// recording device — file_id names the creator, and the creator's
    /// device_info completes it.
    #[test]
    fn source_device_prefers_file_id_and_creator_over_the_first_device_info() {
        let s = |v: &str| Some(v.to_string());
        // The real shape of the FR920XT files: sensor first, creator later.
        let msgs = vec![
            device_msg(MesgNum::FileId, &[("type", "activity"), ("manufacturer", "garmin"), ("garmin_product", "fr920xt")]),
            device_msg(MesgNum::DeviceInfo, &[("device_index", "2"), ("manufacturer", "garmin"), ("garmin_product", "1620")]),
            device_msg(MesgNum::DeviceInfo, &[("device_index", "creator"), ("manufacturer", "garmin"), ("garmin_product", "fr920xt")]),
        ];
        assert_eq!(source_device_of(&msgs), s("Garmin fr920xt"));

        // Wahoo: file_id has only the numeric product, the creator device_info
        // carries product_name — merged, the name wins.
        let msgs = vec![
            device_msg(MesgNum::FileId, &[("manufacturer", "wahoo_fitness"), ("product", "31")]),
            device_msg(MesgNum::DeviceInfo, &[("device_index", "0"), ("manufacturer", "wahoo_fitness"), ("product", "31"), ("product_name", "ELEMNT BOLT")]),
        ];
        assert_eq!(source_device_of(&msgs), s("ELEMNT BOLT"));

        // No creator record: file_id alone is enough.
        let msgs = vec![device_msg(MesgNum::FileId, &[("manufacturer", "garmin"), ("garmin_product", "fenix6x")])];
        assert_eq!(source_device_of(&msgs), s("Garmin fenix6x"));

        // file_id without a device (type only) and no creator: the first
        // device_info is the last resort, as before.
        let msgs = vec![
            device_msg(MesgNum::FileId, &[("type", "activity")]),
            device_msg(MesgNum::DeviceInfo, &[("device_index", "1"), ("manufacturer", "garmin"), ("garmin_product", "hrm_pro")]),
        ];
        assert_eq!(source_device_of(&msgs), s("Garmin hrm_pro"));

        // An empty device_info (battery report: timestamp + index only)
        // does not take the fallback slot from the next one that names a
        // device — the old first-wins code skipped it too.
        let msgs = vec![
            device_msg(MesgNum::FileId, &[("type", "activity")]),
            device_msg(MesgNum::DeviceInfo, &[("device_index", "1"), ("battery_status", "good")]),
            device_msg(MesgNum::DeviceInfo, &[("device_index", "1"), ("manufacturer", "garmin"), ("garmin_product", "fenix7")]),
        ];
        assert_eq!(source_device_of(&msgs), s("Garmin fenix7"));

        // Zero products are unknown, not a model.
        let msgs = vec![device_msg(MesgNum::FileId, &[("manufacturer", "garmin"), ("garmin_product", "0")])];
        assert_eq!(source_device_of(&msgs), s("Garmin"));

        assert_eq!(source_device_of(&[]), None);
        assert_eq!(source_device_of_bytes(b"not a fit file"), None);
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
        let scratch = crate::test_support::ScratchDir::new("fit_bad");
        let tmp = scratch.join("test_bad.fit");
        std::fs::write(&tmp, b"not a valid FIT file").unwrap();

        let result = parse_fit(tmp.to_str().unwrap(), "test-bad");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Failed to parse FIT"));
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
