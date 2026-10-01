import { describe, expect, it } from "vitest";
import type { GearItem } from "./types";
import { gearChoicesFor, kindSports, kindsForSport, rulesSummary, sensorLabel, wearState, wornItems, WEAR_COLORS, WEAR_WARN } from "./gear";

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
  rules: [],
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

describe("rule labels", () => {
  const assioma = { serial: "3632674300", device_type: "bike_power", manufacturer: "favero_electronics", product: "assioma_duo", count: 169 };
  const bare = { serial: "111", device_type: null, manufacturer: null, product: null, count: 2 };
  const typed = { serial: "222", device_type: "bike_speed", manufacturer: null, product: null, count: 1 };

  it("names a sensor from its maker and model, else its type and serial", () => {
    expect(sensorLabel(assioma)).toBe("Favero Electronics Assioma Duo (bike power)");
    expect(sensorLabel(typed)).toBe("Bike Speed 222");
    expect(sensorLabel(bare)).toBe("Sensor 111");
    expect(sensorLabel({ ...assioma, device_type: null })).toBe("Favero Electronics Assioma Duo");
  });

  it("summarises an item's rules, naming a sensor the vault knows and showing the serial of one it does not", () => {
    const candidates = { profiles: [{ value: "ROAD", count: 169 }], sensors: [assioma] };
    expect(
      rulesSummary(
        [
          { kind: "profile_name", value: "ROAD" },
          { kind: "sensor_serial", value: "3632674300" },
          { kind: "sensor_serial", value: "999" },
        ],
        candidates,
      ),
    ).toBe("profile ROAD · sensor Favero Electronics Assioma Duo (bike power) · sensor 999");
    expect(rulesSummary([{ kind: "sensor_serial", value: "3632674300" }], undefined)).toBe("sensor 3632674300");
    expect(rulesSummary([], candidates)).toBe("");
  });
});

describe("wornItems", () => {
  const withWear = (id: string, km: number, limitKm: number | null, retired = false): GearItem => ({
    ...item(id, "shoes", retired),
    distance_limit_m: limitKm == null ? null : limitKm * 1000,
    stats: { activities: 1, distance_m: km * 1000, duration_s: 0, elev_gain_m: 0, last_used: null },
  });

  it("lists the items in use at or past the warning share of their limit, most worn first", () => {
    expect(WEAR_WARN).toBe(0.8);
    const worn = wornItems([
      withWear("fresh", 100, 800),
      withWear("edge", 640, 800),
      withWear("gone", 900, 800),
      withWear("no-limit", 5000, null),
      withWear("retired", 900, 800, true),
      withWear("almost", 790, 800),
    ]);
    expect(worn.map((w) => w.item.id)).toEqual(["gone", "almost", "edge"]);
    expect(worn[0].wear).toBe(1);
    expect(worn[2].wear).toBeCloseTo(0.8);
    // Past the limit the bar is full for both, but the one further past
    // comes first — not the one earlier in the alphabet.
    const over = wornItems([withWear("a-little", 900, 800), withWear("z-far", 1600, 800)]);
    expect(over.map((w) => w.item.id)).toEqual(["z-far", "a-little"]);
    expect(over.map((w) => w.wear)).toEqual([1, 1]);
  });

  it("names the wear state and its color by one rule", () => {
    expect(wearState(0.1)).toBe("ok");
    expect(wearState(0.79)).toBe("ok");
    expect(wearState(0.8)).toBe("warn");
    expect(wearState(0.999)).toBe("warn");
    expect(wearState(1)).toBe("over");
    expect(WEAR_COLORS.over).toBe("var(--danger)");
  });
});
