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

/// What Settings → Vault shows above the delete-range control.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MonitoringSummary {
    /// Stored days (one row per local day).
    pub days: i64,
    pub first_date: Option<String>,
    pub last_date: Option<String>,
    /// Monitor raw files kept in the vault.
    pub files: i64,
}

/// Outcome of `delete_monitoring_range`.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MonitoringDeleted {
    pub days: i64,
    /// Raw files actually removed from the vault (their hashes freed for
    /// re-import) — counted per successful removal, not per row found.
    pub files: i64,
    /// Files the OS refused to remove; their rows (and hashes) stay and the
    /// next delete of any range tries them again. `error` carries the first
    /// reason.
    pub failed: i64,
    pub error: Option<String>,
}
