import { describe, it, expect } from "vitest";
import { drawCrop, cropOutputSize } from "./shareCanvas";
import { fullCrop, type CropRect } from "./shareLayout";

// Minimal 2D-context stub that records transform ops + drawImage args; lets us
// reconstruct the CTM and assert how natural-image coords land in the output.
function makeFakeCtx() {
  type M = [number, number, number, number, number, number];
  const calls: { drawImageArgs: unknown[][]; matrixAtDraw: M[] } = { drawImageArgs: [], matrixAtDraw: [] };
  let m: M = [1, 0, 0, 1, 0, 0];
  const stack: M[] = [];
  const mul = (p: M, q: M): M => [
    p[0] * q[0] + p[2] * q[1],
    p[1] * q[0] + p[3] * q[1],
    p[0] * q[2] + p[2] * q[3],
    p[1] * q[2] + p[3] * q[3],
    p[0] * q[4] + p[2] * q[5] + p[4],
    p[1] * q[4] + p[3] * q[5] + p[5],
  ];
  const ctx = {
    save() { stack.push([...m] as M); },
    restore() { const s = stack.pop(); if (s) m = s; },
    translate(x: number, y: number) { m = mul(m, [1, 0, 0, 1, x, y]); },
    scale(x: number, y: number) { m = mul(m, [x, 0, 0, y, 0, 0]); },
    rotate(r: number) { m = mul(m, [Math.cos(r), Math.sin(r), -Math.sin(r), Math.cos(r), 0, 0]); },
    drawImage(...args: unknown[]) { calls.drawImageArgs.push(args); calls.matrixAtDraw.push([...m] as M); },
  };
  const apply = (mm: M, x: number, y: number) => ({ x: mm[0] * x + mm[2] * y + mm[4], y: mm[1] * x + mm[3] * y + mm[5] });
  return { ctx, calls, apply };
}

const IMG = {} as CanvasImageSource;
const cropWith = (over: Partial<CropRect>): CropRect => ({ ...fullCrop(), ...over });
const draw = (crop: CropRect, natW: number, natH: number, outW: number, outH: number) => {
  const f = makeFakeCtx();
  drawCrop(f.ctx as unknown as CanvasRenderingContext2D, IMG, crop, natW, natH, outW, outH);
  return f;
};

describe("cropOutputSize", () => {
  it("orientation 0 / 180 keep dims; 90 / 270 swap width↔height", () => {
    const c = cropWith({ w: 0.8, h: 0.3 });
    expect(cropOutputSize({ ...c, orientation: 0 }, 4000, 3000)).toEqual({ W: 3200, H: 900 });
    expect(cropOutputSize({ ...c, orientation: 180 }, 4000, 3000)).toEqual({ W: 3200, H: 900 });
    expect(cropOutputSize({ ...c, orientation: 90 }, 4000, 3000)).toEqual({ W: 900, H: 3200 });
    expect(cropOutputSize({ ...c, orientation: 270 }, 4000, 3000)).toEqual({ W: 900, H: 3200 });
  });

  it("keeps the output aspect on the crop aspect for an extreme-aspect crop (DRIFT-1)", () => {
    // a hair-thin crop where independent rounding of W and H would drift the aspect
    // (→ an empty/clipped strip along the long side). Assert relative aspect drift.
    const crop = cropWith({ x: 0.4, w: 0.0109, h: 0.91 });
    const cropAspect = (crop.w * 1138) / (crop.h * 5588);
    const { W, H } = cropOutputSize(crop, 1138, 5588);
    expect(Math.abs((W / H) / cropAspect - 1)).toBeLessThan(0.005); // <0.5% drift
    const swapped = cropOutputSize({ ...crop, orientation: 90 }, 1138, 5588);
    expect(Math.abs((swapped.W / swapped.H) * cropAspect - 1)).toBeLessThan(0.005);
  });
});

describe("drawCrop", () => {
  it("no rotation: plain source-rect drawImage (regression-safe fast path)", () => {
    const { calls } = draw(cropWith({ x: 0.25, y: 0.25, w: 0.5, h: 0.5 }), 4000, 3000, 2000, 1500);
    expect(calls.drawImageArgs).toHaveLength(1);
    expect(calls.drawImageArgs[0].slice(1)).toEqual([1000, 750, 2000, 1500, 0, 0, 2000, 1500]);
  });

  it("orientation 90 produces a vertical output and rotates content a quarter-turn", () => {
    // landscape 2:1 photo, full frame, rotate 90° CW → portrait output 2000×4000
    const crop = cropWith({ orientation: 90 });
    const { calls, apply } = draw(crop, 4000, 2000, 2000, 4000);
    expect(calls.drawImageArgs[0]).toEqual([IMG, 0, 0]);
    const m = calls.matrixAtDraw[0];
    // crop center → output center
    expect(apply(m, 2000, 1000)).toMatchObject({ x: expect.closeTo(1000, 3), y: expect.closeTo(2000, 3) });
    // the photo's right-center maps to the output's bottom-center (90° CW)
    const right = apply(m, 3000, 1000);
    expect(right.x).toBeCloseTo(1000, 3);
    expect(right.y).toBeCloseTo(3000, 3);
  });

  it("orientation 90 of the full frame maps the photo exactly onto the output corners", () => {
    // 4000×3000 photo, full frame, 90° CW; the swapped output is fully covered.
    const crop = cropWith({ orientation: 90 });
    const { calls, apply } = draw(crop, 4000, 3000, 3000, 4000);
    const m = calls.matrixAtDraw[0];
    expect(apply(m, 2000, 1500)).toMatchObject({ x: expect.closeTo(1500, 3), y: expect.closeTo(2000, 3) });
    expect(apply(m, 0, 0)).toMatchObject({ x: expect.closeTo(3000, 3), y: expect.closeTo(0, 3) });
    expect(apply(m, 4000, 3000)).toMatchObject({ x: expect.closeTo(0, 3), y: expect.closeTo(4000, 3) });
  });

  it("straighten samples a tilted crop rect (the rotatable frame), de-rotated to upright", () => {
    // centered 0.5×0.5 crop, 20° frame tilt. Crop centre → output centre, and a point
    // 100px along the +20° axis on the photo lands 100px right of the output centre,
    // proving the sampled rect is tilted by the straighten angle.
    const crop = cropWith({ x: 0.25, y: 0.25, w: 0.5, h: 0.5, straighten: 20 });
    const { W, H } = cropOutputSize(crop, 2000, 2000); // 1000×1000
    const { calls, apply } = draw(crop, 2000, 2000, W, H);
    const m = calls.matrixAtDraw[0];
    expect(apply(m, 1000, 1000)).toMatchObject({ x: expect.closeTo(W / 2, 3), y: expect.closeTo(H / 2, 3) });
    const t = (20 * Math.PI) / 180;
    const along = apply(m, 1000 + Math.cos(t) * 100, 1000 + Math.sin(t) * 100);
    expect(along.x).toBeCloseTo(W / 2 + 100, 3);
    expect(along.y).toBeCloseTo(H / 2, 3);
  });

  it("the transform has no shear (orthogonal axes) under unequal aspect + straighten", () => {
    const crop = cropWith({ straighten: 30, orientation: 90 });
    const { calls } = draw(crop, 4000, 3000, 3000, 4000);
    const m = calls.matrixAtDraw[0];
    // columns (image x-axis vs y-axis images) must stay perpendicular → no shear
    const dot = m[0] * m[2] + m[1] * m[3];
    const len = Math.hypot(m[0], m[1]) * Math.hypot(m[2], m[3]);
    expect(Math.abs(dot) / len).toBeCloseTo(0, 6);
  });

  it("degenerate inputs are a no-op", () => {
    const { calls } = draw(cropWith({ w: 0, straighten: 10 }), 4000, 3000, 100, 100);
    expect(calls.drawImageArgs).toHaveLength(0);
  });
});
