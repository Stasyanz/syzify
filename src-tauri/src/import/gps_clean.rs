//! GPS sanity filter for jammed and spoofed tracks (#256).
//!
//! Where GPS is jammed or spoofed a track breaks in three ways: no fix for
//! minutes, a teleport of kilometres when the fix comes back, and runs of
//! fake fixes moving at ~100 km/h in loops. The filter breaks the track at
//! every step the sport can't make — a jump past the top speed and the
//! noise of a fix, or a stretch held past the top speed that no descent
//! explains — drops the short pieces a fake track falls apart into, and
//! keeps the chain of pieces whose fixes cover the most time and whose joins
//! are reachable.
//!
//! It only drops what it has a case against, and only a short piece: a jump
//! looks the same whether GPS lied or the rider took a train, so a track
//! whose chain would cost a long plausible piece anywhere is left as it is
//! — a lost teleport beats a lost half of a ride. Rejected fixes lose their
//! position (the row stays: segment efforts address trackpoints by index).

use crate::db::trackpoints::{haversine_m, parse_time_seconds};
use crate::models::activity::SportType;
use crate::models::trackpoint::{DroppedGps, TrackPoint};
use crate::parser::SensorInfo;

/// Fixes; a shorter piece is a fragment of a fake track or a lone glitch.
const MIN_PIECE: usize = 5;
/// Up to this many seconds between fixes, a step is held to the sport's
/// top speed; past it the fixes are a gap and judged on the average speed
/// across it.
const SHORT_STEP_S: f64 = 5.0;
/// What a fix may sit off the last one beyond the top speed: multipath in a
/// street or under trees snaps the position by tens of metres and stays.
const STEP_SLACK_M: f64 = 50.0;
/// Average speed across a gap, as a share of the top speed: nobody holds
/// their sprint through a minute of jamming.
const GAP_SPEED_SHARE: f64 = 0.6;
/// Position noise allowed at the two ends of a gap.
const GAP_SLACK_M: f64 = 200.0;
/// Past the sustained speed this long without a descent is a fake track —
/// the loops a spoofer draws run at 85–110 km/h for up to a minute.
const SUSTAINED_S: f64 = 5.0;
/// The drop that lets a bike hold its sustained speed: at 72 km/h the air
/// takes some 1.4 kW, which only a descent pays for. Lenient (a tailwind, a
/// wheel to follow): a fake track moves on the flat, within a percent.
const DESCENT_GRADE: f64 = 0.02;
/// How long before a fast stretch its descent may have begun: a rider
/// carries speed out of a dip, past its lowest point.
const DESCENT_LEAD_S: f64 = 30.0;
/// A piece outside the chain is dropped only up to this much covered time,
/// a fast stretch only up to this long: a fake lasts a minute or two, and
/// anything longer can't be told from the real ride or a vehicle.
const DROPPED_MAX_S: f64 = 180.0;
/// A piece that stays this close to its first fix for [`MOTIONLESS_MIN_S`]
/// proves nothing: a spoofer holds a position as well as a cafe does.
const MOTIONLESS_RADIUS_M: f64 = 100.0;
const MOTIONLESS_MIN_S: f64 = 30.0;
/// A chain covering less than this share of the track's time is not a
/// damaged track but a fast one (a drive filed as a ride): left as it is.
const MIN_KEPT_SHARE: f64 = 0.5;
/// A pause this long cuts a piece: it takes that long to make a teleport of
/// kilometres look reachable. Shorter steps must not cut, or sparse recording
/// falls apart into single fixes the limit above can't see.
const PAUSE_CUT_MIN_S: f64 = 300.0;
/// Previous pieces a piece may join; bounds the chain search on noisy tracks.
const JOIN_WINDOW: usize = 256;

/// What a sport can do. `None` from [`sport_limits`]: the filter leaves the
/// sport alone — fast on its own (skiing, sailing), unknown, simulated or
/// indoor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SportLimits {
    /// The fastest the sport moves between two fixes, m/s.
    pub top_mps: f64,
    /// The fastest it holds for [`SUSTAINED_S`], m/s.
    pub sustained_mps: f64,
    /// Whether it holds more downhill (a bike does, a runner doesn't) —
    /// then a fast stretch that loses height stays.
    pub faster_downhill: bool,
}

pub fn sport_limits(sport: &SportType) -> Option<SportLimits> {
    match sport {
        // A step up to ~90 km/h; held, 72 km/h on the flat — a pro's sprint,
        // under the 85–110 km/h of a fake track; a descent holds more.
        SportType::Ride | SportType::MountainBike | SportType::Triathlon => {
            Some(SportLimits { top_mps: 25.0, sustained_mps: 20.0, faster_downhill: true })
        }
        // Past a sprinter's 10 m/s.
        SportType::Run
        | SportType::TrailRun
        | SportType::Walk
        | SportType::Hike
        | SportType::Mountaineering
        | SportType::OpenWater
        | SportType::Paddle => {
            Some(SportLimits { top_mps: 12.0, sustained_mps: 12.0, faster_downhill: false })
        }
        _ => None,
    }
}

/// Whether the file names a sensor that measures speed on its own (a wheel
/// sensor, a foot pod): its speed and distance don't come from GPS, so the
/// device's figures stay trustworthy when the fixes don't.
pub fn has_speed_sensor(sensors: &[SensorInfo]) -> bool {
    sensors.iter().any(|s| is_speed_sensor(s.device_type.as_deref()))
}

/// The device types [`has_speed_sensor`] looks for (ANT+ and the BLE
/// names the parser files under the same field), for a reader of stored
/// sensors.
pub fn is_speed_sensor(device_type: Option<&str>) -> bool {
    matches!(
        device_type,
        Some("bike_speed" | "bike_speed_cadence" | "stride_speed_distance" | "footpod")
    )
}

/// What [`clean_trackpoints`] cleared, by trackpoint index.
#[derive(Debug, Default, PartialEq)]
pub struct GpsCleaning {
    /// Trackpoints whose position was dropped.
    pub fixes: Vec<usize>,
    /// Trackpoints whose speed was dropped: GPS speed with no fix left
    /// under it, or far faster than the fixes around it move.
    pub speeds: Vec<usize>,
    /// The dropped values themselves, one entry per touched trackpoint in
    /// index order — kept so a later filter can put them back.
    pub dropped: Vec<DroppedGps>,
}

impl GpsCleaning {
    pub fn is_empty(&self) -> bool {
        self.fixes.is_empty() && self.speeds.is_empty()
    }
}

/// Drop the impossible fixes of a track in place. `speed_from_sensor`: the
/// speed column comes from a wheel sensor or a foot pod and is kept; else it
/// is GPS speed, and on a track that lost fixes it goes where it has no fix
/// or runs far ahead of the fixes. A track that lost nothing keeps every
/// speed — a fast descent's included.
pub fn clean_trackpoints(
    tps: &mut [TrackPoint],
    sport: &SportType,
    speed_from_sensor: bool,
) -> GpsCleaning {
    let Some(limits) = sport_limits(sport) else {
        return GpsCleaning::default();
    };
    let t: Vec<Option<f64>> =
        tps.iter().map(|p| p.t.as_deref().and_then(parse_time_seconds)).collect();
    let lat: Vec<Option<f64>> = tps.iter().map(|p| p.lat).collect();
    let lon: Vec<Option<f64>> = tps.iter().map(|p| p.lon).collect();
    let alt: Vec<Option<f64>> = tps.iter().map(|p| p.altitude_m).collect();
    let fixes = rejected_fixes(&t, &lat, &lon, &alt, limits);
    if fixes.is_empty() {
        return GpsCleaning::default();
    }
    let original: Vec<(Option<f64>, Option<f64>, Option<f64>)> =
        tps.iter().map(|p| (p.lat, p.lon, p.speed_mps)).collect();
    for &i in &fixes {
        tps[i].lat = None;
        tps[i].lon = None;
    }
    let mut speeds = Vec::new();
    if !speed_from_sensor {
        // The device smooths its GPS speed, so after a fake stretch it still
        // reads 80 km/h for a few seconds on fixes that are already back.
        let lat: Vec<Option<f64>> = tps.iter().map(|p| p.lat).collect();
        let lon: Vec<Option<f64>> = tps.iter().map(|p| p.lon).collect();
        let speed: Vec<Option<f64>> = tps.iter().map(|p| p.speed_mps).collect();
        let lagging = speeds_beyond_motion(&t, &lat, &lon, &speed, limits.top_mps);
        for (i, p) in tps.iter_mut().enumerate() {
            // A speed without a fix is GPS speed without GPS — a rejected
            // fix's, or one the jamming left behind.
            if p.speed_mps.is_some() && (p.lat.is_none() || lagging.binary_search(&i).is_ok()) {
                p.speed_mps = None;
                speeds.push(i);
            }
        }
    }
    let mut touched: Vec<usize> = fixes.iter().chain(&speeds).copied().collect();
    touched.sort_unstable();
    touched.dedup();
    let dropped = touched
        .into_iter()
        .map(|i| {
            let (lat, lon, speed) = original[i];
            let fix = fixes.binary_search(&i).is_ok();
            DroppedGps {
                index: i,
                lat: lat.filter(|_| fix),
                lon: lon.filter(|_| fix),
                speed_mps: speed.filter(|_| speeds.binary_search(&i).is_ok()),
            }
        })
        .collect();
    GpsCleaning { fixes, speeds, dropped }
}

/// Seconds either side of a fix the motion around it is measured over.
const MOTION_WINDOW_S: f64 = 5.0;
/// How much faster than the fixes move a speed sample may read…
const MOTION_FACTOR: f64 = 1.5;
/// …plus this much: pulling away from a stop.
const MOTION_SLACK_MPS: f64 = 3.0;

/// Indices (ascending) of speed samples far faster than the fixes around
/// them move: past [`MOTION_FACTOR`] × the path the fixes within
/// [`MOTION_WINDOW_S`] on either side cover per second, plus
/// [`MOTION_SLACK_MPS`]. The path, not the chord: a turnaround moves the
/// rider without moving the ends. A fix with no other in the window has no
/// motion to compare with; its speed goes past `top_mps`.
fn speeds_beyond_motion(
    t: &[Option<f64>],
    lat: &[Option<f64>],
    lon: &[Option<f64>],
    speed: &[Option<f64>],
    top_mps: f64,
) -> Vec<usize> {
    let fixes = fixes_of(t, lat, lon);
    // Cumulative path along the fixes.
    let mut path = vec![0.0; fixes.len()];
    for k in 1..fixes.len() {
        path[k] = path[k - 1] + dist(&fixes[k - 1], &fixes[k]);
    }
    let mut out = Vec::new();
    for (k, f) in fixes.iter().enumerate() {
        let Some(v) = speed[f.i] else { continue };
        let a = (0..k).rev().take_while(|&j| f.t - fixes[j].t <= MOTION_WINDOW_S).last().unwrap_or(k);
        let b = (k + 1..fixes.len()).take_while(|&j| fixes[j].t - f.t <= MOTION_WINDOW_S).last().unwrap_or(k);
        let dt = fixes[b].t - fixes[a].t;
        let beyond = if dt <= 0.0 {
            v > top_mps
        } else {
            v > MOTION_FACTOR * (path[b] - path[a]) / dt + MOTION_SLACK_MPS
        };
        if beyond {
            out.push(f.i);
        }
    }
    out
}

/// Indices (ascending) of the fixes to drop. A fix is a point with a time
/// and a position; points without either are never rejected. `alt` may be
/// all `None` (no altitude recorded).
pub fn rejected_fixes(
    t: &[Option<f64>],
    lat: &[Option<f64>],
    lon: &[Option<f64>],
    alt: &[Option<f64>],
    limits: SportLimits,
) -> Vec<usize> {
    let fixes = fixes_of(t, lat, lon);
    if fixes.len() < MIN_PIECE {
        return Vec::new();
    }
    let mut breaks = step_breaks(&fixes, alt, limits);
    if !breaks.iter().any(|&b| b) {
        return Vec::new();
    }
    // A jammed stretch or a long pause cuts the track too. What resumes after
    // it may be reachable from a teleport before it, but they are not one
    // piece: the chain decides which of the two sides the road belongs to.
    let impossible = breaks.clone();
    let resumes = fixes_after_gaps(t, lat, lon);
    for (k, step) in breaks.iter_mut().enumerate() {
        *step = *step
            || resumes.binary_search(&fixes[k + 1].i).is_ok()
            || fixes[k + 1].t - fixes[k].t >= PAUSE_CUT_MIN_S;
    }

    // Pieces: runs of fixes between breaks (breaks[k] cuts fixes k | k+1).
    let mut pieces: Vec<&[Fix]> = Vec::new();
    let mut start = 0;
    for (k, &cut) in breaks.iter().enumerate() {
        if cut {
            pieces.push(&fixes[start..=k]);
            start = k + 1;
        }
    }
    pieces.push(&fixes[start..]);
    // A short piece is a fragment only beside an impossible step: a gap or a
    // pause says nothing against it, and sparse recording is all short pieces.
    let fragment = |p: &[Fix]| {
        let first = fixes.partition_point(|f| f.i < p[0].i);
        let last = first + p.len() - 1;
        p.len() < MIN_PIECE
            && ((first > 0 && impossible[first - 1]) || impossible.get(last).copied().unwrap_or(false))
    };
    let pieces: Vec<&[Fix]> = pieces.into_iter().filter(|p| !fragment(p)).collect();
    if pieces.is_empty() {
        return Vec::new();
    }

    // The chain covering the most time: best[j] = time of the best chain
    // ending at j. A motionless piece counts next to nothing, so it joins a
    // chain it fits but can't outweigh one it doesn't.
    let weight = |p: &[Fix]| if motionless(p) { 1.0 } else { covered(p) };
    let n = pieces.len();
    let mut best: Vec<f64> = pieces.iter().map(|p| weight(p)).collect();
    let mut prev: Vec<Option<usize>> = vec![None; n];
    for j in 0..n {
        let first = &pieces[j][0];
        for i in j.saturating_sub(JOIN_WINDOW)..j {
            let last = pieces[i].last().expect("pieces are never empty");
            let total = best[i] + weight(pieces[j]);
            if total > best[j] && !breaks_between(last, first, limits) {
                best[j] = total;
                prev[j] = Some(i);
            }
        }
    }
    let mut end = (0..n).max_by(|&a, &b| best[a].total_cmp(&best[b])).expect("n > 0");
    let mut chain = vec![end];
    while let Some(p) = prev[end] {
        chain.push(p);
        end = p;
    }
    chain.reverse();

    // A long piece outside the chain, anywhere, has no case against it.
    let mut in_chain = vec![false; n];
    for &k in &chain {
        in_chain[k] = true;
    }
    if (0..n).any(|k| !in_chain[k] && recorded(pieces[k]) > DROPPED_MAX_S) {
        return Vec::new();
    }

    let mut kept = vec![false; t.len()];
    for &k in &chain {
        for f in pieces[k] {
            kept[f.i] = true;
        }
    }
    // The share kept, by the time the fixes themselves cover: the minutes a
    // jammer left without a fix belong to neither side.
    let time_of = |k: usize| fixes.get(k + 1).map_or(1.0, |n| (n.t - fixes[k].t).clamp(0.0, SHORT_STEP_S));
    let total: f64 = (0..fixes.len()).map(time_of).sum();
    let kept_time: f64 = (0..fixes.len()).filter(|&k| kept[fixes[k].i]).map(time_of).sum();
    if kept_time < MIN_KEPT_SHARE * total {
        return Vec::new();
    }
    fixes.iter().map(|f| f.i).filter(|&i| !kept[i]).collect()
}

/// A stretch of rows without a fix is a gap from this long on — the same
/// rule as `GPS_GAP_MIN_S` in `src/lib/gpsGaps.ts`, which breaks the map. A
/// step between two rows this long is a pause, and no time without GPS.
pub const GPS_GAP_MIN_S: f64 = 30.0;

/// Indices of the fixes that resume a track after a gap inside it: rows
/// without a fix lasting [`GPS_GAP_MIN_S`], not counting a pause — nothing
/// was recorded in one, with or without a row beside it.
pub fn fixes_after_gaps(t: &[Option<f64>], lat: &[Option<f64>], lon: &[Option<f64>]) -> Vec<usize> {
    let n = t.len().min(lat.len()).min(lon.len());
    let has_fix = |i: usize| lat[i].is_some() && lon[i].is_some();
    let mut out = Vec::new();
    let mut last_fix: Option<usize> = None;
    for i in 0..n {
        if !has_fix(i) {
            continue;
        }
        if let Some(p) = last_fix {
            // Both ends have a time, or the stretch can't be told.
            if i > p + 1 && t[p].is_some() && t[i].is_some() {
                let seconds: f64 = (p..i)
                    .filter_map(|k| Some(t[k + 1]? - t[k]?))
                    .filter(|step| *step < GPS_GAP_MIN_S)
                    .sum();
                if seconds >= GPS_GAP_MIN_S {
                    out.push(i);
                }
            }
        }
        last_fix = Some(i);
    }
    out
}

#[derive(Clone, Copy)]
struct Fix {
    i: usize,
    t: f64,
    lat: f64,
    lon: f64,
}

fn fixes_of(t: &[Option<f64>], lat: &[Option<f64>], lon: &[Option<f64>]) -> Vec<Fix> {
    (0..t.len().min(lat.len()).min(lon.len()))
        .filter_map(|i| Some(Fix { i, t: t[i]?, lat: lat[i]?, lon: lon[i]? }))
        .collect()
}

fn dist(a: &Fix, b: &Fix) -> f64 {
    haversine_m(a.lat, a.lon, b.lat, b.lon)
}

/// The time a run's fixes cover: each up to the next, at most a close
/// step's worth, the last a second — a pause inside it counts for nothing.
fn covered(p: &[Fix]) -> f64 {
    let steps: f64 = p.windows(2).map(|w| (w[1].t - w[0].t).clamp(0.0, SHORT_STEP_S)).sum();
    steps + 1.0
}

/// How long a piece lasted: unlike [`covered`], sparse fixes count in full —
/// nine minutes of a fix every 15 s are nine minutes of a walk, not three.
/// A clock running back takes nothing off it.
fn recorded(p: &[Fix]) -> f64 {
    p.windows(2).map(|w| (w[1].t - w[0].t).max(0.0)).sum()
}

/// Whether a run sits still: [`MOTIONLESS_MIN_S`] long, every fix within
/// [`MOTIONLESS_RADIUS_M`] of the first.
fn motionless(p: &[Fix]) -> bool {
    covered(p) >= MOTIONLESS_MIN_S && p.iter().all(|f| dist(&p[0], f) <= MOTIONLESS_RADIUS_M)
}

/// Whether `b` can't follow `a` at the sport's speed: a close step past the
/// top speed and a fix's noise, a gap past the share of it a rider holds
/// plus the noise of its ends. Time running backwards is not judged.
fn breaks_between(a: &Fix, b: &Fix, limits: SportLimits) -> bool {
    let dt = b.t - a.t;
    if dt < 0.0 {
        return false;
    }
    let dt = dt.max(1.0);
    let max_m = if dt <= SHORT_STEP_S {
        limits.top_mps * dt + STEP_SLACK_M
    } else {
        GAP_SPEED_SHARE * limits.top_mps * dt + GAP_SLACK_M
    };
    dist(a, b) > max_m
}

/// For each step between consecutive fixes, whether the track breaks there:
/// an impossible step ([`breaks_between`]), or a step inside a stretch held
/// past the sustained speed that no descent explains — each of its steps
/// breaks, so a fake track falls apart into single fixes.
///
/// "Held" is the average over every window of [`SUSTAINED_S`] of close
/// steps, not each step: a fake track jitters (0–40 m a second) around its
/// pace. A window never spans a step that breaks on its own, so a teleport
/// does not drag the fixes beside it into a fast stretch. A stretch past
/// [`DROPPED_MAX_S`] is a vehicle, not a fake, and does not break.
fn step_breaks(fixes: &[Fix], alt: &[Option<f64>], limits: SportLimits) -> Vec<bool> {
    let steps = fixes.len().saturating_sub(1);
    let mut breaks: Vec<bool> = (0..steps).map(|k| breaks_between(&fixes[k], &fixes[k + 1], limits)).collect();
    let close = |k: usize| {
        let dt = fixes[k + 1].t - fixes[k].t;
        dt > 0.0 && dt <= SHORT_STEP_S && !breaks[k]
    };
    let mut fast = vec![false; steps];
    for k in 0..steps {
        let (mut j, mut path) = (k, 0.0);
        while j < steps && close(j) && fixes[j].t - fixes[k].t < SUSTAINED_S {
            path += dist(&fixes[j], &fixes[j + 1]);
            j += 1;
        }
        let dt = fixes[j].t - fixes[k].t;
        if dt >= SUSTAINED_S && path > limits.sustained_mps * dt {
            fast[k..j].iter_mut().for_each(|f| *f = true);
        }
    }
    let mut k = 0;
    while k < steps {
        if !fast[k] {
            k += 1;
            continue;
        }
        let mut j = k;
        while j < steps && fast[j] {
            j += 1;
        }
        let next = j;
        // A window reaches past the fast track at its ends: the slow steps
        // there are the real track's.
        let slow = |s: usize| dist(&fixes[s], &fixes[s + 1]) <= limits.sustained_mps * (fixes[s + 1].t - fixes[s].t);
        while k < j && slow(k) {
            k += 1;
        }
        while j > k && slow(j - 1) {
            j -= 1;
        }
        // Steps k..j are fast: fixes k..=j.
        if k < j
            && fixes[j].t - fixes[k].t <= DROPPED_MAX_S
            && !explained_by_descent(fixes, k, j, alt, limits)
        {
            breaks[k..j].iter_mut().for_each(|b| *b = true);
        }
        k = next;
    }
    breaks
}

/// Whether the fast stretch of fixes `k..=j` is a bike going downhill: from
/// the highest point up to [`DESCENT_LEAD_S`] before it or in it, to the
/// lowest after that within it, it loses at least [`DESCENT_GRADE`] of its
/// length — ends alone would miss a dip it climbs back out of. Without an
/// altitude to tell (none recorded, or one value for the whole track, as a
/// GPX's constant `<ele>`) the stretch gets the benefit of the doubt.
fn explained_by_descent(fixes: &[Fix], k: usize, j: usize, alt: &[Option<f64>], limits: SportLimits) -> bool {
    if !limits.faster_downhill {
        return false;
    }
    let alt_of = |f: &Fix| alt.get(f.i).copied().flatten();
    let mut known = fixes.iter().filter_map(alt_of);
    let informative = known.next().is_some_and(|first| known.any(|a| a != first));
    if !informative {
        return true;
    }
    let lead = (0..k).rev().take_while(|&s| fixes[k].t - fixes[s].t <= DESCENT_LEAD_S).last().unwrap_or(k);
    let (mut peak, mut drop): (Option<f64>, f64) = (None, 0.0);
    for (s, f) in fixes.iter().enumerate().take(j + 1).skip(lead) {
        let Some(a) = alt_of(f) else { continue };
        if s >= k {
            drop = drop.max(peak.unwrap_or(a) - a);
        }
        peak = Some(peak.map_or(a, |p| p.max(a)));
    }
    if peak.is_none() {
        return true; // no altitude on this stretch
    }
    let length: f64 = fixes[k..=j].windows(2).map(|w| dist(&w[0], &w[1])).sum();
    drop >= DESCENT_GRADE * length
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAT0: f64 = 55.0;
    const LON0: f64 = 37.0;
    const M_PER_DEG_LAT: f64 = 111_195.0;
    const BIKE: SportLimits = SportLimits { top_mps: 25.0, sustained_mps: 20.0, faster_downhill: true };
    const FOOT: SportLimits = SportLimits { top_mps: 12.0, sustained_mps: 12.0, faster_downhill: false };

    /// A point `north`/`east` metres from the origin.
    fn at(north: f64, east: f64) -> (f64, f64) {
        let m_per_deg_lon = M_PER_DEG_LAT * LAT0.to_radians().cos();
        (LAT0 + north / M_PER_DEG_LAT, LON0 + east / m_per_deg_lon)
    }

    type Pt = (f64, Option<(f64, f64)>);

    /// `secs` fixes, one a second, heading north at `v` m/s from `north`.
    fn ride(t0: f64, secs: usize, north: f64, east: f64, v: f64) -> Vec<Pt> {
        (0..secs).map(|k| (t0 + k as f64, Some((north + v * k as f64, east)))).collect()
    }

    /// Rejected fixes with the altitude `alt(i)` gives each point.
    fn rejected_alt(track: &[Pt], limits: SportLimits, alt: impl Fn(usize) -> Option<f64>) -> Vec<usize> {
        let ll: Vec<Option<(f64, f64)>> = track.iter().map(|p| p.1.map(|(n, e)| at(n, e))).collect();
        let t: Vec<Option<f64>> = track.iter().map(|p| Some(p.0)).collect();
        let lat: Vec<Option<f64>> = ll.iter().map(|p| p.map(|x| x.0)).collect();
        let lon: Vec<Option<f64>> = ll.iter().map(|p| p.map(|x| x.1)).collect();
        let alt: Vec<Option<f64>> = (0..track.len()).map(alt).collect();
        rejected_fixes(&t, &lat, &lon, &alt, limits)
    }

    /// Rejected fixes on flat ground: a barometer's 100 m, give or take.
    fn rejected(track: &[Pt], limits: SportLimits) -> Vec<usize> {
        rejected_alt(track, limits, |i| Some(100.0 + (i % 5) as f64 * 0.2))
    }

    /// `secs` rows without a fix from `t0`.
    fn jam(t0: usize, secs: usize) -> Vec<Pt> {
        (t0..t0 + secs).map(|t| (t as f64, None)).collect()
    }

    /// A minute of fixes circling at 28 m/s from `(n0, 0)`, starting at `t0`.
    fn fake_loop(t0: f64, n0: f64) -> Vec<Pt> {
        let r = 28.0 / 0.1; // radius for 28 m/s at 0.1 rad/s
        (1..=60)
            .map(|k| {
                let a = k as f64 * 0.1;
                (t0 + k as f64, Some((n0 + r * a.sin(), r * (1.0 - a.cos()))))
            })
            .collect()
    }

    #[test]
    fn a_clean_ride_keeps_every_fix() {
        assert!(rejected(&ride(0.0, 600, 0.0, 0.0, 8.0), BIKE).is_empty());
    }

    #[test]
    fn a_fast_minute_on_the_flat_under_the_sustained_speed_is_kept() {
        let mut track = ride(0.0, 100, 0.0, 0.0, 8.0);
        track.extend(ride(100.0, 60, 800.0, 0.0, 19.0)); // 68 km/h
        assert!(rejected(&track, BIKE).is_empty());
    }

    #[test]
    fn a_descent_past_the_limit_is_kept() {
        // 40 s at 26 m/s (94 km/h) losing 10 % of its length, then on.
        let mut track = ride(0.0, 1000, 0.0, 0.0, 8.0);
        track.extend(ride(1000.0, 40, 8_000.0, 0.0, 26.0));
        track.extend(ride(1040.0, 1960, 9_040.0, 0.0, 8.0));
        let alt = |i: usize| Some(500.0 - 2.6 * (i.clamp(1000, 1040) - 1000) as f64);
        assert!(rejected_alt(&track, BIKE, alt).is_empty());
        // No altitude to tell: the descent gets the benefit of the doubt.
        assert!(rejected_alt(&track, BIKE, |_| None).is_empty());
    }

    #[test]
    fn a_lone_spike_is_dropped() {
        let mut track = ride(0.0, 200, 0.0, 0.0, 8.0);
        track[100].1 = Some((5_000.0, 3_000.0));
        assert_eq!(rejected(&track, BIKE), vec![100]);
    }

    #[test]
    fn a_fix_that_snaps_aside_and_stays_keeps_the_track() {
        // Multipath: from 60 % on, every fix sits 30 m to the side.
        let mut track = ride(0.0, 2000, 0.0, 0.0, 8.0);
        for p in &mut track[1200..] {
            p.1 = p.1.map(|(n, e)| (n, e + 30.0));
        }
        assert!(rejected(&track, BIKE).is_empty());
        // On foot: a 14 m jump under the trees, then on from there.
        let mut walk = ride(0.0, 3000, 0.0, 0.0, 3.0);
        for p in &mut walk[1800..] {
            p.1 = p.1.map(|(n, e)| (n, e + 14.0));
        }
        assert!(rejected(&walk, FOOT).is_empty());
    }

    #[test]
    fn a_teleport_after_a_gap_drops_the_far_side() {
        // 10 min riding, 5 min without a fix, then 2 min 20 km away, then
        // back on the road where the rider plausibly is.
        let mut track = ride(0.0, 600, 0.0, 0.0, 8.0);
        track.extend((600..900).map(|t| (t as f64, None)));
        track.extend(ride(900.0, 120, 20_000.0, 0.0, 8.0));
        track.extend(ride(1020.0, 600, 4_800.0 + 8.0 * 420.0, 0.0, 8.0));
        assert_eq!(rejected(&track, BIKE), (900..1020).collect::<Vec<_>>());
    }

    #[test]
    fn a_teleport_after_a_long_jamming_is_still_dropped() {
        // 10 min riding, 25 min without a fix, a minute teleported, 10 min
        // riding: the jammed minutes count for neither side.
        let mut track = ride(0.0, 600, 0.0, 0.0, 8.0);
        track.extend((600..2100).map(|t| (t as f64, None)));
        track.extend(ride(2100.0, 60, 30_000.0, 0.0, 8.0));
        track.extend(ride(2160.0, 600, 4_792.0 + 4.0 * 1561.0, 0.0, 8.0));
        assert_eq!(rejected(&track, BIKE), (2100..2160).collect::<Vec<_>>());
    }

    #[test]
    fn the_share_kept_counts_the_fixes_not_the_jammed_time() {
        // 10 min real, a minute 100 km off, 50 min jammed, 10 min of road:
        // by the clock the road is under half, by its fixes well over.
        let mut track = ride(0.0, 600, 0.0, 0.0, 8.0);
        track.extend(ride(600.0, 60, 100_000.0, 0.0, 8.0));
        track.extend(jam(660, 3000));
        track.extend(ride(3660.0, 600, 4_792.0 + 4.0 * 3061.0, 0.0, 8.0));
        assert_eq!(rejected(&track, BIKE), (600..660).collect::<Vec<_>>());
    }

    #[test]
    fn a_teleport_before_the_jamming_does_not_take_the_road_from_the_real_start() {
        // Real start, a minute 20 km off, 25 min jammed, then the road — which
        // the teleport could reach too, given the time. The real start wins.
        for start in [170, 600] {
            let mut track = ride(0.0, start, 0.0, 0.0, 8.0);
            track.extend(ride(start as f64, 60, 20_000.0, 0.0, 8.0));
            track.extend(jam(start + 60, 1500));
            track.extend(ride((start + 1560) as f64, 600, 8.0 * start as f64 + 5.0 * 1560.0, 0.0, 8.0));
            assert_eq!(rejected(&track, BIKE), (start..start + 60).collect::<Vec<_>>(), "start {start}");
        }
    }

    #[test]
    fn a_long_piece_in_the_middle_is_never_dropped() {
        // A walk, the metro out and back: a jump GPS can't tell from a lie.
        // 30 min at the destination is a walk, not a fake.
        let mut walk = ride(0.0, 2400, 0.0, 0.0, 1.4);
        walk.extend(jam(2400, 600));
        walk.extend(ride(3000.0, 1800, 12_000.0, 0.0, 1.4));
        walk.extend(jam(4800, 600));
        walk.extend(ride(5400.0, 1800, 3_360.0, 0.0, 1.4));
        assert!(rejected(&walk, FOOT).is_empty());
        // A ride with a train through two pauses: no row between the fixes.
        let mut train = ride(0.0, 3600, 0.0, 0.0, 8.0);
        train.extend(ride(3700.0, 2700, 53_000.0, 0.0, 8.0));
        train.extend(ride(6500.0, 3600, 29_000.0, 0.0, 8.0));
        assert!(rejected(&train, BIKE).is_empty());
    }

    /// A walk at 1.4 m/s with fixes at the times given, the middle one 5 km off.
    fn sparse_walk_with_a_spike(times: &[f64]) -> Vec<Pt> {
        let mut track: Vec<Pt> = times.iter().map(|&t| (t, Some((1.4 * t, 0.0)))).collect();
        let mid = times.len() / 2;
        track[mid].1 = Some((1.4 * times[mid], 5_000.0));
        track
    }

    #[test]
    fn a_spike_on_sparse_recording_costs_the_spike_and_its_neighbours_only() {
        // A fix every 30 s or 60 s: every step is a pause's length, so every
        // piece is short — none of them a fragment but the spike's.
        for step in [30.0, 60.0] {
            let times: Vec<f64> = (0..1500).map(|k| step * k as f64).collect();
            let track = sparse_walk_with_a_spike(&times);
            assert_eq!(rejected(&track, FOOT), vec![750], "step {step}");
        }
    }

    /// Fixes every `step` s over `from..to` s, heading north at `v` from `north`.
    fn sparse(from: usize, to: usize, step: usize, north: f64, v: f64) -> Vec<Pt> {
        (from..to).step_by(step).map(|t| (t as f64, Some((north + v * (t - from) as f64, 0.0)))).collect()
    }

    #[test]
    fn a_long_piece_on_sparse_recording_is_never_dropped() {
        for step in [30, 45, 60] {
            // The metro out and back: 30 min at the destination stay.
            let mut walk = sparse(0, 2400, step, 0.0, 1.4);
            walk.extend(sparse(3000, 4800, step, 12_000.0, 1.4));
            walk.extend(sparse(5400, 7200, step, 3_360.0, 1.4));
            assert!(rejected(&walk, FOOT).is_empty(), "walk, step {step}");
            // Two hours, 40 km on a train, 50 min more: the last part stays.
            let mut ride = sparse(0, 7200, step, 0.0, 8.0);
            ride.extend(sparse(9000, 12000, step, 97_600.0, 8.0));
            assert!(rejected(&ride, BIKE).is_empty(), "ride, step {step}");
        }
    }

    #[test]
    fn a_clock_running_back_does_not_shorten_a_piece() {
        // 20 min off the road, the clock set back halfway: first to last it
        // looks like under two minutes, but it recorded twenty.
        let mut track = ride(0.0, 600, 0.0, 0.0, 8.0);
        track.extend(ride(600.0, 600, 20_000.0, 0.0, 8.0));
        track.extend(ride(100.0, 600, 24_800.0, 0.0, 8.0));
        track.extend(ride(700.0, 700, 5_600.0, 0.0, 8.0));
        assert!(rejected(&track, BIKE).is_empty());
    }

    #[test]
    fn a_spike_on_mixed_recording_costs_the_spike_and_its_neighbours_only() {
        // Bursts of `long` and `short` fixes `within` s apart, `between` s
        // between bursts: the short bursts are real, not a fake's fragments.
        for (long, short, within, between) in [(8, 3, 10.0, 35.0), (12, 4, 8.0, 40.0)] {
            let mut times = Vec::new();
            let mut t = 0.0;
            for burst in 0..200 {
                for _ in 0..(if burst % 2 == 0 { long } else { short }) {
                    times.push(t);
                    t += within;
                }
                t += between - within;
            }
            let track = sparse_walk_with_a_spike(&times);
            assert_eq!(rejected(&track, FOOT), vec![times.len() / 2], "{long}+{short}");
        }
    }

    #[test]
    fn sparse_real_fixes_count_their_full_time_against_the_limit() {
        // The metro out and back around nine minutes of a walk with a fix
        // every 15 s: 36 fixes, but no fake lasts nine minutes.
        let mut walk = ride(0.0, 2400, 0.0, 0.0, 1.4);
        walk.extend(jam(2400, 600));
        walk.extend((0..36).map(|k| (3000.0 + 15.0 * k as f64, Some((12_000.0 + 21.0 * k as f64, 0.0)))));
        walk.extend(jam(3540, 600));
        walk.extend(ride(4140.0, 1800, 3_360.0, 0.0, 1.4));
        assert!(rejected(&walk, FOOT).is_empty());
    }

    #[test]
    fn the_limit_on_a_dropped_piece_is_three_minutes() {
        let teleported = |secs: usize| {
            let mut track = ride(0.0, 600, 0.0, 0.0, 8.0);
            track.extend(ride(600.0, secs, 20_000.0, 0.0, 8.0));
            track.extend(ride((600 + secs) as f64, 600, 4_792.0 + 8.0 * (secs + 1) as f64, 0.0, 8.0));
            track
        };
        assert_eq!(rejected(&teleported(170), BIKE).len(), 170);
        assert!(rejected(&teleported(190), BIKE).is_empty());
    }

    #[test]
    fn a_teleport_before_a_pause_does_not_take_the_road_from_the_real_start() {
        // No rows between the fixes this time: the device was paused.
        let mut track = ride(0.0, 170, 0.0, 0.0, 8.0);
        track.extend(ride(170.0, 60, 20_000.0, 0.0, 8.0));
        track.extend(ride(1730.0, 600, 9_160.0, 0.0, 8.0));
        assert_eq!(rejected(&track, BIKE), (170..230).collect::<Vec<_>>());
    }

    #[test]
    fn a_vehicle_with_traffic_lights_is_under_the_share_kept() {
        // An hour's ride, then 100 min of a car whose stretches stop short of
        // the limit: every one looks like a fake, together they outweigh the
        // ride. Too much would go, so nothing does.
        let mut track = ride(0.0, 3600, 0.0, 0.0, 8.0);
        let (mut t, mut north) = (3600.0, 28_800.0);
        while t < 9600.0 {
            track.extend(ride(t, 150, north, 0.0, 30.0));
            north += 30.0 * 150.0;
            track.extend(ride(t + 150.0, 10, north, 0.0, 0.0));
            t += 160.0;
        }
        assert!(rejected(&track, BIKE).is_empty());
    }

    #[test]
    fn a_descent_just_before_a_fast_stretch_explains_it() {
        // 25 s down at 18 m/s, then 10 s on the level at 21 m/s on its speed.
        let mut track = ride(0.0, 600, 0.0, 0.0, 8.0);
        track.extend(ride(600.0, 25, 4_800.0, 0.0, 18.0));
        track.extend(ride(625.0, 10, 5_250.0, 0.0, 21.0));
        track.extend(ride(635.0, 600, 5_460.0, 0.0, 8.0));
        let alt = |i: usize| Some(200.0 - 0.9 * (i.clamp(600, 625) - 600) as f64);
        assert!(rejected_alt(&track, BIKE, alt).is_empty());
    }

    #[test]
    fn a_descent_from_a_peak_inside_the_stretch_explains_it() {
        // 15 s at 21 m/s: 6 m up, then 8 m down. The ends are 2 m apart.
        let mut track = ride(0.0, 600, 0.0, 0.0, 8.0);
        track.extend(ride(600.0, 15, 4_800.0, 0.0, 21.0));
        track.extend(ride(615.0, 600, 5_115.0, 0.0, 8.0));
        let alt = |i: usize| {
            Some(match i {
                600..=604 => 100.0 + 1.5 * (i - 600) as f64,
                605..=614 => 106.0 - 0.8 * (i - 604) as f64,
                n if n > 614 => 98.0,
                _ => 100.0,
            })
        };
        assert!(rejected_alt(&track, BIKE, alt).is_empty());
    }

    #[test]
    fn a_position_held_with_some_drift_is_still_motionless() {
        // The spoofer's point drifts 40 m around: it still must not outweigh
        // the real ends, whose 340 s are less than its six minutes.
        let mut track = ride(0.0, 170, 0.0, 0.0, 8.0);
        track.extend((0..360).map(|k| (170.0 + k as f64, Some((15_000.0 + (k % 2) as f64 * 40.0, 15_000.0)))));
        track.extend(ride(530.0, 170, 1_360.0, 0.0, 8.0));
        assert!(rejected(&track, BIKE).is_empty());
    }

    #[test]
    fn a_long_fast_stretch_is_a_vehicle_not_a_fake() {
        // 40 min in a car on the flat after an hour's ride: nothing goes.
        let mut track = ride(0.0, 3600, 0.0, 0.0, 8.0);
        track.extend(ride(3600.0, 2400, 28_800.0, 0.0, 30.0));
        assert!(rejected(&track, BIKE).is_empty());
    }

    #[test]
    fn a_motionless_fake_does_not_outweigh_short_real_ends() {
        // 170 s real, 6 min held 15 km off, 170 s real: the real ends must not
        // go, and the long fake has no case against it either — left whole.
        let mut track = ride(0.0, 170, 0.0, 0.0, 8.0);
        track.extend(ride(170.0, 360, 15_000.0, 15_000.0, 0.0));
        track.extend(ride(530.0, 170, 1_360.0, 0.0, 8.0));
        assert!(rejected(&track, BIKE).is_empty());
        // A real cafe stop, reachable from both sides, stays in the chain.
        let mut cafe = ride(0.0, 170, 0.0, 0.0, 8.0);
        cafe.extend(ride(170.0, 360, 1_360.0, 0.0, 0.0));
        cafe.extend(ride(530.0, 170, 1_360.0, 0.0, 8.0));
        cafe[600].1 = Some((5_000.0, 3_000.0));
        assert_eq!(rejected(&cafe, BIKE), vec![600]);
    }

    #[test]
    fn a_dip_is_a_descent_though_its_ends_are_level() {
        // 12 s at 25 m/s down, 8 s at 21 m/s back up: level ends, a real dip.
        let mut track = ride(0.0, 600, 0.0, 0.0, 8.0);
        track.extend(ride(600.0, 12, 4_800.0, 0.0, 25.0));
        track.extend(ride(612.0, 8, 5_100.0, 0.0, 21.0));
        track.extend(ride(620.0, 600, 5_268.0, 0.0, 8.0));
        let alt = |i: usize| {
            Some(match i {
                600..=611 => 200.0 - 2.5 * (i - 600) as f64,
                612..=619 => 170.0 + 3.75 * (i - 612) as f64,
                _ => 200.0,
            })
        };
        assert!(rejected_alt(&track, BIKE, alt).is_empty());
    }

    #[test]
    fn a_gentle_descent_keeps_a_fast_stretch() {
        // 73 km/h for 15 s at 2.5 %: a tailwind's worth of help.
        let mut track = ride(0.0, 600, 0.0, 0.0, 8.0);
        track.extend(ride(600.0, 15, 4_800.0, 0.0, 20.3));
        track.extend(ride(615.0, 600, 5_105.0, 0.0, 8.0));
        let alt = |i: usize| Some(200.0 - 0.5 * (i.clamp(600, 615) - 600) as f64);
        assert!(rejected_alt(&track, BIKE, alt).is_empty());
    }

    #[test]
    fn a_constant_altitude_is_no_altitude() {
        // A GPX's <ele>0</ele> on every point can't tell a sprint from a fake.
        let mut track = ride(0.0, 600, 0.0, 0.0, 8.0);
        track.extend(ride(600.0, 15, 4_800.0, 0.0, 21.0));
        track.extend(ride(615.0, 600, 5_115.0, 0.0, 8.0));
        assert!(rejected_alt(&track, BIKE, |_| Some(0.0)).is_empty());
        // The same stretch under a working barometer goes.
        assert!(!rejected(&track, BIKE).is_empty());
    }

    #[test]
    fn a_fake_track_without_altitude_is_given_the_benefit_of_the_doubt() {
        // No barometer (a GPX without <ele>): a bike's fast loop can't be told
        // from a descent, so it stays — on foot it still goes.
        let mut track = ride(0.0, 300, 0.0, 0.0, 7.0);
        let n0 = 7.0 * 299.0;
        track.extend(fake_loop(299.0, n0));
        track.extend(ride(360.0, 300, n0 + 7.0 * 61.0, 0.0, 7.0));
        assert!(rejected_alt(&track, BIKE, |_| None).is_empty());
        assert!(!rejected_alt(&track, FOOT, |_| None).is_empty());
    }

    #[test]
    fn a_fake_track_at_100_kmh_on_the_flat_is_dropped() {
        // A minute of fixes at 28 m/s circling, between two honest stretches.
        let mut track = ride(0.0, 300, 0.0, 0.0, 7.0);
        let n0 = 7.0 * 299.0;
        track.extend(fake_loop(299.0, n0));
        track.extend(ride(360.0, 300, n0 + 7.0 * 61.0, 0.0, 7.0));
        assert_eq!(rejected(&track, BIKE), (300..360).collect::<Vec<_>>());
        // On foot there is no descent to explain it, whatever the altitude.
        assert_eq!(rejected_alt(&track, FOOT, |_| None), (300..360).collect::<Vec<_>>());
    }

    #[test]
    fn a_track_that_starts_on_a_fake_position_keeps_the_real_ride() {
        // The first 30 s sit 15 km away, then the real ride begins.
        let mut track = ride(0.0, 30, 15_000.0, 15_000.0, 0.0);
        track.extend(ride(30.0, 900, 0.0, 0.0, 8.0));
        assert_eq!(rejected(&track, BIKE), (0..30).collect::<Vec<_>>());
    }

    #[test]
    fn a_long_piece_at_an_end_is_never_dropped() {
        // 2 km apart in a second at 60 %: one side is wrong, nothing says
        // which — the track stays whole rather than lose 40 % of a ride.
        let mut track = ride(0.0, 3000, 0.0, 0.0, 8.0);
        for p in &mut track[1800..] {
            p.1 = p.1.map(|(n, e)| (n, e + 2_000.0));
        }
        assert!(rejected(&track, BIKE).is_empty());
    }

    #[test]
    fn a_fake_majority_is_not_kept_over_the_real_ends() {
        // A spoofer holds the position 15 km away for 57 % of the ride, in
        // the middle: keeping it would drop the real start and finish.
        let mut track = ride(0.0, 650, 0.0, 0.0, 8.0);
        track.extend(ride(650.0, 1700, 15_000.0, 15_000.0, 0.0));
        track.extend(ride(2350.0, 650, 8.0 * 2350.0 * 0.3, 0.0, 8.0));
        assert!(rejected(&track, BIKE).is_empty());
    }

    #[test]
    fn a_reachable_gap_bridges_the_pieces() {
        // 3 min without a fix, then the ride continues 1.2 km on (6.7 m/s).
        let mut track = ride(0.0, 300, 0.0, 0.0, 8.0);
        track.extend((300..480).map(|t| (t as f64, None)));
        track.extend(ride(480.0, 300, 8.0 * 299.0 + 1_200.0, 0.0, 8.0));
        assert!(rejected(&track, BIKE).is_empty());
    }

    #[test]
    fn sparse_smart_recording_is_not_a_gap() {
        // A fix every 7 s at 9 m/s: 63 m steps, well inside a gap's allowance.
        let track: Vec<Pt> = (0..400).map(|k| (7.0 * k as f64, Some((63.0 * k as f64, 0.0)))).collect();
        assert!(rejected(&track, BIKE).is_empty());
    }

    #[test]
    fn a_track_of_nothing_but_jumps_is_left_alone() {
        // Every fix 5 km from the last: no piece is long enough to trust.
        let track: Vec<Pt> = (0..40).map(|k| (k as f64, Some(((k % 2) as f64 * 5_000.0, 0.0)))).collect();
        assert!(rejected(&track, BIKE).is_empty());
    }

    #[test]
    fn a_fast_vehicle_track_is_left_alone() {
        // A drive filed as a ride: mostly 30 m/s on the flat. Rejecting most
        // of it would leave nothing, so the filter steps aside.
        let mut track = ride(0.0, 60, 0.0, 0.0, 8.0);
        track.extend(ride(60.0, 600, 480.0, 0.0, 30.0));
        assert!(rejected(&track, BIKE).is_empty());
    }

    #[test]
    fn points_without_time_or_position_and_backward_time_are_not_judged() {
        let mut track = ride(0.0, 200, 0.0, 0.0, 8.0);
        track[100].1 = Some((5_000.0, 3_000.0));
        let ll: Vec<(f64, f64)> = track.iter().map(|p| at(p.1.unwrap().0, p.1.unwrap().1)).collect();
        let mut t: Vec<Option<f64>> = track.iter().map(|p| Some(p.0)).collect();
        let lat: Vec<Option<f64>> = ll.iter().map(|p| Some(p.0)).collect();
        let lon: Vec<Option<f64>> = ll.iter().map(|p| Some(p.1)).collect();
        let alt = vec![None; track.len()];
        t[50] = None;
        assert_eq!(rejected_fixes(&t, &lat, &lon, &alt, BIKE), vec![100]);
        assert!(rejected_fixes(&t[..4], &lat[..4], &lon[..4], &alt[..4], BIKE).is_empty());
        // Two files joined out of order: the clock jumps back, not the rider.
        let mut back = ride(1000.0, 300, 0.0, 0.0, 8.0);
        back.extend(ride(0.0, 300, 8.0 * 300.0, 0.0, 8.0));
        assert!(rejected(&back, BIKE).is_empty());
    }

    #[test]
    fn runs_use_the_foot_limit() {
        // 15 m/s for 10 s: a bike's pace, impossible on foot.
        let mut track = ride(0.0, 300, 0.0, 0.0, 3.0);
        track.extend(ride(300.0, 10, 900.0, 0.0, 15.0));
        track.extend(ride(310.0, 300, 1_050.0, 0.0, 3.0));
        assert!(rejected(&track, BIKE).is_empty());
        // Fix 300 is where the runner was; the fixes after it are not.
        assert_eq!(rejected(&track, FOOT), (301..310).collect::<Vec<_>>());
    }

    #[test]
    fn each_sport_gets_its_limits() {
        for sport in [SportType::Ride, SportType::MountainBike, SportType::Triathlon] {
            assert_eq!(sport_limits(&sport), Some(BIKE));
        }
        for sport in [
            SportType::Run,
            SportType::TrailRun,
            SportType::Walk,
            SportType::Hike,
            SportType::Mountaineering,
            SportType::OpenWater,
            SportType::Paddle,
        ] {
            assert_eq!(sport_limits(&sport), Some(FOOT));
        }
        for sport in [
            SportType::Ski,
            SportType::Snowboard,
            SportType::Sailing,
            SportType::Swim,
            SportType::IndoorRide,
            SportType::VirtualRide,
            SportType::VirtualRun,
            SportType::Other,
        ] {
            assert_eq!(sport_limits(&sport), None);
        }
    }

    fn trackpoints(track: &[Pt], speed: f64) -> Vec<TrackPoint> {
        track
            .iter()
            .map(|&(t, pos)| {
                let ll = pos.map(|(n, e)| at(n, e));
                TrackPoint {
                    activity_id: "a".into(),
                    t: chrono::DateTime::from_timestamp(1_700_000_000 + t as i64, 0)
                        .map(|d| d.to_rfc3339()),
                    lat: ll.map(|p| p.0),
                    lon: ll.map(|p| p.1),
                    altitude_m: Some(100.0),
                    speed_mps: Some(speed),
                    hr: None,
                    cadence: None,
                    power_w: None,
                    temperature_c: None,
                    vertical_oscillation_mm: None,
                    stance_time_ms: None,
                    stance_time_percent: None,
                    step_length_mm: None,
                    grade_percent: None,
                    left_right_balance: None,
                    left_torque_effectiveness: None,
                    right_torque_effectiveness: None,
                    left_pedal_smoothness: None,
                    right_pedal_smoothness: None,
                }
            })
            .collect()
    }

    /// A ride with a teleported fix at 100.
    fn spiked() -> Vec<Pt> {
        let mut track = ride(0.0, 200, 0.0, 0.0, 8.0);
        track[100].1 = Some((5_000.0, 3_000.0));
        track
    }

    #[test]
    fn cleaning_drops_the_position_and_the_gps_speed() {
        let mut tps = trackpoints(&spiked(), 8.0);
        tps[150].speed_mps = Some(65.0); // a bogus GPS speed on a good fix
        let spike = (tps[100].lat, tps[100].lon);
        let c = clean_trackpoints(&mut tps, &SportType::Ride, false);
        assert_eq!((c.fixes, c.speeds), (vec![100], vec![100, 150]));
        // What went is kept, value by value.
        assert_eq!(
            c.dropped,
            vec![
                DroppedGps { index: 100, lat: spike.0, lon: spike.1, speed_mps: Some(8.0) },
                DroppedGps { index: 150, lat: None, lon: None, speed_mps: Some(65.0) },
            ]
        );
        assert!(tps[100].lat.is_none() && tps[100].lon.is_none() && tps[100].speed_mps.is_none());
        assert!(tps[150].lat.is_some() && tps[150].speed_mps.is_none());
        assert_eq!(tps[99].speed_mps, Some(8.0));
    }

    #[test]
    fn a_lagging_device_speed_after_a_fake_stretch_is_dropped() {
        // After the teleported fix the device still reads 89 → 69 km/h for
        // three seconds on fixes that move at 8 m/s again.
        let mut tps = trackpoints(&spiked(), 8.0);
        for (i, v) in [(101, 24.7), (102, 21.0), (103, 19.2)] {
            tps[i].speed_mps = Some(v);
        }
        tps[104].speed_mps = Some(11.0); // a sprint's worth over 8 m/s stays
        let c = clean_trackpoints(&mut tps, &SportType::Ride, false);
        assert_eq!(c.speeds, vec![100, 101, 102, 103]);
        assert_eq!(tps[104].speed_mps, Some(11.0));
    }

    #[test]
    fn a_speed_without_a_fix_goes_on_a_cleaned_track_only() {
        let mut track = ride(0.0, 200, 0.0, 0.0, 8.0);
        track[150].1 = None; // jammed for a second, the device still wrote a speed
        let mut untouched = trackpoints(&track, 8.0);
        assert!(clean_trackpoints(&mut untouched, &SportType::Ride, false).is_empty());
        assert_eq!(untouched[150].speed_mps, Some(8.0));

        track[100].1 = Some((5_000.0, 3_000.0));
        let mut cleaned = trackpoints(&track, 8.0);
        let c = clean_trackpoints(&mut cleaned, &SportType::Ride, false);
        assert_eq!(c.speeds, vec![100, 150]);
    }

    #[test]
    fn a_speed_on_a_lone_fix_is_held_to_the_top_speed() {
        // Smart recording, a fix every 7 s at 9 m/s: no other fix within the
        // window to compare with, so only a speed past the top one goes.
        let mut track: Vec<Pt> = (0..300).map(|k| (7.0 * k as f64, Some((63.0 * k as f64, 0.0)))).collect();
        track[100].1 = Some((63.0 * 100.0, 20_000.0)); // makes it a cleaned track
        let mut tps = trackpoints(&track, 9.0);
        tps[200].speed_mps = Some(30.0);
        tps[201].speed_mps = Some(20.0);
        let c = clean_trackpoints(&mut tps, &SportType::Ride, false);
        assert_eq!(c.speeds, vec![100, 200]);
        assert_eq!(tps[201].speed_mps, Some(20.0));
    }

    #[test]
    fn an_untouched_track_keeps_its_speeds() {
        // No fix dropped: a 26 m/s reading stays — a descent the device saw.
        let mut tps = trackpoints(&ride(0.0, 200, 0.0, 0.0, 8.0), 8.0);
        tps[50].speed_mps = Some(26.0);
        assert!(clean_trackpoints(&mut tps, &SportType::Ride, false).is_empty());
        assert_eq!(tps[50].speed_mps, Some(26.0));
    }

    #[test]
    fn a_turnaround_keeps_its_speed() {
        // Out and back at 5 m/s over fixes 150..=160: the ends of the window
        // meet, the rider moved. (The spike at 100 makes it a cleaned track.)
        let mut track = ride(0.0, 150, 0.0, 0.0, 8.0);
        track[100].1 = Some((5_000.0, 3_000.0));
        for k in 150..=160 {
            let out = 5 - (k as i64 - 155).abs();
            track.push((k as f64, Some((1_200.0 + 5.0 * out as f64, 0.0))));
        }
        track.extend(ride(161.0, 200, 1_208.0, 0.0, 8.0));
        let mut tps = trackpoints(&track, 8.0);
        tps[155].speed_mps = Some(5.0);
        let c = clean_trackpoints(&mut tps, &SportType::Ride, false);
        assert!(!c.speeds.contains(&155), "{:?}", c.speeds);
    }

    #[test]
    fn a_speed_sensor_keeps_the_speed_column() {
        let mut tps = trackpoints(&spiked(), 8.0);
        let c = clean_trackpoints(&mut tps, &SportType::Ride, true);
        assert_eq!((c.fixes, c.speeds), (vec![100], vec![]));
        assert_eq!(c.dropped[0].speed_mps, None);
        assert_eq!(tps[100].speed_mps, Some(8.0));
    }

    #[test]
    fn sports_without_limits_are_untouched() {
        for sport in [SportType::Ski, SportType::Sailing, SportType::VirtualRide, SportType::Other] {
            let mut tps = trackpoints(&spiked(), 40.0);
            assert!(clean_trackpoints(&mut tps, &sport, false).is_empty());
            assert!(tps[100].lat.is_some());
        }
    }

    #[test]
    fn gaps_inside_the_track_are_found_like_the_map_finds_them() {
        // Mirrors src/lib/gpsGaps.test.ts: 100 fixes, 100 s without, 100 on.
        let t: Vec<Option<f64>> = (0..300).map(|i| Some(i as f64)).collect();
        let fix = |i: usize| !(100..200).contains(&i);
        let lat: Vec<Option<f64>> = (0..300).map(|i| fix(i).then_some(55.0)).collect();
        let lon = lat.clone();
        assert_eq!(fixes_after_gaps(&t, &lat, &lon), vec![200]);
        // Shorter than the minimum, at the very start, or a pause: no break.
        let short: Vec<Option<f64>> = (0..300).map(|i| (!(100..120).contains(&i)).then_some(55.0)).collect();
        assert!(fixes_after_gaps(&t, &short, &short).is_empty());
        let late: Vec<Option<f64>> = (0..300).map(|i| (i >= 100).then_some(55.0)).collect();
        assert!(fixes_after_gaps(&t, &late, &late).is_empty());
        let paused = [Some(0.0), Some(1.0), Some(600.0)];
        let all = [Some(55.0), Some(55.0), Some(55.0)];
        assert!(fixes_after_gaps(&paused, &all, &all).is_empty());
        // A pause with a lone row without a fix beside it: still a pause.
        let beside = [Some(0.0), Some(1.0), Some(2.0), Some(2304.0), Some(2305.0)];
        let fixes = [Some(55.0), Some(55.0), None, Some(55.0), Some(55.0)];
        assert!(fixes_after_gaps(&beside, &fixes, &fixes).is_empty());
        // Real jamming on one side of the pause counts, the pause does not.
        let t: Vec<Option<f64>> = (0..60).map(|k| Some(k as f64)).chain([Some(900.0)]).collect();
        let lat: Vec<Option<f64>> = (0..61).map(|k| (k == 0 || k == 60).then_some(55.0)).collect();
        assert_eq!(fixes_after_gaps(&t, &lat, &lat), vec![60]);
    }

    #[test]
    fn speed_sensors_are_recognised() {
        let s = |kind: &str| SensorInfo {
            serial: "1".into(),
            device_type: Some(kind.into()),
            manufacturer: None,
            product: None,
        };
        assert!(has_speed_sensor(&[s("heart_rate"), s("bike_speed")]));
        assert!(has_speed_sensor(&[s("bike_speed_cadence")]));
        assert!(has_speed_sensor(&[s("stride_speed_distance")]));
        assert!(has_speed_sensor(&[s("footpod")]));
        assert!(!has_speed_sensor(&[s("heart_rate"), s("bike_power")]));
        assert!(!has_speed_sensor(&[]));
    }
}
