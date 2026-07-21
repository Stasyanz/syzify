use serde::{Deserialize, Serialize};

/// One leg of a multisport activity (triathlon/duathlon/swimrun): the swim,
/// the ride, the run — and the transitions between them. Stored ONLY for
/// files that carry more than one session; single-sport activities have no
/// rows here. Metrics are the leg's own, unlike the parent activity's
/// aggregated totals.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultisportLeg {
    pub id: Option<i64>,
    pub activity_id: String,
    /// 1-based position within the race, file order.
    pub leg_number: i32,
    /// Normalized sport ("swim", "ride", "run"); "transition" for T1/T2.
    pub sport_type: String,
    pub is_transition: bool,
    pub start_time: Option<String>,
    pub total_distance_m: Option<f64>,
    pub total_timer_time_s: Option<f64>,
    pub total_elapsed_time_s: Option<f64>,
    pub avg_speed_mps: Option<f64>,
    pub avg_hr: Option<f64>,
    pub max_hr: Option<f64>,
    pub total_ascent_m: Option<f64>,
    pub total_calories: Option<f64>,
    /// The standalone activity this leg was merged from — makes the leg row
    /// a link to its own detail page. None for FIT-multisport legs (no
    /// separate activity exists) and for transitions.
    pub source_activity_id: Option<String>,
}
