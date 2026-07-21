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

/** Power zone ranges — FIT boundaries only. Unlike HR there is NO sensible
 * default (zones hang off personal FTP), so without boundaries this returns
 * null and the power chart stays a plain line. */
export function powerZoneRanges(zones: TimeInZone[]): ZoneRange[] | null {
  const bounds = zoneBoundaries(zones, "power");
  return bounds.length < 2 ? null : rangesFromBoundaries(bounds);
}

/** Cadence palette, low → high. Unlike HR/power, HIGH cadence is GOOD, so
 * the scale runs red (plodding) → orange → green → teal → purple (elite),
 * reusing the app's sport hues for the cool end. */
export const CADENCE_ZONE_COLORS = [
  "#c0392b", // < Z1: overstriding
  "#e07c3a",
  "#4a9e5c", // the healthy 163–174 spm band
  "#0e7490",
  "#7c3aed", // 185+: elite turnover
];

/** Garmin's universal running-cadence thresholds, in full steps/min. */
const RUN_CADENCE_SPM_BOUNDS = [151, 163, 174, 185];

/**
 * Cadence zone ranges: the device's FIT boundaries when it recorded them
 * (any sport, same priority as power), else Garmin's universal RUNNING
 * thresholds — which only make sense for runs, so other sports stay a line.
 * Devices disagree on units: FIT run cadence is usually single-leg rpm
 * (~75–95) while the thresholds are full spm (~150–190) — when the data's
 * median sits below 120 the samples are per-leg and the thresholds halve.
 */
export function cadenceZoneRanges(
  zones: TimeInZone[],
  sport: string,
  values: number[],
): ZoneRange[] | null {
  const bounds = zoneBoundaries(zones, "cadence");
  if (bounds.length >= 2) return rangesFromBoundaries(bounds, CADENCE_ZONE_COLORS);

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
    return rangesFromBoundaries(bounds.map((b) => b * mpsToUnit));
  }
  if (sport !== "ride") return null;
  return rangesFromBoundaries(
    RIDE_SPEED_BOUNDS_KMH.map((kmh) => (kmh / 3.6) * mpsToUnit),
  );
}

/** Y-range for speed bars: gentler than the tens-based ranges — ±2 units
 * rounded out to fives (a ±10 pad would dwarf a 0–40 km/h ride). */
export function speedVisRange(dMin: number, dMax: number): [number, number] {
  const lo = Math.max(0, Math.floor((dMin - 2) / 5) * 5);
  const hi = Math.ceil((dMax + 2) / 5) * 5;
  return [lo, hi];
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
