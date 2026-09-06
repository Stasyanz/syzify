use serde::Serialize;

/// The dashboard's Recovery card (ADR 0002): the last computed index with
/// its age, the three components behind it, and a sparse history.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RecoveryCard {
    /// The local date the card was computed for (today).
    pub computed_for: String,
    /// The night the index belongs to (the morning's date), if any.
    pub date: Option<String>,
    /// Days between that night and today (0 = last night).
    pub age_days: Option<i64>,
    pub index: Option<i64>,
    /// "intervals_ok" (≥80) | "easy_day" (60–79) | "rest" (<60).
    pub band: Option<String>,
    pub advice: Option<String>,
    pub hr: Option<HrComponent>,
    pub stress: Option<StressComponent>,
    pub load: Option<LoadComponent>,
    /// "hr_above_baseline" when the night ran ≥8 bpm over the baseline.
    pub warning: Option<String>,
    /// Monitoring days stored in the last 90 days, whatever their night
    /// holds — 0 means nothing was imported for the window at all.
    pub days_recorded_90d: i64,
    /// VALID nights (≥120 samples, not awake) recorded in the last 90 days —
    /// what the baseline can draw on, not every night the watch was worn.
    pub nights_recorded_90d: i64,
    /// Nights still missing before a baseline can form (0 once it exists).
    pub nights_needed: i64,
    /// Index per valid night in the last 28 days, oldest first — sparse.
    pub history: Vec<RecoveryPoint>,
}

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

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RecoveryPoint {
    pub date: String,
    pub index: i64,
    /// The index's band, so the frontend never re-derives the edges.
    pub band: String,
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
