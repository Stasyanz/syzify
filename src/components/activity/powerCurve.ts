// Pure helpers for the Power Curve panel (kept out of the component so the
// merge/label logic is unit-testable without uPlot or a DOM).

import type { PowerCurveEnvelopePoint, PowerCurvePoint } from "../../lib/types";

/** X ticks worth labeling on the log axis — the anchors riders compare. */
export const AXIS_SPLITS = [1, 5, 15, 60, 300, 1200, 3600];

/** "45s", "1m", "1m30", "20m", "1h" — compact window labels for axis + tip. */
export function formatWindow(s: number): string {
  if (s < 60) return `${s}s`;
  if (s === 3600) return "1h";
  const m = Math.floor(s / 60);
  const rest = s % 60;
  return rest === 0 ? `${m}m` : `${m}m${rest.toString().padStart(2, "0")}`;
}

export interface MergedCurve {
  /** Sorted union of both curves' windows (the chart's x values). */
  x: number[];
  /** This activity's watts per x, null where its curve has no such window. */
  activity: (number | null)[];
  /** Envelope watts per x — spans at least every activity window. */
  envelope: (number | null)[];
  /** Envelope attribution per x (null where the envelope has no window). */
  record: (PowerCurveEnvelopePoint | null)[];
}

/**
 * Align the activity curve and the envelope onto one x grid. Both come from
 * the same backend window grid, but the envelope usually extends further —
 * longer activities contribute windows this one didn't reach.
 */
export function mergeCurveData(
  points: PowerCurvePoint[],
  envelope: PowerCurveEnvelopePoint[],
): MergedCurve {
  const xs = new Set<number>();
  for (const p of points) xs.add(p.window_s);
  for (const e of envelope) xs.add(e.window_s);
  const x = [...xs].sort((a, b) => a - b);

  const byWindow = new Map(points.map((p) => [p.window_s, p.watts]));
  const envByWindow = new Map(envelope.map((e) => [e.window_s, e]));
  return {
    x,
    activity: x.map((w) => byWindow.get(w) ?? null),
    envelope: x.map((w) => envByWindow.get(w)?.watts ?? null),
    record: x.map((w) => envByWindow.get(w) ?? null),
  };
}

/** Month + day of an ISO timestamp for the tooltip ("Aug 30"). */
export function shortDate(iso: string): string {
  const d = new Date(iso);
  if (isNaN(d.getTime())) return "";
  return d.toLocaleDateString("en-US", { month: "short", day: "numeric" });
}

/**
 * Tooltip line for a hovered window: this activity's value, and the all-time
 * record with its source when the record was set elsewhere. The record's
 * name falls back to its date — untitled imports are the norm, not the edge.
 */
export function tooltipText(
  windowS: number,
  activityW: number | null,
  record: PowerCurveEnvelopePoint | null,
  activityId: string,
): string {
  const parts: string[] = [];
  if (activityW != null) parts.push(`${formatWindow(windowS)} · ${Math.round(activityW)} W`);
  else parts.push(formatWindow(windowS));
  if (record) {
    if (record.activity_id === activityId) {
      parts.push("all-time best");
    } else {
      const label = record.title || shortDate(record.start_time);
      parts.push(`best ${Math.round(record.watts)} W (${label})`);
    }
  }
  return parts.join(" — ");
}
