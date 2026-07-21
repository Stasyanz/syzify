use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HrvSample {
    pub id: Option<i64>,
    pub activity_id: String,
    pub sample_index: i32,
    pub rr_interval_ms: f64,
}
