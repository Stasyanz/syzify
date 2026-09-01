use serde::{Deserialize, Serialize};

/// A user-saved route segment: an independent copy of a selected slice of an
/// activity's track. It owns its polyline (`segment_point`), so it survives
/// deletion of the source activity and can later be matched against any
/// activity's track. The source reference is metadata only.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    pub id: String,
    pub name: String,
    /// Inherited from the source activity — a run and a ride over the same
    /// road are different segments.
    pub sport: String,
    pub source_activity_id: Option<String>,
    /// Trackpoint indices of the first/last point actually copied (the GPS
    /// filter can slide them inward from the raw selection). Meaningful only
    /// while `source_activity_id` is set — deleting the activity nulls the
    /// id but leaves these as historical metadata.
    pub source_start_idx: Option<i64>,
    pub source_end_idx: Option<i64>,
    /// Polyline length recomputed from the copied GPS points (not the
    /// activity's stored cumulative distance).
    pub distance_m: f64,
    pub elev_delta_m: Option<f64>,
    pub avg_grade_pct: Option<f64>,
    pub start_lat: f64,
    pub start_lon: f64,
    pub end_lat: f64,
    pub end_lon: f64,
    pub min_lat: f64,
    pub max_lat: f64,
    pub min_lon: f64,
    pub max_lon: f64,
    pub created_at: String,
}

/// One vertex of a segment's polyline. `distance_m` is cumulative from the
/// segment start.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentPoint {
    pub lat: f64,
    pub lon: f64,
    pub altitude_m: Option<f64>,
    pub distance_m: f64,
}

/// Identity/provenance inputs for building a new segment — everything else
/// (geometry, stats) is computed from the trackpoints.
#[derive(Debug, Clone, Copy)]
pub struct NewSegmentMeta<'a> {
    pub name: &'a str,
    pub sport: &'a str,
    pub activity_id: &'a str,
    pub id: &'a str,
    pub created_at: &'a str,
}

/// A close-match hit for the pre-save duplicate warning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimilarSegment {
    pub id: String,
    pub name: String,
    pub distance_m: f64,
}

/// One row of the /segments page: the segment plus its effort aggregates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentSummaryRow {
    pub id: String,
    pub name: String,
    pub sport: String,
    pub distance_m: f64,
    pub avg_grade_pct: Option<f64>,
    pub elev_delta_m: Option<f64>,
    pub created_at: String,
    /// Timed efforts only — what the leaderboard ranks.
    pub effort_count: i64,
    pub best_elapsed_s: Option<f64>,
    /// Average power of the FASTEST timed effort — the same pass the "Best"
    /// time belongs to, so the two columns describe one ride. NULL when that
    /// pass carried no meter (or no timed efforts exist).
    pub best_effort_power_w: Option<f64>,
}

/// One leaderboard row of a segment: an effort with its activity context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentLeaderboardRow {
    pub id: i64,
    pub activity_id: String,
    pub activity_title: Option<String>,
    /// The activity's start time (local-offset TEXT, like everywhere else).
    pub start_time: String,
    pub distance_m: f64,
    pub elapsed_s: Option<f64>,
    /// Mean power over the pass (zeros included); NULL without a power meter.
    pub avg_power_w: Option<f64>,
    /// 1-based competition rank among timed efforts; NULL if untimed.
    pub rank: Option<i64>,
}

/// One segment pass inside an activity, as the activity page shows it.
/// Indices address the activity's full trackpoint arrays; speed/pace and
/// other per-effort stats are derived frontend-side from the track the page
/// already holds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentEffortRow {
    pub id: i64,
    pub segment_id: String,
    pub segment_name: String,
    pub start_idx: i64,
    pub end_idx: i64,
    /// Actual path length of this pass (may differ ±10% from the segment).
    pub distance_m: f64,
    /// Wall-clock seconds between entry and exit; NULL for timeless tracks.
    pub elapsed_s: Option<f64>,
    /// Mean power over the pass (zeros included); NULL without a power meter.
    pub avg_power_w: Option<f64>,
    /// The segment's average grade (context for the row).
    pub avg_grade_pct: Option<f64>,
    /// 1-based standing among the segment's timed efforts; NULL if untimed.
    pub rank: Option<i64>,
    /// How many timed efforts the segment has in total.
    pub effort_count: i64,
}
