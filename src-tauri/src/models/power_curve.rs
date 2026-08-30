use serde::{Deserialize, Serialize};

/// One mean-max point: the best average power held for `window_s` seconds.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PowerCurvePoint {
    pub window_s: i64,
    pub watts: f64,
}

/// One point of the library-wide envelope: the all-time best for a window,
/// attributed to the activity that set it (for the chart tooltip/link).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerCurveEnvelopePoint {
    pub window_s: i64,
    pub watts: f64,
    pub activity_id: String,
    pub title: Option<String>,
    pub start_time: String,
}

/// Everything the activity page's Power Curve panel needs in one fetch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerCurveData {
    pub points: Vec<PowerCurvePoint>,
    pub envelope: Vec<PowerCurveEnvelopePoint>,
}
