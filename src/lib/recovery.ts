// The dashboard's Recovery card, pure part (ADR 0002): what the backend's
// RecoveryCard says in words, and the geometry of its 28-day sparkline.
// Everything is keyed on the card's own `computed_for` day, never on the
// frontend clock — the query is invalidated at midnight and the backend
// answers for its "today".

import type { RecoveryCard } from "./types";
import { dateOfDayKey } from "./calendar";

export type Band = NonNullable<RecoveryCard["band"]>;

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

/** When the shown index was measured, relative to the card's day: the
 * night before `computed_for` is "Last night"; anything older names the
 * morning's date and how far back it is. */
export function describeAge(date: string, ageDays: number): string {
  if (ageDays <= 0) return "Last night";
  const ago = ageDays === 1 ? "1 day ago" : `${ageDays} days ago`;
  return `${shortDate(date)} · ${ago}`;
}

export type EmptyState =
  /** No monitoring day stored in the last 90 days and no index ever — the
   * watch's Monitor files have not been imported. */
  | { kind: "no_data" }
  /** Monitoring days exist but no night reached a full record: the watch
   * was worn by day only. */
  | { kind: "no_nights" }
  /** Valid nights exist but the baseline has not formed yet. */
  | { kind: "building"; nightsNeeded: number };

/** The card's empty state, or null when there is an index to show. */
export function emptyState(card: RecoveryCard): EmptyState | null {
  if (card.index != null) return null;
  if (card.days_recorded_90d === 0) return { kind: "no_data" };
  if (card.nights_recorded_90d === 0) return { kind: "no_nights" };
  return { kind: "building", nightsNeeded: card.nights_needed };
}

/** The baseline needs 3 valid nights and the index lands on the night
 * AFTER it forms — so the count is about the baseline, never a promise
 * of an index: "2 more nights to build the baseline" / "Baseline ready —
 * the next recorded night gets an index". */
export function buildingText(nightsNeeded: number): string {
  if (nightsNeeded <= 0) return "Baseline ready — the next recorded night gets an index";
  return `${nightsNeeded} more night${nightsNeeded === 1 ? "" : "s"} to build the baseline`;
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
  history: RecoveryCard["history"],
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
