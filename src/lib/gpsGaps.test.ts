import { describe, expect, it } from "vitest";
import { fixRuns, gapBridges, gpsGaps, GPS_GAP_MIN_S, secondsWithoutGps } from "./gpsGaps";

/** One row a second from t = 0; `fix(i)` decides whether row i has a position. */
function track(n: number, fix: (i: number) => boolean) {
  const t: number[] = [];
  const lat: (number | null)[] = [];
  const lon: (number | null)[] = [];
  for (let i = 0; i < n; i++) {
    t.push(i);
    lat.push(fix(i) ? 55 + i * 1e-4 : null);
    lon.push(fix(i) ? 37 : null);
  }
  return { t, lat, lon };
}

describe("gpsGaps", () => {
  it("finds a stretch without a fix inside the track", () => {
    const tr = track(300, (i) => i < 100 || i >= 200);
    expect(gpsGaps(tr)).toEqual([{ before: 99, after: 200, seconds: 101 }]);
    expect(secondsWithoutGps(tr)).toBe(101);
  });

  it("ignores a stretch shorter than the minimum", () => {
    const tr = track(300, (i) => i < 100 || i >= 100 + GPS_GAP_MIN_S - 2);
    expect(gpsGaps(tr)).toEqual([]);
  });

  it("finds a missing start and end, but counts only the holes in the route", () => {
    const tr = track(300, (i) => (i >= 60 && i < 120) || (i >= 180 && i < 240));
    expect(gpsGaps(tr)).toEqual([
      { before: -1, after: 60, seconds: 60 },
      { before: 119, after: 180, seconds: 61 },
      { before: 239, after: 300, seconds: 60 },
    ]);
    expect(secondsWithoutGps(tr)).toBe(61);
    expect(secondsWithoutGps(track(300, () => false))).toBe(0);
  });

  it("skips a gap whose ends have no time", () => {
    const tr = track(300, (i) => i < 100 || i >= 200);
    tr.t[99] = null as unknown as number;
    expect(gpsGaps({ ...tr, t: tr.t as (number | null)[] })).toEqual([]);
  });

  it("does not take a pause for a gap", () => {
    // Recording stopped for 10 minutes: no rows between the two fixes.
    const tr = { t: [0, 1, 2, 602, 603], lat: [55, 55, 55, 55.01, 55.01], lon: [37, 37, 37, 37, 37] };
    expect(gpsGaps(tr)).toEqual([]);
  });
});

describe("gpsGaps beside a pause", () => {
  it("does not take a pause with a lone row without a fix for lost GPS", () => {
    // The real ride that said "No GPS for 44 minutes": a stop, one row without a fix.
    const tr = {
      t: [0, 1, 2, 2304, 2305],
      lat: [55, 55, null, 55.01, 55.01],
      lon: [37, 37, null, 37, 37],
    };
    expect(gpsGaps(tr)).toEqual([]);
    expect(secondsWithoutGps(tr)).toBe(0);
    expect(fixRuns(tr).map((r) => r.length)).toEqual([4]);
  });

  it("counts the jammed minute and not the pause next to it", () => {
    const n = 61;
    const tr = {
      t: Array.from({ length: n }, (_, i) => (i === 60 ? 900 : i)),
      lat: Array.from({ length: n }, (_, i) => (i === 0 || i === 60 ? 55 : null)),
      lon: Array.from({ length: n }, (_, i) => (i === 0 || i === 60 ? 37 : null)),
    };
    expect(gpsGaps(tr)).toEqual([{ before: 0, after: 60, seconds: 59 }]);
  });
});

describe("gapBridges", () => {
  const tr = track(300, (i) => i < 100 || i >= 200);

  it("joins the fixes on both sides of a marked gap, and the route stays broken", () => {
    const marks = Array.from({ length: 300 }, (_, i) => i === 200);
    expect(gapBridges({ ...tr, gap_bridged: marks })).toEqual([
      [
        [55 + 99e-4, 37],
        [55 + 200e-4, 37],
      ],
    ]);
    expect(fixRuns({ ...tr, gap_bridged: marks }).map((r) => r.length)).toEqual([100, 100]);
  });

  it("draws one line for each marked gap", () => {
    const two = track(400, (i) => i < 100 || (i >= 200 && i < 300));
    const three = { ...two, lat: [...two.lat], lon: [...two.lon] };
    three.lat[399] = 56;
    three.lon[399] = 37;
    const marks = Array.from({ length: 400 }, (_, i) => i === 200 || i === 399);
    expect(gapBridges({ ...three, gap_bridged: marks }).map((b) => b[1][0])).toEqual([55 + 200e-4, 56]);
  });

  it("has nothing to draw with no mark", () => {
    expect(gapBridges(tr)).toEqual([]);
    expect(gapBridges({ ...tr, gap_bridged: [] })).toEqual([]);
  });

  it("draws no line from nowhere at a mark on the first fix", () => {
    const marks = Array.from({ length: 300 }, (_, i) => i === 0);
    expect(gapBridges({ ...tr, gap_bridged: marks })).toEqual([]);
  });
});

describe("fixRuns", () => {
  it("breaks the line at a gap inside the track only", () => {
    const tr = track(300, (i) => (i >= 20 && i < 100) || (i >= 200 && i < 280));
    const runs = fixRuns(tr);
    expect(runs.map((r) => r.length)).toEqual([80, 80]);
    expect(runs[1][0]).toEqual([55 + 200e-4, 37]);
  });

  it("keeps a short dropout in one line", () => {
    const tr = track(100, (i) => i !== 50);
    expect(fixRuns(tr).map((r) => r.length)).toEqual([99]);
  });

  it("breaks the line at a jump the backend marked", () => {
    const tr = track(5, () => true);
    const runs = fixRuns({ ...tr, teleported: [false, false, false, true, false] });
    expect(runs.map((r) => r.length)).toEqual([3, 2]);
    expect(runs[1][0]).toEqual([55 + 3e-4, 37]);
    expect(gpsGaps(tr)).toEqual([]);
  });

  it("draws one line when nothing is marked", () => {
    const tr = track(5, () => true);
    expect(fixRuns({ ...tr, teleported: [] }).map((r) => r.length)).toEqual([5]);
  });

  it("starts no empty line at a mark on the first fix", () => {
    const tr = track(3, () => true);
    expect(fixRuns({ ...tr, teleported: [true, false, false] }).map((r) => r.length)).toEqual([3]);
  });
});
