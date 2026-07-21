// @vitest-environment happy-dom
import { describe, expect, it } from "vitest";
import L from "leaflet";
import { hoverLines, findNearestPointIndex } from "./RouteMap";
import type { TrackPointColumns } from "../../lib/types";

/** Single-point track with the given metrics (null = channel absent). */
const tp = (over: Partial<TrackPointColumns> = {}): TrackPointColumns => ({
  t: [0],
  lat: [55.7],
  lon: [37.6],
  altitude_m: [120],
  distance_m: [1500],
  speed_mps: [3],
  hr: [150],
  cadence: [null],
  power_w: [null],
  temperature_c: [null],
  vertical_oscillation_mm: [null],
  stance_time_ms: [null],
  stance_time_percent: [null],
  step_length_mm: [null],
  grade_percent: [null],
  left_right_balance: [null],
  left_torque_effectiveness: [null],
  right_torque_effectiveness: [null],
  left_pedal_smoothness: [null],
  right_pedal_smoothness: [null],
  ...over,
});

describe("hoverLines", () => {
  it("shows running pace for foot sports and speed for rides", () => {
    expect(hoverLines(tp({ speed_mps: [1000 / 300] }), 0, "run")).toContain("5:00 /km");
    expect(hoverLines(tp({ speed_mps: [10] }), 0, "ride")).toContain("36.0 km/h");
  });

  it("shows swim pace for swim and open water", () => {
    expect(hoverLines(tp({ speed_mps: [1] }), 0, "open_water")).toContain("1:40 /100m");
    expect(hoverLines(tp({ speed_mps: [1] }), 0, "swim")).toContain("1:40 /100m");
  });

  it("keeps line order and drops absent channels", () => {
    expect(hoverLines(tp(), 0, "ride")).toEqual(["1.50 km", "120 m", "10.8 km/h", "150 bpm"]);
    expect(hoverLines(tp({ altitude_m: [null], hr: [null] }), 0, "ride")).toEqual([
      "1.50 km",
      "10.8 km/h",
    ]);
  });

  it("reads a standstill as -- pace, not a spike", () => {
    expect(hoverLines(tp({ speed_mps: [0] }), 0, "run")).toContain("--");
  });
});

describe("findNearestPointIndex", () => {
  // Three points spaced ~111 m apart along a meridian (0.001° lat ≈ 111 m).
  const track = tp({
    t: [0, 1, 2],
    lat: [55.7, 55.701, 55.702],
    lon: [37.6, 37.6, 37.6],
    altitude_m: [null, null, null],
    distance_m: [null, null, null],
    speed_mps: [null, null, null],
    hr: [null, null, null],
  });

  it("snaps to the closest route point within the radius", () => {
    // ~11 m north of the second point.
    expect(findNearestPointIndex(track, L.latLng(55.7011, 37.6))).toBe(1);
  });

  it("returns -1 for clicks farther than the snap radius", () => {
    // ~1.1 km east of the route.
    expect(findNearestPointIndex(track, L.latLng(55.701, 37.62))).toBe(-1);
  });

  it("skips trackpoints without GPS", () => {
    const gaps = tp({
      t: [0, 1],
      lat: [null, 55.701],
      lon: [null, 37.6],
      altitude_m: [null, null],
      distance_m: [null, null],
      speed_mps: [null, null],
      hr: [null, null],
    });
    expect(findNearestPointIndex(gaps, L.latLng(55.701, 37.6))).toBe(1);
  });

  it("honors a custom snap radius", () => {
    // ~111 m from the nearest point: outside 50 m, inside 200 m.
    expect(findNearestPointIndex(track, L.latLng(55.703, 37.6))).toBe(-1);
    expect(findNearestPointIndex(track, L.latLng(55.703, 37.6), 200)).toBe(2);
  });
});
