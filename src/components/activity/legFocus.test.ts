import { describe, expect, it } from "vitest";
import { isFocusableLeg, legTimeWindow, sliceTrackpoints } from "./legFocus";
import type { MultisportLeg, TrackPointColumns } from "../../lib/types";

const leg = (overrides: Partial<MultisportLeg> = {}): MultisportLeg => ({
  id: null,
  activity_id: "tri-1",
  leg_number: 1,
  sport_type: "run",
  is_transition: false,
  start_time: "2021-07-17T08:30:00+03:00",
  total_distance_m: 4300,
  total_timer_time_s: 1800,
  total_elapsed_time_s: 1851,
  avg_speed_mps: 2.3,
  avg_hr: 148,
  max_hr: 165,
  total_ascent_m: 6,
  total_calories: null,
  source_activity_id: null,
  ...overrides,
});

/** Columnar track with the given t/distance and constant other channels. */
const tp = (t: (number | null)[], distance_m: (number | null)[]): TrackPointColumns => ({
  t,
  lat: t.map((_, i) => 55 + i),
  lon: t.map(() => 37.6),
  altitude_m: t.map((_, i) => 100 + i),
  speed_mps: t.map(() => 3),
  hr: t.map(() => 150),
  cadence: t.map(() => null),
  power_w: t.map(() => null),
  temperature_c: t.map(() => null),
  vertical_oscillation_mm: t.map(() => null),
  stance_time_ms: t.map(() => null),
  stance_time_percent: t.map(() => null),
  step_length_mm: t.map(() => null),
  grade_percent: t.map(() => null),
  distance_m,
  left_right_balance: t.map(() => null),
  left_torque_effectiveness: t.map(() => null),
  right_torque_effectiveness: t.map(() => null),
  left_pedal_smoothness: t.map(() => null),
  right_pedal_smoothness: t.map(() => null),
});

describe("isFocusableLeg", () => {
  it("accepts a FIT-native sport leg with a start and a length", () => {
    expect(isFocusableLeg(leg())).toBe(true);
  });

  it("rejects transitions, merged legs, and legs without a window", () => {
    expect(isFocusableLeg(leg({ is_transition: true }))).toBe(false);
    expect(isFocusableLeg(leg({ source_activity_id: "act-1" }))).toBe(false);
    expect(isFocusableLeg(leg({ start_time: null }))).toBe(false);
    expect(
      isFocusableLeg(leg({ total_elapsed_time_s: null, total_timer_time_s: null })),
    ).toBe(false);
  });
});

describe("legTimeWindow", () => {
  it("spans elapsed time from the leg start (epoch seconds)", () => {
    const from = Date.parse("2021-07-17T08:30:00+03:00") / 1000;
    expect(legTimeWindow(leg())).toEqual([from, from + 1851]);
  });

  it("falls back to timer time when elapsed is missing", () => {
    const from = Date.parse("2021-07-17T08:30:00+03:00") / 1000;
    expect(legTimeWindow(leg({ total_elapsed_time_s: null }))).toEqual([from, from + 1800]);
  });

  it("returns null for unfocusable legs", () => {
    expect(legTimeWindow(leg({ is_transition: true }))).toBeNull();
  });
});

describe("sliceTrackpoints", () => {
  it("keeps only points inside the window, aligned across columns", () => {
    const sliced = sliceTrackpoints(tp([100, 110, 120, 130, 140], [0, 50, 100, 150, 200]), 110, 130);
    expect(sliced.t).toEqual([110, 120, 130]);
    // lat encodes the source index — alignment proof.
    expect(sliced.lat).toEqual([56, 57, 58]);
    expect(sliced.hr).toEqual([150, 150, 150]);
  });

  it("rebases cumulative distance to start at 0, preserving nulls", () => {
    const sliced = sliceTrackpoints(tp([100, 110, 120, 130], [0, 50, null, 150]), 110, 130);
    expect(sliced.distance_m).toEqual([0, null, 100]);
  });

  it("drops points without a timestamp — nothing to window them by", () => {
    const sliced = sliceTrackpoints(tp([100, null, 120], [0, 50, 100]), 100, 120);
    expect(sliced.t).toEqual([100, 120]);
    expect(sliced.lat).toEqual([55, 57]);
  });

  it("stays a valid (empty) structure when the window misses all points", () => {
    const sliced = sliceTrackpoints(tp([100, 110, 120], [0, 50, 100]), 500, 600);
    expect(sliced.t).toEqual([]);
    expect(sliced.distance_m).toEqual([]);
    expect(sliced.hr).toEqual([]);
  });

  it("an inverted window (to < from) yields an empty slice, not a crash", () => {
    const sliced = sliceTrackpoints(tp([100, 110, 120], [0, 50, 100]), 120, 100);
    expect(sliced.t).toEqual([]);
  });
});
