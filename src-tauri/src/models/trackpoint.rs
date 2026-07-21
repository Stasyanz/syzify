use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackPoint {
    pub activity_id: String,
    pub t: Option<String>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub altitude_m: Option<f64>,
    pub speed_mps: Option<f64>,
    pub hr: Option<i32>,
    pub cadence: Option<i32>,
    pub power_w: Option<i32>,
    pub temperature_c: Option<f64>,
    pub vertical_oscillation_mm: Option<f64>,
    pub stance_time_ms: Option<f64>,
    pub stance_time_percent: Option<f64>,
    pub step_length_mm: Option<f64>,
    pub grade_percent: Option<f64>,
    pub left_right_balance: Option<f64>,
    pub left_torque_effectiveness: Option<f64>,
    pub right_torque_effectiveness: Option<f64>,
    pub left_pedal_smoothness: Option<f64>,
    pub right_pedal_smoothness: Option<f64>,
}

/// Columnar format for efficient transfer to frontend.
/// Each Vec has the same length — one entry per trackpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackPointColumns {
    pub t: Vec<Option<f64>>,       // seconds since activity start
    pub lat: Vec<Option<f64>>,
    pub lon: Vec<Option<f64>>,
    pub altitude_m: Vec<Option<f64>>,
    pub speed_mps: Vec<Option<f64>>,
    pub hr: Vec<Option<i32>>,
    pub cadence: Vec<Option<i32>>,
    pub power_w: Vec<Option<i32>>,
    pub temperature_c: Vec<Option<f64>>,
    pub vertical_oscillation_mm: Vec<Option<f64>>,
    pub stance_time_ms: Vec<Option<f64>>,
    pub stance_time_percent: Vec<Option<f64>>,
    pub step_length_mm: Vec<Option<f64>>,
    pub grade_percent: Vec<Option<f64>>,
    pub distance_m: Vec<Option<f64>>, // cumulative distance
    pub left_right_balance: Vec<Option<f64>>,
    pub left_torque_effectiveness: Vec<Option<f64>>,
    pub right_torque_effectiveness: Vec<Option<f64>>,
    pub left_pedal_smoothness: Vec<Option<f64>>,
    pub right_pedal_smoothness: Vec<Option<f64>>,
}
