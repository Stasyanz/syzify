use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwimLength {
    pub id: Option<i64>,
    pub activity_id: String,
    pub length_number: i32,
    pub start_time: Option<String>,
    pub total_elapsed_time_s: Option<f64>,
    pub total_timer_time_s: Option<f64>,
    pub avg_speed_mps: Option<f64>,
    pub avg_swimming_cadence: Option<f64>,
    pub swim_stroke: Option<String>,
    pub total_strokes: Option<i32>,
    pub total_calories: Option<f64>,
    pub length_type: Option<String>,
}
