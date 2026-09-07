import type { TimeInZone } from "../../lib/types";
import { HR_ZONE_COLORS, POWER_ZONE_COLORS, zoneIndexOffset } from "./chartZones";

export type ZoneType = "hr" | "power";

/** One drawn zone of the Time in Zones card. */
export interface ZoneRow {
  /** Zone number as the user knows it (1-based). */
  zone: number;
  /** "Z4 Threshold" */
  label: string;
  /** "149–167 bpm", "> 353 W", or "" when the device wrote no boundaries. */
  range: string;
  timeS: number;
  /** Share of the timer duration, 0–100. */
  pct: number;
  color: string;
}

/** Garmin's zone names, index = zone − 1. */
export const HR_ZONE_NAMES = ["Warm up", "Easy", "Aerobic", "Threshold", "Maximum"];
export const POWER_ZONE_NAMES = [
  "Active recovery",
  "Endurance",
  "Tempo",
  "Threshold",
  "VO2 max",
  "Anaerobic",
  "Neuromuscular",
];

const UNIT: Record<ZoneType, string> = { hr: "bpm", power: "W" };
const NAMES: Record<ZoneType, string[]> = { hr: HR_ZONE_NAMES, power: POWER_ZONE_NAMES };
const COLORS: Record<ZoneType, string[]> = { hr: HR_ZONE_COLORS, power: POWER_ZONE_COLORS };

/**
 * The zone rows of one type, Z1 first — the Garmin Connect zones panel.
 *
 * Garmin writes one bucket per index: 0 = below Z1 (its ceiling is the Z1
 * floor), 1…N = the zones, and for HR one more "above max" bucket without
 * a boundary; power's top bucket carries a sentinel ceiling (3393 W).
 * Like Garmin, only Z1…N are shown, the edge buckets are dropped and the
 * percentages are of the timer duration — a list may sum below 100 %.
 * The top zone's own ceiling is never read (sentinel / null): it shows as
 * "> floor". Which bucket is Z1 comes from `zoneIndexOffset`, shared with
 * the charts so the bar here and the band behind the HR chart agree.
 *
 * Older imports keep one copy of each bucket per lap and for the session,
 * so the MAX per index is taken. Pre-dedup imports also left rows longer
 * than the activity (days' worth of seconds) that a MAX would pick; only
 * their TIME is dropped — the row still carries a valid boundary and its
 * index, and losing either would shift the whole list by a zone.
 *
 * Null when the type has no rows or no time in any shown zone.
 */
export function timeInZoneRows(
  zones: TimeInZone[],
  type: ZoneType,
  durationS: number | null,
): ZoneRow[] | null {
  const secs = new Map<number, number>();
  const ceil = new Map<number, number | null>();
  // A zero/negative duration is junk, not a bound.
  const maxS = durationS != null && durationS > 0 ? durationS : Infinity;
  for (const z of zones) {
    if (z.zone_type !== type) continue;
    const b = z.zone_high_boundary;
    const usable = b != null && isFinite(b);
    if (!ceil.has(z.zone_index) || (usable && ceil.get(z.zone_index) == null)) {
      ceil.set(z.zone_index, usable ? b : null);
    }
    const t = z.time_s;
    const valid = isFinite(t) && t >= 0 && t <= maxS;
    secs.set(z.zone_index, Math.max(secs.get(z.zone_index) ?? 0, valid ? t : 0));
  }
  if (secs.size === 0) return null;

  const names = NAMES[type];
  const n = names.length;
  const offset = zoneIndexOffset(secs.keys(), n);
  const unit = UNIT[type];
  const total = durationS != null && durationS > 0 ? durationS : null;

  const rows: ZoneRow[] = [];
  for (let zone = 1; zone <= n; zone++) {
    const idx = zone - 1 + offset;
    const timeS = secs.get(idx) ?? 0;
    const floor = idx > 0 ? (ceil.get(idx - 1) ?? null) : 0;
    const top = zone === n ? null : (ceil.get(idx) ?? null);
    rows.push({
      zone,
      label: `Z${zone} ${names[zone - 1]}`,
      range: rangeLabel(floor, top, zone === n, unit),
      timeS,
      pct: total ? (timeS / total) * 100 : 0,
      color: COLORS[type][zone - 1],
    });
  }
  return rows.some((r) => r.timeS > 0) ? rows : null;
}

/** "93–112 bpm"; a ceiling not above the floor is degenerate (the charts
 * fold such a zone away) and only the ceiling is shown. */
function rangeLabel(
  floor: number | null,
  top: number | null,
  open: boolean,
  unit: string,
): string {
  const f = floor == null ? null : Math.round(floor);
  const t = top == null ? null : Math.round(top);
  if (open) return f == null ? "" : `> ${f} ${unit}`;
  if (f != null && t != null && t > f) return `${f}–${t} ${unit}`;
  if (t != null) return `≤ ${t} ${unit}`;
  if (f != null) return `> ${f} ${unit}`;
  return "";
}

/** Whether the card has anything to draw for this activity. */
export function hasTimeInZones(zones: TimeInZone[], durationS: number | null): boolean {
  return (
    timeInZoneRows(zones, "power", durationS) != null ||
    timeInZoneRows(zones, "hr", durationS) != null
  );
}
