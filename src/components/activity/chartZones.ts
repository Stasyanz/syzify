import type { TimeInZone } from "../../lib/types";

/** A bpm range and the color HR bars falling into it are painted with. */
export interface ZoneRange {
  from: number;
  to: number;
  color: string;
}

/** Design-system HRChart palette, cool → hot (Syzify Design System,
 * redesign/dir-trailhead-app.jsx). */
export const HR_ZONE_COLORS = [
  "#4a9e5c", // recovery
  "#c9941a", // easy
  "#e07c3a", // aerobic
  "#c0392b", // threshold
  "#8e1a0e", // maximum
];

/** Design fallback for a value outside every range. */
export const HR_FALLBACK_COLOR = "#d95f2b";

/** The design's default zones — used when the activity carries no zone
 * boundaries (GPX/TCX imports; FIT stores per-user boundaries). */
export const DEFAULT_HR_RANGES: ZoneRange[] = [
  { from: 0, to: 100, color: HR_ZONE_COLORS[0] },
  { from: 100, to: 120, color: HR_ZONE_COLORS[1] },
  { from: 120, to: 145, color: HR_ZONE_COLORS[2] },
  { from: 145, to: 175, color: HR_ZONE_COLORS[3] },
  { from: 175, to: Infinity, color: HR_ZONE_COLORS[4] },
];

/** Which stored bucket index is zone 1 — 1 when the writer has Garmin's
 * below-Z1 bucket at index 0, 0 when it starts at Z1. Garmin's below-Z1
 * bucket makes MORE buckets than there are zones (HR 7 = 5 + below +
 * above, power 8 = 7 + below) and puts zone k at index k, so a top index
 * at or beyond the zone count is the tell; a writer with exactly one
 * bucket per zone tops out at N − 1. No such device in the vault, but a
 * FIT can come from anything, and a one-step shift either way is silent.
 * Judged by the top INDEX, not the bucket count, so a set with a missing
 * bucket does not shift. One function for the chart bands and the Time
 * in Zones card: the bar and the band of one range must agree. */
export function zoneIndexOffset(indices: Iterable<number>, zoneCount: number): 0 | 1 {
  let top = -1;
  for (const i of indices) if (i > top) top = i;
  return top >= zoneCount ? 1 : 0;
}

/** Zone ranges from the device's own time_in_zone buckets, colored by ZONE
 * INDEX: bucket k is zone k and wears palette[k − 1]. Garmin writes one
 * bucket per index — 0 is "below zone 1" (its ceiling is the Z1 floor; for
 * power it is 0 W and folds away), 1…N are the zones with their ceilings,
 * and the top bucket is open-ended by definition: HR's "above max" bucket
 * carries no boundary, power's zone 7 carries a SENTINEL ceiling (3393 /
 * 5540 W). Reading that sentinel as a real boundary once made 8 ranges out
 * of a 7-color palette and painted every bar one zone too cool; anchoring
 * the palette to the top end did the same to HR (Z1 and Z2 both green, the
 * "maximum" color reserved for HR above the configured max). So: below-Z1
 * shares the coolest color, everything above the last bucket keeps the
 * hottest, and the top INDEX's stored ceiling is ignored. Rows are keyed
 * by index first — the parser keeps one copy per lap AND the session, so
 * positional "last row" would let a lap's copy of the sentinel back in as
 * a boundary. Degenerate middle boundaries (null, non-increasing) fold
 * into the next zone. Null unless at least three ranges in two colors
 * survive — two buckets carry a single boundary, not a zone system, and
 * must not displace a proper fallback (Coggan-from-FTP, the fixed bands).
 * `scale` converts recorded units to display units (speed: m/s → km/h). */
function deviceZoneRanges(
  zones: TimeInZone[],
  zoneType: string,
  palette: string[],
  scale = 1,
): ZoneRange[] | null {
  // First usable boundary per zone index (lap copies repeat the session's).
  const byIndex = new Map<number, number | null>();
  for (const z of zones) {
    if (z.zone_type !== zoneType) continue;
    const b = z.zone_high_boundary;
    const usable = b != null && isFinite(b);
    if (!byIndex.has(z.zone_index) || (usable && byIndex.get(z.zone_index) == null)) {
      byIndex.set(z.zone_index, usable ? b : null);
    }
  }
  const indices = [...byIndex.keys()].sort((a, b) => a - b);
  const top = indices[indices.length - 1];
  const offset = zoneIndexOffset(indices, palette.length);
  const out: ZoneRange[] = [];
  let from = 0;
  for (const index of indices) {
    const b = byIndex.get(index);
    const to = index === top ? Infinity : b == null ? NaN : b * scale;
    // A missing/non-increasing ceiling folds its span into the NEXT zone,
    // which then wears its own (hotter) color over the merged span.
    if (!(to > from)) continue;
    const color = palette[Math.min(palette.length - 1, Math.max(0, index - offset))];
    out.push({ from, to, color });
    from = to;
  }
  // Note for future consumers: the range count is NOT the palette length
  // (7 HR ranges over 5 colors) — index ranges by value (zoneColorFor),
  // never by position.
  const distinct = new Set(out.map((r) => r.color)).size;
  return out.length >= 3 && distinct >= 2 ? out : null;
}

/** Contiguous ranges over [0, ∞) from strictly-increasing boundaries, the
 * palette anchored to the TOP end — for the FIXED band lists (Coggan
 * %-of-FTP, ride cadence/speed, run cadence), whose bound counts match
 * their palettes. Device buckets go through deviceZoneRanges instead. */
function rangesFromBoundaries(
  bounds: number[],
  palette: string[] = HR_ZONE_COLORS,
): ZoneRange[] {
  const edges = [0, ...bounds, Infinity];
  const rangeCount = edges.length - 1;
  const shift = palette.length - rangeCount;
  return Array.from({ length: rangeCount }, (_, i) => ({
    from: edges[i],
    to: edges[i + 1],
    color: palette[Math.min(palette.length - 1, Math.max(0, i + shift))],
  }));
}

/** HR zone ranges for an activity — its FIT boundaries, else the design
 * defaults (HR thresholds are universal enough for a fallback). */
export function hrZoneRanges(zones: TimeInZone[]): ZoneRange[] {
  return deviceZoneRanges(zones, "hr", HR_ZONE_COLORS) ?? DEFAULT_HR_RANGES;
}

/** Coggan power zone ceilings as fractions of FTP — the de-facto standard
 * (Z1 recovery <55% … Z7 neuromuscular >150%). Used when the device wrote
 * time-in-power-zone without the boundary array (Edge units do exactly
 * that for power while still writing HR boundaries). */
const COGGAN_FTP_FACTORS = [0.55, 0.75, 0.9, 1.05, 1.2, 1.5];

/** Coggan's Z7 is open-ended, which painted a 500 W surge and a 1000 W
 * max sprint the same purple — an extra FTP-relative band separates all-out
 * sprints. 3×FTP ≈ where trained riders' short max efforts live. */
const SPRINT_FTP_FACTOR = 3.0;
const SPRINT_COLOR = "#7a4a2b"; // brown — earthy top, visible in both themes

/** Power palette, one hue per Coggan zone (recovery → neuromuscular). The
 * 5-color HR palette left Z1–Z3 sharing green and Z6/Z7 both dark red —
 * everything above ~1.2×FTP read as one color. House/earthy hues (the
 * trailhead look — no purple); the sprint band above Z7 tops out brown.
 * Neighbors differ in LIGHTNESS, not just hue (the a11y lesson from the
 * elevation bands — hue-only steps vanish under red-green colorblindness). */
export const POWER_ZONE_COLORS = [
  "#86b273", // Z1 recovery (light green, from the elevation valley band)
  "#4a9e5c", // Z2 endurance (green, same as HR recovery)
  "#0e7490", // Z3 tempo (teal)
  "#c9941a", // Z4 threshold (gold)
  "#e07c3a", // Z5 VO2max (orange)
  "#c0392b", // Z6 anaerobic (red)
  "#8e1a0e", // Z7 neuromuscular (dark red)
];

/** Power zone ranges — the device's FIT boundaries first; else Coggan
 * %-of-FTP zones when the file carried an FTP (threshold_power). Power zones
 * hang off personal FTP, so with neither the chart stays a plain line. */
export function powerZoneRanges(
  zones: TimeInZone[],
  ftpW?: number | null,
): ZoneRange[] | null {
  const device = deviceZoneRanges(zones, "power", POWER_ZONE_COLORS);
  if (device) return device;
  if (ftpW != null && isFinite(ftpW) && ftpW > 0) {
    // The sprint band rides ON TOP of the 7 Coggan zones — appended
    // explicitly (boundary + color) rather than folded into the palette,
    // so the device-boundary path above keeps its exact 7-color mapping.
    return rangesFromBoundaries(
      [...COGGAN_FTP_FACTORS, SPRINT_FTP_FACTOR].map((f) => Math.round(f * ftpW)),
      [...POWER_ZONE_COLORS, SPRINT_COLOR],
    );
  }
  return null;
}

/** Cadence palette, low → high. Unlike HR/power, HIGH cadence is GOOD, so
 * the scale runs red (plodding) → orange → green → teal → brown. The brown
 * top matches the power sprint band — house code for "off the scale"
 * (purple sat outside the earthy palette). */
export const CADENCE_ZONE_COLORS = [
  "#c0392b", // < Z1: overstriding / grinding
  "#e07c3a",
  "#4a9e5c", // the healthy band (163–174 spm run, 75–90 rpm ride)
  "#0e7490",
  "#7a4a2b", // elite turnover / spinning
];

/** Garmin's universal running-cadence thresholds, in full steps/min. */
const RUN_CADENCE_SPM_BOUNDS = [151, 163, 174, 185];

/** Fixed ride-cadence thresholds, in rpm — a product decision like the ride
 * speed bounds: no authority defines cycling cadence zones (Strava/Garmin
 * both draw a plain line for bikes), but the physiology is well known:
 * <60 grinding, 75–90 the optimal band, 105+ spinning/sprint. */
const RIDE_CADENCE_RPM_BOUNDS = [60, 75, 90, 105];

/**
 * Cadence zone ranges: the device's FIT boundaries when it recorded them
 * (any sport, same priority as power), else Garmin's universal RUNNING
 * thresholds for runs or the fixed rpm bands for rides; other sports stay
 * a line. Devices disagree on units: FIT run cadence is usually single-leg
 * rpm (~75–95) while the thresholds are full spm (~150–190) — when the
 * data's median sits below 120 the samples are per-leg and the thresholds
 * halve. Ride cadence is crank rpm, no such ambiguity.
 */
export function cadenceZoneRanges(
  zones: TimeInZone[],
  sport: string,
  values: number[],
): ZoneRange[] | null {
  const device = deviceZoneRanges(zones, "cadence", CADENCE_ZONE_COLORS);
  if (device) return device;

  if (sport === "ride") {
    return rangesFromBoundaries(RIDE_CADENCE_RPM_BOUNDS, CADENCE_ZONE_COLORS);
  }
  if (sport !== "run") return null;
  const nonZero = values.filter((v) => v > 0).sort((a, b) => a - b);
  if (nonZero.length === 0) return null;
  const median = nonZero[nonZero.length >> 1];
  const scale = median < 120 ? 0.5 : 1;
  return rangesFromBoundaries(
    RUN_CADENCE_SPM_BOUNDS.map((t) => t * scale),
    CADENCE_ZONE_COLORS,
  );
}

/** The color for one bar's bpm value. */
export function zoneColorFor(v: number, ranges: ZoneRange[]): string {
  return ranges.find((r) => v >= r.from && v < r.to)?.color ?? HR_FALLBACK_COLOR;
}

/** Design y-range for the HR bars: ±10 bpm of the data, rounded out to
 * tens, clamped to the plausible 40..220 window. */
export function hrVisRange(dMin: number, dMax: number): [number, number] {
  const lo = Math.max(40, Math.floor((dMin - 10) / 10) * 10);
  const hi = Math.min(220, Math.ceil((dMax + 10) / 10) * 10);
  return [lo, hi];
}

/** Same shape for power bars: ±10 W rounded to tens, floored at 0 — watts
 * have no universal ceiling, so no upper clamp. */
export function powerVisRange(dMin: number, dMax: number): [number, number] {
  const lo = Math.max(0, Math.floor((dMin - 10) / 10) * 10);
  const hi = Math.ceil((dMax + 10) / 10) * 10;
  return [lo, hi];
}

/** Speed palette — the HR palette reversed: unlike HR (where red = strain),
 * HIGH speed is GOOD, so the top range reads green and the crawl reads
 * dark red. Same hues keep the chart family consistent. */
export const SPEED_ZONE_COLORS = [...HR_ZONE_COLORS].reverse();

/** Fixed ride-speed thresholds, in km/h — a product decision, not a
 * standard: no authority defines universal speed zones (terrain and wind
 * dominate), but consistent colors across rides beat no colors. */
const RIDE_SPEED_BOUNDS_KMH = [15, 25, 30, 35];

/**
 * Speed zone ranges, in the chart's DISPLAY unit. FIT speed boundaries
 * (recorded in m/s) take priority for any sport; without them, rides get
 * the fixed km/h thresholds. `mpsToUnit` is the same m/s → display factor
 * the speed series itself is converted with (3.6 for km/h, 2.24 for mph),
 * so the ranges always match the plotted numbers. Other sports stay a line.
 */
export function speedZoneRanges(
  zones: TimeInZone[],
  sport: string,
  mpsToUnit: number,
): ZoneRange[] | null {
  const device = deviceZoneRanges(zones, "speed", SPEED_ZONE_COLORS, mpsToUnit);
  if (device) return device;
  if (sport !== "ride") return null;
  return rangesFromBoundaries(
    RIDE_SPEED_BOUNDS_KMH.map((kmh) => (kmh / 3.6) * mpsToUnit),
    SPEED_ZONE_COLORS,
  );
}

/** Y-range for speed bars: gentler than the tens-based ranges — ±2 units
 * rounded out to fives (a ±10 pad would dwarf a 0–40 km/h ride). */
export function speedVisRange(dMin: number, dMax: number): [number, number] {
  const lo = Math.max(0, Math.floor((dMin - 2) / 5) * 5);
  const hi = Math.ceil((dMax + 2) / 5) * 5;
  return [lo, hi];
}

/** Climbs at or above this grade count as climbs: the line leaves its flat
 * teal and the profile gets a fill under it (the cycling-app look where a
 * climb's steepness shows as a colored band). A product constant like the
 * ride speed bounds: 1.5% is where a road stops reading as flat to the
 * legs (3% left the last 400 m of a climb's summit unpainted while the
 * profile plainly still went up; 2% left a 900 m rise from the sea to
 * 20 m — 1.9% steady — teal). Below it, and on every descent, the line
 * is teal and the altitude fill stands alone — the chart colors EFFORT,
 * and effort lives uphill. */
export const GRADE_FILL_MIN_PCT = 1.5;

/** A band shorter than this much road is not a band: the vote can flip
 * for a few samples where its histogram sits near even, and painting a
 * 10 m sliver says nothing about the climb. Measured between the run's
 * first and last sample with a distance. On a mountain ride this alone
 * took the stripe count from 66 to 49 without losing a climb. */
export const GRADE_MIN_BAND_M = 100;

/** The climb-or-not vote counts a sample as climbing from three quarters
 * of the fill threshold, not from the threshold itself: a 2.5 % rise
 * jitters across 2 % on half its samples, and a vote held at the
 * threshold flips like a coin along it, chopping one climb into a barcode
 * (a 780 m 2.3 % rise came out as six slivers). The paint threshold still
 * holds — a run is filled only if its own average reaches
 * GRADE_FILL_MIN_PCT — and the lower bar only decides where runs start
 * and end. Lower still (1 %) lets runs swallow their false-flat approaches
 * and dilutes the average below the threshold: real climbs vanish. */
export const GRADE_VOTE_MIN_PCT = GRADE_FILL_MIN_PCT * 0.75;

/** Grade category ceilings in percent — the first is the climb threshold
 * above, so the line's first warm step and the fill's first band start at
 * the same sample (one rule for both, the chart-and-card invariant); 5%
 * splits the gentle approach from the real drag (a 2% run-in and a 7%
 * grind should not wear one color); 16%+ is wall territory. */
export const GRADE_BOUNDS_PCT = [GRADE_FILL_MIN_PCT, 5, 8, 12, 16];

/** Grade palette, flat → wall. Index 0 is the elevation line's own teal so
 * flat terrain looks unchanged; the climb steps darken monotonically from
 * gold to wall red (CIE L* ≈ 77 → 70 → 62 → 45 → 31: the two gentle steps
 * 7–8 apart and shifting hue from yellow to orange as well, the hard ones
 * at least 14 apart), so neighbors differ in lightness, not just hue (the
 * standing a11y rule) — and now that the fill paints them as blocks, the
 * rule carries the whole area, not a 2 px line. Honest caveat: to a
 * dichromat the yellow-to-orange hue turn is invisible and the gentle
 * steps shrink to ≈4 of L*, so 2–5, 5–8 and 8–12 % read as one band; six
 * steps cannot fit between 77 and 31 any wider — gold cannot go lighter
 * (1.74:1 on the light card already) and amber cannot go darker without
 * landing on orange. The tooltip's number is the ground truth for the
 * gentle steps (#126). Plain #rrggbb only: the zone bars append an alpha
 * suffix and the fill uses the entries verbatim. */
export const GRADE_COLORS = [
  "#0e7490", // < GRADE_FILL_MIN_PCT (1.5%): flat / descent (the elevation line color)
  "#e5b83f", // 1.5–5%: gentle
  "#e39c3b", // 5–8%: noticeable
  "#e07c3a", // 8–12%: hard
  "#c0392b", // 12–16%: steep
  "#8e1a0e", // 16%+: wall
];

/** Palette index for one grade value (null/NaN → flat). */
export function gradeCategory(pct: number | null): number {
  if (pct == null || !isFinite(pct)) return 0;
  const i = GRADE_BOUNDS_PCT.findIndex((b) => pct < b);
  return i === -1 ? GRADE_BOUNDS_PCT.length : i;
}

/** Distance window for grade smoothing, meters. Raw per-point grade from
 * GPS elevation is noise (±1 m altitude error over ~8 m point spacing is
 * ±12% "grade"); a window this size reads through it while still resolving
 * real pitches. Distance-based, NOT point-count-based — points bunch up
 * exactly where climbs slow the rider down. */
export const GRADE_WINDOW_M = 30;

/** A window that collapsed below this span (standing still, track ends)
 * yields no trustworthy grade — better a gap than a spike. */
const MIN_GRADE_SPAN_M = 5;

/** No road or trail is steeper than this; a grade beyond it is noise — a
 * barometer drifting 5 m during a stop while the GPS wandered 6 m read as
 * 93% on a real ride — and becomes a gap rather than a wall. */
export const GRADE_MAX_ABS_PCT = 40;

/**
 * Smoothed grade (%) per trackpoint from cumulative distance + altitude,
 * both in meters: for each point, the altitude delta across a centered
 * ±window/2 distance span divided by that span. Points missing either
 * input get null, as do windows spanning less than MIN_GRADE_SPAN_M.
 * Assumes distance ascending (cumulative); two pointers keep it O(N).
 */
export function gradeSeries(
  distM: (number | null)[],
  altM: (number | null)[],
  windowM: number = GRADE_WINDOW_M,
): (number | null)[] {
  const n = distM.length;
  const out: (number | null)[] = new Array(n).fill(null);
  // Compact to the points carrying both inputs; indices map back via idx.
  const idx: number[] = [];
  for (let i = 0; i < n; i++) {
    if (distM[i] != null && altM[i] != null) idx.push(i);
  }
  const m = idx.length;
  if (m < 2) return out;

  const half = windowM / 2;
  let lo = 0;
  let hi = 0;
  for (let j = 0; j < m; j++) {
    const d = distM[idx[j]]!;
    while (distM[idx[lo]]! < d - half) lo++;
    while (hi + 1 < m && distM[idx[hi + 1]]! <= d + half) hi++;
    // Sparse recording (Garmin Smart Recording spaces points wider than
    // the window at speed) collapses the window to the point itself —
    // widen to the immediate neighbors: less smoothing, but the sparsity
    // already smoothed the data.
    let wLo = lo;
    let wHi = hi;
    if (wLo === wHi) {
      wLo = Math.max(0, j - 1);
      wHi = Math.min(m - 1, j + 1);
    }
    const span = distM[idx[wHi]]! - distM[idx[wLo]]!;
    if (span >= MIN_GRADE_SPAN_M) {
      const g = ((altM[idx[wHi]]! - altM[idx[wLo]]!) / span) * 100;
      if (Math.abs(g) <= GRADE_MAX_ABS_PCT) out[idx[j]] = g;
    }
  }
  return out;
}

/** Index of the ascending array's value nearest to x (binary search) —
 * maps a drag-selection edge back to a chart point. */
export function nearestIdx(xs: number[], x: number): number {
  let lo = 0;
  let hi = xs.length - 1;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (xs[mid] < x) lo = mid + 1;
    else hi = mid;
  }
  return lo > 0 && x - xs[lo - 1] < xs[lo] - x ? lo - 1 : lo;
}

/** Chart index for a trackpoint index, tolerating holes: points without a
 * value are absent from `reverseMap`, so search outward from the exact
 * index. Maps a stored effort range onto the chart's own point space. */
export function nearestChartIdx(
  reverseMap: Map<number, number>,
  tpIdx: number,
  maxTpIdx: number,
): number | null {
  for (let d = 0; d <= maxTpIdx; d++) {
    const below = reverseMap.get(tpIdx - d);
    if (below != null) return below;
    const above = reverseMap.get(tpIdx + d);
    if (above != null) return above;
  }
  return null;
}

/** True when both trackpoint ranges select the same span (or both are off).
 * The chart's echo guard: an external range equal to what the chart itself
 * published needs no redraw. */
export function rangesEqual(
  a: [number, number] | null,
  b: [number, number] | null,
): boolean {
  if (a === b) return true;
  return a != null && b != null && a[0] === b[0] && a[1] === b[1];
}

/** Chart column span for an externally published trackpoint range — null
 * when it can't be resolved to two DISTINCT chart points (no data, or the
 * whole range collapses into one column). */
export function externalSelectionCols(
  range: [number, number],
  reverseMap: Map<number, number>,
  maxTpIdx: number,
): [number, number] | null {
  const a = nearestChartIdx(reverseMap, range[0], maxTpIdx);
  const b = nearestChartIdx(reverseMap, range[1], maxTpIdx);
  if (a == null || b == null || a === b) return null;
  return [Math.min(a, b), Math.max(a, b)];
}

/** What a drag-selected slice of the elevation chart works out to. */
export interface SelectionGrade {
  /** Horizontal span between the endpoints, meters. */
  distanceM: number;
  /** Net altitude change end minus start, meters (signed). */
  deltaM: number;
  /** Average grade over the span, percent (signed). */
  gradePct: number;
  /** Wall-clock elapsed seconds between the endpoints (pauses included),
   * or null when either endpoint carries no timestamp. */
  durationS?: number | null;
}

/**
 * Average grade of a trackpoint range [a, b] (either order): net altitude
 * delta over the distance span between the endpoints. Endpoints slide
 * inward to the nearest points carrying both inputs; a span under
 * MIN_GRADE_SPAN_M (or no two usable points) returns null — same guard as
 * the per-point series.
 */
export function selectionGrade(
  distM: (number | null)[],
  altM: (number | null)[],
  a: number,
  b: number,
  tSec?: (number | null)[],
): SelectionGrade | null {
  let lo = Math.max(0, Math.min(a, b));
  let hi = Math.min(distM.length - 1, Math.max(a, b));
  while (lo <= hi && (distM[lo] == null || altM[lo] == null)) lo++;
  while (hi >= lo && (distM[hi] == null || altM[hi] == null)) hi--;
  if (lo >= hi) return null;
  const span = distM[hi]! - distM[lo]!;
  if (span < MIN_GRADE_SPAN_M) return null;
  const delta = altM[hi]! - altM[lo]!;
  // Elapsed at the SAME slid-inward endpoints the distance/climb use — a
  // negative delta (clock weirdness across a device restart) reads as null.
  const t0 = tSec?.[lo];
  const t1 = tSec?.[hi];
  const durationS = t0 != null && t1 != null && t1 > t0 ? t1 - t0 : null;
  return { distanceM: span, deltaM: delta, gradePct: (delta / span) * 100, durationS };
}

/** The line and the fill paint the category that owns most of the road
 * around a point, not the point's own: rough ground and GPS jitter flip
 * the sample category every few meters (one mountain ride had 1180 runs,
 * 670 shorter than 20 m), and a fill painted per flip is a barcode, not a
 * profile. The window is a fixed length of road: a real pitch is a few
 * hundred meters at least, whatever the ride's length — 1 % of an 86 km
 * ride (860 m) outvoted every 300 m ledge of a 66 km one. */
export const GRADE_PAINT_WINDOW_M = 300;

/** The steepness of a climb is judged over a wider stretch than whether it
 * is a climb: the 2 % edge of a pitch is a sharp thing on the road, but the
 * ground inside one rolls between 6 and 10 % every few dozen meters, and a
 * color per roll is the barcode again. Three climb windows, on the rides
 * this was tuned on, halved the stripes without moving a single edge. */
export const GRADE_STEEPNESS_WINDOW_M = 3 * GRADE_PAINT_WINDOW_M;

/** The category each sample carries a `weight` of road for, or null. */
type Vote = { cat: number | null; weight: number };

/** The category holding the most road inside a window of `windowM`
 * centered on each of `votes` (positions `dist`, ascending), among `bins`
 * categories. One pass with a running histogram, O(n). A null category
 * casts no vote; a window with no votes is 0. Ties go to the sample's own
 * category, else to the higher one. */
function dominantCategory(dist: number[], votes: Vote[], windowM: number, bins: number): number[] {
  const m = votes.length;
  const out = new Array<number>(m).fill(0);
  const half = windowM / 2;
  const hist = new Array<number>(bins).fill(0);
  let lo = 0;
  let hi = -1;
  for (let k = 0; k < m; k++) {
    while (hi + 1 < m && dist[hi + 1] <= dist[k] + half) {
      hi++;
      const c = votes[hi].cat;
      if (c != null) hist[c] += votes[hi].weight;
    }
    while (dist[lo] < dist[k] - half) {
      const c = votes[lo].cat;
      if (c != null) hist[c] -= votes[lo].weight;
      lo++;
    }
    let best = 0;
    for (const v of hist) if (v > best) best = v;
    if (best <= 0) continue;
    const own = votes[k].cat;
    let chosen = -1;
    for (let c = bins - 1; c >= 0; c--) {
      if (hist[c] >= best * (1 - 1e-9)) {
        if (chosen === -1) chosen = c;
        if (c === own) chosen = c;
      }
    }
    out[k] = chosen;
  }
  return out;
}

/**
 * The grade category per trackpoint that the line and the fill paint,
 * decided in two votes over the road centered on each point, every sample
 * weighted by the road it stands for (halfway to its neighbors). First,
 * inside `windowM`, is this a climb at all — the samples at or above
 * GRADE_VOTE_MIN_PCT against the rest, so a pitch whose jitter dips under
 * the fill threshold every few meters still wins as a whole (the fill
 * threshold itself is enforced later, on the run's average). Then, inside
 * the wider `steepWindowM`
 * and among the climb samples only, which steepness holds the most road —
 * so a flat stretch never outvotes the steepness of a climb it borders,
 * and the rolls inside a climb don't stripe it. Both passes are O(n) with
 * a running histogram, and neither depends on order: a climb chopped by
 * jitter stays a climb, a blip inside a flat vanishes, and alternating
 * pitches keep alternating rather than collapsing into whichever came
 * first. A sample without a grade (a gap, a capped spike) casts no vote;
 * a point without a distance keeps its raw category. Ties go to the
 * point's own category, else to the steeper one. These categories only
 * cut the road into runs: the color a run finally wears is the category
 * of its own average (see gradeRunAverages), so a lone run with no
 * average paints flat.
 */
export function gradeCategories(
  distM: (number | null)[],
  grades: (number | null)[],
  windowM: number = GRADE_PAINT_WINDOW_M,
  steepWindowM: number = (windowM * GRADE_STEEPNESS_WINDOW_M) / GRADE_PAINT_WINDOW_M,
): number[] {
  const n = grades.length;
  const raw = grades.map((g) => (g == null ? null : gradeCategory(g)));
  const out: number[] = raw.map((c) => c ?? 0);
  // The samples that carry a distance, in order.
  const idx: number[] = [];
  for (let i = 0; i < n; i++) if (distM[i] != null) idx.push(i);
  const m = idx.length;
  if (m === 0) return out;
  const dist = idx.map((i) => distM[i]!);
  // The road each sample stands for: halfway to the neighbors on both sides.
  const weightAt = (k: number) => {
    const left = k > 0 ? (dist[k - 1] + dist[k]) / 2 : dist[k];
    const right = k < m - 1 ? (dist[k] + dist[k + 1]) / 2 : dist[k];
    return right - left;
  };
  const weights = idx.map((_, k) => weightAt(k));
  const climbVotes: Vote[] = idx.map((i, k) => {
    const g = grades[i];
    return { cat: g == null ? null : g >= GRADE_VOTE_MIN_PCT ? 1 : 0, weight: weights[k] };
  });
  const isClimb = dominantCategory(dist, climbVotes, windowM, 2);
  // Steepness is judged among the same samples the first vote counts as
  // climbing, at the lowest climb category if their own grade is under
  // the fill threshold — otherwise the minority above 2 % would decide
  // the steepness of a 2.3 % climb, and a couple of 8 % spikes in it
  // would cut the run where neither the color nor the average agrees.
  const steepVotes: Vote[] = idx.map((i, k) => {
    const g = grades[i];
    return { cat: g == null || g < GRADE_VOTE_MIN_PCT ? null : Math.max(1, gradeCategory(g)), weight: weights[k] };
  });
  const steepness = dominantCategory(dist, steepVotes, steepWindowM, GRADE_COLORS.length);
  for (let k = 0; k < m; k++) {
    // Both votes count the same samples, so a climb by the first vote
    // always has a steepness vote in reach while steepWindowM covers
    // windowM (the sample that won the first vote sits inside the second
    // window too); the floor only matters to a caller that narrows the
    // steepness window, and keeps such a climb in the lowest category
    // rather than flat.
    out[idx[k]] = isClimb[k] === 1 ? Math.max(1, steepness[k]) : 0;
  }
  return out;
}

/**
 * The average grade of the painted climb each trackpoint sits in, in
 * percent, and null on the flat category. What the tooltip shows over a
 * filled segment: one number for the whole band, not a value that moves
 * with the cursor, and the number the run is colored by, so a band and
 * its label always agree. It is the distance-weighted mean of the smoothed,
 * capped per-sample grades inside the run — the average grade a rider
 * expects (rise over run), but not measured between the run's two end
 * samples, because the ends are exactly where a stop's barometer drift
 * lives: a drifted end sample moves the mean only in proportion to the
 * road it owns, a few meters of a run at least a couple hundred long,
 * where rise over run would take the whole drift. (A median would ignore
 * the drift entirely but is not an average grade: a run half 5 % and half
 * 12 % would read 5 or 12 rather than 8.5.) A sample without a grade or a
 * distance carries no weight; a run with no weight at all has no average,
 * and neither has a run shorter than GRADE_MIN_BAND_M of road.
 * Two neighboring runs whose averages land in one category paint as one
 * band that answers with two numbers, both inside that category. A run
 * the vote formed but whose average stays under the fill threshold is
 * not a band at all — null, so the tooltip falls back to the point's own
 * grade there, the same rule the fill follows.
 */
export function gradeRunAverages(
  distM: (number | null)[],
  grades: (number | null)[],
  cats: number[],
): (number | null)[] {
  const n = cats.length;
  const out: (number | null)[] = new Array(n).fill(null);
  let start = 0;
  for (let i = 1; i <= n; i++) {
    if (i < n && cats[i] === cats[start]) continue;
    if (cats[start] > 0 && runLengthM(distM, start, i) >= GRADE_MIN_BAND_M) {
      let sum = 0;
      let weight = 0;
      for (let k = start; k < i; k++) {
        const g = grades[k];
        const d = distM[k];
        if (g == null || d == null) continue;
        // The road this sample stands for, halfway to its graded neighbors
        // inside the run; a lone sample stands for a point of road, so a
        // run of one graded sample takes its grade outright.
        let prev = k - 1;
        while (prev >= start && (grades[prev] == null || distM[prev] == null)) prev--;
        let next = k + 1;
        while (next < i && (grades[next] == null || distM[next] == null)) next++;
        const left = prev >= start ? (distM[prev]! + d) / 2 : d;
        const right = next < i ? (d + distM[next]!) / 2 : d;
        const w = right - left;
        if (w > 0) {
          sum += g * w;
          weight += w;
        } else if (weight === 0 && prev < start && next >= i) {
          sum = g;
          weight = 1;
        }
      }
      const mean = weight > 0 ? sum / weight : null;
      const avg = mean != null && gradeCategory(mean) > 0 ? mean : null;
      for (let k = start; k < i; k++) out[k] = avg;
    }
    start = i;
  }
  return out;
}

/** The road a run covers: between its first and last sample that carry a
 * distance; none → 0. */
function runLengthM(distM: (number | null)[], start: number, end: number): number {
  let a = start;
  while (a < end && distM[a] == null) a++;
  let b = end - 1;
  while (b > a && distM[b] == null) b--;
  return a < b ? distM[b]! - distM[a]! : 0;
}

/**
 * Horizontal gradient stops (offset 0 = plot left, 1 = right) painting the
 * elevation LINE by grade category with sharp transitions — the vertical
 * sibling of bandGradientStops (which paints the FILL by altitude). Category
 * boundaries sit at the midpoint between neighboring samples; offsets are
 * clamped and kept monotonic with the same NaN guard (a poisoned offset
 * would make addColorStop throw and kill the chart).
 */
export function gradeGradientStops(
  xs: number[],
  cats: number[],
  xPosOf: (x: number) => number,
  left: number,
  width: number,
): { offset: number; color: string }[] {
  return sharpStops(xs, (i) => GRADE_COLORS[cats[i] ?? 0], xPosOf, left, width);
}

/** The fill's "nothing here" — a stop that paints nothing. */
export const GRADE_FILL_NONE = "rgba(0, 0, 0, 0)";

/** The fill palette by grade category: nothing for the flat category (the
 * altitude fill stays), the line's own color, OPAQUE, for every climb step.
 * Not a tint over the hypsometric bands: a translucent fill halved the
 * lightness step between neighboring categories and let the altitude band
 * underneath move the lightness more than the category did — the
 * colorblind rule in the palette's header was gone. Under a climb the
 * grade replaces the altitude tint outright. Built once — the stops walk
 * every sample of the track on every redraw. */
const GRADE_FILL_COLORS = GRADE_COLORS.map((c, i) => (i === 0 ? GRADE_FILL_NONE : c));

/** Fill color under one grade sample: the line's category, so the band
 * under the line starts and ends exactly where the line changes color. */
export function gradeFillColor(pct: number | null): string {
  return GRADE_FILL_COLORS[gradeCategory(pct)];
}

/**
 * Horizontal gradient stops painting the FILL under the elevation line by
 * grade category — the sibling of gradeGradientStops for the line,
 * transparent wherever the category is the flat one.
 */
export function gradeFillStops(
  xs: number[],
  cats: number[],
  xPosOf: (x: number) => number,
  left: number,
  width: number,
): { offset: number; color: string }[] {
  return sharpStops(xs, (i) => GRADE_FILL_COLORS[cats[i] ?? 0], xPosOf, left, width);
}

/**
 * Sharp-stop horizontal gradient over the samples: one color per run of
 * equal `colorAt`, the change at the midpoint between neighbors. Offsets
 * are clamped and kept monotonic with a NaN guard (a poisoned offset would
 * make addColorStop throw and kill the chart).
 */
function sharpStops(
  xs: number[],
  colorAt: (i: number) => string,
  xPosOf: (x: number) => number,
  left: number,
  width: number,
): { offset: number; color: string }[] {
  const stops: { offset: number; color: string }[] = [];
  if (xs.length === 0) return stops;
  let color = colorAt(0);
  let prev = 0;
  for (let i = 1; i < xs.length; i++) {
    const c = colorAt(i);
    if (c === color) continue;
    const midX = (xs[i - 1] + xs[i]) / 2;
    const raw = (xPosOf(midX) - left) / width;
    const t = Number.isNaN(raw) ? prev : Math.min(1, Math.max(prev, raw));
    stops.push({ offset: prev, color });
    stops.push({ offset: t, color });
    prev = t;
    color = c;
  }
  stops.push({ offset: prev, color });
  stops.push({ offset: 1, color });
  return stops;
}

export interface ElevationBand {
  /** Band ceiling in display units (last band: Infinity). */
  to: number;
  color: string;
}

/** Hypsometric tints, atlas convention: green valleys → sandy foothills →
 * brown slopes → grey rock → eternal snow. Ceilings in METERS (the chart
 * converts them with the data). Like every atlas, the scale is deliberately
 * conventional, not ecological — real treelines/snowlines shift with
 * latitude. MUST stay strictly ascending with an Infinity ceiling last:
 * bandGradientStops would not throw on a misordered array, it would clamp
 * silently and paint the wrong colors (guarded by a test). */
export const ELEVATION_BANDS_M: ElevationBand[] = [
  { to: 200, color: "rgba(134, 178, 115, 0.45)" },
  { to: 500, color: "rgba(190, 205, 125, 0.45)" },
  { to: 1000, color: "rgba(226, 200, 134, 0.5)" },
  { to: 2000, color: "rgba(207, 162, 112, 0.5)" },
  { to: 3000, color: "rgba(178, 128, 98, 0.5)" },
  // Darker than a pure hue-shift from the brown neighbor: at the old
  // rgba(158,150,143,.55) the 3000 m boundary matched it in lightness and
  // survived only as a hue change — invisible to red-green colorblindness.
  { to: 4500, color: "rgba(140, 132, 125, 0.6)" },
  { to: Infinity, color: "rgba(240, 248, 255, 0.9)" },
];

/**
 * Vertical gradient stops (offset 0 = plot top, 1 = bottom) painting
 * hypsometric elevation bands with sharp transitions. Bands come ascending
 * by ceiling; offsets are clamped and kept monotonic, so a sea-level ride
 * collapses every mountain band to zero width and stays all-green, and an
 * all-alpine track is all snow.
 */
export function bandGradientStops(
  bands: ElevationBand[],
  yPosOf: (v: number) => number,
  top: number,
  height: number,
): { offset: number; color: string }[] {
  const stops: { offset: number; color: string }[] = [];
  let prev = 0;
  // Top of the chart shows the HIGHEST band — walk bands top-down.
  for (let i = bands.length - 1; i >= 0; i--) {
    const yPx = i === 0 ? top + height : yPosOf(bands[i - 1].to);
    const raw = (yPx - top) / height;
    // ±Infinity clamps correctly below; NaN would poison the clamp and make
    // addColorStop throw, killing the whole chart — collapse it instead.
    const t = Number.isNaN(raw) ? prev : Math.min(1, Math.max(prev, raw));
    stops.push({ offset: prev, color: bands[i].color });
    stops.push({ offset: t, color: bands[i].color });
    prev = t;
  }
  return stops;
}

/** Target px between bar centers — the design's HRChart look (~40 bars on
 * a half-width card at the default 1200px window). */
const BAR_TARGET_PX = 14;

/**
 * Zone-bar count for one chart card `cardW` CSS-px wide — the caller passes
 * the actual slot width (the full-width first slot gets ~2× the bars of a
 * half-width card). Quantized to steps of 5 so a live window drag
 * re-buckets occasionally, not per-pixel; clamped so a tiny card still
 * reads as bars (20) and a huge one doesn't dissolve into a comb (120).
 * A non-positive width (not yet measured) returns the design default 40.
 */
export function zoneBarCount(cardW: number): number {
  if (!isFinite(cardW) || cardW <= 0) return 40;
  const stepped = Math.round(cardW / BAR_TARGET_PX / 5) * 5;
  return Math.min(120, Math.max(20, stepped));
}

export interface BarBuckets {
  /** Bar x positions (window midpoints of the source x values). */
  xs: number[];
  /** Bar heights — the max sample of each window (design: max HR). */
  values: number[];
  /** Source index of each bar's max sample (drives the map-hover sync). */
  srcIdx: number[];
  /** Bar index for every source index (the reverse of srcIdx). */
  barOf: number[];
  /** Window width in x units — the x scale must widen by half of this on
   * each side, or the edge bars (centered on the scale's min/max) get
   * clipped to half-width by the plot area. */
  step: number;
}

/**
 * Downsample a series into at most `maxBars` bars, each the MAX of its
 * window — the design draws ~40 wide rounded bars, not one per trackpoint,
 * and max (not mean) keeps short spikes visible at this resolution.
 *
 * Windows are equal spans of the X AXIS (not equal sample counts): uPlot
 * sizes every bar from the smallest gap between adjacent bar centers, so
 * sample-count windows — whose centers bunch up wherever samples are dense
 * on a distance axis — would collapse ALL bars to slivers. Equal-x windows
 * keep the centers a fixed span apart; windows with no samples simply leave
 * a gap. Assumes `xValues` ascending (time and cumulative distance are).
 */
export function bucketMaxBars(
  xValues: number[],
  values: number[],
  maxBars: number,
): BarBuckets {
  const n = xValues.length;
  const barOf = new Array<number>(n);
  if (n === 0) return { xs: [], values: [], srcIdx: [], barOf, step: 0 };

  const x0 = xValues[0];
  const w = (xValues[n - 1] - x0) / maxBars;
  const xs: number[] = [];
  const out: number[] = [];
  const srcIdx: number[] = [];

  let window = -1;
  for (let i = 0; i < n; i++) {
    // Degenerate span (single sample / all-equal x) → everything in window 0.
    const k = w > 0 ? Math.min(maxBars - 1, Math.floor((xValues[i] - x0) / w)) : 0;
    if (k !== window) {
      window = k;
      xs.push(x0 + (k + 0.5) * w);
      out.push(values[i]);
      srcIdx.push(i);
    } else if (values[i] > out[out.length - 1]) {
      out[out.length - 1] = values[i];
      srcIdx[srcIdx.length - 1] = i;
    }
    barOf[i] = xs.length - 1;
  }
  return { xs, values: out, srcIdx, barOf, step: w };
}
