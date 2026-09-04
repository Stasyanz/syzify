// The metric chart catalogue behind ChartPanel: what each card plots, how
// it is labelled/formatted, and where its "avg" reference comes from. Kept
// apart from the uPlot wiring so all of it is testable without a canvas.

import type { TrackPointColumns } from "../../lib/types";
import { ELEVATION_BANDS_M, type ElevationBand } from "./chartZones";
import { meanPace, paceOfSpeed, seriesMean } from "./chartAverage";
import {
  isImperial,
  distanceUnit,
  elevationUnit,
  speedUnit,
  M_PER_MILE,
  M_PER_100YD,
  FT_PER_M,
  MPH_PER_MPS,
} from "../../lib/units";

/** Summary averages in the units the data model stores them in. */
export interface SummaryAverages {
  hr?: number | null;
  power_w?: number | null;
  cadence?: number | null;
  speed_mps?: number | null;
}

export type ChartType = "elevation" | "hr" | "pace" | "speed" | "cadence" | "power";

export interface ChartConfig {
  key: ChartType;
  label: string;
  unit: string;
  color: string;
  fill: string;
  getData: (tp: TrackPointColumns) => (number | null)[];
  /** Optional y-axis tick formatter (e.g. pace mm:ss). */
  valueFmt?: (v: number) => string;
  /** Flip the y-axis so smaller values read as "higher" — pace is
   * faster the smaller the number, and faster belongs at the top. */
  invertY?: boolean;
  /** Hypsometric fill bands (display units) — elevation's atlas coloring. */
  elevationBands?: ElevationBand[];
  /** No "avg" reference line — average altitude is not a training reference. */
  noAverage?: boolean;
  /** The summary's average in this chart's display units, if the summary
   * carries one (pace/speed convert from m/s). Preferred over the samples. */
  summaryAvg?: (s: SummaryAverages) => number | null | undefined;
  /** Fallback average from the samples. Defaults to the time-weighted mean
   * of the metric's own series; pace derives it from the speeds instead. */
  sampleAvg?: (
    series: (number | null)[],
    speedMps: (number | null)[],
    t: (number | null)[],
  ) => number | null;
}

/** Pace (min/km decimal) → "m:ss". */
export function fmtPace(minPerKm: number): string {
  if (!isFinite(minPerKm) || minPerKm <= 0) return "";
  const m = Math.floor(minPerKm);
  const s = Math.round((minPerKm - m) * 60);
  return s === 60 ? `${m + 1}:00` : `${m}:${String(s).padStart(2, "0")}`;
}

/** Per-point speed (m/s): use the recorded series, else derive it from the
 * cumulative distance + time deltas (GPS-only tracks carry no speed field). */
export function speedSeries(tp: TrackPointColumns): (number | null)[] {
  // Use the recorded field only when it carries real signal; some devices
  // populate speed_mps with all-zeros, which would otherwise suppress the
  // speed/pace chart that is perfectly derivable from distance + time.
  if (tp.speed_mps.some((v) => v != null && v !== 0)) return tp.speed_mps;
  const n = tp.distance_m.length;
  const out: (number | null)[] = new Array(n).fill(null);
  let prev = -1;
  for (let i = 0; i < n; i++) {
    if (tp.distance_m[i] == null || tp.t[i] == null) continue;
    if (prev >= 0) {
      const dd = tp.distance_m[i]! - tp.distance_m[prev]!;
      const dt = tp.t[i]! - tp.t[prev]!;
      if (dt > 0 && dd >= 0) out[i] = dd / dt;
    }
    prev = i;
  }
  return out;
}

/** A metric is worth a chart only if it carries a real signal — at least one
 * non-null AND non-zero sample. An all-zero series (e.g. speed/distance on an
 * indoor strength session) is a flat line at 0, not data worth a card. */
export function hasData(values: (number | null)[]): boolean {
  return values.some((v) => v != null && v !== 0);
}

// Hypsometric band definitions live in chartZones.ts (ELEVATION_BANDS_M),
// next to the gradient math and its ordering guard test.

// Elevation / pace / speed depend on the units setting — built via factories
// so a units change yields fresh configs (see the useMemo in ChartPanel).
export const ELEVATION = (): ChartConfig => ({
  key: "elevation",
  label: "Elevation",
  unit: elevationUnit(),
  color: "#0e7490",
  fill: "rgba(14,116,144,0.12)",
  getData: (tp) =>
    isImperial()
      ? tp.altitude_m.map((v) => (v != null ? v * FT_PER_M : null))
      : tp.altitude_m,
  // Band ceilings stay at honest meter marks in any display unit
  // (Infinity * FT_PER_M is still Infinity).
  elevationBands: ELEVATION_BANDS_M.map((b) => ({
    to: isImperial() ? b.to * FT_PER_M : b.to,
    color: b.color,
  })),
  noAverage: true,
});
export const HR: ChartConfig = {
  key: "hr",
  label: "Heart rate",
  unit: "bpm",
  color: "#dc2626",
  fill: "rgba(220,38,38,0.10)",
  getData: (tp) => tp.hr,
  summaryAvg: (s) => s.hr,
};
// Pace from speed; ignore near-stops so the line doesn't spike to infinity.
// The cutoff and the per-unit distance are shared by the series and its
// average so the two can never drift apart.
export const RUN_STOP_MPS = 0.5;
export const PACE = (): ChartConfig => {
  const perUnit = isImperial() ? M_PER_MILE : 1000;
  return {
    key: "pace",
    label: "Pace",
    unit: `min/${distanceUnit()}`,
    color: "#c2410c",
    fill: "rgba(194,65,12,0.10)",
    getData: (tp) =>
      speedSeries(tp).map((v) => (v != null && v > RUN_STOP_MPS ? perUnit / v / 60 : null)),
    summaryAvg: (s) => paceOfSpeed(s.speed_mps, perUnit),
    sampleAvg: (_series, speedMps, t) => meanPace(speedMps, RUN_STOP_MPS, perUnit, t),
    valueFmt: fmtPace,
    invertY: true,
  };
};
// Swim pace per 100 m/yd; the near-stop cutoff sits lower than running's
// (0.2 m/s ≈ 8:20 /100m) — swim speeds live well below running speeds.
export const SWIM_STOP_MPS = 0.2;
export const SWIM_PACE = (): ChartConfig => {
  const per100 = isImperial() ? M_PER_100YD : 100;
  return {
    key: "pace",
    label: "Pace",
    unit: `min/100${isImperial() ? "yd" : "m"}`,
    color: "#c2410c",
    fill: "rgba(194,65,12,0.10)",
    getData: (tp) =>
      speedSeries(tp).map((v) => (v != null && v > SWIM_STOP_MPS ? per100 / v / 60 : null)),
    summaryAvg: (s) => paceOfSpeed(s.speed_mps, per100),
    sampleAvg: (_series, speedMps, t) => meanPace(speedMps, SWIM_STOP_MPS, per100, t),
    valueFmt: fmtPace,
    invertY: true,
  };
};
export const SPEED = (): ChartConfig => {
  const factor = isImperial() ? MPH_PER_MPS : 3.6;
  return {
    key: "speed",
    label: "Speed",
    unit: speedUnit(),
    color: "#3f7d4e",
    fill: "rgba(63,125,78,0.10)",
    getData: (tp) => speedSeries(tp).map((v) => (v != null ? v * factor : null)),
    summaryAvg: (s) => (s.speed_mps != null ? s.speed_mps * factor : null),
  };
};
export const CADENCE: ChartConfig = {
  key: "cadence",
  label: "Cadence",
  unit: "spm",
  color: "#ca8a04",
  fill: "rgba(202,138,4,0.10)",
  getData: (tp) => tp.cadence,
  summaryAvg: (s) => s.cadence,
};
export const POWER: ChartConfig = {
  key: "power",
  label: "Power",
  unit: "W",
  color: "#7c3aed",
  fill: "rgba(124,58,237,0.10)",
  getData: (tp) => tp.power_w,
  summaryAvg: (s) => s.power_w,
};

/** Per-metric value for the "avg" reference line: the device's summary
 * average where the caller has one (it matches the summary tiles), else
 * the time-weighted mean of the charted samples — a focused leg's slice
 * averages that leg, a GPX import needs no summary at all. Pace needs the
 * raw speeds (its series is already inverted into min/unit), materialized
 * once here rather than re-derived per config. */
export function resolveAverages(
  available: ChartConfig[],
  seriesByKey: Map<ChartType, (number | null)[]>,
  trackpoints: TrackPointColumns,
  summary: SummaryAverages,
): Map<ChartType, number | null> {
  const m = new Map<ChartType, number | null>();
  const needsSpeed = available.some((c) => c.key === "pace");
  const speedMps = needsSpeed ? speedSeries(trackpoints) : [];
  for (const c of available) {
    if (c.noAverage) continue;
    const fromSummary = c.summaryAvg?.(summary);
    if (fromSummary != null && isFinite(fromSummary)) {
      m.set(c.key, fromSummary);
      continue;
    }
    const series = seriesByKey.get(c.key) ?? [];
    m.set(
      c.key,
      c.sampleAvg
        ? c.sampleAvg(series, speedMps, trackpoints.t)
        : seriesMean(series, trackpoints.t),
    );
  }
  return m;
}
