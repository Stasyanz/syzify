//! Best-effort splits: the fastest time to cover a standard race distance
//! within a run, found with a sliding window over (cumulative distance, time).
//! This is how "best 10k" stays correct even inside a longer run.

/// Standard race distances we compute best-effort splits for (metres).
pub const BEST_EFFORT_DISTANCES: [(&str, f64); 4] = [
    ("5 km", 5000.0),
    ("10 km", 10000.0),
    ("Half marathon", 21097.0),
    ("Marathon", 42195.0),
];

/// Fastest plausible sustained running speed (m/s, ~2:30 / km — just above world
/// records). A window implying a faster pace comes from a GPS glitch where the
/// cumulative distance jumps between two points, so it is discarded instead of
/// being stored as a superhuman "record".
const MAX_RUNNING_SPEED_MPS: f64 = 6.7;

/// How far the track's own distance may exceed the activity's recorded distance
/// before the whole track is treated as spike-corrupted. GPS teleports inflate
/// the cumulative distance, so a track that runs well past the device-recorded
/// distance can't be trusted for splits at all (e.g. a "5 km" pulled out of a
/// stretch that is mostly a position glitch).
const MAX_TRACK_OVERCOUNT: f64 = 1.15;

/// Fastest time (s) for each standard distance the track actually covers.
/// `distance_m` is cumulative distance, `t` is elapsed seconds (same length,
/// both monotonic non-decreasing). `recorded_distance_m` is the activity's own
/// distance (from the source summary) when known, used to reject tracks whose
/// distance is inflated by GPS spikes. Returns (distance_m, duration_s) pairs.
pub fn compute_best_efforts(
    distance_m: &[Option<f64>],
    t: &[Option<f64>],
    recorded_distance_m: Option<f64>,
) -> Vec<(f64, f64)> {
    let n = distance_m.len().min(t.len());
    let mut pts: Vec<(f64, f64)> = Vec::with_capacity(n);
    for i in 0..n {
        if let (Some(d), Some(tt)) = (distance_m[i], t[i]) {
            pts.push((d, tt));
        }
    }
    if pts.len() < 2 {
        return Vec::new();
    }
    let total = pts.last().map(|p| p.0).unwrap_or(0.0);

    // A track that overshoots the recorded distance has GPS spikes fabricating
    // distance; no split from it is trustworthy, so produce none.
    if let Some(recorded) = recorded_distance_m {
        if recorded > 0.0 && total > recorded * MAX_TRACK_OVERCOUNT {
            return Vec::new();
        }
    }

    let mut out = Vec::new();
    for (_, target) in BEST_EFFORT_DISTANCES {
        if total < target {
            continue;
        }
        if let Some(secs) = fastest_window(&pts, target) {
            out.push((target, secs));
        }
    }
    out
}

/// Minimum elapsed time of any window covering at least `target` metres.
fn fastest_window(pts: &[(f64, f64)], target: f64) -> Option<f64> {
    let n = pts.len();
    let mut i = 0usize;
    let mut best = f64::INFINITY;
    for j in 0..n {
        // Tighten the left edge while the window still covers the distance.
        while i < j && pts[j].0 - pts[i + 1].0 >= target {
            i += 1;
        }
        if pts[j].0 - pts[i].0 >= target {
            let secs = pts[j].1 - pts[i].1;
            // Reject glitch windows that cover the distance impossibly fast.
            if secs > 0.0 && secs < best && secs * MAX_RUNNING_SPEED_MPS >= target {
                best = secs;
            }
        }
    }
    if best.is_finite() {
        Some(best)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get(be: &[(f64, f64)], d: f64) -> Option<f64> {
        be.iter().find(|(dd, _)| (*dd - d).abs() < 0.1).map(|(_, s)| *s)
    }

    #[test]
    fn best_efforts_uses_fastest_split_not_total() {
        // 15 km at a steady 5:00/km (300 s/km).
        let mut dist = Vec::new();
        let mut t = Vec::new();
        for k in 0..=15 {
            dist.push(Some(k as f64 * 1000.0));
            t.push(Some(k as f64 * 300.0));
        }
        let be = compute_best_efforts(&dist, &t, None);
        assert_eq!(get(&be, 5000.0), Some(1500.0));
        // The 10k record is the 10k split (3000 s), NOT the 15 km total (4500 s).
        assert_eq!(get(&be, 10000.0), Some(3000.0));
        // The run is only 15 km — no half/marathon.
        assert_eq!(get(&be, 21097.0), None);
        assert_eq!(get(&be, 42195.0), None);
    }

    #[test]
    fn best_efforts_finds_the_fast_stretch() {
        // 10 km total; the second 5 km is faster than the first.
        let dist = vec![Some(0.0), Some(5000.0), Some(10000.0)];
        let t = vec![Some(0.0), Some(1600.0), Some(2800.0)]; // 1600 s then 1200 s
        let be = compute_best_efforts(&dist, &t, None);
        assert_eq!(get(&be, 5000.0), Some(1200.0)); // the faster 5 km
        assert_eq!(get(&be, 10000.0), Some(2800.0));
    }

    #[test]
    fn best_efforts_skips_impossible_gps_jumps() {
        // A GPS glitch: cumulative distance jumps 5 km between two points only
        // 20 s apart — a ~250 m/s "sprint". No window over 5 km is plausible.
        let dist = vec![Some(0.0), Some(5000.0), Some(5200.0)];
        let t = vec![Some(0.0), Some(20.0), Some(40.0)];
        let be = compute_best_efforts(&dist, &t, None);
        assert_eq!(get(&be, 5000.0), None, "superhuman 5 km split discarded");
    }

    #[test]
    fn best_efforts_rejects_spike_inflated_track() {
        // The track says it covered 6 km, but the activity's recorded distance is
        // only 5 km — the extra 1 km is GPS spikes, so no split is trustworthy.
        let dist = vec![Some(0.0), Some(2500.0), Some(6000.0)];
        let t = vec![Some(0.0), Some(900.0), Some(1800.0)];
        let be = compute_best_efforts(&dist, &t, Some(5000.0));
        assert!(be.is_empty(), "spike-inflated track yields no splits");
        // With a matching recorded distance the same track is trusted; the only
        // window covering 5 km spans the whole 6 km in 1800 s.
        let ok = compute_best_efforts(&dist, &t, Some(6000.0));
        assert_eq!(get(&ok, 5000.0), Some(1800.0));
    }

    #[test]
    fn best_efforts_empty_without_gps() {
        assert!(compute_best_efforts(&[], &[], None).is_empty());
        assert!(compute_best_efforts(&[None, None], &[None, None], None).is_empty());
    }
}
