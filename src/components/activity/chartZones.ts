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

/** Usable boundaries of one zone type: sorted by zone index, degenerate
 * values (zero, repeats, nulls) dropped, strictly increasing. */
function zoneBoundaries(zones: TimeInZone[], zoneType: string): number[] {
  const bounds: number[] = [];
  for (const z of [...zones].sort((a, b) => a.zone_index - b.zone_index)) {
    if (z.zone_type !== zoneType) continue;
    const b = z.zone_high_boundary;
    if (b == null || !isFinite(b) || b <= 0) continue;
    if (bounds.length === 0 || b > bounds[bounds.length - 1]) bounds.push(b);
  }
  return bounds;
}

/** Contiguous ranges over [0, ∞) from strictly-increasing boundaries, the
 * palette anchored to the TOP end — the top range always gets the palette's
 * last color no matter how many zones the device reports, extra bottom
 * ranges reuse the first (Garmin's 5 HR boundaries make 6 ranges: below-Z1
 * and Z1 both read as recovery green; 6 power boundaries spread the same). */
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
  const bounds = zoneBoundaries(zones, "hr");
  return bounds.length < 2 ? DEFAULT_HR_RANGES : rangesFromBoundaries(bounds);
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
  const bounds = zoneBoundaries(zones, "power");
  if (bounds.length >= 2) return rangesFromBoundaries(bounds, POWER_ZONE_COLORS);
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
  const bounds = zoneBoundaries(zones, "cadence");
  if (bounds.length >= 2) return rangesFromBoundaries(bounds, CADENCE_ZONE_COLORS);

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
  const bounds = zoneBoundaries(zones, "speed");
  if (bounds.length >= 2) {
    return rangesFromBoundaries(bounds.map((b) => b * mpsToUnit), SPEED_ZONE_COLORS);
  }
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

/** Grade category ceilings in percent — a product decision like the ride
 * speed bounds (Strava/Komoot pick their own): below 4% reads as rolling,
 * 16%+ is wall territory. Descents and flats share the base color — the
 * chart colors EFFORT, and effort lives uphill. */
export const GRADE_BOUNDS_PCT = [4, 8, 12, 16];

/** Grade palette, flat → wall. Index 0 is the elevation line's own teal so
 * flat terrain looks unchanged; the climb steps reuse the HR warm ramp.
 * Neighbors differ in lightness, not just hue (the standing a11y rule). */
export const GRADE_COLORS = [
  "#0e7490", // < 4%: flat / descent (the elevation line color)
  "#c9941a", // 4–8%: noticeable
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
      out[idx[j]] = ((altM[idx[wHi]]! - altM[idx[wLo]]!) / span) * 100;
    }
  }
  return out;
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
  grades: (number | null)[],
  xPosOf: (x: number) => number,
  left: number,
  width: number,
): { offset: number; color: string }[] {
  const stops: { offset: number; color: string }[] = [];
  if (xs.length === 0) return stops;
  let cat = gradeCategory(grades[0] ?? null);
  let prev = 0;
  for (let i = 1; i < xs.length; i++) {
    const c = gradeCategory(grades[i] ?? null);
    if (c === cat) continue;
    const midX = (xs[i - 1] + xs[i]) / 2;
    const raw = (xPosOf(midX) - left) / width;
    const t = Number.isNaN(raw) ? prev : Math.min(1, Math.max(prev, raw));
    stops.push({ offset: prev, color: GRADE_COLORS[cat] });
    stops.push({ offset: t, color: GRADE_COLORS[cat] });
    prev = t;
    cat = c;
  }
  stops.push({ offset: prev, color: GRADE_COLORS[cat] });
  stops.push({ offset: 1, color: GRADE_COLORS[cat] });
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
