import { describe, expect, it } from "vitest";
import {
  bandGradientStops,
  bucketMaxBars,
  ELEVATION_BANDS_M,
  CADENCE_ZONE_COLORS,
  cadenceZoneRanges,
  DEFAULT_HR_RANGES,
  GRADE_BOUNDS_PCT,
  GRADE_COLORS,
  gradeCategory,
  gradeGradientStops,
  gradeSeries,
  HR_FALLBACK_COLOR,
  nearestIdx,
  selectionGrade,
  HR_ZONE_COLORS,
  POWER_ZONE_COLORS,
  SPEED_ZONE_COLORS,
  hrVisRange,
  hrZoneRanges,
  powerVisRange,
  powerZoneRanges,
  speedVisRange,
  externalSelectionCols,
  nearestChartIdx,
  rangesEqual,
  speedZoneRanges,
  zoneBarCount,
  zoneColorFor,
} from "./chartZones";
import type { TimeInZone } from "../../lib/types";

const zone = (
  zone_index: number,
  zone_high_boundary: number | null,
  zone_type = "hr",
): TimeInZone => ({
  id: null,
  activity_id: "a",
  zone_type,
  zone_index,
  time_s: 60,
  zone_high_boundary,
});

describe("hrZoneRanges", () => {
  // The shape every Garmin FIT in the vault carries (fenix 6X, 2026-09-04
  // ride, max HR 186): bucket 0 = below Z1 with the Z1 floor as its
  // ceiling, 1–5 = Z1–Z5 with ceilings, 6 = above max, no boundary.
  const fenix = [
    zone(6, null),
    zone(3, 149),
    zone(0, 93),
    zone(5, 186),
    zone(1, 112),
    zone(4, 167),
    zone(2, 130),
  ];

  it("colors by zone index: below-Z1 shares Z1's green, Z5 is maximum, above max stays maximum", () => {
    const ranges = hrZoneRanges(fenix);
    expect(ranges.map((r) => r.to)).toEqual([93, 112, 130, 149, 167, 186, Infinity]);
    // Contiguous cover of [0, ∞).
    expect(ranges[0].from).toBe(0);
    for (let i = 1; i < ranges.length; i++) {
      expect(ranges[i].from).toBe(ranges[i - 1].to);
    }
    expect(ranges.map((r) => r.color)).toEqual([
      HR_ZONE_COLORS[0], // below Z1
      HR_ZONE_COLORS[0], // Z1 recovery
      HR_ZONE_COLORS[1], // Z2 easy — used to read as green
      HR_ZONE_COLORS[2], // Z3 aerobic
      HR_ZONE_COLORS[3], // Z4 threshold
      HR_ZONE_COLORS[4], // Z5 maximum — used to read as threshold red
      HR_ZONE_COLORS[4], // above the configured max
    ]);
    // The bars of the ride that exposed the bug.
    expect(zoneColorFor(128, ranges)).toBe(HR_ZONE_COLORS[1]);
    expect(zoneColorFor(170, ranges)).toBe(HR_ZONE_COLORS[4]);
  });

  it("folds degenerate middle boundaries into the next zone and ignores other metrics", () => {
    const ranges = hrZoneRanges([
      zone(0, 115),
      zone(1, 115), // repeated — folds into Z2
      zone(2, 0), // zero — folds
      zone(3, null), // missing — folds
      zone(4, 185),
      zone(5, 195),
      zone(0, 250, "power"), // other metric — dropped
    ]);
    expect(ranges.map((r) => r.to)).toEqual([115, 185, Infinity]);
    // The surviving buckets keep THEIR zone's color: 4 → threshold.
    expect(ranges.map((r) => r.color)).toEqual([
      HR_ZONE_COLORS[0],
      HR_ZONE_COLORS[3],
      HR_ZONE_COLORS[4],
    ]);
  });

  it("treats the top index as open-ended whatever ceiling it stores", () => {
    // A device that writes Z5's ceiling (= max HR) but no above-max bucket:
    // HR past the configured max is still Z5, not "outside every range".
    const ranges = hrZoneRanges([zone(0, 93), zone(1, 112), zone(5, 186)]);
    expect(ranges.map((r) => r.to)).toEqual([93, 112, Infinity]);
    expect(zoneColorFor(195, ranges)).toBe(HR_ZONE_COLORS[4]);
  });

  it("survives per-lap duplicates: the top index stays open, no sentinel sneaks back", () => {
    // The parser keeps one row set per lap plus the session's — every
    // index appears several times. A positional "last row" rule would
    // read a lap's copy of the top ceiling as a real boundary.
    const twoLaps = [...fenix, ...fenix];
    expect(hrZoneRanges(twoLaps)).toEqual(hrZoneRanges(fenix));
    const power = [0, 129, 176, 212, 247, 282, 353, 3393].map((b, i) => zone(i, b, "power"));
    const ranges = powerZoneRanges([...power, ...power, ...power]);
    expect(ranges!.map((r) => r.to)).toEqual([129, 176, 212, 247, 282, 353, Infinity]);
  });

  it("lets a usable lap copy fill a null session boundary, and vice versa", () => {
    // The 38 multisport activities in the vault carry partially null
    // boundaries next to lap copies of the same index — whichever row of
    // an index has a real ceiling wins, regardless of order.
    const holes = fenix.map((z) => (z.zone_index === 2 ? { ...z, zone_high_boundary: null } : z));
    expect(hrZoneRanges([...holes, ...fenix])).toEqual(hrZoneRanges(fenix));
    expect(hrZoneRanges([...fenix, ...holes])).toEqual(hrZoneRanges(fenix));
  });

  it("maps bucket 0 to Z1 for a writer without a below-Z1 bucket (one bucket per zone)", () => {
    // Not a Garmin shape (none in the vault) — exactly as many buckets as
    // the palette has zones, so index 0 IS zone 1 and nothing shifts.
    const ranges = hrZoneRanges([
      zone(0, 120),
      zone(1, 140),
      zone(2, 160),
      zone(3, 175),
      zone(4, 190),
    ]);
    expect(ranges.map((r) => r.to)).toEqual([120, 140, 160, 175, Infinity]);
    expect(ranges.map((r) => r.color)).toEqual(HR_ZONE_COLORS);
  });

  it("refuses a two-bucket device (one boundary is not a zone system) so the defaults take over", () => {
    expect(hrZoneRanges([zone(0, 100), zone(1, 140)])).toBe(DEFAULT_HR_RANGES);
  });

  it("falls back to the design defaults without usable boundaries", () => {
    expect(hrZoneRanges([])).toBe(DEFAULT_HR_RANGES);
    expect(hrZoneRanges([zone(0, 115)])).toBe(DEFAULT_HR_RANGES);
    expect(hrZoneRanges([zone(0, null), zone(1, null)])).toBe(DEFAULT_HR_RANGES);
  });
});

describe("powerZoneRanges", () => {
  it("maps Garmin's 8 power buckets 1:1 onto the 7 zones, ignoring the sentinel ceiling", () => {
    // The fenix 6X rows of the 2026-09-04 ride at FTP 235: bucket 0 is an
    // empty 0 W zone, 1–7 are Coggan Z1–Z7, and Z7's stored ceiling
    // (3393 W) is a sentinel, not a boundary. Read as a boundary it made
    // an 8th range and shifted every color one zone too cool.
    const ranges = powerZoneRanges([
      zone(0, 0, "power"),
      zone(1, 129, "power"),
      zone(2, 176, "power"),
      zone(3, 212, "power"),
      zone(4, 247, "power"),
      zone(5, 282, "power"),
      zone(6, 353, "power"),
      zone(7, 3393, "power"),
      zone(0, 115), // hr row — must not leak in
    ]);
    expect(ranges).not.toBeNull();
    expect(ranges!.map((r) => r.to)).toEqual([129, 176, 212, 247, 282, 353, Infinity]);
    expect(ranges![0].from).toBe(0);
    // Every zone gets its own color — Z6 vs Z7 used to merge into one red.
    expect(ranges!.map((r) => r.color)).toEqual(POWER_ZONE_COLORS);
    // The bars that exposed the bug: 210 W is Z3 tempo (teal), not Z2 green.
    expect(zoneColorFor(210, ranges!)).toBe(POWER_ZONE_COLORS[2]);
    expect(zoneColorFor(233, ranges!)).toBe(POWER_ZONE_COLORS[3]);
    expect(zoneColorFor(272, ranges!)).toBe(POWER_ZONE_COLORS[4]);
    expect(zoneColorFor(331, ranges!)).toBe(POWER_ZONE_COLORS[5]);
    expect(zoneColorFor(450, ranges!)).toBe(POWER_ZONE_COLORS[6]);
    expect(zoneColorFor(4000, ranges!)).toBe(POWER_ZONE_COLORS[6]);
  });

  it("returns null without usable power boundaries — no FTP, no zones", () => {
    expect(powerZoneRanges([])).toBeNull();
    expect(powerZoneRanges([zone(0, 250, "power")])).toBeNull();
    // HR boundaries alone must not enable power bars.
    expect(powerZoneRanges([zone(0, 115), zone(1, 135)])).toBeNull();
  });

  it("derives Coggan zones from FTP when boundaries are missing", () => {
    // Edge units write time-in-power-zone WITHOUT the boundary array; the
    // session FTP (299 W here) then anchors the standard %-of-FTP zones.
    const ranges = powerZoneRanges([], 299);
    expect(ranges).not.toBeNull();
    // 7 Coggan zones + the all-out sprint band above 3×FTP.
    expect(ranges!).toHaveLength(8);
    // 55/75/90/105/120/150/300% of 299, rounded to whole watts.
    expect(ranges!.map((r) => r.to)).toEqual([164, 224, 269, 314, 359, 449, 897, Infinity]);
    expect(ranges!.slice(0, 7).map((r) => r.color)).toEqual(POWER_ZONE_COLORS);
    // The sprint band has its own color — a 1000 W max effort no longer
    // shares the Z7 purple with a 500 W surge.
    expect(ranges![7].color).not.toBe(POWER_ZONE_COLORS[6]);
    expect(new Set(ranges!.map((r) => r.color)).size).toBe(8);
  });

  it("real FIT boundaries beat the FTP fallback; bad FTP stays a line", () => {
    const ranges = powerZoneRanges(
      [zone(0, 100, "power"), zone(1, 200, "power"), zone(2, null, "power")],
      299,
    );
    expect(ranges!.map((r) => r.to)).toEqual([100, 200, Infinity]);
    // Two buckets carry one boundary → the FTP-derived Coggan zones win.
    const flat = powerZoneRanges([zone(0, 100, "power"), zone(1, 200, "power")], 299);
    expect(flat!).toHaveLength(8);
    expect(powerZoneRanges([], 0)).toBeNull();
    expect(powerZoneRanges([], null)).toBeNull();
    expect(powerZoneRanges([], NaN)).toBeNull();
  });
});

describe("zoneBarCount", () => {
  it("targets ~14px per bar of the CARD width, in steps of 5, clamped 20..120", () => {
    // Half-width card of the default 1200px window ((1200-16)/2 = 592) →
    // the design's classic 40 bars; the full-width first slot gets ~2×.
    expect(zoneBarCount(592)).toBe(40);
    expect(zoneBarCount(1200)).toBe(85);
    // Tiny card floors at 20, huge card ceils at 120.
    expect(zoneBarCount(250)).toBe(20);
    expect(zoneBarCount(2500)).toBe(120);
    // Quantized: +30px within one 5-bar step must not change the count
    // (no rebucket churn during a live window drag).
    expect(zoneBarCount(590)).toBe(zoneBarCount(560));
  });

  it("falls back to 40 when the card is unmeasured", () => {
    expect(zoneBarCount(0)).toBe(40);
    expect(zoneBarCount(-5)).toBe(40);
    expect(zoneBarCount(NaN)).toBe(40);
  });
});

describe("powerVisRange", () => {
  it("pads by 10 and rounds to tens with a 0 floor and no ceiling", () => {
    expect(powerVisRange(140, 380)).toEqual([130, 390]);
    expect(powerVisRange(5, 1250)).toEqual([0, 1260]);
  });
});

describe("cadenceZoneRanges", () => {
  const runValues = (base: number) =>
    Array.from({ length: 100 }, (_, i) => base + (i % 10));

  it("prefers FIT boundaries for any sport, colored by zone index", () => {
    // Garmin bucket shape: below-Z1, Z1–Z5 with ceilings, above-top open.
    const ranges = cadenceZoneRanges(
      [
        zone(0, 60, "cadence"),
        zone(1, 75, "cadence"),
        zone(2, 90, "cadence"),
        zone(3, 105, "cadence"),
        zone(4, 120, "cadence"),
        zone(5, null, "cadence"),
      ],
      "ride",
      runValues(75),
    );
    expect(ranges).not.toBeNull();
    expect(ranges!.map((r) => r.to)).toEqual([60, 75, 90, 105, 120, Infinity]);
    // Zone 5 wears the brown top; below-Z1 shares Z1's red.
    expect(ranges![5].color).toBe(CADENCE_ZONE_COLORS[4]);
    expect(ranges![0].color).toBe(CADENCE_ZONE_COLORS[0]);
    expect(ranges![1].color).toBe(CADENCE_ZONE_COLORS[0]);
  });

  it("falls back to Garmin run thresholds, halved for per-leg data", () => {
    // Per-leg rpm (~78): thresholds halve to 75.5/81.5/87/92.5.
    const perLeg = cadenceZoneRanges([], "run", runValues(75));
    expect(perLeg!.map((r) => r.to)).toEqual([75.5, 81.5, 87, 92.5, Infinity]);
    // Full-spm data (~160): thresholds stay 151/163/174/185.
    const fullSpm = cadenceZoneRanges([], "run", runValues(155));
    expect(fullSpm!.map((r) => r.to)).toEqual([151, 163, 174, 185, Infinity]);
    // A healthy 165 spm lands in the green band.
    expect(zoneColorFor(165, fullSpm!)).toBe(CADENCE_ZONE_COLORS[2]);
  });

  it("rides get the fixed rpm bands (crank rpm — no per-leg halving)", () => {
    const ranges = cadenceZoneRanges([], "ride", runValues(85));
    // A two-bucket device (one boundary) yields to the same fixed bands.
    expect(cadenceZoneRanges([zone(0, 70, "cadence"), zone(1, 85, "cadence")], "ride", runValues(85))).toEqual(ranges);
    expect(ranges!.map((r) => r.to)).toEqual([60, 75, 90, 105, Infinity]);
    // 85 rpm sits in the optimal green band; 110+ is the brown top.
    expect(zoneColorFor(85, ranges!)).toBe(CADENCE_ZONE_COLORS[2]);
    expect(zoneColorFor(110, ranges!)).toBe(CADENCE_ZONE_COLORS[4]);
  });

  it("returns null for other sports and empty run data without FIT zones", () => {
    expect(cadenceZoneRanges([], "swim", runValues(85))).toBeNull();
    expect(cadenceZoneRanges([], "run", [])).toBeNull();
    expect(cadenceZoneRanges([], "run", [0, 0])).toBeNull();
    // HR boundaries must not enable cadence bars.
    expect(cadenceZoneRanges([zone(0, 115), zone(1, 135)], "swim", runValues(85))).toBeNull();
  });
});

describe("speedZoneRanges", () => {
  it("prefers FIT boundaries (m/s) converted to the display unit", () => {
    const ranges = speedZoneRanges(
      [
        zone(0, 2.5, "speed"),
        zone(1, 5, "speed"),
        zone(2, 7.5, "speed"),
        zone(3, 10, "speed"),
        zone(4, 12.5, "speed"),
        zone(5, null, "speed"),
      ],
      "run", // FIT zones apply to any sport
      3.6,
    );
    expect(ranges).not.toBeNull();
    expect(ranges!.map((r) => r.to)).toEqual([9, 18, 27, 36, 45, Infinity]);
    // Inverted palette: the fastest zone reads green.
    expect(ranges![5].color).toBe(SPEED_ZONE_COLORS[4]);
    expect(SPEED_ZONE_COLORS[4]).toBe(HR_ZONE_COLORS[0]);
  });

  it("falls back to fixed ride thresholds in the display unit", () => {
    const kmh = speedZoneRanges([], "ride", 3.6);
    // Round away the m/s round-trip float noise (15/3.6*3.6 ≠ exactly 15).
    expect(kmh!.map((r) => Math.round(r.to * 1000) / 1000)).toEqual([
      15, 25, 30, 35, Infinity,
    ]);
    // Imperial: same physical thresholds expressed in mph.
    const mph = speedZoneRanges([], "ride", 2.23694);
    expect(mph!.map((r) => Math.round(r.to * 10) / 10)).toEqual([
      9.3, 15.5, 18.6, 21.7, Infinity,
    ]);
    // Five ranges map the inverted palette exactly: dark-red bottom
    // (crawling), green top (fastest).
    expect(kmh![0].color).toBe(HR_ZONE_COLORS[4]);
    expect(kmh![4].color).toBe(HR_ZONE_COLORS[0]);
  });

  it("returns null off the bike without FIT zones", () => {
    expect(speedZoneRanges([], "other", 3.6)).toBeNull();
    // Cadence boundaries must not enable speed bars.
    expect(
      speedZoneRanges([zone(0, 70, "cadence"), zone(1, 85, "cadence")], "other", 3.6),
    ).toBeNull();
  });
});

describe("speedVisRange", () => {
  it("pads by 2 and rounds to fives with a 0 floor", () => {
    expect(speedVisRange(12.4, 38.2)).toEqual([10, 45]);
    expect(speedVisRange(1, 20)).toEqual([0, 25]);
  });
});

describe("gradeCategory", () => {
  it("maps grade to the palette index, flats and descents to 0", () => {
    expect(gradeCategory(null)).toBe(0);
    expect(gradeCategory(NaN)).toBe(0);
    expect(gradeCategory(-12)).toBe(0); // descents share the flat color
    expect(gradeCategory(0)).toBe(0);
    expect(gradeCategory(3.9)).toBe(0);
    expect(gradeCategory(4)).toBe(1);
    expect(gradeCategory(8)).toBe(2);
    expect(gradeCategory(12)).toBe(3);
    expect(gradeCategory(16)).toBe(4);
    expect(gradeCategory(45)).toBe(4);
  });

  it("has one color per category", () => {
    expect(GRADE_COLORS.length).toBe(GRADE_BOUNDS_PCT.length + 1);
  });
});

describe("gradeSeries", () => {
  // 10 m point spacing, 0..100 m.
  const dist = Array.from({ length: 11 }, (_, i) => i * 10);

  it("reports a constant climb's true grade at every point", () => {
    const alt = dist.map((d) => d * 0.1); // steady 10%
    for (const g of gradeSeries(dist, alt)) expect(g).toBeCloseTo(10);
  });

  it("smooths a single-point altitude spike below its raw grade", () => {
    const alt = dist.map(() => 100);
    alt[5] = 103; // +3 m over 10 m = 30% raw
    const g = gradeSeries(dist, alt);
    // The ±15 m window spans 50→53→50: net 0 at the spike itself.
    expect(Math.abs(g[5]!)).toBeLessThan(1);
    expect(g[0]).toBeCloseTo(0);
  });

  it("skips points missing altitude and windows without span", () => {
    const alt: (number | null)[] = dist.map((d) => d * 0.05);
    alt[3] = null;
    const g = gradeSeries(dist, alt);
    expect(g[3]).toBeNull();
    expect(g[4]).toBeCloseTo(5);
    // Standing still: all samples at one distance — no span, no grade.
    expect(gradeSeries([50, 50, 50], [100, 101, 100])).toEqual([null, null, null]);
    // Fewer than two usable points can't form a slope.
    expect(gradeSeries([0, 10], [100, null])).toEqual([null, null]);
  });

  it("widens to neighbors when points are sparser than the window", () => {
    // 50 m spacing > the 30 m window: a strict centered window would
    // collapse to the point itself and yield null everywhere.
    const d = [0, 50, 100, 150, 200];
    const a = d.map((x) => x * 0.08); // steady 8%
    for (const g of gradeSeries(d, a)) expect(g).toBeCloseTo(8);
  });

  it("is unaffected by null distance rows (pause gaps)", () => {
    const d = [0, 10, null, 20, 30];
    const a = [0, 1, 999, 2, 3];
    const g = gradeSeries(d, a);
    expect(g[2]).toBeNull();
    expect(g[3]).toBeCloseTo(10);
  });
});

describe("selectionGrade", () => {
  const dist = [0, 100, 200, 300, 400];
  const alt = [100, 108, 120, 126, 128];

  it("averages net climb over the selected span, either endpoint order", () => {
    const s = selectionGrade(dist, alt, 1, 3)!;
    expect(s.distanceM).toBe(200);
    expect(s.deltaM).toBe(18);
    expect(s.gradePct).toBeCloseTo(9);
    expect(selectionGrade(dist, alt, 3, 1)).toEqual(s);
  });

  it("slides endpoints inward past missing samples and clamps out-of-range", () => {
    const holes: (number | null)[] = [null, 108, 120, 126, null];
    const s = selectionGrade(dist, holes, 0, 4)!;
    expect(s.distanceM).toBe(200); // 100..300
    expect(s.gradePct).toBeCloseTo(9);
    expect(selectionGrade(dist, holes, -5, 99)!.distanceM).toBe(200);
  });

  it("returns null on degenerate spans", () => {
    expect(selectionGrade(dist, alt, 2, 2)).toBeNull();
    expect(selectionGrade([0, 2], [100, 101], 0, 1)).toBeNull(); // < 5 m
    expect(selectionGrade(dist, [null, null, null, null, null], 0, 4)).toBeNull();
  });

  it("reports descents with negative delta and grade", () => {
    const s = selectionGrade([0, 500], [200, 150], 0, 1)!;
    expect(s.deltaM).toBe(-50);
    expect(s.gradePct).toBeCloseTo(-10);
  });

  it("measures elapsed time at the same slid-inward endpoints", () => {
    const t = [1000, 1060, 1130, 1190, 1250];
    expect(selectionGrade(dist, alt, 1, 3, t)!.durationS).toBe(130);
    // Endpoints slide past missing altitude — time follows them.
    const holes: (number | null)[] = [null, 108, 120, 126, null];
    expect(selectionGrade(dist, holes, 0, 4, t)!.durationS).toBe(130);
  });

  it("reads absent or non-increasing timestamps as no duration", () => {
    expect(selectionGrade(dist, alt, 1, 3)!.durationS).toBeNull();
    const gaps: (number | null)[] = [1000, null, 1130, 1190, 1250];
    expect(selectionGrade(dist, alt, 1, 3, gaps)!.durationS).toBeNull();
    const backwards = [1000, 900, 800, 700, 600];
    expect(selectionGrade(dist, alt, 1, 3, backwards)!.durationS).toBeNull();
  });
});

describe("gradeGradientStops", () => {
  // Identity-ish mapping: x 0..100 → px 0..100 over a 100px plot.
  const xPosOf = (x: number) => x;

  it("paints one flat color when the category never changes", () => {
    const stops = gradeGradientStops([0, 50, 100], [1, 2, 1], xPosOf, 0, 100);
    expect(stops).toEqual([
      { offset: 0, color: GRADE_COLORS[0] },
      { offset: 1, color: GRADE_COLORS[0] },
    ]);
  });

  it("puts a sharp double-stop at the midpoint of a category change", () => {
    const stops = gradeGradientStops([0, 40, 60, 100], [0, 0, 10, 10], xPosOf, 0, 100);
    // Flat until mid(40,60)=50 → 0.5, then the 8–12% color to the end.
    expect(stops).toEqual([
      { offset: 0, color: GRADE_COLORS[0] },
      { offset: 0.5, color: GRADE_COLORS[0] },
      { offset: 0.5, color: GRADE_COLORS[2] },
      { offset: 1, color: GRADE_COLORS[2] },
    ]);
  });

  it("treats null grades as flat and keeps offsets monotonic in [0,1]", () => {
    const stops = gradeGradientStops(
      [0, 25, 50, 75, 100],
      [null, 20, null, 5, null],
      xPosOf,
      0,
      100,
    );
    for (let i = 0; i < stops.length; i++) {
      expect(stops[i].offset).toBeGreaterThanOrEqual(0);
      expect(stops[i].offset).toBeLessThanOrEqual(1);
      if (i > 0) expect(stops[i].offset).toBeGreaterThanOrEqual(stops[i - 1].offset);
    }
    expect(stops[0].color).toBe(GRADE_COLORS[0]);
    expect(stops[stops.length - 1].color).toBe(GRADE_COLORS[0]);
  });

  it("returns no stops for an empty series", () => {
    expect(gradeGradientStops([], [], xPosOf, 0, 100)).toEqual([]);
  });

  it("collapses a NaN position to the previous stop instead of poisoning", () => {
    // addColorStop throws on NaN offsets and kills the chart — the guard
    // must swallow a NaN projection (e.g. an unmeasured plot).
    const stops = gradeGradientStops([0, 50, 100], [0, 10, 10], () => NaN, 0, 100);
    expect(stops).toEqual([
      { offset: 0, color: GRADE_COLORS[0] },
      { offset: 0, color: GRADE_COLORS[0] },
      { offset: 0, color: GRADE_COLORS[2] },
      { offset: 1, color: GRADE_COLORS[2] },
    ]);
  });
});

describe("nearestIdx", () => {
  const xs = [0, 10, 20, 30];

  it("snaps to the closest value, either side", () => {
    expect(nearestIdx(xs, 0)).toBe(0);
    expect(nearestIdx(xs, 14)).toBe(1);
    expect(nearestIdx(xs, 16)).toBe(2);
    expect(nearestIdx(xs, 30)).toBe(3);
  });

  it("clamps values outside the array's range", () => {
    expect(nearestIdx(xs, -5)).toBe(0);
    expect(nearestIdx(xs, 99)).toBe(3);
  });
});

describe("ELEVATION_BANDS_M", () => {
  /** bandGradientStops would NOT throw on a misordered array — the clamp
   * silently eats non-monotonic boundaries and paints wrong colors. This
   * guard makes any future edit to the scale fail loudly instead. */
  it("is strictly ascending and ends at Infinity", () => {
    for (let i = 1; i < ELEVATION_BANDS_M.length; i++) {
      expect(ELEVATION_BANDS_M[i].to).toBeGreaterThan(ELEVATION_BANDS_M[i - 1].to);
    }
    expect(ELEVATION_BANDS_M[ELEVATION_BANDS_M.length - 1].to).toBe(Infinity);
  });
});

describe("bandGradientStops", () => {
  const BANDS = [
    { to: 200, color: "green" },
    { to: 1000, color: "sand" },
    { to: Infinity, color: "snow" },
  ];
  // Linear y-mapping over a 100px plot showing 0..2000 units: top px 0.
  const yPosOf = (v: number) => 100 - (v / 2000) * 100;

  it("paints sharp top-down bands at the ceiling positions", () => {
    const stops = bandGradientStops(BANDS, yPosOf, 0, 100);
    // snow 0 → y(1000)=0.5, sand 0.5 → y(200)=0.9, green 0.9 → 1.
    expect(stops).toEqual([
      { offset: 0, color: "snow" },
      { offset: 0.5, color: "snow" },
      { offset: 0.5, color: "sand" },
      { offset: 0.9, color: "sand" },
      { offset: 0.9, color: "green" },
      { offset: 1, color: "green" },
    ]);
  });

  /** Canvas rejects non-monotonic or out-of-range offsets with an
   * exception — these invariants are what keeps the chart alive. */
  const assertValid = (stops: { offset: number; color: string }[]) => {
    for (let i = 0; i < stops.length; i++) {
      expect(stops[i].offset).toBeGreaterThanOrEqual(0);
      expect(stops[i].offset).toBeLessThanOrEqual(1);
      if (i > 0) expect(stops[i].offset).toBeGreaterThanOrEqual(stops[i - 1].offset);
    }
  };

  it("gives the whole range to the single band containing it", () => {
    // Visible 250..450 sits strictly inside the 200..1000 band: both
    // boundaries project outside the plot (above and below).
    const yPosOf = (v: number) => ((450 - v) / 200) * 100;
    const stops = bandGradientStops(BANDS, yPosOf, 0, 100);
    assertValid(stops);
    const sand = stops.filter((s) => s.color === "sand");
    expect(sand[0].offset).toBe(0);
    expect(sand[sand.length - 1].offset).toBe(1);
    // Neighbors collapse to zero width.
    const snow = stops.filter((s) => s.color === "snow");
    expect(snow[0].offset).toBe(snow[1].offset);
  });

  it("covers everything with the top band on an all-alpine range", () => {
    // Visible 5000..6000: every finite ceiling is far below the bottom.
    const yPosOf = (v: number) => ((6000 - v) / 1000) * 100;
    const stops = bandGradientStops(BANDS, yPosOf, 0, 100);
    assertValid(stops);
    const snow = stops.filter((s) => s.color === "snow");
    expect(snow[0].offset).toBe(0);
    expect(snow[snow.length - 1].offset).toBe(1);
  });

  it("keeps offsets valid with a ceiling exactly on the plot edge", () => {
    // Visible 0..1000: the 1000 boundary lands exactly on the top edge.
    const stops = bandGradientStops(BANDS, (v) => 100 - v / 10, 0, 100);
    assertValid(stops);
    // Degenerate zero-width snow stop at the very top is allowed.
    const snow = stops.filter((s) => s.color === "snow");
    expect(snow[0].offset).toBe(0);
    expect(snow[1].offset).toBe(0);
  });

  it("survives a NaN projection without producing invalid offsets", () => {
    const stops = bandGradientStops(BANDS, () => NaN, 0, 100);
    assertValid(stops);
    // The bottom band (constant projection) still covers the plot.
    expect(stops[stops.length - 1]).toEqual({ offset: 1, color: "green" });
  });

  it("clamps to a monotonic 0..1 — sea-level rides stay all green", () => {
    // Visible range 0..100 units: every ceiling maps far above the top.
    const seaLevel = bandGradientStops(BANDS, (v) => 100 - v, 0, 100);
    for (const s of seaLevel) expect(s.offset).toBeGreaterThanOrEqual(0);
    // Monotonic non-decreasing (canvas requirement)…
    for (let i = 1; i < seaLevel.length; i++) {
      expect(seaLevel[i].offset).toBeGreaterThanOrEqual(seaLevel[i - 1].offset);
    }
    // …with the mountain bands collapsed and green covering everything.
    expect(seaLevel[seaLevel.length - 1]).toEqual({ offset: 1, color: "green" });
    expect(seaLevel.filter((s) => s.color === "green")[0].offset).toBeLessThan(0.2);
  });
});

describe("zoneColorFor", () => {
  it("picks the range containing the value, falling back for misses", () => {
    expect(zoneColorFor(90, DEFAULT_HR_RANGES)).toBe(HR_ZONE_COLORS[0]);
    expect(zoneColorFor(130, DEFAULT_HR_RANGES)).toBe(HR_ZONE_COLORS[2]);
    expect(zoneColorFor(200, DEFAULT_HR_RANGES)).toBe(HR_ZONE_COLORS[4]);
    // Boundary belongs to the upper range (design: v >= lo && v < hi).
    expect(zoneColorFor(120, DEFAULT_HR_RANGES)).toBe(HR_ZONE_COLORS[2]);
    expect(zoneColorFor(-5, DEFAULT_HR_RANGES)).toBe(HR_FALLBACK_COLOR);
  });
});

describe("hrVisRange", () => {
  it("pads by 10, rounds to tens and clamps to 40..220", () => {
    expect(hrVisRange(83, 172)).toEqual([70, 190]);
    expect(hrVisRange(45, 210)).toEqual([40, 220]);
    expect(hrVisRange(0, 300)).toEqual([40, 220]);
  });
});

describe("bucketMaxBars", () => {
  it("keeps short spikes via window max and maps indices both ways", () => {
    const xs = Array.from({ length: 10 }, (_, i) => i);
    const vals = [100, 100, 180, 100, 100, 100, 100, 100, 100, 100];
    const b = bucketMaxBars(xs, vals, 5); // x-windows of width 1.8
    expect(b.values).toEqual([100, 180, 100, 100, 100]);
    expect(b.srcIdx[1]).toBe(2); // the spike sample represents its bar
    expect(b.barOf[2]).toBe(1);
    expect(b.barOf[9]).toBe(4);
    expect(b.xs).toHaveLength(5);
  });

  it("centers bars on equal x-windows so widths can't collapse", () => {
    // Samples bunched on the x axis (dense trackpoints on a distance axis):
    // equal-x windows keep bar centers a fixed span apart, leaving a gap
    // for the empty middle window instead of squeezing centers together.
    const b = bucketMaxBars([0, 1, 9], [1, 2, 3], 3); // windows of width 3
    expect(b.values).toEqual([2, 3]);
    expect(b.xs).toEqual([1.5, 7.5]); // window centers, 2 windows apart
    expect(b.barOf).toEqual([0, 0, 1]);
    // The window width feeds the x-scale padding that keeps the first and
    // last bars (centered on the scale edges) from being clipped in half.
    expect(b.step).toBe(3);
  });

  it("caps the bar count and survives short/degenerate series", () => {
    const xs = Array.from({ length: 1000 }, (_, i) => i);
    const vals = xs.map((i) => 100 + (i % 50));
    expect(bucketMaxBars(xs, vals, 40).values.length).toBeLessThanOrEqual(40);

    const tiny = bucketMaxBars([0, 1], [5, 7], 40);
    expect(tiny.values).toEqual([5, 7]);
    expect(bucketMaxBars([], [], 40).values).toEqual([]);
    // Zero x-span (all samples at one x) folds into a single bar.
    expect(bucketMaxBars([5, 5], [1, 9], 40).values).toEqual([9]);
  });
});

describe("nearestChartIdx", () => {
  it("prefers the exact mapping and searches outward over holes", () => {
    // Trackpoints 0..6, chart kept only 0,2,3,6 (holes at 1,4,5).
    const rm = new Map([
      [0, 0],
      [2, 1],
      [3, 2],
      [6, 3],
    ]);
    expect(nearestChartIdx(rm, 3, 6)).toBe(2); // exact
    expect(nearestChartIdx(rm, 1, 6)).toBe(0); // hole → nearest neighbor
    expect(nearestChartIdx(rm, 5, 6)).toBe(3); // 4 and 6 tie-break upward is fine either way
    expect(nearestChartIdx(new Map(), 3, 6)).toBeNull();
  });
});

describe("rangesEqual", () => {
  it("compares spans by value, null-safe", () => {
    expect(rangesEqual(null, null)).toBe(true);
    expect(rangesEqual([1, 5], [1, 5])).toBe(true);
    expect(rangesEqual([1, 5], [1, 6])).toBe(false);
    expect(rangesEqual([1, 5], null)).toBe(false);
    expect(rangesEqual(null, [1, 5])).toBe(false);
  });
});

describe("externalSelectionCols", () => {
  const rm = new Map([
    [0, 0],
    [2, 1],
    [3, 2],
    [6, 3],
  ]);

  it("maps a trackpoint range to an ordered chart-column span", () => {
    expect(externalSelectionCols([0, 6], rm, 6)).toEqual([0, 3]);
    // Holes resolve to neighbors; reversed input still comes out ordered.
    expect(externalSelectionCols([5, 1], rm, 6)).toEqual([0, 3]);
  });

  it("rejects ranges that collapse to a single column or find nothing", () => {
    expect(externalSelectionCols([3, 3], rm, 6)).toBeNull();
    expect(externalSelectionCols([1, 3], new Map(), 6)).toBeNull();
  });
});
