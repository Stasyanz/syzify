import { describe, it, expect } from "vitest";
import { isPaceSport, isWaterSport, SPORT_TYPES } from "./types";

describe("isPaceSport", () => {
  it("covers every running form, matching the backend RUNNING_SPORTS", () => {
    // Regression: a run|walk|hike-only check showed trail_run/treadmill as
    // speed while their record card (backend pace PBs) showed pace.
    for (const s of ["run", "trail_run", "treadmill", "walk", "hike", "mountaineering"]) {
      expect(isPaceSport(s), `${s} should be pace`).toBe(true);
    }
  });

  it("is false for wheels and water", () => {
    for (const s of ["ride", "mountain_bike", "swim", "open_water", "strength"]) {
      expect(isPaceSport(s), `${s} should not be pace`).toBe(false);
    }
  });

  it("every known sport is classified without throwing", () => {
    for (const s of SPORT_TYPES) expect(typeof isPaceSport(s)).toBe("boolean");
  });
});

describe("isWaterSport", () => {
  it("is the two water sports only", () => {
    expect(isWaterSport("swim")).toBe(true);
    expect(isWaterSport("open_water")).toBe(true);
    expect(isWaterSport("run")).toBe(false);
    expect(isWaterSport("sailing")).toBe(false);
  });
});
