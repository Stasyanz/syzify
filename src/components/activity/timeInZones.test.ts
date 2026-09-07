import { describe, it, expect } from "vitest";
import type { TimeInZone } from "../../lib/types";
import { HR_ZONE_COLORS, POWER_ZONE_COLORS, hrZoneRanges, powerZoneRanges } from "./chartZones";
import { hasTimeInZones, timeInZoneRows } from "./timeInZones";

function row(
  zone_type: string,
  zone_index: number,
  time_s: number,
  zone_high_boundary: number | null,
): TimeInZone {
  return { id: null, activity_id: "a", zone_type, zone_index, time_s, zone_high_boundary };
}

// The reference evening ride of 2026-09-06 (fenix, 8652 s timer): Garmin's
// HR buckets 0…6 with the Z1 floor at 93 and no ceiling on "above max".
const HR_RIDE: TimeInZone[] = [
  row("hr", 0, 19, 93),
  row("hr", 1, 2673.966, 112),
  row("hr", 2, 4601.967, 130),
  row("hr", 3, 1356.99, 149),
  row("hr", 4, 0, 167),
  row("hr", 5, 0, 186),
  row("hr", 6, 0, null),
];
// Power buckets 0…7, bucket 7 carrying the 3393 W sentinel.
const POWER_RIDE: TimeInZone[] = [
  row("power", 0, 0, 0),
  row("power", 1, 2660.991, 129),
  row("power", 2, 2898.014, 176),
  row("power", 3, 1798.941, 212),
  row("power", 4, 824.976, 247),
  row("power", 5, 326.015, 282),
  row("power", 6, 134.984, 353),
  row("power", 7, 8.002, 3393),
];
const DURATION = 8652;

describe("timeInZoneRows", () => {
  it("shows Garmin's Z1–Z5 for HR with ranges, times and shares of the timer", () => {
    const rows = timeInZoneRows(HR_RIDE, "hr", DURATION)!;
    expect(rows.map((r) => r.label)).toEqual([
      "Z1 Warm up",
      "Z2 Easy",
      "Z3 Aerobic",
      "Z4 Threshold",
      "Z5 Maximum",
    ]);
    expect(rows.map((r) => r.range)).toEqual([
      "93–112 bpm",
      "112–130 bpm",
      "130–149 bpm",
      "149–167 bpm",
      "> 167 bpm",
    ]);
    expect(rows.map((r) => Math.round(r.timeS))).toEqual([2674, 4602, 1357, 0, 0]);
    expect(rows.map((r) => Math.round(r.pct))).toEqual([31, 53, 16, 0, 0]);
    expect(rows.map((r) => r.color)).toEqual(HR_ZONE_COLORS);
  });

  it("shows Z1–Z7 for power and never reads the sentinel ceiling", () => {
    const rows = timeInZoneRows(POWER_RIDE, "power", DURATION)!;
    expect(rows).toHaveLength(7);
    expect(rows[0].range).toBe("0–129 W");
    expect(rows[6]).toMatchObject({ label: "Z7 Neuromuscular", range: "> 353 W" });
    expect(rows[6].timeS).toBeCloseTo(8.002);
    expect(rows.map((r) => r.color)).toEqual(POWER_ZONE_COLORS);
    // Below-Z1 time is not among the shown rows.
    expect(rows.reduce((s, r) => s + r.pct, 0)).toBeLessThanOrEqual(100);
  });

  it("drops the below-Z1 and above-max time rather than folding it in", () => {
    const zones = [
      row("hr", 0, 600, 93),
      row("hr", 1, 100, 112),
      ...HR_RIDE.slice(2, 6),
      row("hr", 6, 900, null),
    ];
    const rows = timeInZoneRows(zones, "hr", 10_000)!;
    expect(rows[0].timeS).toBe(100);
    expect(rows[4].timeS).toBe(0);
    expect(Math.round(rows.reduce((s, r) => s + r.pct, 0))).toBe(
      Math.round(((100 + 4601.967 + 1356.99) / 10_000) * 100),
    );
  });

  it("agrees with the chart bands on a sparse bucket set — one offset rule", () => {
    // Buckets 3–5 missing: the card and hrZoneRanges must still put the
    // 93–112 range in the same zone and color.
    const sparse = [row("hr", 0, 10, 93), row("hr", 1, 600, 112), row("hr", 2, 300, 130), row("hr", 6, 0, null)];
    const rows = timeInZoneRows(sparse, "hr", 3600)!;
    const bands = hrZoneRanges(sparse);
    const z1 = rows[0];
    expect(z1.range).toBe("93–112 bpm");
    expect(bands.find((b) => b.from === 93)!.color).toBe(z1.color);
    expect(z1.color).toBe(HR_ZONE_COLORS[0]);
    // Power, buckets 4–6 missing: 0–129 W is Z1 for both.
    const sparseP = [row("power", 0, 0, 0), row("power", 1, 600, 129), row("power", 2, 300, 176), row("power", 3, 100, 212), row("power", 7, 5, 3393)];
    const prow = timeInZoneRows(sparseP, "power", 3600)![0];
    expect(prow.range).toBe("0–129 W");
    expect(powerZoneRanges(sparseP)!.find((b) => b.from === 0)!.color).toBe(prow.color);
  });

  it("drops only the TIME of a corrupt row, never its index or boundary", () => {
    // Single-copy power set with the corrupt row on the top index: the
    // set must not lose bucket 7, or every zone would shift by one.
    const zones = POWER_RIDE.map((z) => (z.zone_index === 7 ? { ...z, time_s: 786_604 } : z));
    const rows = timeInZoneRows(zones, "power", DURATION)!;
    expect(rows[0]).toMatchObject({ label: "Z1 Active recovery", range: "0–129 W" });
    expect(Math.round(rows[0].timeS)).toBe(2661);
    expect(rows[6]).toMatchObject({ label: "Z7 Neuromuscular", range: "> 353 W", timeS: 0 });
    // A corrupt row that is the only carrier of its boundary still lends it.
    const hr = [
      row("hr", 0, 19, 93),
      row("hr", 1, 100, 112),
      row("hr", 2, 200, 130),
      row("hr", 3, 133_988, 149),
      row("hr", 4, 50, 167),
      row("hr", 5, 0, 186),
      row("hr", 6, 0, null),
    ];
    const hrRows = timeInZoneRows(hr, "hr", 7200)!;
    expect(hrRows[2]).toMatchObject({ range: "130–149 bpm", timeS: 0 });
    expect(hrRows[3]).toMatchObject({ range: "149–167 bpm", timeS: 50 });
  });

  it("shows only the ceiling when the ceilings do not climb", () => {
    const zones = [
      row("hr", 0, 10, 120),
      row("hr", 1, 100, 112), // below its floor
      row("hr", 2, 100, 112), // equal to its floor
      row("hr", 3, 100, 149),
      row("hr", 4, 0, 167),
      row("hr", 5, 0, 186),
      row("hr", 6, 0, null),
    ];
    const rows = timeInZoneRows(zones, "hr", 1000)!;
    expect(rows.map((r) => r.range)).toEqual([
      "≤ 112 bpm",
      "≤ 112 bpm",
      "112–149 bpm",
      "149–167 bpm",
      "> 167 bpm",
    ]);
  });

  it("takes the MAX over lap copies and ignores rows longer than the activity", () => {
    const zones = [
      // Session + two lap copies of Z2: MAX, not SUM.
      row("hr", 2, 3600, 130),
      row("hr", 2, 1800, 130),
      row("hr", 2, 1800, 130),
      // A corrupt pre-dedup row in Z1 — days' worth of seconds.
      row("hr", 1, 133_988, 112),
      row("hr", 1, 68, 112),
      row("hr", 0, 10, 93),
      row("hr", 6, 0, null),
    ];
    const rows = timeInZoneRows(zones, "hr", 7200)!;
    expect(rows[1].timeS).toBe(3600);
    expect(rows[0].timeS).toBe(68);
    // Without a duration nothing can be judged: the long row is kept.
    expect(timeInZoneRows(zones, "hr", null)![0].timeS).toBe(133_988);
  });

  it("puts Z1 at index 0 when the writer has no below-Z1 bucket", () => {
    const zones = [
      row("hr", 0, 100, 112),
      row("hr", 1, 200, 130),
      row("hr", 2, 300, 149),
      row("hr", 3, 0, 167),
      row("hr", 4, 0, null),
    ];
    const rows = timeInZoneRows(zones, "hr", 600)!;
    expect(rows.map((r) => r.timeS)).toEqual([100, 200, 300, 0, 0]);
    expect(rows[0].range).toBe("0–112 bpm");
    expect(rows[4].range).toBe("> 167 bpm");
  });

  it("degrades the range label when boundaries are missing", () => {
    const noBounds = HR_RIDE.map((z) => ({ ...z, zone_high_boundary: null }));
    const rows = timeInZoneRows(noBounds, "hr", DURATION)!;
    expect(rows.every((r) => r.range === "")).toBe(true);
    // A lap copy may carry the boundary the session row lacks.
    const mixed = [
      ...noBounds,
      row("hr", 0, 19, 93),
      row("hr", 2, 4601.967, 130),
    ];
    const rows2 = timeInZoneRows(mixed, "hr", DURATION)!;
    expect(rows2.map((r) => r.range)).toEqual([
      "> 93 bpm", // floor known, Z1's own ceiling missing
      "≤ 130 bpm", // ceiling known, floor missing
      "> 130 bpm",
      "",
      "", // the open top zone needs a floor
    ]);
  });

  it("is null without rows of the type, or with time only in hidden buckets", () => {
    expect(timeInZoneRows(HR_RIDE, "power", DURATION)).toBeNull();
    expect(timeInZoneRows([], "hr", DURATION)).toBeNull();
    const idle = [row("hr", 0, 500, 93), row("hr", 1, 0, 112), row("hr", 6, 0, null)];
    expect(timeInZoneRows(idle, "hr", 500)).toBeNull();
    expect(timeInZoneRows([row("hr", 1, NaN, 112), row("hr", 2, -5, 130)], "hr", 100)).toBeNull();
  });

  it("reports 0 % without a usable duration", () => {
    expect(timeInZoneRows(HR_RIDE, "hr", null)![1].pct).toBe(0);
    expect(timeInZoneRows(HR_RIDE, "hr", 0)![1].pct).toBe(0);
  });
});

describe("hasTimeInZones", () => {
  it("is true with either type and false with none", () => {
    expect(hasTimeInZones(HR_RIDE, DURATION)).toBe(true);
    expect(hasTimeInZones(POWER_RIDE, DURATION)).toBe(true);
    // Time only below Z1 (a Garmin-shaped set, so bucket 0 IS below Z1).
    expect(hasTimeInZones([row("hr", 0, 30, 93), row("hr", 6, 0, null)], DURATION)).toBe(false);
    expect(hasTimeInZones([], DURATION)).toBe(false);
  });
});
