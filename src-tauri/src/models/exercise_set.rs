use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExerciseSet {
    pub id: Option<i64>,
    pub activity_id: String,
    pub set_number: i32,
    pub start_time: Option<String>,
    pub category: Option<String>,
    pub category_subtype: Option<String>,
    pub set_type: Option<String>,
    pub duration_s: Option<f64>,
    pub repetitions: Option<i32>,
    pub weight_kg: Option<f64>,
    pub wkt_step_index: Option<i32>,
}
