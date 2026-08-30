import { describe, it, expect } from "vitest";
import { formatWindow, mergeCurveData, shortDate, tooltipText } from "./powerCurve";
import type { PowerCurveEnvelopePoint } from "../../lib/types";

const env = (
  window_s: number,
  watts: number,
  activity_id = "other",
  title: string | null = "Sapadere",
): PowerCurveEnvelopePoint => ({
  window_s,
  watts,
  activity_id,
  title,
  start_time: "2026-08-30T09:23:25+03:00",
});

describe("formatWindow", () => {
  it("labels seconds, minutes and the hour compactly", () => {
    expect(formatWindow(1)).toBe("1s");
    expect(formatWindow(45)).toBe("45s");
    expect(formatWindow(60)).toBe("1m");
    expect(formatWindow(90)).toBe("1m30");
    expect(formatWindow(1200)).toBe("20m");
    expect(formatWindow(3600)).toBe("1h");
  });
});

describe("mergeCurveData", () => {
  it("aligns both curves onto the union grid with nulls in the holes", () => {
    const m = mergeCurveData(
      [
        { window_s: 5, watts: 615 },
        { window_s: 60, watts: 339 },
      ],
      [env(5, 646), env(60, 346), env(300, 280)],
    );
    expect(m.x).toEqual([5, 60, 300]);
    expect(m.activity).toEqual([615, 339, null]);
    expect(m.envelope).toEqual([646, 346, 280]);
    expect(m.record[2]?.activity_id).toBe("other");
  });

  it("keeps activity windows missing from the envelope", () => {
    // Shouldn't happen (the envelope is a max over stored curves, this
    // activity's included) — but a stale cache must not crash the chart.
    const m = mergeCurveData([{ window_s: 5, watts: 500 }], []);
    expect(m.x).toEqual([5]);
    expect(m.envelope).toEqual([null]);
    expect(m.record).toEqual([null]);
  });
});

describe("tooltipText", () => {
  it("shows the window and watts, plus a foreign record with its source", () => {
    expect(tooltipText(60, 339, env(60, 346), "me")).toBe(
      "1m · 339 W — best 346 W (Sapadere)",
    );
  });

  it("marks the hovered window as all-time best when this activity holds it", () => {
    expect(tooltipText(300, 280, env(300, 280, "me"), "me")).toBe(
      "5m · 280 W — all-time best",
    );
  });

  it("falls back to the record's date when it has no title", () => {
    expect(tooltipText(60, null, env(60, 346, "other", null), "me")).toBe(
      "1m — best 346 W (Aug 30)",
    );
  });
});

describe("shortDate", () => {
  it("degrades to empty on garbage", () => {
    expect(shortDate("not-a-date")).toBe("");
  });
});
