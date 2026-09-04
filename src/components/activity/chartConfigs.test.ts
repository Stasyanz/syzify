import { afterEach, describe, expect, it } from "vitest";
import type { TrackPointColumns } from "../../lib/types";
import { useUnitsStore, M_PER_MILE, MPH_PER_MPS, FT_PER_M } from "../../lib/units";
import {
  CADENCE,
  ELEVATION,
  HR,
  PACE,
  POWER,
  SPEED,
  SWIM_PACE,
  RUN_STOP_MPS,
  SWIM_STOP_MPS,
  fmtPace,
  hasData,
  resolveAverages,
  speedSeries,
  type ChartConfig,
  type ChartType,
} from "./chartConfigs";

/** A trackpoint column set with every column null-filled to `n`, then the
 * given columns overlaid. */
function columns(n: number, over: Partial<TrackPointColumns> = {}): TrackPointColumns {
  const nulls = () => new Array<number | null>(n).fill(null);
  return {
    t: nulls(),
    lat: nulls(),
    lon: nulls(),
    altitude_m: nulls(),
    speed_mps: nulls(),
    hr: nulls(),
    cadence: nulls(),
    power_w: nulls(),
    temperature_c: nulls(),
    vertical_oscillation_mm: nulls(),
    stance_time_ms: nulls(),
    stance_time_percent: nulls(),
    step_length_mm: nulls(),
    grade_percent: nulls(),
    distance_m: nulls(),
    left_right_balance: nulls(),
    left_torque_effectiveness: nulls(),
    right_torque_effectiveness: nulls(),
    left_pedal_smoothness: nulls(),
    right_pedal_smoothness: nulls(),
    ...over,
  };
}

afterEach(() => useUnitsStore.setState({ mode: "metric" }));

describe("fmtPace", () => {
  it("formats decimal minutes as m:ss", () => {
    expect(fmtPace(5.5)).toBe("5:30");
    expect(fmtPace(4)).toBe("4:00");
    expect(fmtPace(5.0083)).toBe("5:00");
  });

  it("rolls 60 seconds into the next minute", () => {
    expect(fmtPace(4.9999)).toBe("5:00");
  });

  it("is empty for non-positive or non-finite paces", () => {
    expect(fmtPace(0)).toBe("");
    expect(fmtPace(-1)).toBe("");
    expect(fmtPace(Infinity)).toBe("");
  });
});

describe("speedSeries", () => {
  it("uses the recorded speed when it carries signal", () => {
    const tp = columns(3, { speed_mps: [1, 2, 3] });
    expect(speedSeries(tp)).toBe(tp.speed_mps);
  });

  it("derives speed from distance and time when the field is absent or all zeros", () => {
    const tp = columns(3, {
      speed_mps: [0, 0, 0],
      distance_m: [0, 10, 30],
      t: [0, 5, 10],
    });
    expect(speedSeries(tp)).toEqual([null, 2, 4]);
  });

  it("skips points without distance or time and refuses backwards motion", () => {
    const tp = columns(4, {
      distance_m: [0, null, 20, 10],
      t: [0, 5, 10, 15],
    });
    // Point 2 pairs with point 0 (10 s, 20 m); point 3 goes backwards.
    expect(speedSeries(tp)).toEqual([null, null, 2, null]);
  });
});

describe("hasData", () => {
  it("needs at least one non-null, non-zero sample", () => {
    expect(hasData([null, 0, 0])).toBe(false);
    expect(hasData([null, 0, 1])).toBe(true);
    expect(hasData([])).toBe(false);
  });
});

describe("metric configs", () => {
  it("map their columns and summary fields", () => {
    const tp = columns(2, { hr: [120, 130], cadence: [80, 90], power_w: [200, 220] });
    expect(HR.getData(tp)).toBe(tp.hr);
    expect(CADENCE.getData(tp)).toBe(tp.cadence);
    expect(POWER.getData(tp)).toBe(tp.power_w);
    const s = { hr: 125, cadence: 85, power_w: 210, speed_mps: 10 };
    expect(HR.summaryAvg!(s)).toBe(125);
    expect(CADENCE.summaryAvg!(s)).toBe(85);
    expect(POWER.summaryAvg!(s)).toBe(210);
  });

  it("elevation opts out of the average line and converts to feet when imperial", () => {
    const tp = columns(2, { altitude_m: [100, null] });
    expect(ELEVATION().noAverage).toBe(true);
    expect(ELEVATION().getData(tp)).toBe(tp.altitude_m);
    useUnitsStore.setState({ mode: "imperial" });
    expect(ELEVATION().getData(tp)).toEqual([100 * FT_PER_M, null]);
    expect(ELEVATION().elevationBands!.every((b, i) => b.to === ELEVATION().elevationBands![i].to)).toBe(true);
  });

  it("speed converts the series and the summary with one factor", () => {
    const tp = columns(2, { speed_mps: [10, null] });
    expect(SPEED().getData(tp)).toEqual([36, null]);
    expect(SPEED().summaryAvg!({ speed_mps: 10 })).toBe(36);
    expect(SPEED().summaryAvg!({ speed_mps: null })).toBeNull();
    useUnitsStore.setState({ mode: "imperial" });
    expect(SPEED().getData(tp)).toEqual([10 * MPH_PER_MPS, null]);
    expect(SPEED().summaryAvg!({ speed_mps: 10 })).toBeCloseTo(10 * MPH_PER_MPS, 9);
  });

  describe.each([
    ["run", PACE, RUN_STOP_MPS, 1000, M_PER_MILE],
    ["swim", SWIM_PACE, SWIM_STOP_MPS, 100, 91.44],
  ] as const)("%s pace", (_name, factory, cutoff, perMetric, perImperial) => {
    it("hides near-stops with the SAME cutoff in the series and the average", () => {
      const tp = columns(3, { speed_mps: [cutoff, cutoff + 1, null] });
      const cfg = factory();
      expect(cfg.getData(tp)).toEqual([null, perMetric / (cutoff + 1) / 60, null]);
      expect(cfg.sampleAvg!([], tp.speed_mps, tp.t)).toBeCloseTo(
        perMetric / (cutoff + 1) / 60,
        9,
      );
    });

    it("derives the summary pace from the summary speed", () => {
      expect(factory().summaryAvg!({ speed_mps: 2 })).toBeCloseTo(perMetric / 2 / 60, 9);
      expect(factory().summaryAvg!({ speed_mps: 0 })).toBeNull();
      expect(factory().summaryAvg!({})).toBeNull();
    });

    it("switches the per-unit distance with the units setting", () => {
      useUnitsStore.setState({ mode: "imperial" });
      expect(factory().summaryAvg!({ speed_mps: 2 })).toBeCloseTo(perImperial / 2 / 60, 6);
      expect(factory().invertY).toBe(true);
      expect(factory().valueFmt).toBe(fmtPace);
    });
  });
});

describe("resolveAverages", () => {
  const seriesOf = (configs: ChartConfig[], tp: TrackPointColumns) =>
    new Map<ChartType, (number | null)[]>(configs.map((c) => [c.key, c.getData(tp)]));

  it("prefers the summary value and falls back to the samples per metric", () => {
    const tp = columns(3, { hr: [100, 120, 140], power_w: [200, 200, 260], t: [0, 1, 2] });
    const configs = [HR, POWER];
    const avg = resolveAverages(configs, seriesOf(configs, tp), tp, { hr: 118 });
    expect(avg.get("hr")).toBe(118);
    expect(avg.get("power")).toBe(220);
  });

  it("ignores a non-finite summary value", () => {
    const tp = columns(2, { hr: [100, 120] });
    const avg = resolveAverages([HR], seriesOf([HR], tp), tp, { hr: NaN });
    expect(avg.get("hr")).toBe(110);
  });

  it("skips elevation entirely", () => {
    const tp = columns(2, { altitude_m: [10, 20] });
    const configs = [ELEVATION()];
    const avg = resolveAverages(configs, seriesOf(configs, tp), tp, {});
    expect(avg.has("elevation")).toBe(false);
  });

  it("derives the pace fallback from the raw speeds, not the pace series", () => {
    // 2 m/s and 4 m/s → mean speed 3 m/s → 5:33 /km (not the 6:15 a mean of
    // the paces would give).
    const tp = columns(2, { speed_mps: [2, 4] });
    const configs = [PACE()];
    const avg = resolveAverages(configs, seriesOf(configs, tp), tp, {});
    expect(avg.get("pace")).toBeCloseTo(1000 / 3 / 60, 9);
  });

  it("is null when neither the summary nor the samples carry a value", () => {
    const tp = columns(2);
    const avg = resolveAverages([HR], seriesOf([HR], tp), tp, {});
    expect(avg.get("hr")).toBeNull();
    // A config whose series was never materialized is null too, not a crash.
    expect(resolveAverages([HR], new Map(), tp, {}).get("hr")).toBeNull();
  });
});
