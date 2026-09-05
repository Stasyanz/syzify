pub mod fit;
pub mod gpx;
pub mod monitoring;
pub mod tcx;

use crate::models::exercise_set::ExerciseSet;
use crate::models::hrv_sample::HrvSample;
use crate::models::lap::Lap;
use crate::models::multisport_leg::MultisportLeg;
use crate::models::swim_length::SwimLength;
use crate::models::time_in_zone::TimeInZone;
use crate::models::trackpoint::TrackPoint;

/// Result of parsing a workout file into a normalized structure.
#[derive(Debug, Clone)]
pub struct ParsedActivity {
    pub start_time: Option<String>,
    pub sport_type: Option<String>,
    pub title: Option<String>,
    pub source_device: Option<String>,
    pub trackpoints: Vec<TrackPoint>,
    /// Pre-computed session metrics from the file (e.g. FIT session message).
    /// These take priority over computed metrics when available.
    pub session_metrics: Option<SessionMetrics>,
    pub laps: Vec<Lap>,
    pub lengths: Vec<SwimLength>,
    pub sets: Vec<ExerciseSet>,
    pub time_in_zones: Vec<TimeInZone>,
    pub hrv_samples: Vec<HrvSample>,
    /// Per-leg breakdown for multisport files (empty for single-sport).
    pub legs: Vec<MultisportLeg>,
}

#[derive(Debug, Clone, Default)]
pub struct SessionMetrics {
    pub total_distance_m: Option<f64>,
    pub total_elapsed_time_s: Option<f64>,
    pub total_timer_time_s: Option<f64>,
    pub total_ascent_m: Option<f64>,
    pub total_descent_m: Option<f64>,
    pub avg_speed_mps: Option<f64>,
    pub max_speed_mps: Option<f64>,
    pub avg_hr: Option<f64>,
    pub max_hr: Option<f64>,
    pub avg_cadence: Option<f64>,
    pub max_cadence: Option<f64>,
    pub total_calories: Option<f64>,
    pub avg_temperature_c: Option<f64>,
    pub max_temperature_c: Option<f64>,
    pub avg_power_w: Option<f64>,
    pub max_power_w: Option<f64>,
    pub normalized_power_w: Option<f64>,
    pub total_work_kj: Option<f64>,
    pub threshold_power_w: Option<f64>,
    pub training_stress_score: Option<f64>,
    pub intensity_factor: Option<f64>,
    pub training_effect_aerobic: Option<f64>,
    pub training_effect_anaerobic: Option<f64>,
    pub training_load_peak: Option<f64>,
    pub avg_vertical_oscillation_mm: Option<f64>,
    pub avg_stance_time_ms: Option<f64>,
    pub avg_stance_time_percent: Option<f64>,
    pub avg_step_length_mm: Option<f64>,
    pub total_strides: Option<i64>,
    pub min_hr: Option<f64>,
    pub moving_time_s: Option<f64>,
    pub sub_sport: Option<String>,
    pub avg_respiration_rate: Option<f64>,
    pub max_respiration_rate: Option<f64>,
    pub hrv_rmssd: Option<f64>,
    pub hrv_sdrr: Option<f64>,
    pub end_lat: Option<f64>,
    pub end_lon: Option<f64>,
    pub avg_left_torque_effectiveness: Option<f64>,
    pub avg_right_torque_effectiveness: Option<f64>,
    pub avg_left_pedal_smoothness: Option<f64>,
    pub avg_right_pedal_smoothness: Option<f64>,
    /// Power balance as the RIGHT-pedal percentage (FIT flag bit decoded).
    pub avg_left_right_balance: Option<f64>,
    // Cycling Dynamics — see the matching Activity fields.
    pub avg_left_pco_mm: Option<f64>,
    pub avg_right_pco_mm: Option<f64>,
    pub avg_left_power_phase_start_deg: Option<f64>,
    pub avg_left_power_phase_end_deg: Option<f64>,
    pub avg_left_power_phase_peak_start_deg: Option<f64>,
    pub avg_left_power_phase_peak_end_deg: Option<f64>,
    pub avg_right_power_phase_start_deg: Option<f64>,
    pub avg_right_power_phase_end_deg: Option<f64>,
    pub avg_right_power_phase_peak_start_deg: Option<f64>,
    pub avg_right_power_phase_peak_end_deg: Option<f64>,
    pub avg_power_seated_w: Option<f64>,
    pub avg_power_standing_w: Option<f64>,
    pub max_power_seated_w: Option<f64>,
    pub max_power_standing_w: Option<f64>,
    pub avg_cadence_seated: Option<f64>,
    pub avg_cadence_standing: Option<f64>,
    pub max_cadence_seated: Option<f64>,
    pub max_cadence_standing: Option<f64>,
    pub time_standing_s: Option<f64>,
    pub stand_count: Option<i64>,
}

/// Sum of the Some values; None when every input is None (so a metric no
/// session reported stays absent instead of becoming a fake 0).
fn opt_sum(vals: impl Iterator<Item = Option<f64>>) -> Option<f64> {
    vals.flatten().fold(None, |acc, v| Some(acc.unwrap_or(0.0) + v))
}

fn opt_max(vals: impl Iterator<Item = Option<f64>>) -> Option<f64> {
    vals.flatten().fold(None, |acc: Option<f64>, v| Some(acc.map_or(v, |a| a.max(v))))
}

fn opt_min(vals: impl Iterator<Item = Option<f64>>) -> Option<f64> {
    vals.flatten().fold(None, |acc: Option<f64>, v| Some(acc.map_or(v, |a| a.min(v))))
}

impl SessionMetrics {
    /// Collapse a file's sessions into one activity's metrics. Single-session
    /// files (the overwhelming majority) pass through untouched; multisport
    /// files (one session per triathlon leg) genuinely combine:
    ///   - totals (distance, times, ascent, calories, work, strides) add up;
    ///   - max/min fields take the extreme across legs;
    ///   - averages are weighted by each leg's timer time — a 40 km ride must
    ///     outweigh a 1.5 km swim in the whole-activity avg HR;
    ///   - training load is race-wide: TSS and load sum across legs
    ///     (HR-based devices report them PER session — taking the swim's
    ///     would undercount the race several-fold), training effect takes
    ///     the peak of its 0–5 scale;
    ///   - leg-specific extras (running dynamics, power model, HRV) keep the
    ///     first reporting leg's value — only one leg ever reports them;
    ///   - the end position is the LAST leg's (where the workout ended);
    ///   - sub_sport is dropped: no single leg's flavor describes the combo.
    pub fn aggregate(sessions: Vec<SessionMetrics>) -> SessionMetrics {
        if sessions.len() == 1 {
            return sessions.into_iter().next().expect("len checked");
        }

        let weight = |s: &SessionMetrics| {
            s.total_timer_time_s.or(s.total_elapsed_time_s).unwrap_or(1.0)
        };
        let wavg = |get: fn(&SessionMetrics) -> Option<f64>| {
            let mut sum = 0.0;
            let mut wsum = 0.0;
            for s in &sessions {
                if let Some(v) = get(s) {
                    let w = weight(s);
                    sum += v * w;
                    wsum += w;
                }
            }
            if wsum > 0.0 { Some(sum / wsum) } else { None }
        };
        let first = |get: fn(&SessionMetrics) -> Option<f64>| {
            sessions.iter().find_map(get)
        };

        SessionMetrics {
            total_distance_m: opt_sum(sessions.iter().map(|s| s.total_distance_m)),
            total_elapsed_time_s: opt_sum(sessions.iter().map(|s| s.total_elapsed_time_s)),
            total_timer_time_s: opt_sum(sessions.iter().map(|s| s.total_timer_time_s)),
            total_ascent_m: opt_sum(sessions.iter().map(|s| s.total_ascent_m)),
            total_descent_m: opt_sum(sessions.iter().map(|s| s.total_descent_m)),
            total_calories: opt_sum(sessions.iter().map(|s| s.total_calories)),
            total_work_kj: opt_sum(sessions.iter().map(|s| s.total_work_kj)),
            moving_time_s: opt_sum(sessions.iter().map(|s| s.moving_time_s)),
            total_strides: {
                let strides: Vec<i64> = sessions.iter().filter_map(|s| s.total_strides).collect();
                if strides.is_empty() { None } else { Some(strides.iter().sum()) }
            },

            max_speed_mps: opt_max(sessions.iter().map(|s| s.max_speed_mps)),
            max_hr: opt_max(sessions.iter().map(|s| s.max_hr)),
            max_cadence: opt_max(sessions.iter().map(|s| s.max_cadence)),
            max_power_w: opt_max(sessions.iter().map(|s| s.max_power_w)),
            max_temperature_c: opt_max(sessions.iter().map(|s| s.max_temperature_c)),
            max_respiration_rate: opt_max(sessions.iter().map(|s| s.max_respiration_rate)),
            min_hr: opt_min(sessions.iter().map(|s| s.min_hr)),

            avg_speed_mps: wavg(|s| s.avg_speed_mps),
            avg_hr: wavg(|s| s.avg_hr),
            avg_cadence: wavg(|s| s.avg_cadence),
            avg_power_w: wavg(|s| s.avg_power_w),
            avg_temperature_c: wavg(|s| s.avg_temperature_c),
            avg_respiration_rate: wavg(|s| s.avg_respiration_rate),

            training_stress_score: opt_sum(sessions.iter().map(|s| s.training_stress_score)),
            training_load_peak: opt_sum(sessions.iter().map(|s| s.training_load_peak)),
            training_effect_aerobic: opt_max(sessions.iter().map(|s| s.training_effect_aerobic)),
            training_effect_anaerobic: opt_max(sessions.iter().map(|s| s.training_effect_anaerobic)),

            // Non-additive power-model numbers: first reporting leg.
            normalized_power_w: first(|s| s.normalized_power_w),
            threshold_power_w: first(|s| s.threshold_power_w),
            intensity_factor: first(|s| s.intensity_factor),
            avg_vertical_oscillation_mm: first(|s| s.avg_vertical_oscillation_mm),
            avg_stance_time_ms: first(|s| s.avg_stance_time_ms),
            avg_stance_time_percent: first(|s| s.avg_stance_time_percent),
            avg_step_length_mm: first(|s| s.avg_step_length_mm),
            hrv_rmssd: first(|s| s.hrv_rmssd),
            hrv_sdrr: first(|s| s.hrv_sdrr),
            avg_left_torque_effectiveness: first(|s| s.avg_left_torque_effectiveness),
            avg_right_torque_effectiveness: first(|s| s.avg_right_torque_effectiveness),
            avg_left_pedal_smoothness: first(|s| s.avg_left_pedal_smoothness),
            avg_right_pedal_smoothness: first(|s| s.avg_right_pedal_smoothness),
            avg_left_right_balance: first(|s| s.avg_left_right_balance),

            // Cycling Dynamics: only the ride leg ever reports them — the
            // averages pass through first(); the standing totals add (same
            // result for one reporting leg, correct if several ever do).
            avg_left_pco_mm: first(|s| s.avg_left_pco_mm),
            avg_right_pco_mm: first(|s| s.avg_right_pco_mm),
            avg_left_power_phase_start_deg: first(|s| s.avg_left_power_phase_start_deg),
            avg_left_power_phase_end_deg: first(|s| s.avg_left_power_phase_end_deg),
            avg_left_power_phase_peak_start_deg: first(|s| s.avg_left_power_phase_peak_start_deg),
            avg_left_power_phase_peak_end_deg: first(|s| s.avg_left_power_phase_peak_end_deg),
            avg_right_power_phase_start_deg: first(|s| s.avg_right_power_phase_start_deg),
            avg_right_power_phase_end_deg: first(|s| s.avg_right_power_phase_end_deg),
            avg_right_power_phase_peak_start_deg: first(|s| s.avg_right_power_phase_peak_start_deg),
            avg_right_power_phase_peak_end_deg: first(|s| s.avg_right_power_phase_peak_end_deg),
            avg_power_seated_w: first(|s| s.avg_power_seated_w),
            avg_power_standing_w: first(|s| s.avg_power_standing_w),
            max_power_seated_w: opt_max(sessions.iter().map(|s| s.max_power_seated_w)),
            max_power_standing_w: opt_max(sessions.iter().map(|s| s.max_power_standing_w)),
            avg_cadence_seated: first(|s| s.avg_cadence_seated),
            avg_cadence_standing: first(|s| s.avg_cadence_standing),
            max_cadence_seated: opt_max(sessions.iter().map(|s| s.max_cadence_seated)),
            max_cadence_standing: opt_max(sessions.iter().map(|s| s.max_cadence_standing)),
            time_standing_s: opt_sum(sessions.iter().map(|s| s.time_standing_s)),
            stand_count: {
                let counts: Vec<i64> = sessions.iter().filter_map(|s| s.stand_count).collect();
                if counts.is_empty() { None } else { Some(counts.iter().sum()) }
            },

            end_lat: sessions.iter().rev().find_map(|s| s.end_lat),
            end_lon: sessions.iter().rev().find_map(|s| s.end_lon),
            sub_sport: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leg(
        sport_time_s: f64,
        distance_m: f64,
        avg_hr: Option<f64>,
        max_hr: Option<f64>,
    ) -> SessionMetrics {
        SessionMetrics {
            total_timer_time_s: Some(sport_time_s),
            total_elapsed_time_s: Some(sport_time_s + 60.0),
            total_distance_m: Some(distance_m),
            avg_hr,
            max_hr,
            ..Default::default()
        }
    }

    /// A single session (the overwhelming majority of files) passes through
    /// byte-identical — aggregation must not perturb it.
    #[test]
    fn aggregate_single_session_is_identity() {
        let mut s = leg(1800.0, 5000.0, Some(150.0), Some(175.0));
        s.sub_sport = Some("trail".to_string());
        s.avg_step_length_mm = Some(1100.0);
        let out = SessionMetrics::aggregate(vec![s.clone()]);
        assert_eq!(out.total_distance_m, s.total_distance_m);
        assert_eq!(out.sub_sport, s.sub_sport);
        assert_eq!(out.avg_step_length_mm, s.avg_step_length_mm);
    }

    /// The triathlon case: totals add, extremes take the max, averages weight
    /// by leg duration (a 4000 s ride outweighs an 1800 s swim), and metrics
    /// only one leg reports (running dynamics) survive un-diluted.
    #[test]
    fn aggregate_combines_multisport_legs() {
        let swim = leg(1800.0, 1500.0, Some(140.0), Some(160.0));
        let mut ride = leg(4000.0, 40000.0, Some(150.0), Some(172.0));
        ride.end_lat = Some(52.5);
        let mut run = leg(2400.0, 10000.0, Some(165.0), Some(185.0));
        run.avg_step_length_mm = Some(1150.0);
        run.end_lat = Some(52.6);

        let out = SessionMetrics::aggregate(vec![swim, ride, run]);

        assert_eq!(out.total_distance_m, Some(51500.0));
        assert_eq!(out.total_timer_time_s, Some(8200.0));
        assert_eq!(out.max_hr, Some(185.0));
        // (140*1800 + 150*4000 + 165*2400) / 8200 ≈ 152.2
        let avg = out.avg_hr.unwrap();
        assert!((avg - 152.195).abs() < 0.01, "got {avg}");
        // Run-only dynamics survive; the combo has no single sub_sport; the
        // end position is the LAST leg's.
        assert_eq!(out.avg_step_length_mm, Some(1150.0));
        assert_eq!(out.sub_sport, None);
        assert_eq!(out.end_lat, Some(52.6));
    }

    /// Training load is race-wide, not the first leg's: HR-based devices
    /// write TSS/TE into EVERY session, and taking the swim's (always first)
    /// undercounted the race several-fold. TSS sums; effect takes the peak.
    #[test]
    fn aggregate_sums_training_load_across_legs() {
        let mut swim = leg(1800.0, 1500.0, None, None);
        swim.training_stress_score = Some(30.0);
        swim.training_effect_aerobic = Some(2.1);
        let mut ride = leg(4000.0, 40000.0, None, None);
        ride.training_stress_score = Some(120.0);
        ride.training_effect_aerobic = Some(3.9);
        let mut run = leg(2400.0, 10000.0, None, None);
        run.training_stress_score = Some(80.0);
        run.training_effect_aerobic = Some(4.4);

        let out = SessionMetrics::aggregate(vec![swim, ride, run]);
        assert_eq!(out.training_stress_score, Some(230.0));
        assert_eq!(out.training_effect_aerobic, Some(4.4));
    }

    /// Metrics nobody reported stay None — never a fabricated zero.
    #[test]
    fn aggregate_keeps_unreported_metrics_absent() {
        let out = SessionMetrics::aggregate(vec![
            SessionMetrics::default(),
            SessionMetrics::default(),
        ]);
        assert_eq!(out.total_distance_m, None);
        assert_eq!(out.avg_hr, None);
        assert_eq!(out.max_hr, None);
        assert_eq!(out.total_strides, None);
    }
}
