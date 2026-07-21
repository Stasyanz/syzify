use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lap {
    pub id: Option<i64>,
    pub activity_id: String,
    pub lap_number: i32,
    pub start_time: Option<String>,
    pub total_elapsed_time_s: Option<f64>,
    pub total_timer_time_s: Option<f64>,
    pub total_distance_m: Option<f64>,
    pub avg_speed_mps: Option<f64>,
    pub max_speed_mps: Option<f64>,
    pub avg_hr: Option<f64>,
    pub max_hr: Option<f64>,
    pub avg_cadence: Option<f64>,
    pub max_cadence: Option<f64>,
    pub total_ascent_m: Option<f64>,
    pub total_descent_m: Option<f64>,
    pub total_calories: Option<f64>,
    pub avg_power_w: Option<f64>,
    pub max_power_w: Option<f64>,
    pub normalized_power_w: Option<f64>,
    pub avg_vertical_oscillation_mm: Option<f64>,
    pub avg_stance_time_ms: Option<f64>,
    pub avg_step_length_mm: Option<f64>,
}
