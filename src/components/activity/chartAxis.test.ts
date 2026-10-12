import { describe, it, expect } from "vitest";
import type uPlot from "uplot";
import { yAxisSize } from "./chartAxis";

// No canvas needed: the only thing yAxisSize asks of the plot is a context
// that measures text. 6 px per character is close to 11px sans-serif digits.
// The context starts in the font uPlot draws labels with on a 2x screen and
// records the font each measurement was taken in.
function fakePlot(pxPerChar = 6) {
  const measuredIn: string[] = [];
  const ctx = {
    font: "22px sans-serif",
    measureText(s: string) {
      measuredIn.push(this.font);
      return { width: s.length * pxPerChar };
    },
  };
  return { plot: { ctx } as unknown as uPlot, ctx, measuredIn };
}

describe("yAxisSize", () => {
  it("is as wide as the widest label plus the tick marks and gap", () => {
    const { plot } = fakePlot();
    expect(yAxisSize(plot, ["0", "200", "800"])).toBe(3 * 6 + 15);
    expect(yAxisSize(plot, ["20:00", "5:00"])).toBe(5 * 6 + 15);
  });

  it("gives a five-character label more than the 40 px that cut it (#219)", () => {
    const { plot } = fakePlot();
    expect(yAxisSize(plot, ["800", "1,000"])).toBeGreaterThan(40);
  });

  it("rounds a fractional width up, never down into the label", () => {
    const { plot } = fakePlot(5.5);
    expect(yAxisSize(plot, ["1,000"])).toBe(Math.ceil(27.5) + 15);
  });

  it("never goes below 28 px for short labels", () => {
    const { plot } = fakePlot();
    expect(yAxisSize(plot, ["0"])).toBe(28);
  });

  it("falls back to 40 px before the labels exist", () => {
    const { plot } = fakePlot();
    expect(yAxisSize(plot, null)).toBe(40);
    expect(yAxisSize(plot, [])).toBe(40);
  });

  it("measures in the 11px label font and puts the context's font back", () => {
    const { plot, ctx, measuredIn } = fakePlot();
    yAxisSize(plot, ["100", "1,000"]);
    expect(measuredIn).toEqual(["11px sans-serif", "11px sans-serif"]);
    // uPlot caches the font it set and will not set it again: a font left
    // behind would shrink every label on the next redraw of a 2x canvas.
    expect(ctx.font).toBe("22px sans-serif");
  });
});
