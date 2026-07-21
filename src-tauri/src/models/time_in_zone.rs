use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeInZone {
    pub id: Option<i64>,
    pub activity_id: String,
    pub zone_type: String,
    pub zone_index: i32,
    pub time_s: f64,
    pub zone_high_boundary: Option<f64>,
}
