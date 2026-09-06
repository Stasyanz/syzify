import { describe, it, expect } from "vitest";
import { daysBetween, formatDelta, sparkline } from "./recovery";

describe("wording", () => {
  it("formats the HR delta with a sign", () => {
    expect(formatDelta(2.6)).toBe("+3");
    expect(formatDelta(-1.4)).toBe("−1");
    expect(formatDelta(0.2)).toBe("0");
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
