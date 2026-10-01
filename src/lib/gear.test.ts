import { describe, expect, it } from "vitest";
import type { GearItem } from "./types";
import { gearChoicesFor, kindSports, kindsForSport } from "./gear";

const item = (id: string, kind: GearItem["kind"], retired = false): GearItem => ({
  id,
  kind,
  name: id,
  brand: null,
  model: null,
  purchased_at: null,
  initial_distance_m: 0,
  distance_limit_m: null,
  retired_at: retired ? "2026-01-01T00:00:00" : null,
  notes: null,
  created_at: "2026-01-01T00:00:00",
  stats: { activities: 0, distance_m: 0, duration_s: 0, elev_gain_m: 0, last_used: null },
  default_for: [],
});

describe("gear kinds and sports", () => {
  it("offers bikes to rides, shoes to feet and anything else to every sport, both ways round", () => {
    expect(kindSports("bike")).toEqual(["ride", "mountain_bike"]);
    expect(kindSports("shoes")).toContain("hike");
    expect(kindSports("other").length).toBeGreaterThan(10);
    expect(kindsForSport("ride")).toEqual(["other", "bike"]);
    expect(kindsForSport("trail_run")).toEqual(["other", "shoes"]);
    expect(kindsForSport("swim")).toEqual(["other"]);
    expect(kindsForSport("made_up")).toEqual(["other"]);
  });

  it("offers an activity the items in use of a fitting kind, plus whatever it is on now", () => {
    const items = [item("road", "bike"), item("old-road", "bike", true), item("pegasus", "shoes"), item("helmet", "other")];
    const ids = (sport: string, current: string | null) => gearChoicesFor(items, sport, current).map((g) => g.id);
    expect(ids("ride", null)).toEqual(["road", "helmet"]);
    expect(ids("run", null)).toEqual(["pegasus", "helmet"]);
    // The retired bike it is on stays offered so a save keeps it; so does a
    // pair of shoes on a ride, odd as that is.
    expect(ids("ride", "old-road")).toEqual(["road", "old-road", "helmet"]);
    expect(ids("ride", "pegasus")).toEqual(["road", "pegasus", "helmet"]);
  });
});
