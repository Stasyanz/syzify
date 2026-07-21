// @vitest-environment happy-dom
// (leaflet touches `window` at import time; the math under test is pure)
import { describe, expect, it } from "vitest";
import {
  clusterLocations,
  clusterSpreadMeters,
} from "./clusterLocations";

// Two Berlin parks ~5 km apart and one Munich point ~500 km away.
const TIERGARTEN = { lat: 52.5145, lon: 13.35, id: "a" };
const TREPTOWER = { lat: 52.4884, lon: 13.4699, id: "b" };
const MUNICH = { lat: 48.1374, lon: 11.5755, id: "c" };

describe("clusterLocations", () => {
  it("merges everything nearby when zoomed out and splits as zoom grows", () => {
    const points = [TIERGARTEN, TREPTOWER, MUNICH];

    // Country level: the two Berlin parks are pixels apart → one cluster,
    // Munich stays its own.
    const far = clusterLocations(points, 6);
    expect(far.map((c) => c.members.length).sort()).toEqual([1, 2]);

    // City level: the parks are hundreds of pixels apart → three singles.
    const near = clusterLocations(points, 12);
    expect(near).toHaveLength(3);
    expect(near.every((c) => c.members.length === 1)).toBe(true);
  });

  it("places the cluster marker at the members' centroid", () => {
    const [cluster] = clusterLocations([TIERGARTEN, TREPTOWER], 6).filter(
      (c) => c.members.length === 2,
    );
    expect(cluster.lat).toBeCloseTo((TIERGARTEN.lat + TREPTOWER.lat) / 2, 6);
    expect(cluster.lon).toBeCloseTo((TIERGARTEN.lon + TREPTOWER.lon) / 2, 6);
  });

  it("keeps every input point exactly once", () => {
    const points = Array.from({ length: 40 }, (_, i) => ({
      id: String(i),
      lat: 52.5 + (i % 7) * 0.01,
      lon: 13.3 + Math.floor(i / 7) * 0.01,
    }));
    for (const zoom of [4, 9, 13, 17]) {
      const ids = clusterLocations(points, zoom)
        .flatMap((c) => c.members.map((m) => m.id))
        .sort((a, b) => Number(a) - Number(b));
      expect(ids).toEqual(points.map((p) => p.id));
    }
  });

  it("handles empty input", () => {
    expect(clusterLocations([], 10)).toEqual([]);
  });
});

describe("clusterSpreadMeters", () => {
  it("is ~0 for one spot and grows with real spread", () => {
    expect(clusterSpreadMeters([TIERGARTEN])).toBe(0);
    expect(
      clusterSpreadMeters([TIERGARTEN, { ...TIERGARTEN }]),
    ).toBeCloseTo(0, 5);
    const spread = clusterSpreadMeters([TIERGARTEN, TREPTOWER]);
    expect(spread).toBeGreaterThan(4000);
    expect(spread).toBeLessThan(12000);
  });
});
