import { describe, it, expect, beforeEach, afterEach } from "vitest";
import {
  useUnitsStore,
  readStoredUnits,
  isImperial,
  distanceUnit,
  elevationUnit,
  speedUnit,
  toDistance,
  toElevation,
  toWeight,
  weightUnit,
  LB_PER_KG,
  M_PER_MILE,
  FT_PER_M,
  unitLabel,
  type UnitSymbol,
} from "./units";
import { useLanguageStore } from "./i18n";

describe("unitLabel (#232)", () => {
  afterEach(() => useLanguageStore.getState().setPref("system"));

  const ALL: UnitSymbol[] = [
    "km", "mi", "m", "ft", "yd", "km/h", "mph", "kg", "lb", "bpm", "W", "rpm", "spm", "strokes/min", "kcal", "h",
    "min", "mm",
  ];

  it("says a symbol as it is in English, strokes as steps", () => {
    expect(ALL.map(unitLabel)).toEqual(ALL.map((u) => (u === "strokes/min" ? "spm" : u)));
  });

  it("writes the Russian symbols", () => {
    useLanguageStore.getState().setPref("ru");
    expect(ALL.map(unitLabel)).toEqual([
      "км", "ми", "м", "фт", "ярд", "км/ч", "ми/ч", "кг", "фнт", "уд/мин", "Вт", "об/мин", "шаг/мин", "гр/мин",
      "ккал", "ч", "мин", "мм",
    ]);
  });

  it("keeps the international symbols in Italian, steps and strokes per minute aside", () => {
    useLanguageStore.getState().setPref("it");
    const own: Partial<Record<UnitSymbol, string>> = { spm: "ppm", "strokes/min": "br/min" };
    expect(ALL.map(unitLabel)).toEqual(ALL.map((u) => own[u] ?? u));
  });
});

describe("units store", () => {
  afterEach(() => useUnitsStore.setState({ mode: "metric" }));

  it("defaults to metric when storage is unavailable", () => {
    // The node test env has no localStorage; readStoredUnits must degrade to
    // the metric default rather than throw (the guard in units.ts).
    expect(readStoredUnits()).toBe("metric");
  });

  it("setMode updates the mode without throwing when storage is absent", () => {
    expect(() => useUnitsStore.getState().setMode("imperial")).not.toThrow();
    expect(useUnitsStore.getState().mode).toBe("imperial");
    useUnitsStore.getState().setMode("metric");
    expect(useUnitsStore.getState().mode).toBe("metric");
  });
});

describe("unit conversions", () => {
  beforeEach(() => useUnitsStore.setState({ mode: "metric" }));
  afterEach(() => useUnitsStore.setState({ mode: "metric" }));

  it("metric passes distance/elevation through unchanged", () => {
    expect(isImperial()).toBe(false);
    expect(distanceUnit()).toBe("km");
    expect(elevationUnit()).toBe("m");
    expect(speedUnit()).toBe("km/h");
    expect(toDistance(5000)).toBeCloseTo(5, 9); // metres → km
    expect(toElevation(120)).toBe(120);
  });

  it("imperial converts and round-trips without drift", () => {
    useUnitsStore.setState({ mode: "imperial" });
    expect(distanceUnit()).toBe("mi");
    expect(elevationUnit()).toBe("ft");
    expect(speedUnit()).toBe("mph");
    expect(weightUnit()).toBe("lb");
    expect(toWeight(100)).toBeCloseTo(100 * LB_PER_KG, 6);

    // 1 mile in metres → 1.0 mi display.
    expect(toDistance(M_PER_MILE)).toBeCloseTo(1, 9);
    expect(toElevation(100)).toBeCloseTo(100 * FT_PER_M, 6);

    // Round-trip a filter value (display → metres → display) is stable.
    const metres = 4200;
    const displayed = toDistance(metres); // miles
    const backToMetres = displayed * M_PER_MILE;
    expect(backToMetres).toBeCloseTo(metres, 6);
  });
});
