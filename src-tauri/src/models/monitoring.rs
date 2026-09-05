use serde::{Deserialize, Serialize};

/// One local calendar day of Garmin monitoring, aggregated from the stored
/// samples (ADR 0002). Night = 00:00–07:00 local; day = 07:00–24:00.
/// `computed_at` is None for a day whose samples are stored but whose
/// aggregates have not been (re)computed yet.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MonitoringDay {
    /// Local date, YYYY-MM-DD.
    pub date: String,
    /// The UTC offset the day's files were read under, seconds east.
    pub tz_offset_s: i32,
    /// A file cut at local midnight confirmed that offset.
    pub tz_confirmed: bool,
    pub night_samples: i64,
    pub night_hr_min: Option<f64>,
    pub night_hr_p10: Option<f64>,
    pub night_hr_median: Option<f64>,
    pub night_stress_avg: Option<f64>,
    pub day_stress_avg: Option<f64>,
    pub resp_night_avg: Option<f64>,
    pub spo2_night_avg: Option<f64>,
    /// Garmin's resting HR for the day — reference only.
    pub rhr_garmin: Option<i64>,
    pub rhr_garmin_7d: Option<i64>,
    pub steps: Option<f64>,
    pub distance_m: Option<f64>,
    pub active_calories: Option<f64>,
    pub active_time_s: Option<f64>,
    pub moderate_min: Option<f64>,
    pub vigorous_min: Option<f64>,
    pub computed_at: Option<String>,
}
