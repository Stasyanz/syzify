import { describe, it, expect } from "vitest";
import { elevationIsNoise, hasSimulatedCourse, isPaceSport, isTrainerRide, isWaterSport, SPORT_TYPES, triathlonDiscipline } from "./types";

describe("isPaceSport", () => {
  it("covers every running form, matching the backend RUNNING_SPORTS", () => {
    // Regression: a run|walk|hike-only check showed trail_run/treadmill as
    // speed while their record card (backend pace PBs) showed pace.
    for (const s of ["run", "trail_run", "treadmill", "walk", "hike", "mountaineering"]) {
      expect(isPaceSport(s), `${s} should be pace`).toBe(true);
    }
  });

  it("is false for wheels and water", () => {
    for (const s of ["ride", "mountain_bike", "indoor_ride", "virtual_ride", "swim", "open_water", "strength"]) {
      expect(isPaceSport(s), `${s} should not be pace`).toBe(false);
    }
  });

  it("every known sport is classified without throwing", () => {
    for (const s of SPORT_TYPES) expect(typeof isPaceSport(s)).toBe("boolean");
  });
});

describe("isTrainerRide", () => {
  it("is the two rides that never left the room (#189)", () => {
    expect(isTrainerRide("indoor_ride")).toBe(true);
    expect(isTrainerRide("virtual_ride")).toBe(true);
    expect(isTrainerRide("ride")).toBe(false);
    expect(isTrainerRide("treadmill")).toBe(false);
  });

  it("is cycling, but no leg of an event", () => {
    expect(SPORT_TYPES).toContain("indoor_ride");
    expect(SPORT_TYPES).toContain("virtual_ride");
    expect(triathlonDiscipline("ride")).toBe("bike");
    expect(triathlonDiscipline("indoor_ride")).toBeNull();
    expect(triathlonDiscipline("virtual_ride")).toBeNull();
  });
});

describe("hasSimulatedCourse", () => {
  it("is the virtual ride alone (#190)", () => {
    expect(hasSimulatedCourse("virtual_ride")).toBe(true);
    expect(hasSimulatedCourse("indoor_ride")).toBe(false);
    expect(hasSimulatedCourse("ride")).toBe(false);
  });
});

describe("elevationIsNoise", () => {
  it("hides the gain of water and of a trainer with no course, keeps a virtual climb", () => {
    expect(elevationIsNoise("swim")).toBe(true);
    expect(elevationIsNoise("indoor_ride")).toBe(true);
    expect(elevationIsNoise("virtual_ride")).toBe(false);
    expect(elevationIsNoise("ride")).toBe(false);
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
