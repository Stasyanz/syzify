import { describe, it, expect } from "vitest";
import { tipTransform } from "./chartTip";

/** Where a tip `tipW` px wide lands under the transform: its left edge, by
 * CSS clamp(MIN, VAL, MAX) = max(MIN, min(VAL, MAX)) with % of the tip's own
 * width — what the browser computes from the string. */
function placed(x: number, width: number, tipW: number): number {
  const m = tipTransform(x, width).match(
    /^translateX\(clamp\((-?[\d.]+)px, -50%, calc\((-?[\d.]+)px - 100%\)\)\)$/,
  );
  expect(m, tipTransform(x, width)).toBeTruthy();
  const min = Number(m![1]);
  const max = Number(m![2]) - tipW;
  return x + Math.max(min, Math.min(-tipW / 2, max));
}

describe("tipTransform (#206)", () => {
  const W = 800;

  it("centres the tip on the anchor while it fits", () => {
    expect(placed(400, W, 200)).toBe(300);
    expect(placed(150, W, 200)).toBe(50);
  });

  it("pins the tip to the edge it would cross, at either end", () => {
    expect(placed(10, W, 200)).toBe(0);
    expect(placed(0, W, 200)).toBe(0);
    expect(placed(790, W, 200)).toBe(W - 200);
    expect(placed(W, W, 200)).toBe(W - 200);
  });

  it("fits a tip wider than half the box anywhere, with no jump across the middle", () => {
    for (const x of [0, 100, 399, 400, 401, 700, W]) {
      const left = placed(x, W, 500);
      expect(left).toBeGreaterThanOrEqual(0);
      expect(left + 500).toBeLessThanOrEqual(W);
    }
    expect(placed(401, W, 300) - placed(399, W, 300)).toBe(2);
  });

  it("sits a tip wider than the whole box flush left", () => {
    expect(placed(700, W, 900)).toBe(0);
  });
});
