import { describe, it, expect } from "vitest";
import { computeMapLayout, computeElevationLayout } from "./shareGeometry";

const ROUTE = [
  { lat: 50.0, lon: 30.0 },
  { lat: 50.01, lon: 30.02 },
  { lat: 50.02, lon: 30.01 },
  { lat: 50.015, lon: 30.03 },
];
const ALTS = [100, 120, 110, 140, 130, 160];

describe("computeMapLayout", () => {
  it("returns null below two points", () => {
    expect(computeMapLayout(1920, [])).toBeNull();
    expect(computeMapLayout(1920, [ROUTE[0]])).toBeNull();
  });

  it("keeps the route inside the inner box (proportional padding, no overflow)", () => {
    const L = computeMapLayout(1920, ROUTE)!;
    expect(L.start.x).toBeGreaterThanOrEqual(0);
    expect(L.start.y).toBeGreaterThanOrEqual(0);
    expect(L.end.x).toBeLessThanOrEqual(L.innerW);
    expect(L.end.y).toBeLessThanOrEqual(L.innerH);
  });

  it("never collapses to a negative/empty box even for a tiny export width", () => {
    // The old const padding (8px) made (W - pad*2) go negative for small W; the
    // proportional 0.04 padding keeps the drawable region positive at any size.
    for (const w of [40, 80, 200, 1920]) {
      const L = computeMapLayout(w, ROUTE);
      if (!L) continue;
      expect(L.innerW).toBeGreaterThan(0);
      expect(L.innerH).toBeGreaterThan(0);
      // start/end stay finite and within the box
      expect(Number.isFinite(L.start.x)).toBe(true);
      expect(L.start.x).toBeGreaterThanOrEqual(0);
      expect(L.start.x).toBeLessThanOrEqual(L.innerW);
    }
  });

  it("scales geometry linearly with export width (preview == export by construction)", () => {
    // preview width 800 of a 1920 export → the SVG viewBox is the export layout,
    // and the only difference on screen is the uniform k = 800/1920 from viewBox.
    // So the layout itself must be a pure function of exportW: doubling exportW
    // (mod rounding) doubles innerW/innerH and the coordinates.
    const a = computeMapLayout(1000, ROUTE)!;
    const b = computeMapLayout(2000, ROUTE)!;
    expect(b.innerW / a.innerW).toBeCloseTo(2, 1);
    expect(b.innerH / a.innerH).toBeCloseTo(2, 1);
    expect(b.start.x / a.start.x).toBeCloseTo(2, 1);
  });
});

describe("computeElevationLayout", () => {
  it("returns null below two samples", () => {
    expect(computeElevationLayout(1920, [])).toBeNull();
    expect(computeElevationLayout(1920, [100])).toBeNull();
  });

  it("produces a line and a closed fill path inside the box", () => {
    const L = computeElevationLayout(1920, ALTS)!;
    expect(L.lineD.startsWith("M")).toBe(true);
    expect(L.fillD.trimEnd().endsWith("Z")).toBe(true);
    expect(L.blockW).toBe(L.innerW + L.padding * 2);
    expect(L.blockH).toBe(L.innerH + L.padding * 2);
  });

  it("stays positive for a tiny export width", () => {
    const L = computeElevationLayout(60, ALTS)!;
    expect(L.innerW).toBeGreaterThan(0);
    expect(L.innerH).toBeGreaterThan(0);
    // no NaN coordinates leaked into the path
    expect(L.lineD).not.toContain("NaN");
    expect(L.fillD).not.toContain("NaN");
  });
});
