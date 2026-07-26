use serde::{Deserialize, Serialize};

/// Normalized activity types, aligned with the activity profiles a Garmin
/// watch records. Many vendor/FIT variant strings collapse into each one (see
/// `from_str`). Serialized snake_case across the IPC boundary; the frontend
/// mirrors this set in `types.ts` (labels, colors, icons).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SportType {
    Run,
    TrailRun,
    Treadmill,
    Ride,
    MountainBike,
    Walk,
    Hike,
    Mountaineering,
    Swim,
    OpenWater,
    Sailing,
    Paddle,
    Fishing,
    Triathlon,
    Strength,
    Cardio,
    Yoga,
    Ski,
    SkiXc,
    Snowboard,
    Golf,
    Tennis,
    Soccer,
    Basketball,
    Other,
}

impl SportType {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            // ── Running ──
            "run" | "running" | "track_running" | "track running" | "track"
            | "indoor_running" | "indoor running" | "street_running"
            | "road_running" | "road running" | "virtual_run" | "virtual run" => SportType::Run,

            "trail_running" | "trail running" | "trail_run" | "trail" => SportType::TrailRun,

            "treadmill_running" | "treadmill running" | "treadmill" => SportType::Treadmill,

            // ── Cycling ──
            "ride" | "cycling" | "biking" | "bicycle" | "bike"
            | "road_cycling" | "road cycling" | "road"
            | "gravel_cycling" | "gravel cycling" | "gravel" | "cyclocross"
            | "indoor_cycling" | "indoor cycling" | "virtual_ride" | "virtual ride"
            | "e_bike_ride" | "e-bike" | "ebike" | "spin" | "spinning" => SportType::Ride,

            "mountain_biking" | "mountain biking" | "mtb" | "mountain_bike"
            | "mountain" | "downhill_mtb" | "enduro_mtb" => SportType::MountainBike,

            // ── Walking ──
            "walk" | "walking" | "casual_walking" | "casual walking"
            | "speed_walking" | "speed walking" | "nordic_walking" | "nordic walking" => {
                SportType::Walk
            }

            // ── Hiking / mountaineering ──
            "hike" | "hiking" | "trail_hiking" | "trail hiking" | "backpacking" => SportType::Hike,
            "mountaineering" | "alpinism" | "mountain_climbing" | "climbing"
            | "rock_climbing" | "bouldering" => SportType::Mountaineering,

            // ── Swimming / water ──
            "swim" | "swimming" | "lap_swimming" | "lap swimming"
            | "pool_swimming" | "pool swimming" | "pool" => SportType::Swim,
            "open_water_swimming" | "open water swimming" | "open_water" | "openwater" => {
                SportType::OpenWater
            }
            "sailing" | "sail" | "windsurfing" | "kitesurfing" | "kiteboarding" => {
                SportType::Sailing
            }
            "paddle" | "paddling" | "stand_up_paddleboarding" | "sup" | "kayaking" | "kayak"
            | "canoeing" | "canoe" | "rowing" | "indoor_rowing" | "whitewater"
            | "rafting" => SportType::Paddle,
            "fishing" => SportType::Fishing,

            // ── Multisport ──
            "triathlon" | "tri" | "multisport" | "multi_sport" | "brick" | "duathlon"
            | "aquathlon" | "swimrun" => SportType::Triathlon,

            // ── Gym ──
            "strength" | "strength_training" | "strength training"
            | "weight_training" | "weight training" | "weights" | "weight_lifting"
            | "gym" | "fitness" | "crossfit"
            | "functional_training" | "functional training"
            | "bodyweight" | "calisthenics" => SportType::Strength,
            "cardio" | "cardio_training" | "hiit" | "elliptical" | "elliptical_trainer"
            | "stair_stepper" | "stair_climbing" | "stairmaster" | "indoor_cardio"
            | "fitness_equipment" => SportType::Cardio,
            "yoga" | "flexibility_training" | "flexibility" | "pilates" | "stretching"
            | "breathwork" | "meditation" => SportType::Yoga,

            // ── Snow ──
            "alpine_skiing" | "alpine skiing" | "downhill_skiing" | "downhill skiing"
            | "skiing" | "ski" | "downhill" | "resort_skiing" | "backcountry_skiing" => {
                SportType::Ski
            }
            "cross_country_skiing" | "cross country skiing" | "ski_xc"
            | "nordic_skiing" | "classic_skiing" | "skate_skiing" | "xc_ski" => SportType::SkiXc,
            "snowboarding" | "snowboard" => SportType::Snowboard,

            // ── Ball / other sports ──
            "golf" => SportType::Golf,
            "tennis" | "table_tennis" | "pickleball" | "racquetball" | "squash"
            | "padel" | "badminton" => SportType::Tennis,
            "soccer" | "football" => SportType::Soccer,
            "basketball" => SportType::Basketball,

            _ => SportType::Other,
        }
    }

    /// Resolve from a FIT `sport` + `sub_sport` pair: a specific match on
    /// `sub_sport` (e.g. trail, treadmill, open_water) wins over the broad
    /// `sport`, otherwise fall back to `sport`.
    pub fn resolve(sport: Option<&str>, sub_sport: Option<&str>) -> Self {
        if let Some(sub) = sub_sport {
            let st = Self::from_str(sub);
            if st != SportType::Other {
                return st;
            }
        }
        match sport {
            Some(s) => Self::from_str(s),
            None => SportType::Other,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            SportType::Run => "run",
            SportType::TrailRun => "trail_run",
            SportType::Treadmill => "treadmill",
            SportType::Ride => "ride",
            SportType::MountainBike => "mountain_bike",
            SportType::Walk => "walk",
            SportType::Hike => "hike",
            SportType::Mountaineering => "mountaineering",
            SportType::Swim => "swim",
            SportType::OpenWater => "open_water",
            SportType::Sailing => "sailing",
            SportType::Paddle => "paddle",
            SportType::Fishing => "fishing",
            SportType::Triathlon => "triathlon",
            SportType::Strength => "strength",
            SportType::Cardio => "cardio",
            SportType::Yoga => "yoga",
            SportType::Ski => "ski",
            SportType::SkiXc => "ski_xc",
            SportType::Snowboard => "snowboard",
            SportType::Golf => "golf",
            SportType::Tennis => "tennis",
            SportType::Soccer => "soccer",
            SportType::Basketball => "basketball",
            SportType::Other => "other",
        }
    }

    /// Human-readable label (mirrors SPORT_LABELS on the frontend).
    pub fn label(&self) -> &'static str {
        match self {
            SportType::Run => "Run",
            SportType::TrailRun => "Trail Run",
            SportType::Treadmill => "Treadmill",
            SportType::Ride => "Ride",
            SportType::MountainBike => "Mountain Bike",
            SportType::Walk => "Walk",
            SportType::Hike => "Hike",
            SportType::Mountaineering => "Mountaineering",
            SportType::Swim => "Swim",
            SportType::OpenWater => "Open Water",
            SportType::Sailing => "Sailing",
            SportType::Paddle => "Paddling",
            SportType::Fishing => "Fishing",
            SportType::Triathlon => "Triathlon",
            SportType::Strength => "Strength",
            SportType::Cardio => "Cardio",
            SportType::Yoga => "Yoga",
            SportType::Ski => "Ski",
            SportType::SkiXc => "XC Ski",
            SportType::Snowboard => "Snowboard",
            SportType::Golf => "Golf",
            SportType::Tennis => "Racquet",
            SportType::Soccer => "Soccer",
            SportType::Basketball => "Basketball",
            SportType::Other => "Activity",
        }
    }
}

/// Generate a default activity title for files that carry no name (e.g. FIT):
/// "{time of day} {sport}", such as "Morning Strength" or "Evening Run".
/// `start_time` is the local wall-clock ISO string ("YYYY-MM-DDThh:mm:ss").
pub fn default_activity_title(sport: &SportType, start_time: &str) -> String {
    let hour: u32 = start_time
        .get(11..13)
        .and_then(|h| h.parse().ok())
        .unwrap_or(12);
    let part = match hour {
        5..=11 => "Morning",
        12..=16 => "Afternoon",
        17..=20 => "Evening",
        _ => "Night",
    };
    format!("{} {}", part, sport.label())
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Activity {
    pub id: String,
    pub start_time: String,
    pub timezone_offset: Option<i32>,
    pub sport_type: String,
    pub title: Option<String>,
    pub notes: Option<String>,
    pub distance_m: Option<f64>,
    pub duration_s: Option<f64>,
    pub elev_gain_m: Option<f64>,
    pub elev_loss_m: Option<f64>,
    pub avg_speed_mps: Option<f64>,
    pub max_speed_mps: Option<f64>,
    pub avg_hr: Option<f64>,
    pub max_hr: Option<f64>,
    pub avg_cadence: Option<f64>,
    pub calories: Option<f64>,
    pub avg_temperature_c: Option<f64>,
    pub max_temperature_c: Option<f64>,
    pub source_device: Option<String>,
    pub location_name: Option<String>,
    pub start_lat: Option<f64>,
    pub start_lon: Option<f64>,
    pub avg_power_w: Option<f64>,
    pub max_power_w: Option<f64>,
    pub normalized_power_w: Option<f64>,
    pub total_work_kj: Option<f64>,
    pub threshold_power_w: Option<f64>,
    pub training_stress_score: Option<f64>,
    pub intensity_factor: Option<f64>,
    pub training_effect_aerobic: Option<f64>,
    pub training_effect_anaerobic: Option<f64>,
    pub training_load_peak: Option<f64>,
    pub avg_vertical_oscillation_mm: Option<f64>,
    pub avg_stance_time_ms: Option<f64>,
    pub avg_stance_time_percent: Option<f64>,
    pub avg_step_length_mm: Option<f64>,
    pub total_strides: Option<i64>,
    pub min_hr: Option<f64>,
    pub moving_time_s: Option<f64>,
    pub sub_sport: Option<String>,
    pub avg_respiration_rate: Option<f64>,
    pub max_respiration_rate: Option<f64>,
    pub hrv_rmssd: Option<f64>,
    pub hrv_sdrr: Option<f64>,
    pub end_lat: Option<f64>,
    pub end_lon: Option<f64>,
    pub avg_left_torque_effectiveness: Option<f64>,
    pub avg_right_torque_effectiveness: Option<f64>,
    pub avg_left_pedal_smoothness: Option<f64>,
    pub avg_right_pedal_smoothness: Option<f64>,
    pub avg_left_right_balance: Option<f64>,
    // Cycling Dynamics (dual-sided pedals): platform center offset, power
    // phase angles (degrees, 0° = top dead center, clockwise), seated vs
    // standing split.
    pub avg_left_pco_mm: Option<f64>,
    pub avg_right_pco_mm: Option<f64>,
    pub avg_left_power_phase_start_deg: Option<f64>,
    pub avg_left_power_phase_end_deg: Option<f64>,
    pub avg_left_power_phase_peak_start_deg: Option<f64>,
    pub avg_left_power_phase_peak_end_deg: Option<f64>,
    pub avg_right_power_phase_start_deg: Option<f64>,
    pub avg_right_power_phase_end_deg: Option<f64>,
    pub avg_right_power_phase_peak_start_deg: Option<f64>,
    pub avg_right_power_phase_peak_end_deg: Option<f64>,
    pub avg_power_seated_w: Option<f64>,
    pub avg_power_standing_w: Option<f64>,
    pub max_power_seated_w: Option<f64>,
    pub max_power_standing_w: Option<f64>,
    pub avg_cadence_seated: Option<f64>,
    pub avg_cadence_standing: Option<f64>,
    pub max_cadence_seated: Option<f64>,
    pub max_cadence_standing: Option<f64>,
    pub time_standing_s: Option<f64>,
    pub stand_count: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
    /// Set on a leg of a merged multisport activity → its triathlon container.
    /// None for standalone activities and for containers themselves.
    pub parent_id: Option<String>,
}

impl Activity {
    /// An activity with the given id/start and every metric empty — the base
    /// for a merged triathlon container, whose headline fields the caller
    /// then fills from the aggregated legs.
    pub fn empty(id: &str, start_time: &str) -> Activity {
        Activity {
            id: id.to_string(),
            start_time: start_time.to_string(),
            ..Default::default()
        }
    }
}

/// Lightweight summary for library list view
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivitySummary {
    pub id: String,
    pub start_time: String,
    pub sport_type: String,
    pub title: Option<String>,
    pub distance_m: Option<f64>,
    pub duration_s: Option<f64>,
    pub elev_gain_m: Option<f64>,
    pub avg_speed_mps: Option<f64>,
    pub avg_hr: Option<f64>,
    pub location_name: Option<String>,
    pub tags: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sport_type_from_str_known() {
        assert_eq!(SportType::from_str("run"), SportType::Run);
        assert_eq!(SportType::from_str("running"), SportType::Run);
        assert_eq!(SportType::from_str("Running"), SportType::Run);
        assert_eq!(SportType::from_str("ride"), SportType::Ride);
        assert_eq!(SportType::from_str("cycling"), SportType::Ride);
        assert_eq!(SportType::from_str("biking"), SportType::Ride);
        assert_eq!(SportType::from_str("walk"), SportType::Walk);
        assert_eq!(SportType::from_str("walking"), SportType::Walk);
        assert_eq!(SportType::from_str("hike"), SportType::Hike);
        assert_eq!(SportType::from_str("hiking"), SportType::Hike);
        assert_eq!(SportType::from_str("swim"), SportType::Swim);
        assert_eq!(SportType::from_str("swimming"), SportType::Swim);
        assert_eq!(SportType::from_str("strength"), SportType::Strength);
        assert_eq!(SportType::from_str("strength_training"), SportType::Strength);
        assert_eq!(SportType::from_str("gym"), SportType::Strength);
    }

    #[test]
    fn sport_type_extended_garmin_variants() {
        assert_eq!(SportType::from_str("trail_running"), SportType::TrailRun);
        assert_eq!(SportType::from_str("treadmill"), SportType::Treadmill);
        assert_eq!(SportType::from_str("mountain_biking"), SportType::MountainBike);
        assert_eq!(SportType::from_str("open_water_swimming"), SportType::OpenWater);
        assert_eq!(SportType::from_str("yoga"), SportType::Yoga);
        assert_eq!(SportType::from_str("cardio_training"), SportType::Cardio);
        assert_eq!(SportType::from_str("alpine_skiing"), SportType::Ski);
        assert_eq!(SportType::from_str("cross_country_skiing"), SportType::SkiXc);
        assert_eq!(SportType::from_str("snowboarding"), SportType::Snowboard);
        assert_eq!(SportType::from_str("rowing"), SportType::Paddle);
        assert_eq!(SportType::from_str("golf"), SportType::Golf);
    }

    #[test]
    fn sport_type_unknown_falls_back_to_other() {
        assert_eq!(SportType::from_str(""), SportType::Other);
        assert_eq!(SportType::from_str("kabaddi"), SportType::Other);
    }

    #[test]
    fn sport_type_triathlon_and_multisport() {
        assert_eq!(SportType::from_str("triathlon"), SportType::Triathlon);
        assert_eq!(SportType::from_str("multisport"), SportType::Triathlon);
        assert_eq!(SportType::from_str("duathlon"), SportType::Triathlon);
        assert_eq!(SportType::from_str("swimrun"), SportType::Triathlon);
    }

    #[test]
    fn sport_type_resolve_prefers_specific_sub_sport() {
        // FIT often records sport=running, sub_sport=trail|treadmill.
        assert_eq!(SportType::resolve(Some("running"), Some("trail")), SportType::TrailRun);
        assert_eq!(SportType::resolve(Some("running"), Some("treadmill")), SportType::Treadmill);
        // Generic/unknown sub falls back to the main sport.
        assert_eq!(SportType::resolve(Some("running"), Some("generic")), SportType::Run);
        assert_eq!(SportType::resolve(Some("cycling"), None), SportType::Ride);
        assert_eq!(SportType::resolve(None, None), SportType::Other);
    }

    #[test]
    fn default_title_uses_time_of_day_and_sport() {
        assert_eq!(
            default_activity_title(&SportType::Strength, "2026-04-09T08:01:00"),
            "Morning Strength"
        );
        assert_eq!(
            default_activity_title(&SportType::Run, "2026-04-09T18:30:00"),
            "Evening Run"
        );
        assert_eq!(
            default_activity_title(&SportType::Ride, "2026-04-09T13:00:00"),
            "Afternoon Ride"
        );
        assert_eq!(
            default_activity_title(&SportType::Swim, "2026-04-09T23:00:00"),
            "Night Swim"
        );
        // Unknown sport reads as a generic "Activity".
        assert_eq!(
            default_activity_title(&SportType::Other, "2026-04-09T09:00:00"),
            "Morning Activity"
        );
    }

    #[test]
    fn sport_type_roundtrip() {
        for st in [
            SportType::Run, SportType::TrailRun, SportType::Treadmill, SportType::Ride,
            SportType::MountainBike, SportType::Walk, SportType::Hike, SportType::Mountaineering,
            SportType::Swim, SportType::OpenWater, SportType::Sailing, SportType::Paddle,
            SportType::Fishing, SportType::Triathlon, SportType::Strength, SportType::Cardio, SportType::Yoga,
            SportType::Ski, SportType::SkiXc, SportType::Snowboard, SportType::Golf,
            SportType::Tennis, SportType::Soccer, SportType::Basketball, SportType::Other,
        ] {
            assert_eq!(SportType::from_str(st.as_str()), st);
        }
    }
}

/// Fields that can be updated by the user
#[derive(Debug, Clone, Deserialize, Default)]
pub struct ActivityUpdate {
    pub title: Option<String>,
    pub notes: Option<String>,
    pub sport_type: Option<String>,
    pub location_name: Option<String>,
    pub start_lat: Option<f64>,
    pub start_lon: Option<f64>,
}

/// Filters for querying activities
#[derive(Debug, Clone, Deserialize, Default)]
pub struct ActivityFilters {
    /// Free-text search over title / notes / location name.
    pub search: Option<String>,
    /// Match ANY of these sports; None/empty = all sports.
    pub sport_types: Option<Vec<String>>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub distance_min: Option<f64>,
    pub distance_max: Option<f64>,
    pub duration_min: Option<f64>,
    pub duration_max: Option<f64>,
    pub elev_gain_min: Option<f64>,
    pub elev_gain_max: Option<f64>,
    pub tag_ids: Option<Vec<i64>>,
    /// Some(true) = only activities WITH a GPS track, Some(false) = only
    /// those without, None = both. "Has a track" means at least one
    /// trackpoint carries a latitude — see push_facet_conditions.
    pub has_gps: Option<bool>,
    pub sort_by: Option<String>,
    pub sort_dir: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

/// A record this activity holds within its sport (drives the header trophy
/// chips). `kind` is the metric ("distance" | "elevation" | "duration" |
/// "pace"); the frontend formats the value from the activity itself.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RecordBadge {
    pub kind: String,
    pub all_time: bool,
}

/// A single activity entry inside a calendar day (for dots + hover rows).
#[derive(Debug, Clone, Serialize)]
pub struct CalDayActivity {
    pub id: String,
    pub sport_type: String,
    pub title: Option<String>,
    pub distance_m: Option<f64>,
    pub duration_s: Option<f64>,
}

/// Daily summary for the calendar view. `activities` lists each workout that
/// day (ordered by start time); the aggregate fields are derived from it.
#[derive(Debug, Clone, Serialize)]
pub struct DaySummary {
    pub date: String, // "YYYY-MM-DD"
    pub activity_count: i64,
    pub total_distance_m: f64,
    pub total_duration_s: f64,
    pub sport_types: Vec<String>,
    pub activities: Vec<CalDayActivity>,
}

/// Per-device activity stats, used for device detection.
#[derive(Debug, Clone, Serialize)]
pub struct DeviceStats {
    pub device_name: String,
    pub activity_count: i64,
    pub last_activity: String,
}

/// An activity's start location, for the library map view.
#[derive(Debug, Clone, Serialize)]
pub struct ActivityLocation {
    pub id: String,
    pub start_time: String,
    pub sport_type: String,
    pub title: Option<String>,
    pub distance_m: Option<f64>,
    pub duration_s: Option<f64>,
    pub lat: f64,
    pub lon: f64,
}
