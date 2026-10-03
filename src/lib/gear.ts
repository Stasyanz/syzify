import { SPORT_TYPES, type GearItem, type GearKind, type GearRule, type RuleCandidates, type SportType } from "./types";
import { formatDistance } from "./format";

/** The sports an item of a kind is offered to be the default for — and
 * the kinds an activity of a sport is offered in Edit Activity. Offered,
 * not enforced: "other" goes with everything. */
export function kindSports(kind: GearKind): SportType[] {
  switch (kind) {
    case "bike":
      return ["ride", "mountain_bike", "indoor_ride", "virtual_ride"];
    case "shoes":
      return ["run", "trail_run", "treadmill", "virtual_run", "walk", "hike", "mountaineering"];
    default:
      return SPORT_TYPES;
  }
}

/** The kinds offered to an activity of `sport` (see kindSports). */
export function kindsForSport(sport: string): GearKind[] {
  const kinds: GearKind[] = ["other"];
  for (const kind of ["bike", "shoes"] as const) {
    if ((kindSports(kind) as string[]).includes(sport)) kinds.push(kind);
  }
  return kinds;
}

/** The items Edit Activity offers for an activity: the ones in use of a
 * kind that fits the sport, plus whatever is on it now, retired or not,
 * so a save keeps it. Bikes before shoes before the rest, by name. */
export function gearChoicesFor(items: GearItem[], sport: string, currentId: string | null): GearItem[] {
  const kinds = kindsForSport(sport);
  return items.filter((g) => g.id === currentId || (g.retired_at == null && kinds.includes(g.kind)));
}

/** "favero_electronics" → "Favero Electronics", "assioma_duo" → "Assioma Duo". */
function words(raw: string | null | undefined): string {
  return (raw ?? "")
    .split(/[_\s]+/)
    .filter(Boolean)
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
    .join(" ");
}

/** How a sensor is named to the user: "Favero Electronics Assioma Duo
 * (power meter)", falling back to its type and serial. */
export function sensorLabel(sensor: RuleCandidates["sensors"][number]): string {
  const name = [words(sensor.manufacturer), words(sensor.product)].filter(Boolean).join(" ");
  const type = sensor.device_type ? words(sensor.device_type).toLowerCase() : "";
  if (name) return type ? `${name} (${type})` : name;
  return type ? `${words(type)} ${sensor.serial}` : `Sensor ${sensor.serial}`;
}

/** The one-line summary of an item's rules for its card: "profile ROAD ·
 * sensor Favero Electronics Assioma Duo (power meter)". */
export function rulesSummary(rules: GearRule[], candidates: RuleCandidates | undefined): string {
  return rules
    .map((r) => {
      if (r.kind === "profile_name") return `profile ${r.value}`;
      const sensor = candidates?.sensors.find((s) => s.serial === r.value);
      return `sensor ${sensor ? sensorLabel(sensor) : r.value}`;
    })
    .join(" · ");
}

/** The odometer: what the item had done before Syzify plus what it has
 * done in it. */
export function odometerM(item: Pick<GearItem, "initial_distance_m" | "stats">): number {
  return item.initial_distance_m + item.stats.distance_m;
}

/** How far along its limit the item is, 0–1 (over the limit clamps to 1),
 * or null without a limit. */
export function wearFraction(item: Pick<GearItem, "initial_distance_m" | "stats" | "distance_limit_m">): number | null {
  if (item.distance_limit_m == null || item.distance_limit_m <= 0) return null;
  return Math.min(1, odometerM(item) / item.distance_limit_m);
}

/** The wear bar's tooltip: "7305.82 km of 8000.00 km · 91 %", or "past
 * the limit" once over it. */
export function wearTip(item: Pick<GearItem, "initial_distance_m" | "stats" | "distance_limit_m">, wear: number): string {
  const odo = formatDistance(odometerM(item));
  const limit = formatDistance(item.distance_limit_m);
  const over = odometerM(item) >= (item.distance_limit_m ?? Infinity);
  // floor: 99.6 % must not read as 100 % while the limit is not reached.
  return `${odo} of ${limit} · ${over ? "past the limit" : `${Math.floor(wear * 100)} %`}`;
}

/** From this share of its limit an item is "wearing out" and the dashboard
 * says so; past the limit it is "worn out". A warning, not a service log. */
export const WEAR_WARN = 0.8;

/** Where an item stands against its limit — one rule for the bar colors in
 * the Garage and the dashboard's reminder. */
export type WearState = "ok" | "warn" | "over";
export function wearState(wear: number): WearState {
  return wear >= 1 ? "over" : wear >= WEAR_WARN ? "warn" : "ok";
}
export const WEAR_COLORS: Record<WearState, string> = {
  ok: "var(--good)",
  warn: "var(--warn)",
  over: "var(--danger)",
};

/** The items in use that are at or past WEAR_WARN of their limit, most
 * worn first — what the dashboard reminds about. `wear` is the bar's
 * clamped share; the order uses the raw ratio, so of two items past their
 * limit the one further past comes first. */
export function wornItems(items: GearItem[]): { item: GearItem; wear: number }[] {
  return items
    .filter((g) => g.retired_at == null && g.distance_limit_m != null && g.distance_limit_m > 0)
    .map((item) => ({ item, wear: wearFraction(item) ?? 0, ratio: odometerM(item) / item.distance_limit_m! }))
    .filter((x) => x.wear >= WEAR_WARN)
    .sort((a, b) => b.ratio - a.ratio || a.item.name.localeCompare(b.item.name))
    .map(({ item, wear }) => ({ item, wear }));
}
