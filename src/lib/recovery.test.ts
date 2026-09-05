import { describe, it, expect } from "vitest";
import type { RecoveryCard } from "./types";
import {
  buildingText,
  daysBetween,
  describeAge,
  emptyState,
  formatDelta,
  sparkline,
} from "./recovery";

const base: RecoveryCard = {
  computed_for: "2026-09-05",
  date: "2026-09-05",
  age_days: 0,
  index: 98,
  band: "intervals_ok",
  advice: "Intervals are fine today",
  hr: { night_median: 53, baseline: 53, delta: 0, score: 100 },
  stress: { night_avg: 12.4, score: 94 },
  load: { tss_yesterday: 30, ctl: 60, score: 100 },
  warning: null,
  days_recorded_90d: 9,
  nights_recorded_90d: 8,
  nights_needed: 0,
  history: [{ date: "2026-09-05", index: 98, band: "intervals_ok" }],
};

describe("bands and wording", () => {
  it("says when the index was measured", () => {
    expect(describeAge("2026-09-05", 0)).toBe("Last night");
    expect(describeAge("2026-09-04", 1)).toBe("Sep 4 · 1 day ago");
    expect(describeAge("2026-07-25", 42)).toBe("Jul 25 · 42 days ago");
  });

  it("formats the HR delta with a sign", () => {
    expect(formatDelta(2.6)).toBe("+3");
    expect(formatDelta(-1.4)).toBe("−1");
    expect(formatDelta(0.2)).toBe("0");
  });

  it("counts the nights the BASELINE still needs — the index comes one night later", () => {
    expect(buildingText(3)).toBe("3 more nights to build the baseline");
    expect(buildingText(1)).toBe("1 more night to build the baseline");
    expect(buildingText(0)).toBe("Baseline ready — the next recorded night gets an index");
  });
});

describe("emptyState", () => {
  it("is null once there is an index, however old", () => {
    expect(emptyState(base)).toBeNull();
    expect(emptyState({ ...base, age_days: 130, nights_recorded_90d: 0 })).toBeNull();
  });

  it("tells a never-imported vault from day-only wear from a baseline still building", () => {
    const blank = {
      ...base,
      index: null,
      band: null,
      days_recorded_90d: 0,
      nights_recorded_90d: 0,
      nights_needed: 3,
    };
    expect(emptyState(blank)).toEqual({ kind: "no_data" });
    // Monitoring days exist, none holds a full night: an import will not help.
    expect(emptyState({ ...blank, days_recorded_90d: 5 })).toEqual({ kind: "no_nights" });
    expect(
      emptyState({ ...blank, days_recorded_90d: 5, nights_recorded_90d: 1, nights_needed: 2 }),
    ).toEqual({ kind: "building", nightsNeeded: 2 });
  });
});

describe("sparkline", () => {
  const box = { width: 100, height: 40, pad: 0, days: 28 };

  it("puts each night on its own day and drops nights outside the window", () => {
    const s = sparkline(
      [
        { date: "2026-08-01", index: 70, band: "easy_day" as const }, // 35 days back — outside
        { date: "2026-08-09", index: 80, band: "intervals_ok" as const }, // exactly 27 days back — first slot
        { date: "2026-09-05", index: 100, band: "intervals_ok" as const },
      ],
      "2026-09-05",
      box,
    );
    expect(s.points.map((p) => p.date)).toEqual(["2026-08-09", "2026-09-05"]);
    expect(s.points[0].x).toBe(0);
    expect(s.points[1].x).toBe(100);
    expect(s.points[1].y).toBe(0);
    expect(s.points[0].y).toBeCloseTo(8);
    expect(s.points[0].band).toBe("intervals_ok");
    expect(s.guides).toEqual({ y80: 8, y60: 16 });
  });

  it("joins consecutive nights only — a missed night is a gap", () => {
    const s = sparkline(
      [
        { date: "2026-09-01", index: 90, band: "intervals_ok" as const },
        { date: "2026-09-02", index: 85, band: "intervals_ok" as const },
        { date: "2026-09-05", index: 98, band: "intervals_ok" as const },
      ],
      "2026-09-05",
      box,
    );
    expect(s.segments.map(([a, b]) => [a.date, b.date])).toEqual([["2026-09-01", "2026-09-02"]]);
  });

  it("sorts unordered history by day", () => {
    const s = sparkline(
      [
        { date: "2026-09-05", index: 98, band: "intervals_ok" as const },
        { date: "2026-09-04", index: 90, band: "intervals_ok" as const },
      ],
      "2026-09-05",
      box,
    );
    expect(s.points.map((p) => p.date)).toEqual(["2026-09-04", "2026-09-05"]);
    expect(s.segments).toHaveLength(1);
  });

  it("counts whole days across a DST change", () => {
    expect(daysBetween("2026-03-28", "2026-03-30")).toBe(2);
    expect(daysBetween("2026-10-24", "2026-10-26")).toBe(2);
  });
});
