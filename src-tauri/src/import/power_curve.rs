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
pub(crate) const MAX_HOLD_S: usize = 10;

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
pub(crate) fn resample_1s(t: &[Option<f64>], power_w: &[Option<i32>]) -> Vec<f64> {
    let mut last: Option<(usize, f64)> = None;
    on_1s_grid(t, power_w)
        .iter()
        .enumerate()
        .map(|(i, slot)| match slot {
            Some(v) => {
                last = Some((i, *v));
                *v
            }
            None => match last {
                Some((j, v)) if i - j <= MAX_HOLD_S => v,
                _ => 0.0,
            },
        })
        .collect()
}

/// The 1 Hz series on timer time, for a metric that leaves a pause out
/// rather than counting it as 0 W: samples up to [`MAX_HOLD_S`] apart are
/// held across the gap, farther apart it is `None` from end to end — no
/// held seconds leak into the pause. The same line the power zones draw
/// (`recompute_power_zones`), so a filled TSS and the zones count one
/// timer.
pub(crate) fn resample_1s_gapped(t: &[Option<f64>], power_w: &[Option<i32>]) -> Vec<Option<f64>> {
    let slots = on_1s_grid(t, power_w);
    let mut series = slots.clone();
    let mut last: Option<(usize, f64)> = None;
    for (i, slot) in slots.iter().enumerate() {
        if let Some(v) = slot {
            if let Some((j, held)) = last {
                if i - j <= MAX_HOLD_S {
                    series[j + 1..i].fill(Some(held));
                }
            }
            last = Some((i, *v));
        }
    }
    series
}

/// The samples placed on a 1 Hz elapsed grid, `None` where a second has
/// none; empty when no sample has both a time and a power.
fn on_1s_grid(t: &[Option<f64>], power_w: &[Option<i32>]) -> Vec<Option<f64>> {
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
    slots
}

/// Rolling window of normalized power, seconds — Coggan's definition.
const NP_WINDOW_S: usize = 30;

/// Normalized power from the track, for a file whose device wrote none
/// (#224 — Zwift writes average power only): the 30 s rolling mean of the
/// 1 Hz series, raised to the fourth power, averaged, fourth root. On
/// timer time like the device's own: a pause's seconds are left out, not
/// counted as 0 W, while a real 0 W sample (coasting) stays in. Returns
/// the NP with the timer seconds it was worked out over — the duration a
/// TSS from it must use. `None` when there is less than one window of
/// samples or no positive power.
pub fn normalized_power(t: &[Option<f64>], power_w: &[Option<i32>]) -> Option<(f64, f64)> {
    let series: Vec<f64> = resample_1s_gapped(t, power_w).into_iter().flatten().collect();
    if series.len() < NP_WINDOW_S {
        return None;
    }
    let mut window_sum: f64 = series[..NP_WINDOW_S].iter().sum();
    let mut fourth_sum = (window_sum / NP_WINDOW_S as f64).powi(4);
    for i in NP_WINDOW_S..series.len() {
        window_sum += series[i] - series[i - NP_WINDOW_S];
        fourth_sum += (window_sum / NP_WINDOW_S as f64).powi(4);
    }
    let windows = (series.len() - NP_WINDOW_S + 1) as f64;
    let np = (fourth_sum / windows).powf(0.25);
    (np.is_finite() && np > 0.0).then_some((np, series.len() as f64))
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

    /// The textbook computation, window by window, to check the rolling one.
    fn np_by_the_book(series: &[f64]) -> f64 {
        let means: Vec<f64> = series.windows(NP_WINDOW_S).map(|w| w.iter().sum::<f64>() / w.len() as f64).collect();
        (means.iter().map(|m| m.powi(4)).sum::<f64>() / means.len() as f64).powf(0.25)
    }

    #[test]
    fn normalized_power_of_a_steady_ride_is_its_power() {
        let (np, _) = normalized_power(&t_range(600), &vec![Some(250); 600]).unwrap();
        assert!((np - 250.0).abs() < 1e-9);
    }

    /// Intervals weigh more than their average: NP sits above it, at the
    /// value of the window-by-window definition.
    #[test]
    fn normalized_power_weighs_intervals_by_the_book() {
        let series: Vec<f64> = (0..600).map(|i| if (i / 60) % 2 == 0 { 300.0 } else { 100.0 }).collect();
        let power: Vec<Option<i32>> = series.iter().map(|p| Some(*p as i32)).collect();
        let (np, _) = normalized_power(&t_range(600), &power).unwrap();
        assert!(np > 200.0, "NP {np} above the 200 W average");
        assert!((np - np_by_the_book(&series)).abs() < 1e-6);
    }

    /// A pause is off the timer and leaves NP alone; coasting at 0 W is on
    /// it and pulls NP down.
    #[test]
    fn normalized_power_skips_a_pause_but_counts_coasting() {
        let mut t = t_range(60);
        t.extend((0..60).map(|i| Some(1_000_000.0 + 660.0 + i as f64)));
        let (np, timer_s) = normalized_power(&t, &vec![Some(300); 120]).unwrap();
        assert!((np - 300.0).abs() < 1e-9, "the 600 s pause is not 0 W");
        assert_eq!(timer_s, 120.0, "two minutes on the timer, not twelve elapsed");

        let mut coasting = vec![Some(300); 60];
        coasting.extend(vec![Some(0); 60]);
        assert!(normalized_power(&t_range(120), &coasting).unwrap().0 < 300.0);
    }

    /// Smart recording's few missing seconds are held on the timer; a pause
    /// is off it end to end — while the curve's series still holds into the
    /// pause and zero-fills the rest, as it always has.
    #[test]
    fn the_timer_series_holds_short_gaps_and_drops_long_ones() {
        let at = |s: &[f64]| s.iter().map(|x| Some(1_000_000.0 + x)).collect::<Vec<_>>();
        let w = |n: usize| vec![Some(200); n];
        // Samples 10 s apart (0 → 10): held. 11 s apart (10 → 21): a pause.
        let t = at(&[0.0, 10.0, 21.0]);
        let gapped = resample_1s_gapped(&t, &w(3));
        assert_eq!(gapped.iter().flatten().count(), 12, "0..=10 held, 21 itself; 11..21 off the timer");
        assert!(gapped[11..21].iter().all(Option::is_none));

        let curve = resample_1s(&t, &w(3));
        assert_eq!(curve.len(), 22);
        assert_eq!(&curve[11..21], &[200.0; 10], "held ten seconds into the pause");
        assert_eq!(curve[21], 200.0);
        let far = at(&[0.0, 12.0]);
        assert_eq!(resample_1s(&far, &w(2))[11], 0.0, "and zero-filled past them");
    }

    #[test]
    fn normalized_power_needs_one_full_window_of_positive_power() {
        assert!(normalized_power(&t_range(29), &vec![Some(250); 29]).is_none());
        assert!(normalized_power(&t_range(30), &vec![Some(250); 30]).is_some());
        assert!(normalized_power(&t_range(60), &vec![Some(0); 60]).is_none());
        assert!(normalized_power(&t_range(60), &vec![None; 60]).is_none());
    }
}
