use serde::Serialize;

/// Components of a night's recovery index (ADR 0002).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct HrComponent {
    pub night_median: f64,
    pub baseline: f64,
    pub delta: f64,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct StressComponent {
    pub night_avg: f64,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LoadComponent {
    /// hrTSS of the day before the night.
    pub tss_yesterday: f64,
    /// Chronic training load (42-day EWMA of daily hrTSS) as of that day.
    pub ctl: f64,
    pub score: f64,
}

/// One night's index for the calendar (ADR 0002, #97): the same numbers
/// the card shows, for every indexed night — the frontend cuts a month.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RecoveryNight {
    /// The morning's date, "YYYY-MM-DD".
    pub date: String,
    pub index: i64,
    pub band: String,
    pub hr: HrComponent,
    pub stress: Option<StressComponent>,
    pub load: Option<LoadComponent>,
    /// "hr_above_baseline" when the night ran ≥8 bpm over the baseline.
    pub warning: Option<String>,
}
