//! Detect passes ("efforts") of a saved segment inside an activity track.
//!
//! Pure geometry: entry near the segment start, forward scan to an exit near
//! the end with a matching path length, then a corridor check that the track
//! actually followed the segment's polyline (rejects a parallel road between
//! the same endpoints). Directional by construction — a reversed pass never
//! finds the start first. Repeats after an exit are separate efforts.

use crate::db::trackpoints::haversine_m;

/// A track point counts as "at" a segment endpoint within this radius.
pub const ENDPOINT_RADIUS_M: f64 = 50.0;
/// A sampled segment point must lie this close to the track to be covered.
pub const CORRIDOR_RADIUS_M: f64 = 40.0;
/// Fraction of sampled segment points that must be covered.
pub const CORRIDOR_MIN_COVERAGE: f64 = 0.8;
/// Path length between entry and exit must match the segment within this.
pub const LENGTH_TOLERANCE: f64 = 0.10;
/// Give up scanning for the exit after this multiple of the segment length.
const MAX_SCAN_FACTOR: f64 = 1.5;
/// Corridor sampling spacing along the segment polyline.
const SAMPLE_SPACING_M: f64 = 50.0;
/// Base half-width of the along-path window a sampled segment point is
/// looked up in; grows by LENGTH_TOLERANCE·distance to absorb pace drift.
const CORRIDOR_WINDOW_BASE_M: f64 = 100.0;
/// Hard cap on the corridor window — past this, drift tolerance would start
/// matching a sample against a DIFFERENT lap of a repeating course.
const CORRIDOR_WINDOW_MAX_M: f64 = 300.0;
/// Meters per degree of latitude on the shared 6 371 km sphere (see
/// db/segments.rs — the boxes and haversine must agree on one Earth model).
const M_PER_DEG_LAT: f64 = 111_194.93;

/// One detected pass. Indices address the FULL activity track (the same
/// index space the frontend and `segment.source_*_idx` use).
#[derive(Debug, Clone, PartialEq)]
pub struct EffortMatch {
    pub start_idx: usize,
    pub end_idx: usize,
    /// Actual haversine path length along the track between the endpoints.
    pub distance_m: f64,
}

/// GPS-bearing track point with its original index and cumulative distance.
struct GpsPt {
    idx: usize,
    lat: f64,
    lon: f64,
    cum: f64,
}

/// Find every pass of the segment (polyline `seg_lat/seg_lon/seg_cum`, all
/// dense and same length, `seg_cum` cumulative from 0) in the activity track
/// (columnar, holes allowed). No bbox prefilter: the entry scan is already
/// one cheap haversine per point, single-digit ms on the largest tracks.
pub fn find_efforts(
    seg_lat: &[f64],
    seg_lon: &[f64],
    seg_cum: &[f64],
    lat: &[Option<f64>],
    lon: &[Option<f64>],
) -> Vec<EffortMatch> {
    let seg_n = seg_lat.len().min(seg_lon.len()).min(seg_cum.len());
    if seg_n < 2 {
        return Vec::new();
    }
    let seg_len = seg_cum[seg_n - 1];
    if seg_len <= 0.0 {
        return Vec::new();
    }
    let (start_lat, start_lon) = (seg_lat[0], seg_lon[0]);
    let (end_lat, end_lon) = (seg_lat[seg_n - 1], seg_lon[seg_n - 1]);

    // Track → GPS-bearing points with cumulative haversine distance.
    let mut pts: Vec<GpsPt> = Vec::new();
    let mut cum = 0.0;
    for i in 0..lat.len().min(lon.len()) {
        let (Some(la), Some(lo)) = (lat[i], lon[i]) else {
            continue;
        };
        if let Some(prev) = pts.last() {
            cum += haversine_m(prev.lat, prev.lon, la, lo);
        }
        pts.push(GpsPt { idx: i, lat: la, lon: lo, cum });
    }
    if pts.len() < 2 {
        return Vec::new();
    }

    // Corridor samples: segment indices ~SAMPLE_SPACING_M apart, ends always in.
    let mut samples: Vec<usize> = vec![0];
    let mut next_at = SAMPLE_SPACING_M;
    for (j, &c) in seg_cum.iter().enumerate().take(seg_n) {
        if c >= next_at {
            samples.push(j);
            next_at = c + SAMPLE_SPACING_M;
        }
    }
    if *samples.last().unwrap_or(&0) != seg_n - 1 {
        samples.push(seg_n - 1);
    }

    let mut out = Vec::new();
    let mut i = 0;
    while i < pts.len() {
        if haversine_m(pts[i].lat, pts[i].lon, start_lat, start_lon) > ENDPOINT_RADIUS_M {
            i += 1;
            continue;
        }
        // Entry = the closest point of this contiguous visit to the start
        // circle (a slow rider produces many in-radius points).
        let mut entry = i;
        let mut entry_d = haversine_m(pts[i].lat, pts[i].lon, start_lat, start_lon);
        while i < pts.len() {
            let d = haversine_m(pts[i].lat, pts[i].lon, start_lat, start_lon);
            if d > ENDPOINT_RADIUS_M {
                break;
            }
            if d < entry_d {
                entry = i;
                entry_d = d;
            }
            i += 1;
        }
        // i is now the first point past the start circle; if no effort is
        // confirmed the outer loop resumes from here (no rescan, no loops).

        // Exit = among ALL finish-circle candidates in the scan window, the
        // one whose path length agrees best with the segment. Taking the
        // whole window (not the first visit) keeps a graze-then-return past
        // the finish from recording a short, mis-timed effort; choosing by
        // length error (not point proximity) keeps GPS jitter at the finish
        // from swinging elapsed times. The extra `local_step` slack lets
        // sparse recordings (smart recording, simplified GPX) reach the
        // tolerance band their sampling would otherwise quantize past.
        let mut exit: Option<usize> = None;
        let mut exit_err = f64::INFINITY;
        let mut k = entry + 1;
        while k < pts.len() {
            let path = pts[k].cum - pts[entry].cum;
            if path > seg_len * MAX_SCAN_FACTOR {
                break;
            }
            if haversine_m(pts[k].lat, pts[k].lon, end_lat, end_lon) <= ENDPOINT_RADIUS_M {
                let local_step = pts[k].cum - pts[k - 1].cum;
                let err = (path - seg_len).abs();
                if err <= LENGTH_TOLERANCE * seg_len + local_step && err < exit_err {
                    exit = Some(k);
                    exit_err = err;
                }
            }
            k += 1;
        }

        if let Some(x) = exit {
            if corridor_covered(seg_lat, seg_lon, seg_cum, &samples, &pts[entry..=x]) {
                out.push(EffortMatch {
                    start_idx: pts[entry].idx,
                    end_idx: pts[x].idx,
                    distance_m: pts[x].cum - pts[entry].cum,
                });
                // Next pass can only begin after this one ended.
                i = x + 1;
            }
        }
    }
    out
}

/// Distance in meters from point `p` to the track edge `a→b`, via a local
/// planar projection around `p` (fine at corridor scales). Longitude deltas
/// wrap ±180° so an antimeridian-straddling edge doesn't explode.
fn point_edge_dist_m(
    (plat, plon): (f64, f64),
    (alat, alon): (f64, f64),
    (blat, blon): (f64, f64),
) -> f64 {
    let wrap = |d: f64| (d + 540.0).rem_euclid(360.0) - 180.0;
    let kx = M_PER_DEG_LAT * plat.to_radians().cos();
    let ax = wrap(alon - plon) * kx;
    let ay = (alat - plat) * M_PER_DEG_LAT;
    let bx = wrap(blon - plon) * kx;
    let by = (blat - plat) * M_PER_DEG_LAT;
    let (dx, dy) = (bx - ax, by - ay);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 {
        (-(ax * dx + ay * dy) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let (cx, cy) = (ax + t * dx, ay + t * dy);
    (cx * cx + cy * cy).sqrt()
}

/// For each sampled segment point (at distance `c` along the segment), look
/// only at subtrack EDGES whose along-path distance falls inside a window
/// around `c` — both curves progress along the same road, so the candidate
/// set is small and the scan stays linear overall. A distance window is
/// robust where a greedy nearest-point walk is not: real GPS noise makes
/// point-to-point distances non-monotonic and stalls a greedy pointer.
/// Measuring to edges (not vertices) makes coverage independent of the
/// track's recording density — a 200 m straight stored as two points still
/// covers every sample on it.
fn corridor_covered(
    seg_lat: &[f64],
    seg_lon: &[f64],
    seg_cum: &[f64],
    samples: &[usize],
    sub: &[GpsPt],
) -> bool {
    let base = sub[0].cum;
    let mut lo = 0usize;
    let mut covered = 0usize;
    for &s in samples {
        let c = seg_cum[s];
        let w = (CORRIDOR_WINDOW_BASE_M + LENGTH_TOLERANCE * c).min(CORRIDOR_WINDOW_MAX_M);
        // The window's lower edge only moves forward (c - w grows with c).
        while lo < sub.len() && sub[lo].cum - base < c - w {
            lo += 1;
        }
        let p = (seg_lat[s], seg_lon[s]);
        let mut q = lo;
        let mut hit = false;
        while q < sub.len() && sub[q].cum - base <= c + w {
            let d = if q + 1 < sub.len() {
                point_edge_dist_m(
                    p,
                    (sub[q].lat, sub[q].lon),
                    (sub[q + 1].lat, sub[q + 1].lon),
                )
            } else {
                haversine_m(sub[q].lat, sub[q].lon, p.0, p.1)
            };
            if d <= CORRIDOR_RADIUS_M {
                hit = true;
                break;
            }
            q += 1;
        }
        if hit {
            covered += 1;
        }
    }
    // ceil(0.8·n) equals n for n ≤ 4, which would demand 100% coverage on
    // short segments — always grant at least one miss (the entry sample sits
    // up to ENDPOINT_RADIUS_M away by construction).
    let allowed = ((samples.len() as f64) * (1.0 - CORRIDOR_MIN_COVERAGE))
        .floor()
        .max(1.0) as usize;
    covered + allowed >= samples.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ~11.1 m per step of 0.0001° latitude — a dense straight track north.
    const STEP_DEG: f64 = 0.0001;

    /// A straight track of `n` points going north along `lon`.
    fn track(n: usize, lon: f64) -> (Vec<Option<f64>>, Vec<Option<f64>>) {
        let lat: Vec<Option<f64>> = (0..n).map(|i| Some(55.0 + i as f64 * STEP_DEG)).collect();
        let lon = vec![Some(lon); n];
        (lat, lon)
    }

    /// A segment cut from the same straight line: indices `a..=b` of the
    /// canonical track, with proper cumulative distances.
    fn segment(a: usize, b: usize) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
        let lat: Vec<f64> = (a..=b).map(|i| 55.0 + i as f64 * STEP_DEG).collect();
        let lon = vec![37.0; lat.len()];
        let mut cum = vec![0.0];
        for w in lat.windows(2) {
            let d = haversine_m(w[0], 37.0, w[1], 37.0);
            cum.push(cum.last().unwrap() + d);
        }
        (lat, lon, cum)
    }

    #[test]
    fn clean_pass_is_one_effort_with_track_indices() {
        let (slat, slon, scum) = segment(20, 50);
        let (lat, lon) = track(100, 37.0);
        let efforts = find_efforts(&slat, &slon, &scum, &lat, &lon);
        assert_eq!(efforts.len(), 1);
        let e = &efforts[0];
        assert_eq!(e.start_idx, 20);
        assert_eq!(e.end_idx, 50);
        assert!((e.distance_m - scum.last().unwrap()).abs() < 1.0);
    }

    #[test]
    fn reversed_direction_never_matches() {
        let (slat, slon, scum) = segment(20, 50);
        let (mut lat, lon) = track(100, 37.0);
        lat.reverse();
        assert!(find_efforts(&slat, &slon, &scum, &lat, &lon).is_empty());
    }

    #[test]
    fn parallel_road_fails_the_corridor() {
        // Track shares the segment's endpoints but bulges ~90 m east in the
        // middle — same length class, different road.
        let (slat, slon, scum) = segment(20, 50);
        let (lat, mut lon) = track(100, 37.0);
        for l in lon.iter_mut().take(45).skip(28) {
            // 0.0014° of longitude at 55°N ≈ 89 m — outside the 40 m corridor.
            *l = Some(37.0014);
        }
        assert!(find_efforts(&slat, &slon, &scum, &lat, &lon).is_empty());
    }

    #[test]
    fn gpsless_holes_are_skipped_not_fatal() {
        let (slat, slon, scum) = segment(20, 50);
        let (mut lat, mut lon) = track(100, 37.0);
        lat[35] = None;
        lon[35] = None;
        let efforts = find_efforts(&slat, &slon, &scum, &lat, &lon);
        assert_eq!(efforts.len(), 1);
    }

    #[test]
    fn hill_repeats_are_separate_efforts() {
        // Ride up (20..50), roll back down, ride up again.
        let (slat, slon, scum) = segment(20, 50);
        let (lat0, _) = track(60, 37.0);
        let up: Vec<Option<f64>> = lat0.clone();
        let down: Vec<Option<f64>> = lat0.iter().rev().cloned().collect();
        let mut lat: Vec<Option<f64>> = Vec::new();
        lat.extend(&up);
        lat.extend(&down);
        lat.extend(&up);
        let lon = vec![Some(37.0); lat.len()];
        let efforts = find_efforts(&slat, &slon, &scum, &lat, &lon);
        assert_eq!(efforts.len(), 2);
        // Second pass lands in the third copy of the climb.
        assert!(efforts[1].start_idx > 120);
    }

    #[test]
    fn cutting_the_corner_is_rejected_by_path_length() {
        // L-shaped segment: 15 steps north, then 15 steps east. A track that
        // rides the diagonal hits both endpoints but covers only ~71% of the
        // segment's length — outside the ±10% tolerance, so the exit is
        // never accepted (the corridor check doesn't even get a chance).
        let step_lon = 0.000175; // ≈11 m of longitude at 55°N
        let mut slat = vec![55.0];
        let mut slon = vec![37.0];
        for i in 1..=15 {
            slat.push(55.0 + i as f64 * STEP_DEG);
            slon.push(37.0);
        }
        for j in 1..=15 {
            slat.push(55.0 + 15.0 * STEP_DEG);
            slon.push(37.0 + j as f64 * step_lon);
        }
        let mut scum = vec![0.0];
        for w in 1..slat.len() {
            let d = haversine_m(slat[w - 1], slon[w - 1], slat[w], slon[w]);
            scum.push(scum.last().unwrap() + d);
        }

        let n = 30;
        let lat: Vec<Option<f64>> = (0..=n)
            .map(|i| Some(55.0 + 15.0 * STEP_DEG * (i as f64 / n as f64)))
            .collect();
        let lon: Vec<Option<f64>> = (0..=n)
            .map(|i| Some(37.0 + 15.0 * step_lon * (i as f64 / n as f64)))
            .collect();
        assert!(find_efforts(&slat, &slon, &scum, &lat, &lon).is_empty());
    }

    #[test]
    fn sparse_smart_recording_still_matches() {
        // Same straight line, but the track keeps only every 8th point
        // (~89 m spacing — Garmin smart recording territory). Edge-based
        // corridor distance and the local-step length slack must both hold.
        let (slat, slon, scum) = segment(20, 50);
        let (lat0, lon0) = track(100, 37.0);
        let lat: Vec<Option<f64>> = lat0.iter().step_by(8).cloned().collect();
        let lon: Vec<Option<f64>> = lon0.iter().step_by(8).cloned().collect();
        let efforts = find_efforts(&slat, &slon, &scum, &lat, &lon);
        assert_eq!(efforts.len(), 1);
    }

    #[test]
    fn short_segment_matches_despite_few_corridor_samples() {
        // ~111 m segment has ≤4 corridor samples; requiring literally 80%
        // of them used to mean 100%. The always-granted miss keeps it sane.
        let (slat, slon, scum) = segment(20, 30);
        let (lat, lon) = track(100, 37.0);
        let efforts = find_efforts(&slat, &slon, &scum, &lat, &lon);
        assert_eq!(efforts.len(), 1);
        assert_eq!((efforts[0].start_idx, efforts[0].end_idx), (20, 30));
    }

    #[test]
    fn out_and_back_segment() {
        // Segment: north 15 steps, then back south to the start (start ==
        // end — the nastiest endpoint geometry). A full out-and-back pass
        // matches; a track that turns around halfway fails the length gate.
        let up: Vec<f64> = (0..=15).map(|i| 55.0 + i as f64 * STEP_DEG).collect();
        let down: Vec<f64> = up.iter().rev().skip(1).cloned().collect();
        let slat: Vec<f64> = up.iter().chain(down.iter()).cloned().collect();
        let slon = vec![37.0; slat.len()];
        let mut scum = vec![0.0];
        for w in 1..slat.len() {
            let d = haversine_m(slat[w - 1], 37.0, slat[w], 37.0);
            scum.push(scum.last().unwrap() + d);
        }

        let full: Vec<Option<f64>> = slat.iter().map(|&v| Some(v)).collect();
        let lon = vec![Some(37.0); full.len()];
        assert_eq!(find_efforts(&slat, &slon, &scum, &full, &lon).len(), 1);

        // Turnaround at step 8 of 15: endpoints agree (start == end) but the
        // path is only ~53% of the segment.
        let half_up: Vec<f64> = (0..=8).map(|i| 55.0 + i as f64 * STEP_DEG).collect();
        let half: Vec<Option<f64>> = half_up
            .iter()
            .chain(half_up.iter().rev().skip(1))
            .map(|&v| Some(v))
            .collect();
        let lon2 = vec![Some(37.0); half.len()];
        assert!(find_efforts(&slat, &slon, &scum, &half, &lon2).is_empty());
    }

    #[test]
    fn jittery_finish_picks_the_length_consistent_exit() {
        // Rider reaches the finish area, wanders back and forth inside the
        // circle. The exit must be the candidate whose path length matches
        // the segment best — not whichever point lands nearest the finish
        // coordinates after extra wandering distance has accumulated.
        let (slat, slon, scum) = segment(20, 50);
        let seg_len = *scum.last().unwrap();
        let (lat0, _) = track(50, 37.0);
        let mut lat: Vec<Option<f64>> = lat0[..=49].to_vec();
        // Wander: 49 → back to 46 → forward to exactly 50.
        for i in (46..49).rev() {
            lat.push(Some(55.0 + i as f64 * STEP_DEG));
        }
        for i in 47..=50 {
            lat.push(Some(55.0 + i as f64 * STEP_DEG));
        }
        let lon = vec![Some(37.0); lat.len()];
        let efforts = find_efforts(&slat, &slon, &scum, &lat, &lon);
        assert_eq!(efforts.len(), 1);
        // The best-length candidate is the FIRST arrival near idx 49/50,
        // before the wandering inflated the path.
        assert!((efforts[0].distance_m - seg_len).abs() <= seg_len * 0.05,
            "expected a length-consistent exit, got {} vs {}", efforts[0].distance_m, seg_len);
    }

    #[test]
    fn degenerate_inputs_yield_nothing() {
        let (lat, lon) = track(10, 37.0);
        assert!(find_efforts(&[], &[], &[], &lat, &lon).is_empty());
        let (slat, slon, scum) = segment(2, 5);
        assert!(find_efforts(&slat, &slon, &scum, &[], &[]).is_empty());
        let none: Vec<Option<f64>> = vec![None; 10];
        assert!(find_efforts(&slat, &slon, &scum, &none, &none).is_empty());
    }
}
