// Recovery index, pure frontend part (ADR 0002): band labels and tokens
// shared by the calendar and the sparkline, and the sparkline's geometry.
// Dates are "YYYY-MM-DD" keys from the backend, never the frontend clock.

import type { RecoveryNight } from "./types";
import { dateOfDayKey } from "./calendar";

export type Band = RecoveryNight["band"];

/** What the sparkline needs of a night. */
export interface SparkHistoryPoint {
  date: string;
  index: number;
  band: Band;
}

export const BAND_LABEL: Record<Band, string> = {
  intervals_ok: "Intervals OK",
  easy_day: "Easy day",
  rest: "Rest",
};

/** CSS token behind each band — the tint the index and its chip take. */
export const BAND_TOKEN: Record<Band, "--good" | "--warn" | "--danger"> = {
  intervals_ok: "--good",
  easy_day: "--warn",
  rest: "--danger",
};

/** Whole days from `from` to `to` ("YYYY-MM-DD" keys), DST-proof. */
export function daysBetween(from: string, to: string): number {
  const ms = dateOfDayKey(to).getTime() - dateOfDayKey(from).getTime();
  return Math.round(ms / 86_400_000);
}

/** "Sep 5" — the short local date of a "YYYY-MM-DD" key. */
export function shortDate(key: string): string {
  return dateOfDayKey(key).toLocaleDateString("en-US", { month: "short", day: "numeric" });
}

/** "+3" / "−2" / "0" for the night HR's distance from the baseline. */
export function formatDelta(delta: number): string {
  const r = Math.round(delta);
  if (r > 0) return `+${r}`;
  if (r < 0) return `−${Math.abs(r)}`;
  return "0";
}

export interface SparkPoint {
  x: number;
  y: number;
  date: string;
  index: number;
  band: Band;
}

export interface Sparkline {
  /** One point per valid night inside the window, oldest first. */
  points: SparkPoint[];
  /** Lines only between CONSECUTIVE nights — a missing night stays a gap. */
  segments: [SparkPoint, SparkPoint][];
  /** y of the 80 and 60 band edges. */
  guides: { y80: number; y60: number };
}

export interface SparkBox {
  width: number;
  height: number;
  /** Inset so the outermost points are not clipped. */
  pad: number;
  /** Days on the x axis, the last one being `computedFor`. */
  days: number;
}

/** Sparse sparkline of the index over the last `days` days ending on the
 * card's day. Points sit on their day's x, so gaps in the record are
 * visible as gaps; points outside the window are dropped. */
export function sparkline(
  history: SparkHistoryPoint[],
  computedFor: string,
  box: SparkBox,
): Sparkline {
  const innerW = box.width - 2 * box.pad;
  const innerH = box.height - 2 * box.pad;
  const yOf = (index: number) => box.pad + ((100 - index) / 100) * innerH;
  const points: SparkPoint[] = [];
  for (const h of history) {
    const offset = box.days - 1 - daysBetween(h.date, computedFor);
    if (offset < 0 || offset > box.days - 1) continue;
    points.push({
      x: box.pad + (offset / Math.max(1, box.days - 1)) * innerW,
      y: yOf(h.index),
      date: h.date,
      index: h.index,
      band: h.band,
    });
  }
  points.sort((a, b) => a.x - b.x);
  const segments: [SparkPoint, SparkPoint][] = [];
  for (let i = 1; i < points.length; i++) {
    if (daysBetween(points[i - 1].date, points[i].date) === 1) {
      segments.push([points[i - 1], points[i]]);
    }
  }
  return { points, segments, guides: { y80: yOf(80), y60: yOf(60) } };
}
