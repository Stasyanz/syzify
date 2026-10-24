import type { TimeInZone } from "../../lib/types";
import { HR_ZONE_COLORS, POWER_ZONE_COLORS, zoneIndexOffset } from "./chartZones";
import { unitLabelIn, type UnitSymbol } from "../../lib/units";
import { currentLanguage, formatNumber, translate, type Language } from "../../lib/i18n";
import type { MessageKey } from "../../locales/types";

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

/** The zones' names as messages, index = zone − 1: Garmin's for heart
 * rate, Coggan's levels for power. */
const HR_ZONE_NAMES = [
  "activity.zones.hr.warmUp",
  "activity.zones.hr.easy",
  "activity.zones.hr.aerobic",
  "activity.zones.hr.threshold",
  "activity.zones.hr.maximum",
] as const satisfies readonly MessageKey[];
const POWER_ZONE_NAMES = [
  "activity.zones.power.recovery",
  "activity.zones.power.endurance",
  "activity.zones.power.tempo",
  "activity.zones.power.threshold",
  "activity.zones.power.vo2max",
  "activity.zones.power.anaerobic",
  "activity.zones.power.neuromuscular",
] as const satisfies readonly MessageKey[];

const UNIT: Record<ZoneType, UnitSymbol> = { hr: "bpm", power: "W" };
type ZoneName = (typeof HR_ZONE_NAMES)[number] | (typeof POWER_ZONE_NAMES)[number];
const NAMES: Record<ZoneType, readonly ZoneName[]> = { hr: HR_ZONE_NAMES, power: POWER_ZONE_NAMES };
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
 * Null when the type has no rows or no time in any shown zone. Names,
 * numbers and units all in `lang`; "Z1" is the same in every language.
 */
export function timeInZoneRows(
  zones: TimeInZone[],
  type: ZoneType,
  durationS: number | null,
  lang: Language = currentLanguage(),
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
    const s = z.time_s;
    const valid = isFinite(s) && s >= 0 && s <= maxS;
    secs.set(z.zone_index, Math.max(secs.get(z.zone_index) ?? 0, valid ? s : 0));
  }
  if (secs.size === 0) return null;

  const names = NAMES[type];
  const n = names.length;
  const offset = zoneIndexOffset(secs.keys(), n);
  const unit = unitLabelIn(lang, UNIT[type]);
  const total = durationS != null && durationS > 0 ? durationS : null;

  const rows: ZoneRow[] = [];
  for (let zone = 1; zone <= n; zone++) {
    const idx = zone - 1 + offset;
    const timeS = secs.get(idx) ?? 0;
    const floor = idx > 0 ? (ceil.get(idx - 1) ?? null) : 0;
    const top = zone === n ? null : (ceil.get(idx) ?? null);
    rows.push({
      zone,
      label: `Z${zone} ${translate(lang, names[zone - 1])}`, // i18n-ignore: "Z" + a number names a zone everywhere
      range: rangeLabel(floor, top, zone === n, unit, lang),
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
  lang: Language,
): string {
  const f = floor == null ? null : Math.round(floor);
  const c = top == null ? null : Math.round(top);
  const num = (v: number) => formatNumber(v, 0, lang);
  if (open) return f == null ? "" : `> ${num(f)} ${unit}`;
  if (f != null && c != null && c > f) return `${num(f)}–${num(c)} ${unit}`;
  if (c != null) return `≤ ${num(c)} ${unit}`;
  if (f != null) return `> ${num(f)} ${unit}`;
  return "";
}

/** Whether the card has anything to draw for this activity. */
export function hasTimeInZones(zones: TimeInZone[], durationS: number | null): boolean {
  return (
    timeInZoneRows(zones, "power", durationS) != null ||
    timeInZoneRows(zones, "hr", durationS) != null
  );
}
