import { describe, it, expect } from "vitest";
import { cadenceUnit, elevationIsNoise, hasSimulatedCourse, isCyclingSport, isPaceSport, isSportType, isTrainerRide, isWaterSport, sportLabel, SPORT_TYPES, triathlonDiscipline } from "./types";
import { translate, useLanguageStore, type Translator } from "./i18n";

describe("sportLabel (#233)", () => {
  it("names a known sport in the interface language", () => {
    expect(sportLabel("trail_run")).toBe("Trail Run");
    useLanguageStore.getState().setPref("ru");
    try {
      expect(SPORT_TYPES.slice(0, 5).map((s) => sportLabel(s))).toEqual([
        "Бег", "Трейл", "Беговая дорожка", "Виртуальный бег", "Велосипед",
      ]);
    } finally {
      useLanguageStore.getState().setPref("system");
    }
    useLanguageStore.getState().setPref("it");
    try {
      expect(sportLabel("hike")).toBe("Escursionismo");
    } finally {
      useLanguageStore.getState().setPref("system");
    }
  });

  it("shows an id it does not know as it came, or by the caller's word", () => {
    expect(isSportType("kitesurf")).toBe(false);
    expect(isSportType("run")).toBe(true);
    expect(sportLabel("kitesurf")).toBe("kitesurf");
    expect(sportLabel("kitesurf", "Activity")).toBe("Activity");
    expect(sportLabel("run", "Activity")).toBe("Run");
  });

  it("speaks the caller's t when given one, whatever the store says", () => {
    const ru: Translator = (key, ...args) => translate("ru", key, ...args);
    expect(sportLabel("swim", "swim", ru)).toBe("Плавание");
    expect(sportLabel("kitesurf", "Тренировка", ru)).toBe("Тренировка");
  });

  it("keeps the display order, every sport once", () => {
    expect(SPORT_TYPES[0]).toBe("run");
    expect(SPORT_TYPES[SPORT_TYPES.length - 1]).toBe("other");
    expect(new Set(SPORT_TYPES).size).toBe(SPORT_TYPES.length);
  });
});

describe("isPaceSport", () => {
  it("covers every running form, matching the backend RUNNING_SPORTS", () => {
    // Regression: a run|walk|hike-only check showed trail_run/treadmill as
    // speed while their record card (backend pace PBs) showed pace.
    for (const s of ["run", "trail_run", "treadmill", "virtual_run", "walk", "hike", "mountaineering"]) {
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

describe("isCyclingSport and cadenceUnit (#232)", () => {
  it("count a swim's and a paddle's cadence in strokes", () => {
    for (const s of ["swim", "open_water", "paddle"]) {
      expect(isCyclingSport(s)).toBe(false);
      expect(cadenceUnit(s)).toBe("strokes/min");
    }
  });

  it("count a ride's cadence in crank turns, anything else in steps", () => {
    for (const s of ["ride", "mountain_bike", "indoor_ride", "virtual_ride"]) {
      expect(isCyclingSport(s)).toBe(true);
      expect(cadenceUnit(s)).toBe("rpm");
    }
    for (const s of ["run", "trail_run", "virtual_run", "walk", "hike"]) {
      expect(isCyclingSport(s)).toBe(false);
      expect(cadenceUnit(s)).toBe("spm");
    }
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
  it("is the virtual ride and the virtual run (#190, #192)", () => {
    expect(hasSimulatedCourse("virtual_ride")).toBe(true);
    expect(hasSimulatedCourse("virtual_run")).toBe(true);
    expect(hasSimulatedCourse("treadmill")).toBe(false);
    expect(hasSimulatedCourse("run")).toBe(false);
    expect(hasSimulatedCourse("indoor_ride")).toBe(false);
    expect(hasSimulatedCourse("ride")).toBe(false);
  });
});

describe("virtual run (#192)", () => {
  it("is a running sport of its own: pace, shoes' side of the list, no leg of an event", () => {
    expect(SPORT_TYPES).toContain("virtual_run");
    expect(isPaceSport("virtual_run")).toBe(true);
    expect(isTrainerRide("virtual_run")).toBe(false);
    expect(triathlonDiscipline("treadmill")).toBe("run");
    expect(triathlonDiscipline("virtual_run")).toBeNull();
    // A simulated climb is the course's, as on a virtual ride.
    expect(elevationIsNoise("virtual_run")).toBe(false);
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
