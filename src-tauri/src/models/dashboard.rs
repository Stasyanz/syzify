use std::collections::HashMap;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct DashboardData {
    pub total_activities: i64,
    pub total_distance_m: f64,
    pub total_duration_s: f64,
    pub total_elev_gain_m: f64,
    pub avg_hr: Option<f64>,
    /// Totals for the current calendar week (from Monday 00:00 local) —
    /// drives the "this week" stat tiles, independent of the selected period.
    pub week: WeekTotals,
    /// Daily volume for the last 7 days (by sport) — the dashboard's 7-bar chart.
    pub week_volume: Vec<VolumeBucket>,
    pub volume_buckets: Vec<VolumeBucket>,
    pub sport_distribution: Vec<SportEntry>,
    /// Sport split for the last 7 days — the 5 busiest sports with integer
    /// shares that sum to exactly 100. Drives the "By sport" donut.
    pub week_sport_distribution: Vec<SportShare>,
    /// All-time records grouped by sport — up to the 5 most-frequent sports.
    pub records_by_sport: Vec<SportRecords>,
}

/// One sport's slice of the last-7-days activity count, with a whole-percent
/// share (the shares across the returned sports sum to 100).
#[derive(Debug, Clone, Serialize)]
pub struct SportShare {
    pub sport_type: String,
    pub activities: i64,
    pub share_pct: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SportRecords {
    pub sport_type: String,
    pub activity_count: i64,
    pub records: Records,
    /// For running sports: best time on standard race distances
    /// (empty for other sports — the frontend then shows `records`).
    pub distance_pbs: Vec<DistancePb>,
}

/// Best time for a standard running distance (e.g. fastest marathon).
#[derive(Debug, Clone, Serialize)]
pub struct DistancePb {
    pub label: String,
    pub activity_id: String,
    pub title: Option<String>,
    pub date: String,
    pub duration_s: f64,
    pub distance_m: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct WeekTotals {
    pub activities: i64,
    pub distance_m: f64,
    pub duration_s: f64,
    pub elev_gain_m: f64,
    pub avg_hr: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct VolumeBucket {
    pub label: String,
    pub start_date: String,
    pub distance_m: f64,
    pub duration_s: f64,
    pub activities: i64,
    pub by_sport: HashMap<String, SportBucket>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SportBucket {
    pub distance_m: f64,
    pub duration_s: f64,
    pub activities: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SportEntry {
    pub sport_type: String,
    pub activities: i64,
    pub distance_m: f64,
    pub duration_s: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PersonalRecord {
    pub activity_id: String,
    pub title: Option<String>,
    pub date: String,
    pub value: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Records {
    pub longest_distance: Option<PersonalRecord>,
    pub longest_duration: Option<PersonalRecord>,
    pub highest_elevation: Option<PersonalRecord>,
    pub fastest_speed: Option<PersonalRecord>,
    /// Heaviest single set (kg) — for strength/gym sports with weight data.
    pub heaviest_set: Option<PersonalRecord>,
}
