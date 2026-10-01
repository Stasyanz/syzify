import { SPORT_TYPES, type GearItem, type GearKind, type GearRule, type RuleCandidates, type SportType } from "./types";

/** The sports an item of a kind is offered to be the default for — and
 * the kinds an activity of a sport is offered in Edit Activity. Offered,
 * not enforced: "other" goes with everything. */
export function kindSports(kind: GearKind): SportType[] {
  switch (kind) {
    case "bike":
      return ["ride", "mountain_bike"];
    case "shoes":
      return ["run", "trail_run", "treadmill", "walk", "hike", "mountaineering"];
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
