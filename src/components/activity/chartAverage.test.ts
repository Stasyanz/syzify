import { describe, expect, it } from "vitest";
import {
  avgLabelSide,
  avgLineLayout,
  meanPace,
  paceOfSpeed,
  rangeSpanning,
  seriesMean,
  snapLineY,
} from "./chartAverage";

describe("seriesMean", () => {
  it("averages the samples evenly without timestamps", () => {
    expect(seriesMean([100, 200, 300])).toBe(200);
  });

  it("skips gaps (null) and non-finite values without counting them", () => {
    expect(seriesMean([100, null, 300, NaN, Infinity])).toBe(200);
  });

  it("counts recorded zeros — coasting is part of the average", () => {
    expect(seriesMean([0, 200])).toBe(100);
  });

  it("is null with no usable sample", () => {
    expect(seriesMean([])).toBeNull();
    expect(seriesMean([null, null])).toBeNull();
  });

  it("weights each sample by the time it stands for", () => {
    // 100 W held for 3 s, then 400 W for 1 s (last sample weighs 1).
    expect(seriesMean([100, 400], [0, 3])).toBe(175);
  });

  it("caps a sample's weight so a pause cannot dominate", () => {
    // 100 W, then a 5-minute gap: the first sample stands for 10 s at most.
    expect(seriesMean([100, 400], [0, 300])).toBeCloseTo((100 * 10 + 400) / 11, 9);
  });

  it("falls back to an even weight when a timestamp is missing or unordered", () => {
    expect(seriesMean([100, 400], [null, 5])).toBe(250);
    expect(seriesMean([100, 400], [5, 5])).toBe(250);
  });

  it("applies the keep filter before weighting", () => {
    expect(seriesMean([0, 10, 20], [0, 1, 3], (v) => v > 0)).toBe((10 * 2 + 20) / 3);
  });
});

describe("meanPace", () => {
  it("derives pace from the mean speed, not the mean of paces", () => {
    // 2 m/s and 4 m/s → mean 3 m/s → 1000/3/60 = 5:33 /km. Averaging the
    // paces (8:20 and 4:10) would give 6:15 — the slow-end bias.
    expect(meanPace([2, 4], 0.5, 1000)).toBeCloseTo(1000 / 3 / 60, 6);
  });

  it("skips near-stops at or below the cutoff, like the pace series does", () => {
    expect(meanPace([0, 0.5, null, 4], 0.5, 1000)).toBeCloseTo(1000 / 4 / 60, 6);
  });

  it("is null when nothing moves", () => {
    expect(meanPace([0, 0.2], 0.5, 1000)).toBeNull();
    expect(meanPace([], 0.5, 1000)).toBeNull();
  });

  it("honors the per-unit distance (miles, 100 m swim)", () => {
    expect(meanPace([2], 0.2, 100)).toBeCloseTo(100 / 2 / 60, 6);
  });

  it("weights speeds by time like seriesMean", () => {
    // 2 m/s for 3 s, 4 m/s for 1 s → 2.5 m/s.
    expect(meanPace([2, 4], 0.5, 1000, [0, 3])).toBeCloseTo(1000 / 2.5 / 60, 6);
  });
});

describe("paceOfSpeed", () => {
  it("converts a summary speed to pace", () => {
    expect(paceOfSpeed(4, 1000)).toBeCloseTo(1000 / 4 / 60, 6);
  });

  it("is null for missing, zero or negative speeds", () => {
    expect(paceOfSpeed(null, 1000)).toBeNull();
    expect(paceOfSpeed(undefined, 1000)).toBeNull();
    expect(paceOfSpeed(0, 1000)).toBeNull();
    expect(paceOfSpeed(-1, 1000)).toBeNull();
  });
});

describe("rangeSpanning", () => {
  it("leaves a range alone when the value is inside or missing", () => {
    expect(rangeSpanning(100, 300, 200)).toEqual([100, 300]);
    expect(rangeSpanning(100, 300, null)).toEqual([100, 300]);
    expect(rangeSpanning(100, 300, undefined)).toEqual([100, 300]);
    expect(rangeSpanning(100, 300, NaN)).toEqual([100, 300]);
    expect(rangeSpanning(100, 300, Infinity)).toEqual([100, 300]);
  });

  it("lowers the floor when the average sits below the bucket maxes", () => {
    expect(rangeSpanning(180, 300, 150)).toEqual([150, 300]);
  });

  it("raises the ceiling when the average sits above the data", () => {
    expect(rangeSpanning(100, 200, 250)).toEqual([100, 250]);
  });
});

describe("avgLabelSide", () => {
  it("puts the label above the line by default", () => {
    expect(avgLabelSide(100, 10, 14)).toBe("above");
  });

  it("flips below when the line is within a label height of the plot top", () => {
    expect(avgLabelSide(20, 10, 14)).toBe("below");
    expect(avgLabelSide(24, 10, 14)).toBe("above");
  });
});

describe("snapLineY", () => {
  it("centers an odd-width stroke on a half pixel", () => {
    expect(snapLineY(100.3, 1)).toBe(100.5);
    expect(snapLineY(100.7, 3)).toBe(101.5);
  });

  it("centers an even-width stroke on a whole pixel (Retina 2px)", () => {
    expect(snapLineY(100.3, 2)).toBe(100);
    expect(snapLineY(100.7, 2)).toBe(101);
  });
});

describe("avgLineLayout", () => {
  // Plot area: top 10, height 200 → spans 10..210.
  it("places the label above a line in the middle of the plot", () => {
    expect(avgLineLayout(100.2, 10, 200, 1, 16)).toEqual({ y: 100.5, side: "above" });
  });

  it("flips the label below a line near the top", () => {
    expect(avgLineLayout(14, 10, 200, 2, 16)).toEqual({ y: 14, side: "below" });
  });

  it("keeps the label above a line at the very bottom", () => {
    expect(avgLineLayout(210, 10, 200, 2, 16)).toEqual({ y: 210, side: "above" });
  });

  it("is null when the line falls outside the plot or is not finite", () => {
    expect(avgLineLayout(5, 10, 200, 1, 16)).toBeNull();
    expect(avgLineLayout(215, 10, 200, 1, 16)).toBeNull();
    expect(avgLineLayout(NaN, 10, 200, 1, 16)).toBeNull();
  });
});
