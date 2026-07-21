import { describe, it, expect, afterEach } from "vitest";
import type { TrackPointColumns } from "../../lib/types";
import { autoLaps } from "./LapsTable";
import { useUnitsStore, M_PER_MILE } from "../../lib/units";

/** Minimal TrackPointColumns with only the fields autoLaps reads. */
function track(distance_m: number[], t: number[], altitude_m: (number | null)[] = [], hr: (number | null)[] = []): TrackPointColumns {
  return {
    distance_m,
    t,
    altitude_m: altitude_m.length ? altitude_m : distance_m.map(() => null),
    hr: hr.length ? hr : distance_m.map(() => null),
  } as unknown as TrackPointColumns;
}

describe("autoLaps", () => {
  it("splits a steady run into 1 km laps + a trailing partial", () => {
    // 2.5 km at 5:00/km → 300 s per 1 km.
    const dist = [0, 500, 1000, 1500, 2000, 2500];
    const t = [0, 150, 300, 450, 600, 750];
    const laps = autoLaps(track(dist, t), "run");

    expect(laps.length).toBe(3);
    expect(laps[0].distance).toBeCloseTo(1000, 5);
    expect(laps[0].time).toBeCloseTo(300, 5);
    expect(laps[1].distance).toBeCloseTo(1000, 5);
    expect(laps[2].distance).toBeCloseTo(500, 5); // trailing partial
    // avg speed = 1000 / 300 ≈ 3.33 m/s
    expect(laps[0].speed!).toBeCloseTo(1000 / 300, 5);
  });

  it("uses 5 km laps for rides", () => {
    const dist = [0, 5000, 10000];
    const t = [0, 600, 1200];
    const laps = autoLaps(track(dist, t), "ride");
    expect(laps.length).toBe(2);
    expect(laps[0].distance).toBeCloseTo(5000, 5);
  });

  it("sums ascent and averages HR within a lap", () => {
    const dist = [0, 500, 1000];
    const t = [0, 150, 300];
    const alt = [10, 15, 12]; // +5 then -3 → ascent 5
    const hr = [140, 150, 160]; // avg over points after start: (150+160)/2 = 155
    const laps = autoLaps(track(dist, t, alt, hr), "run");
    expect(laps.length).toBe(1);
    expect(laps[0].ascent).toBeCloseTo(5, 5);
    expect(laps[0].hr!).toBeCloseTo(155, 5);
  });

  it("yields under 2 rows for a track shorter than one lap (table hides it)", () => {
    const laps = autoLaps(track([0, 200, 400], [0, 60, 120]), "run");
    expect(laps.length).toBeLessThan(2);
  });

  describe("imperial units", () => {
    afterEach(() => useUnitsStore.setState({ mode: "metric" }));

    it("splits runs into 1-mile laps", () => {
      useUnitsStore.setState({ mode: "imperial" });
      // Two full miles + a short trailing partial.
      const dist = [0, M_PER_MILE, 2 * M_PER_MILE, 2 * M_PER_MILE + 300];
      const t = [0, 480, 960, 1050];
      const laps = autoLaps(track(dist, t), "run");
      expect(laps.length).toBe(3);
      expect(laps[0].distance).toBeCloseTo(M_PER_MILE, 3);
      expect(laps[1].distance).toBeCloseTo(M_PER_MILE, 3);
    });

    it("keeps swim laps at 100 m regardless of units", () => {
      useUnitsStore.setState({ mode: "imperial" });
      const dist = [0, 100, 200, 300];
      const t = [0, 90, 180, 270];
      const laps = autoLaps(track(dist, t), "swim");
      expect(laps.length).toBe(3);
      expect(laps[0].distance).toBeCloseTo(100, 5);
    });
  });
});
