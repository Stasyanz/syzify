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
    VirtualRun,
    Ride,
    MountainBike,
    IndoorRide,
    VirtualRide,
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
            | "road_running" | "road running" => SportType::Run,

            "trail_running" | "trail running" | "trail_run" | "trail" => SportType::TrailRun,

            "treadmill_running" | "treadmill running" | "treadmill" => SportType::Treadmill,

            // A run on a simulator (Garmin sport running + sub_sport
            // virtual_activity, Strava VirtualRun): a treadmill with the
            // coordinates of a virtual world (#192).
            "virtual_run" | "virtual run" | "virtualrun" | "virtual_running" => SportType::VirtualRun,

            // ── Cycling ──
            "ride" | "cycling" | "biking" | "bicycle" | "bike"
            | "road_cycling" | "road cycling" | "road"
            | "gravel_cycling" | "gravel cycling" | "gravel" | "cyclocross"
            | "e_bike_ride" | "e-bike" | "ebike" => SportType::Ride,

            // A trainer with no course (Garmin sub_sport indoor_cycling, a
            // spin class) and a smart trainer on a simulator (Garmin
            // sub_sport virtual_activity, Strava VirtualRide) are rides of
            // their own kind: no map, a speed that is the trainer's word.
            "indoor_ride" | "indoor_cycling" | "indoor cycling" | "indoor_bike"
            | "spin" | "spinning" | "stationary_bike" | "stationary bike" => SportType::IndoorRide,
            "virtual_ride" | "virtual ride" | "virtualride" | "virtual_cycling" => SportType::VirtualRide,

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
        let broad = sport.map(Self::from_str).unwrap_or(SportType::Other);
        if let Some(sub) = sub_sport {
            let st = Self::from_str(sub);
            if st != SportType::Other {
                return st;
            }
            // Garmin's `virtual_activity` (sub_sport 58) is shared by every
            // sport done on a simulator, so on its own it names nothing;
            // under a ride it names the smart trainer (#189), under a run
            // the treadmill on a simulator (#192).
            if is_virtual_activity(sub) {
                match broad {
                    SportType::Ride => return SportType::VirtualRide,
                    SportType::Run => return SportType::VirtualRun,
                    _ => {}
                }
            }
        }
        broad
    }

    /// The sports whose streams compare with each other: a trainer ride
    /// and a road ride, a treadmill run and a trail run. Empty for a sport
    /// that stands alone. Shared by the power-curve envelope and the import
    /// dedup, so a ride re-imported from a file that names its trainer
    /// still meets the copy that came in as a plain ride.
    pub fn family(sport: &str) -> &'static [&'static str] {
        match sport {
            "ride" | "mountain_bike" | "indoor_ride" | "virtual_ride" => {
                &["ride", "mountain_bike", "indoor_ride", "virtual_ride"]
            }
            "run" | "trail_run" | "treadmill" | "virtual_run" => {
                &["run", "trail_run", "treadmill", "virtual_run"]
            }
            _ => &[],
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            SportType::Run => "run",
            SportType::TrailRun => "trail_run",
            SportType::Treadmill => "treadmill",
            SportType::VirtualRun => "virtual_run",
            SportType::Ride => "ride",
            SportType::MountainBike => "mountain_bike",
            SportType::IndoorRide => "indoor_ride",
            SportType::VirtualRide => "virtual_ride",
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
            SportType::VirtualRun => "Virtual Run",
            SportType::Ride => "Ride",
            SportType::MountainBike => "Mountain Bike",
            SportType::IndoorRide => "Indoor Ride",
            SportType::VirtualRide => "Virtual Ride",
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

/// A sport whose coordinates come from a simulator, not the ground (#190,
/// #192): a virtual ride or run carries the lat/lon of its virtual world
/// (Watopia sits in the Solomon Islands). Such points draw no map, name no
/// place and match no segment. Mirrors `hasSimulatedCourse` in
/// src/lib/types.ts.
pub fn has_simulated_course(sport: &str) -> bool {
    SIMULATED_COURSE_SPORTS.contains(&sport)
}

/// The sports behind [`has_simulated_course`], for SQL filters that take
/// them as parameters — one list, so a kind added here reaches every
/// query at once.
pub const SIMULATED_COURSE_SPORTS: &[&str] = &["virtual_ride", "virtual_run"];

fn is_virtual_activity(sub_sport: &str) -> bool {
    matches!(sub_sport.to_lowercase().as_str(), "virtual_activity" | "virtual activity")
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
    /// The recording device as the file named it (see `describeDevice` in
    /// the frontend for the display form).
    pub source_device: Option<String>,
    /// The gear item the activity was done on (ADR 0003), if assigned.
    pub gear_id: Option<String>,
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

    /// A ride off the road is its own sport (#189): the Garmin sub_sport
    /// names the trainer kind, Strava's export its type.
    #[test]
    fn indoor_and_virtual_rides_are_sports_of_their_own() {
        assert_eq!(SportType::resolve(Some("cycling"), Some("indoor_cycling")), SportType::IndoorRide);
        assert_eq!(SportType::resolve(Some("cycling"), Some("virtual_activity")), SportType::VirtualRide);
        assert_eq!(SportType::resolve(Some("cycling"), Some("road")), SportType::Ride);
        assert_eq!(SportType::resolve(Some("cycling"), Some("generic")), SportType::Ride);
        // A trainer that records as gym equipment still names the kind.
        assert_eq!(SportType::resolve(Some("fitness_equipment"), Some("indoor_cycling")), SportType::IndoorRide);
        assert_eq!(SportType::from_str("Virtual Ride"), SportType::VirtualRide);
        assert_eq!(SportType::from_str("Indoor Cycling"), SportType::IndoorRide);
        assert_eq!(SportType::from_str("spin"), SportType::IndoorRide);
        assert_eq!(SportType::from_str("e-bike"), SportType::Ride);
        for sport in [SportType::IndoorRide, SportType::VirtualRide] {
            assert_eq!(SportType::from_str(sport.as_str()), sport, "{:?} round-trips", sport);
        }
        assert_eq!(SportType::IndoorRide.label(), "Indoor Ride");
        assert_eq!(SportType::VirtualRide.label(), "Virtual Ride");
    }

    /// Garmin's `virtual_activity` is one sub_sport for every simulator: it
    /// names a kind only under a ride (#189) or a run (#192); any other
    /// parent sport stays as it is.
    #[test]
    fn virtual_activity_names_a_kind_only_under_a_ride_or_a_run() {
        assert_eq!(SportType::from_str("virtual_activity"), SportType::Other);
        assert_eq!(SportType::resolve(None, Some("virtual_activity")), SportType::Other);
        for (sport, expected) in [
            ("cycling", SportType::VirtualRide),
            ("road_cycling", SportType::VirtualRide),
            ("e_bike_ride", SportType::VirtualRide),
            ("running", SportType::VirtualRun),
            ("trail_running", SportType::TrailRun),
            ("rowing", SportType::Paddle),
            ("walking", SportType::Walk),
            ("fitness_equipment", SportType::Cardio),
            ("mountain_biking", SportType::MountainBike),
            ("kabaddi", SportType::Other),
        ] {
            assert_eq!(SportType::resolve(Some(sport), Some("virtual_activity")), expected, "{sport}");
            assert_eq!(SportType::resolve(Some(sport), Some("Virtual Activity")), expected, "{sport}");
        }
        // The explicit names need no parent sport.
        assert_eq!(SportType::resolve(Some("running"), Some("virtual_ride")), SportType::VirtualRide);
        assert_eq!(SportType::resolve(Some("running"), Some("indoor_cycling")), SportType::IndoorRide);
        assert_eq!(SportType::resolve(Some("cycling"), Some("virtual_run")), SportType::VirtualRun);
    }

    /// A run on a simulator is a sport of its own (#192): Garmin's
    /// sub_sport under running, Strava's VirtualRun type.
    #[test]
    fn a_virtual_run_is_a_sport_of_its_own() {
        assert_eq!(SportType::from_str("VirtualRun"), SportType::VirtualRun);
        assert_eq!(SportType::from_str("Virtual Run"), SportType::VirtualRun);
        assert_eq!(SportType::from_str("virtual_run"), SportType::VirtualRun);
        assert_eq!(SportType::resolve(Some("running"), Some("generic")), SportType::Run);
        assert_eq!(SportType::resolve(Some("running"), Some("treadmill")), SportType::Treadmill);
        assert_eq!(SportType::VirtualRun.label(), "Virtual Run");
        assert_eq!(SportType::family("virtual_run"), SportType::family("run"));
        assert!(has_simulated_course("virtual_run"));
    }

    #[test]
    fn only_the_virtual_sports_have_a_simulated_course() {
        assert!(has_simulated_course("virtual_ride"));
        assert!(has_simulated_course("virtual_run"));
        for sport in ["ride", "indoor_ride", "treadmill", "run", "other", ""] {
            assert!(!has_simulated_course(sport), "{sport}");
        }
        for sport in SIMULATED_COURSE_SPORTS {
            assert!(has_simulated_course(sport), "{sport}: the SQL list and the predicate agree");
            assert_eq!(SportType::from_str(sport).as_str(), *sport, "{sport} is a known slug");
        }
    }

    #[test]
    fn sport_family_groups_wheels_and_feet() {
        assert_eq!(SportType::family("virtual_ride"), &["ride", "mountain_bike", "indoor_ride", "virtual_ride"]);
        assert_eq!(SportType::family("ride"), SportType::family("indoor_ride"));
        assert_eq!(SportType::family("treadmill"), &["run", "trail_run", "treadmill", "virtual_run"]);
        assert!(SportType::family("swim").is_empty());
        assert!(SportType::family("").is_empty());
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
            SportType::Run, SportType::TrailRun, SportType::Treadmill, SportType::VirtualRun, SportType::Ride,
            SportType::MountainBike, SportType::IndoorRide, SportType::VirtualRide, SportType::Walk, SportType::Hike, SportType::Mountaineering,
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
    /// Match ANY of these recording devices, as stored in `source_device`
    /// (raw strings, e.g. "Garmin fenix6x"); an empty string means
    /// "no device". None/empty = all devices.
    pub devices: Option<Vec<String>>,
    /// Match ANY of these gear items by id; an empty string means "no
    /// gear". None/empty = all.
    pub gear_ids: Option<Vec<String>>,
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
    pub total_elev_gain_m: f64,
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
