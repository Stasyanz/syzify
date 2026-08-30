import { describe, it, expect } from "vitest";
import {
  fullCrop,
  centeredCrop,
  normalizeStraighten,
  normalizeOrientation,
  straightenQuarter,
  autoQuarterOrientation,
  avoidZone,
  orientedPresetRatio,
} from "./shareLayout";

describe("orientedPresetRatio", () => {
  it("inverts a wide ratio for a portrait photo", () => {
    expect(orientedPresetRatio(16 / 9, 1080, 1920)).toBeCloseTo(9 / 16, 5);
  });

  it("keeps a wide ratio on landscape and square photos", () => {
    expect(orientedPresetRatio(16 / 9, 1920, 1080)).toBeCloseTo(16 / 9, 5);
    expect(orientedPresetRatio(16 / 9, 1000, 1000)).toBeCloseTo(16 / 9, 5);
  });

  it("leaves the square preset alone everywhere", () => {
    expect(orientedPresetRatio(1, 1080, 1920)).toBe(1);
    expect(orientedPresetRatio(1, 1920, 1080)).toBe(1);
  });

  it("never inverts an already-vertical ratio", () => {
    expect(orientedPresetRatio(9 / 16, 1920, 1080)).toBeCloseTo(9 / 16, 5);
    expect(orientedPresetRatio(9 / 16, 1080, 1920)).toBeCloseTo(9 / 16, 5);
  });
});

describe("centeredCrop", () => {
  it("returns the full image for the matching ratio", () => {
    const c = centeredCrop(1600, 900, 16 / 9);
    expect(c.w).toBeCloseTo(1, 5);
    expect(c.h).toBeCloseTo(1, 5);
  });

  it("crops width for a square target on a landscape image", () => {
    // 1000×500 (2:1) cropped to 1:1 → half width, full height, centered.
    const c = centeredCrop(1000, 500, 1);
    expect(c.w).toBeCloseTo(0.5, 5);
    expect(c.h).toBeCloseTo(1, 5);
    expect(c.x).toBeCloseTo(0.25, 5);
    expect(c.y).toBeCloseTo(0, 5);
  });

  it("crops height for a wide target on a portrait image", () => {
    // 500×1000 (1:2) cropped to 16:9 → full width, reduced height, centered.
    const c = centeredCrop(500, 1000, 16 / 9);
    expect(c.w).toBeCloseTo(1, 5);
    expect(c.h).toBeCloseTo((500 / 1000) / (16 / 9), 5);
    expect(c.x).toBeCloseTo(0, 5);
    expect(c.y).toBeCloseTo((1 - c.h) / 2, 5);
  });

  it("stays within [0,1] and centered", () => {
    const c = centeredCrop(1920, 1080, 1);
    expect(c.x).toBeGreaterThanOrEqual(0);
    expect(c.y).toBeGreaterThanOrEqual(0);
    expect(c.x + c.w).toBeLessThanOrEqual(1.0001);
    expect(c.h).toBeCloseTo(1, 5);
    expect(c.x).toBeCloseTo((1 - c.w) / 2, 5);
  });

  it("degrades to full crop on bad input", () => {
    expect(centeredCrop(0, 0, 1)).toEqual(fullCrop());
  });
});

describe("normalizeStraighten", () => {
  it("wraps into (-180, 180] and degrades non-finite to 0", () => {
    expect(normalizeStraighten(10)).toBe(10);
    expect(normalizeStraighten(180)).toBe(180);
    expect(normalizeStraighten(181)).toBe(-179);
    expect(normalizeStraighten(200)).toBe(-160);
    expect(normalizeStraighten(-1000)).toBe(80);
    expect(normalizeStraighten(360)).toBe(0);
    expect(normalizeStraighten(NaN)).toBe(0);
    expect(normalizeStraighten(Infinity)).toBe(0);
  });
});

describe("normalizeOrientation", () => {
  it("snaps to the nearest quarter-turn in {0,90,180,270}", () => {
    expect(normalizeOrientation(0)).toBe(0);
    expect(normalizeOrientation(90)).toBe(90);
    expect(normalizeOrientation(270)).toBe(270);
    expect(normalizeOrientation(-90)).toBe(270);
    expect(normalizeOrientation(360)).toBe(0);
    expect(normalizeOrientation(450)).toBe(90);
    expect(normalizeOrientation(44)).toBe(0);
    expect(normalizeOrientation(46)).toBe(90);
    expect(normalizeOrientation(NaN)).toBe(0);
  });
});

describe("avoidZone", () => {
  // Watermark-like reserved zone in the bottom-right corner.
  const ZONE = { x: 0.85, y: 0.85, w: 0.13, h: 0.13 };

  it("leaves a non-overlapping block untouched", () => {
    expect(avoidZone(0.1, 0.1, 0.3, 0.2, ZONE)).toEqual({ x: 0.1, y: 0.1 });
    // Touching edges exactly is not an overlap.
    expect(avoidZone(0.55, 0.9, 0.3, 0.05, ZONE)).toEqual({ x: 0.55, y: 0.9 });
  });

  it("pushes up when that clears the zone with the smaller move", () => {
    // Block dipping into the zone from above: 0.01 vertical penetration vs
    // 0.11 horizontal — up wins.
    const r = avoidZone(0.86, 0.84, 0.1, 0.02, ZONE);
    expect(r.x).toBeCloseTo(0.86, 6);
    expect(r.y).toBeCloseTo(0.85 - 0.02, 6);
  });

  it("pushes left when that move is smaller", () => {
    // Tall thin block clipping the zone's left edge: 0.02 horizontal
    // penetration vs 0.13 vertical — left wins.
    const r = avoidZone(0.84, 0.86, 0.03, 0.12, ZONE);
    expect(r.x).toBeCloseTo(0.85 - 0.03, 6);
    expect(r.y).toBeCloseTo(0.86, 6);
  });

  it("never pushes past the canvas edge", () => {
    // Left push would need x = zone.x - w = -0.1 — clamped to 0.
    const r = avoidZone(0, 0.5, 0.6, 0.48, { x: 0.5, y: 0.85, w: 0.48, h: 0.13 });
    expect(r.x).toBe(0);
    expect(r.y).toBeCloseTo(0.5, 6);
  });

  it("degenerate sizes are a no-op (unmeasured happy-dom elements)", () => {
    expect(avoidZone(0.9, 0.9, 0, 0, ZONE)).toEqual({ x: 0.9, y: 0.9 });
    expect(avoidZone(0.9, 0.9, 0.1, 0.1, { x: 0, y: 0, w: 0, h: 0 })).toEqual({ x: 0.9, y: 0.9 });
  });
});

describe("straightenQuarter", () => {
  it("returns the nearest multiple of 90 to the wrapped angle", () => {
    expect(straightenQuarter(0)).toBe(0);
    expect(straightenQuarter(10)).toBe(0);
    expect(straightenQuarter(46)).toBe(90);
    expect(straightenQuarter(90)).toBe(90);
    expect(straightenQuarter(134)).toBe(90);
    expect(straightenQuarter(136)).toBe(180);
    expect(straightenQuarter(-46)).toBe(-90);
    expect(straightenQuarter(-170)).toBe(-180);
  });

  it("halfway points fold away from zero symmetrically (both slider ends)", () => {
    expect(straightenQuarter(45)).toBe(90);
    expect(straightenQuarter(-45)).toBe(-90);
  });
});

describe("autoQuarterOrientation", () => {
  const base = fullCrop();

  it("folds a quarter turn of the frame into the output orientation", () => {
    // The user's report: a 16:9 frame turned to 90° must export a vertical
    // image, not the photo on its side.
    const prev = { ...base, straighten: 0, orientation: 0 };
    const next = autoQuarterOrientation(prev, { ...prev, straighten: 90 });
    expect(next.orientation).toBe(90);
    expect(next.straighten).toBe(90);
  });

  it("unfolds when the frame turns back", () => {
    const prev = { ...base, straighten: 90, orientation: 90 };
    const next = autoQuarterOrientation(prev, { ...prev, straighten: 0 });
    expect(next.orientation).toBe(0);
  });

  it("leaves orientation alone within the same quarter (plain tilt)", () => {
    const prev = { ...base, straighten: 10, orientation: 90 };
    const next = autoQuarterOrientation(prev, { ...prev, straighten: 30 });
    expect(next.orientation).toBe(90);
  });

  it("stacks on top of a button-chosen orientation", () => {
    const prev = { ...base, straighten: 0, orientation: 90 };
    const next = autoQuarterOrientation(prev, { ...prev, straighten: 90 });
    expect(next.orientation).toBe(180);
  });

  it("is stable across the ±180 wrap (no orientation jump)", () => {
    const prev = { ...base, straighten: 170, orientation: 180 };
    const next = autoQuarterOrientation(prev, { ...prev, straighten: -170 });
    // 170 and -170 fold to 180 and -180: a -360 delta ≡ 0 — nothing changes.
    expect(next.orientation).toBe(180);
  });
});
