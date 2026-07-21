import { describe, it, expect } from "vitest";
import {
  rot,
  snapAngle,
  containRotated,
  resizeRotatedCorner,
  rotateKnobAngle,
  clampSizeRotated,
  clampCropRotated,
  fitRotatedScale,
  MIN_PX,
  type Corner,
  type Rect,
} from "./cropOverlayMath";

const deg2rad = (d: number) => (d * Math.PI) / 180;

describe("rot", () => {
  it("rotates a vector clockwise (screen convention)", () => {
    const r = rot(1, 0, Math.PI / 2);
    expect(r.x).toBeCloseTo(0, 6);
    expect(r.y).toBeCloseTo(1, 6);
  });
});

describe("snapAngle", () => {
  it("snaps to cardinals within 4° (including the ±180 wrap)", () => {
    expect(snapAngle(2, false)).toBe(0);
    expect(snapAngle(88, false)).toBe(90);
    expect(snapAngle(176, false)).toBe(180);
    expect(snapAngle(184, false)).toBe(180); // 184 is within 4° of 180 (via wrap)
    expect(snapAngle(-176, false)).toBe(180); // -176 wraps to within 4° of 180
    expect(snapAngle(-88, false)).toBe(-90);
  });
  it("leaves angles outside the snap zone untouched", () => {
    expect(snapAngle(185, false)).toBe(185);
    expect(snapAngle(40, false)).toBe(40);
  });
  it("snaps to 15° steps with Shift", () => {
    expect(snapAngle(7, true)).toBe(0);
    expect(snapAngle(20, true)).toBe(15);
    expect(snapAngle(-23, true)).toBe(-30);
  });
});

describe("containRotated", () => {
  it("keeps a fitting box inside the photo", () => {
    expect(containRotated(0, 0, 100, 100, 0, 200, 200)).toMatchObject({ x: 0, y: 0 });
    // overflowing position is pulled back in
    const c = containRotated(180, 180, 100, 100, 0, 200, 200);
    expect(c.x).toBeCloseTo(100, 6); // x + w == 200
    expect(c.y).toBeCloseTo(100, 6);
  });
  it("accounts for rotation: a box near the edge is pulled in by its rotated bbox", () => {
    // 100×100 box at top-left, rotated 45° → bbox half-extent ≈ 70.7, so center ≥ 70.7
    const c = containRotated(0, 0, 100, 100, deg2rad(45), 400, 400);
    const cx = c.x + 50;
    expect(cx).toBeGreaterThanOrEqual(70.7 - 0.1);
  });
  it("centers an oversized rotated box (it will overhang)", () => {
    const c = containRotated(0, 0, 200, 200, deg2rad(45), 200, 200);
    expect(c.x + 100).toBeCloseTo(100, 6); // centered on x
    expect(c.y + 100).toBeCloseTo(100, 6);
  });
});

describe("resizeRotatedCorner", () => {
  // sign of the drag delta that grows the box, per corner
  const GROW: Record<Corner, { dx: number; dy: number }> = {
    se: { dx: 1, dy: 1 },
    ne: { dx: 1, dy: -1 },
    sw: { dx: -1, dy: 1 },
    nw: { dx: -1, dy: -1 },
  };
  // the anchor is the opposite corner; its position relative to the center
  const ANCHOR: Record<Corner, { sx: number; sy: number }> = {
    se: { sx: -1, sy: -1 },
    ne: { sx: -1, sy: 1 },
    sw: { sx: 1, sy: -1 },
    nw: { sx: 1, sy: 1 },
  };
  const corners = Object.keys(GROW) as Corner[];
  const anchorOf = (c: Corner, r: Rect, ang: number) => {
    const a = ANCHOR[c];
    const v = rot((a.sx * r.w) / 2, (a.sy * r.h) / 2, ang);
    return { x: r.x + r.w / 2 + v.x, y: r.y + r.h / 2 + v.y };
  };

  it("each corner grows the box with the opposite corner fixed (no rotation)", () => {
    const start = { x: 50, y: 50, w: 100, h: 100 };
    const expected: Record<Corner, Rect> = {
      se: { x: 50, y: 50, w: 120, h: 120 },
      ne: { x: 50, y: 30, w: 120, h: 120 },
      sw: { x: 30, y: 50, w: 120, h: 120 },
      nw: { x: 30, y: 30, w: 120, h: 120 },
    };
    for (const c of corners) {
      const r = resizeRotatedCorner(c, start, 100, 100, 0, GROW[c].dx * 20, GROW[c].dy * 20, null);
      expect(r, c).toMatchObject(expected[c]);
    }
  });

  it("each corner keeps its anchor fixed in screen space when rotated", () => {
    const ang = deg2rad(35);
    const start = { x: 50, y: 50, w: 120, h: 80 };
    const cx = start.x + start.w / 2;
    const cy = start.y + start.h / 2;
    for (const c of corners) {
      const before = anchorOf(c, start, ang);
      const r = resizeRotatedCorner(c, start, cx, cy, ang, 30, -10, null);
      const after = anchorOf(c, r, ang);
      expect(after.x, c).toBeCloseTo(before.x, 4);
      expect(after.y, c).toBeCloseTo(before.y, 4);
    }
  });

  it("honors the ratio lock and the minimum size", () => {
    const start = { x: 0, y: 0, w: 100, h: 100 };
    const r = resizeRotatedCorner("se", start, 50, 50, 0, 100, 0, 2); // ratio 2:1
    expect(r.w / r.h).toBeCloseTo(2, 6);
    const tiny = resizeRotatedCorner("se", start, 50, 50, 0, -1000, -1000, null);
    expect(tiny.w).toBeGreaterThanOrEqual(MIN_PX);
    expect(tiny.h).toBeGreaterThanOrEqual(MIN_PX);
  });

  it("keeps the ratio lock AND the anchor under rotation (combined)", () => {
    const ang = deg2rad(25);
    const start = { x: 40, y: 60, w: 160, h: 80 };
    const cx = start.x + start.w / 2;
    const cy = start.y + start.h / 2;
    for (const c of corners) {
      const before = anchorOf(c, start, ang);
      const r = resizeRotatedCorner(c, start, cx, cy, ang, GROW[c].dx * 25, GROW[c].dy * 15, 2);
      expect(r.w / r.h, c).toBeCloseTo(2, 6);
      const after = anchorOf(c, r, ang);
      expect(after.x, c).toBeCloseTo(before.x, 4);
      expect(after.y, c).toBeCloseTo(before.y, 4);
    }
  });
});

describe("clampSizeRotated", () => {
  it("at 0° matches the axis-aligned clamp (free and ratio-locked)", () => {
    expect(clampSizeRotated(500, 250, 0, 400, 300, null)).toEqual({ w: 400, h: 250 });
    expect(clampSizeRotated(1, 1, 0, 400, 300, null)).toEqual({ w: MIN_PX, h: MIN_PX });
    const r = clampSizeRotated(500, 250, 0, 400, 400, 2);
    expect(r.w).toBeCloseTo(400, 6);
    expect(r.h).toBeCloseTo(200, 6);
  });

  it("at 90° the sides swap bounds — the regression: a quarter-turned frame on a portrait photo grows to the photo HEIGHT", () => {
    // Portrait photo 400×600, frame turned 90°: its local width spans the
    // photo vertically, so it may reach 600 — the old w ≤ bw cap stopped at 400.
    const r = clampSizeRotated(600, 300, deg2rad(90), 400, 600, null);
    expect(r.w).toBeCloseTo(600, 6);
    expect(r.h).toBeCloseTo(300, 6);
    // …and the local height is now capped by the photo WIDTH.
    const r2 = clampSizeRotated(300, 500, deg2rad(90), 400, 600, null);
    expect(r2.h).toBeCloseTo(400, 6);
  });

  it("at 45° the clamped size's rotated bbox fits the photo (no corner overflow)", () => {
    const ang = deg2rad(45);
    const { w, h } = clampSizeRotated(500, 500, ang, 400, 300, null);
    const c = Math.SQRT1_2;
    expect(w * c + h * c).toBeLessThanOrEqual(400 + 1e-6);
    expect(w * c + h * c).toBeLessThanOrEqual(300 + 1e-6);
  });

  it("keeps a ratio lock exact under rotation (uniform shrink to fit)", () => {
    const ang = deg2rad(30);
    const { w, h } = clampSizeRotated(800, 400, ang, 400, 300, 2);
    expect(w / h).toBeCloseTo(2, 6);
    const c = Math.abs(Math.cos(ang));
    const s = Math.abs(Math.sin(ang));
    expect(w * c + h * s).toBeLessThanOrEqual(400 + 1e-6);
    expect(w * s + h * c).toBeLessThanOrEqual(300 + 1e-6);
  });

  it("leaves an in-bounds tilted size unchanged", () => {
    const r = clampSizeRotated(100, 80, deg2rad(20), 400, 300, null);
    expect(r.w).toBeCloseTo(100, 6);
    expect(r.h).toBeCloseTo(80, 6);
  });
});

describe("clampCropRotated", () => {
  it("keeps a legal 90° crop whose normalized w exceeds 1 (portrait photo)", () => {
    // 400×600 photo, frame turned 90°: local 500×250 px, centered — normalized
    // w = 1.25 and x < 0 are correct here and must survive the sanitizer.
    const crop = { x: -0.125, y: 175 / 600, w: 1.25, h: 250 / 600, straighten: 90, orientation: 0 };
    const r = clampCropRotated(crop, 400, 600);
    expect(r.w).toBeCloseTo(1.25, 6);
    expect(r.h).toBeCloseTo(250 / 600, 6);
    expect(r.x).toBeCloseTo(-0.125, 6);
    expect(r.y).toBeCloseTo(175 / 600, 6);
  });

  it("keeps a full-photo crop tilted to an intermediate angle intact (overhang by design)", () => {
    // Regression: the old shrink-to-fit collapsed this into a 1%-wide sliver —
    // the compose preview/export showed a thin vertical strip after dragging
    // the Rotate slider to ~45°. The frame must stay full-size and centered;
    // the export fills the overhanging corners with the backdrop.
    const crop = { x: 0, y: 0, w: 1, h: 1, straighten: 45, orientation: 0 };
    const r = clampCropRotated(crop, 400, 600);
    expect(r.w).toBeCloseTo(1, 6);
    expect(r.h).toBeCloseTo(1, 6);
    expect(r.x).toBeCloseTo(0, 6);
    expect(r.y).toBeCloseTo(0, 6);
  });

  it("caps runaway sides at the photo diagonal (sanity only, not bbox fit)", () => {
    const diag = Math.hypot(400, 600);
    const r = clampCropRotated({ x: 0, y: 0, w: 5, h: 0.5, straighten: 90, orientation: 0 }, 400, 600);
    expect(r.w * 400).toBeCloseTo(diag, 4);
  });

  it("at straighten 0 an in-bounds crop is pulled inside the photo", () => {
    // Position overflows: contained back to x ∈ [0, 1-w], like the old clamp.
    const r = clampCropRotated({ x: 0.8, y: 0.5, w: 0.6, h: 0.3, straighten: 0, orientation: 90 }, 400, 600);
    expect(r.w).toBeCloseTo(0.6, 6);
    expect(r.x).toBeCloseTo(0.4, 6);
    expect(r.y).toBeCloseTo(0.5, 6);
    expect(r.orientation).toBe(90);
  });

  it("falls back to the full photo on non-finite values", () => {
    const r = clampCropRotated(
      { x: NaN, y: NaN, w: NaN, h: NaN, straighten: NaN, orientation: NaN },
      400,
      600
    );
    expect(r).toEqual({ x: 0, y: 0, w: 1, h: 1, straighten: 0, orientation: 0 });
  });
});

describe("fitRotatedScale", () => {
  it("returns 1 when the rect already fits", () => {
    expect(fitRotatedScale(100, 100, 0, 200, 200)).toBe(1);
    expect(fitRotatedScale(100, 50, deg2rad(10), 200, 200)).toBe(1);
  });

  it("shrinks a square so its 45°-rotated bbox fits the box", () => {
    // bbox of a 200×200 square at 45° is 200√2 → scale 1/√2
    expect(fitRotatedScale(200, 200, deg2rad(45), 200, 200)).toBeCloseTo(Math.SQRT1_2, 6);
  });

  it("uses the tighter axis of the rotated bbox", () => {
    // 400×100 at 90°: bbox is 100×400 in a 400×300 box → limited by height, 300/400
    expect(fitRotatedScale(400, 100, deg2rad(90), 400, 300)).toBeCloseTo(0.75, 6);
  });
});

describe("rotateKnobAngle", () => {
  it("starts at the current angle (no jump) when the pointer hasn't moved", () => {
    const start = { x: 0, y: 0, w: 100, h: 100 };
    for (const a of [0, 20, -30, 90]) {
      const deg = rotateKnobAngle(start, 50, 50, deg2rad(a), 28, 0, 0, false);
      expect(deg).toBeCloseTo(a, 4);
    }
  });
});
