//! Mean-max ("power curve") computation, run once at import.
//!
//! The curve is defined over ELAPSED time. Short recording gaps (smart
//! recording emits a sample only when values change, roughly every 1–10 s)
//! carry the last sample forward — zero-filling them would divide real power
//! by the recording interval and wreck every window. Gaps longer than
//! [`MAX_HOLD_S`] are auto-pause / dropped recording and count as 0 W, so a
//! pause can only dilute an average, never inflate it — the same basis head
//! units use for their mean-max pages.
//!
//! Windows follow a fixed log-spaced grid instead of every second in
//! 1..=3600: a full sweep is O(n·w) ≈ tens of millions of multiply-adds for a
//! two-hour ride, while ~two dozen log-spaced points draw the same curve.

use crate::models::power_curve::PowerCurvePoint;

/// Log-spaced mean-max windows, seconds. Includes the conventional anchors
/// (5 s / 1 min / 5 min / 20 min / 1 h) that riders actually compare.
pub const WINDOWS_S: &[i64] = &[
    1, 2, 3, 5, 8, 10, 15, 20, 30, 45, 60, 90, 120, 180, 240, 300, 420, 600, 900, 1200, 1800,
    2400, 3600,
];

/// Longest recording gap a sample is held across. Smart recording stays well
/// under this; anything longer reads as a pause, not a slow recorder.
const MAX_HOLD_S: usize = 10;

/// Timestamps farther than this from the median are corrupt (a single glitch
/// point days away must not stretch the grid and zero out long windows).
const MAX_MEDIAN_OFFSET_S: f64 = 24.0 * 3600.0;

/// Best average power for each grid window that fits the recording, from the
/// columnar track (`t` = epoch seconds, as loaded by
/// `get_trackpoints_columnar`). Empty when the track carries no positive
/// power — the caller then stores nothing and the UI shows no panel.
pub fn compute_power_curve(
    t: &[Option<f64>],
    power_w: &[Option<i32>],
) -> Vec<PowerCurvePoint> {
    let series = resample_1s(t, power_w);
    if series.is_empty() || !series.iter().any(|&p| p > 0.0) {
        return Vec::new();
    }

    // Prefix sums: window mean = (S[i+w] - S[i]) / w, O(1) per position.
    let mut prefix = Vec::with_capacity(series.len() + 1);
    prefix.push(0.0f64);
    for &p in &series {
        prefix.push(prefix.last().copied().unwrap_or(0.0) + p);
    }

    let dur = series.len();
    let mut curve = Vec::new();
    for &w in WINDOWS_S {
        let w_usize = w as usize;
        if w_usize > dur {
            break; // windows are sorted; nothing longer fits either
        }
        let mut best = f64::NEG_INFINITY;
        for i in 0..=(dur - w_usize) {
            let sum = prefix[i + w_usize] - prefix[i];
            if sum > best {
                best = sum;
            }
        }
        curve.push(PowerCurvePoint {
            window_s: w,
            watts: best / w as f64,
        });
    }
    curve
}

/// Place samples on a 1 Hz elapsed grid, hold each value across gaps up to
/// [`MAX_HOLD_S`], leave longer gaps at 0 W (see module doc). Robustness:
/// timestamps are NOT assumed ordered (rows come back in insert order), a
/// duplicate second keeps the later row, and timestamps more than a day from
/// the median are discarded as corrupt instead of stretching the grid.
fn resample_1s(t: &[Option<f64>], power_w: &[Option<i32>]) -> Vec<f64> {
    // Samples that can land on a time grid: timestamp + power value. An
    // explicit 0 W is a real sample (coasting) and must reset the hold.
    let mut samples: Vec<(f64, f64)> = t
        .iter()
        .zip(power_w.iter())
        .filter_map(|(ts, p)| match (ts, p) {
            (Some(ts), Some(p)) if *p >= 0 => Some((*ts, *p as f64)),
            _ => None,
        })
        .collect();
    if samples.is_empty() {
        return Vec::new();
    }

    // Median-anchored outlier filter (median of an odd/even count both fine —
    // we only need a point inside the real ride).
    let mut ts_sorted: Vec<f64> = samples.iter().map(|s| s.0).collect();
    ts_sorted.sort_by(|a, b| a.total_cmp(b));
    let median = ts_sorted[ts_sorted.len() / 2];
    samples.retain(|(ts, _)| (ts - median).abs() <= MAX_MEDIAN_OFFSET_S);
    if samples.is_empty() {
        return Vec::new();
    }

    let t0 = samples.iter().map(|s| s.0).fold(f64::INFINITY, f64::min);
    let t_last = samples.iter().map(|s| s.0).fold(f64::NEG_INFINITY, f64::max);
    let dur = (t_last - t0) as usize + 1;

    // Later rows win a duplicate second — matches "last write" everywhere else.
    let mut slots: Vec<Option<f64>> = vec![None; dur];
    for (ts, p) in &samples {
        let idx = (ts - t0) as usize;
        if idx < dur {
            slots[idx] = Some(*p);
        }
    }

    let mut series = vec![0.0f64; dur];
    let mut last: Option<(usize, f64)> = None;
    for (i, slot) in slots.iter().enumerate() {
        match slot {
            Some(v) => {
                series[i] = *v;
                last = Some((i, *v));
            }
            None => {
                if let Some((j, v)) = last {
                    if i - j <= MAX_HOLD_S {
                        series[i] = v;
                    }
                }
            }
        }
    }
    series
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t_range(n: usize) -> Vec<Option<f64>> {
        (0..n).map(|i| Some(1_000_000.0 + i as f64)).collect()
    }

    fn watts_at(curve: &[PowerCurvePoint], w: i64) -> f64 {
        curve.iter().find(|p| p.window_s == w).unwrap().watts
    }

    #[test]
    fn constant_power_yields_flat_curve() {
        let n = 120;
        let curve = compute_power_curve(&t_range(n), &vec![Some(200); n]);
        // Windows up to 120 s fit; every mean of a constant stream is 200.
        assert_eq!(curve.last().unwrap().window_s, 120);
        for p in &curve {
            assert!((p.watts - 200.0).abs() < 1e-9, "window {}", p.window_s);
        }
    }

    #[test]
    fn smart_recording_interval_does_not_dilute_the_curve() {
        // One sample every 4 s (Garmin smart recording), constant 300 W for
        // 10 minutes: every window must still read 300, not 300/4.
        let t: Vec<Option<f64>> = (0..150).map(|i| Some(1_000_000.0 + (i * 4) as f64)).collect();
        let power = vec![Some(300); 150];
        let curve = compute_power_curve(&t, &power);
        for w in [1, 5, 60, 300] {
            assert!(
                (watts_at(&curve, w) - 300.0).abs() < 1e-9,
                "window {} diluted: {}",
                w,
                watts_at(&curve, w)
            );
        }
    }

    #[test]
    fn gaps_beyond_the_hold_read_as_pause_zeros() {
        // 30 s at 300 W, a 60 s silence (auto-pause), 30 s at 300 W. The
        // hold covers 10 s of the silence; the rest must count as 0 so the
        // 120 s window can't claim 300 W was held throughout.
        let mut t = Vec::new();
        let mut power = Vec::new();
        for i in 0..30 {
            t.push(Some(1_000_000.0 + i as f64));
            power.push(Some(300));
        }
        for i in 90..120 {
            t.push(Some(1_000_000.0 + i as f64));
            power.push(Some(300));
        }
        let curve = compute_power_curve(&t, &power);
        assert!((watts_at(&curve, 30) - 300.0).abs() < 1e-9);
        // 120 s of grid: 30+30 sampled + 10 held + 50 zero ⇒ 70/120 · 300.
        assert!((watts_at(&curve, 120) - 300.0 * 70.0 / 120.0).abs() < 1e-9);
    }

    #[test]
    fn explicit_zero_watt_sample_resets_the_hold() {
        // 300 W, then a 0 W sample (coasting starts), then silence: the hold
        // must carry the 0, not resurrect the 300.
        let t: Vec<Option<f64>> = (0..3).map(|i| Some(1_000_000.0 + (i * 5) as f64)).collect();
        let power = vec![Some(300), Some(0), Some(0)];
        let curve = compute_power_curve(&t, &power);
        // Grid: 5×300 (sample+hold) then 0s ⇒ best 10 s = 1500/10.
        assert!((watts_at(&curve, 10) - 150.0).abs() < 1e-9);
    }

    #[test]
    fn peak_window_finds_the_burst() {
        // 60 s at 100 W with a 10-second 400 W burst in the middle.
        let n = 60;
        let power: Vec<Option<i32>> = (0..n)
            .map(|i| Some(if (20..30).contains(&i) { 400 } else { 100 }))
            .collect();
        let curve = compute_power_curve(&t_range(n), &power);
        assert!((watts_at(&curve, 10) - 400.0).abs() < 1e-9);
        // The full minute dilutes the burst: (50·100 + 10·400) / 60 = 150.
        assert!((watts_at(&curve, 60) - 150.0).abs() < 1e-9);
    }

    #[test]
    fn out_of_order_and_duplicate_timestamps_are_tolerated() {
        // Rows in insert order but shuffled in time, one second twice —
        // the later row wins and the span comes from min/max, not first/last.
        let t = vec![
            Some(1_000_010.0),
            Some(1_000_000.0),
            Some(1_000_005.0),
            Some(1_000_005.0),
        ];
        let power = vec![Some(100), Some(100), Some(999), Some(100)];
        let curve = compute_power_curve(&t, &power);
        assert_eq!(curve.last().unwrap().window_s, 10); // 11 s span, 10 fits
        assert!((watts_at(&curve, 1) - 100.0).abs() < 1e-9); // dup: later row won
    }

    #[test]
    fn corrupt_faraway_timestamp_is_dropped_not_grid_stretching() {
        // A healthy 60 s ride plus one glitch point three days later: the
        // glitch must be discarded — not zero-pad three days of grid (which
        // would let hour-long windows exist at absurdly low watts).
        let mut t = t_range(60);
        let mut power = vec![Some(250); 60];
        t.push(Some(1_000_000.0 + 3.0 * 24.0 * 3600.0));
        power.push(Some(999));
        let curve = compute_power_curve(&t, &power);
        assert_eq!(curve.last().unwrap().window_s, 60);
        assert!((watts_at(&curve, 60) - 250.0).abs() < 1e-9);
    }

    #[test]
    fn no_power_means_no_curve() {
        assert!(compute_power_curve(&t_range(60), &vec![None; 60]).is_empty());
        assert!(compute_power_curve(&t_range(60), &vec![Some(0); 60]).is_empty());
        assert!(compute_power_curve(&[], &[]).is_empty());
    }

    #[test]
    fn untimestamped_samples_are_skipped_not_crashed() {
        let mut t = t_range(30);
        t[5] = None;
        let curve = compute_power_curve(&t, &vec![Some(100); 30]);
        assert!(!curve.is_empty());
    }
}
