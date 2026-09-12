// @vitest-environment happy-dom
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, cleanup, act, fireEvent, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { TrackPointColumns } from "../../lib/types";

vi.mock("../../lib/tauri", () => ({
  api: { getSetting: vi.fn(async () => null), setSetting: vi.fn(async () => undefined) },
}));

// happy-dom has no canvas — a shell in uPlot's place is enough for the
// grid layout this test is about.
/** Every uPlot built by the panel, for the layer tests below. */
type Painter = (u: unknown) => unknown;
type SeriesOpts = { label?: string; stroke?: string | Painter; fill?: string | Painter; width?: number };
type PlotOpts = { series: SeriesOpts[]; height?: number; hooks?: { setCursor?: ((u: unknown) => void)[] } };
const built: { opts: PlotOpts; data: unknown[] }[] = [];
/** Every setSize the panel asked of a plot, in order. */
const sizes: { width: number; height: number; label?: string }[] = [];
/** The bar-path configs handed to uPlot.paths.bars — their fill painter. */
const barCfgs: { disp: { fill: { values: (u: unknown) => string[] } } }[] = [];

vi.mock("uplot", () => ({
  default: class {
    static pxRatio = 1;
    static paths = {
      bars: (cfg: { disp: { fill: { values: (u: unknown) => string[] } } }) => {
        barCfgs.push(cfg);
        return () => null;
      },
    };
    over = document.createElement("div");
    cursor = { idx: null };
    height: number;
    label?: string;
    constructor(opts: unknown, data: unknown, target: HTMLElement) {
      const o = opts as PlotOpts;
      built.push({ opts: o, data: data as unknown[] });
      this.height = o.height ?? 0;
      this.label = o.series[1]?.label;
      target.appendChild(this.over);
    }
    setSize(s: { width: number; height: number }) {
      this.height = s.height;
      sizes.push({ ...s, label: this.label });
    }
    setCursor() {}
    valToPos() {
      return 0;
    }
    destroy() {}
  },
}));
vi.mock("uplot/dist/uPlot.min.css", () => ({}));

import { ChartPanel, clampWideChartHeight, CHART_HEIGHT_PX, WIDE_CHART_MAX_HEIGHT_PX } from "./ChartPanel";
import { api } from "../../lib/tauri";
import { GRADE_COLORS } from "./chartZones";

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

const N = 20;
const seq = (f: (i: number) => number) => Array.from({ length: N }, (_, i) => f(i));
// Heart rate + speed = two charts: the first spans the row, the second is
// alone — a hole. Adding cadence makes three: a pair, no hole.
const twoCharts = columns(N, {
  t: seq((i) => i),
  distance_m: seq((i) => i * 10),
  hr: seq((i) => 120 + i),
  speed_mps: seq(() => 10),
});
const threeCharts = { ...twoCharts, cadence: seq(() => 85) };

function renderPanel(tp: TrackPointColumns, onFillerPlaced: (p: boolean) => void, sport = "ride") {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const filler = <div data-testid="filler">zones</div>;
  const ui = (cols: TrackPointColumns) => (
    <QueryClientProvider client={qc}>
      <ChartPanel trackpoints={cols} sport={sport} filler={filler} onFillerPlaced={onFillerPlaced} />
    </QueryClientProvider>
  );
  const r = render(ui(tp));
  return { ...r, rerenderWith: (cols: TrackPointColumns) => r.rerender(ui(cols)) };
}

beforeEach(() => {
  globalThis.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  } as unknown as typeof ResizeObserver;
});

afterEach(cleanup);

describe("ChartPanel elevation layers", () => {
  beforeEach(() => built.splice(0));

  /** A stand-in for the live uPlot the painters receive: a 100×50 plot
   * whose x maps 1:1 to px, and a canvas context that records gradient
   * stops instead of painting. */
  function fakePlot() {
    const stops: { offset: number; color: string }[] = [];
    const u = {
      bbox: { left: 0, top: 0, width: 100, height: 50 },
      valToPos: (v: number, axis: string) => (axis === "x" ? v : 50 - v / 10),
      ctx: { createLinearGradient: () => ({ addColorStop: (o: number, c: string) => stops.push({ offset: o, color: c }) }) },
      data: [[0, 1], [130, 165]],
    };
    return { u, stops };
  }

  it("draws the profile with grades as two series over one column, other charts as one", () => {
    // Flat, then a 10% pitch, then flat: the fill layer exists and every
    // painter has a real color change to place along x.
    const climb = columns(N, {
      distance_m: seq((i) => i * 100),
      altitude_m: seq((i) => (i < 6 ? 100 : i < 14 ? 100 + (i - 6) * 10 : 180)),
      hr: seq((i) => 120 + i),
    });
    renderPanel(climb, vi.fn());
    const elevation = built.find((b) => b.opts.series[1]?.label?.startsWith("Elevation"));
    expect(elevation, "an elevation chart").toBeTruthy();
    // x + the altitude fill (no stroke) + the grade fill carrying the
    // line; both series read the same column.
    const [, fillOnly, withLine] = elevation!.opts.series;
    expect(elevation!.opts.series).toHaveLength(3);
    // The stack order is the point: fill-only first, the line on top.
    expect(fillOnly.width).toBe(0);
    expect(withLine.width).toBe(2);
    expect(typeof withLine.stroke).toBe("function");
    expect(typeof withLine.fill).toBe("function");
    expect(elevation!.data).toHaveLength(3);
    expect(elevation!.data[2]).toBe(elevation!.data[1]);
    const hr = built.find((b) => b.opts.series[1]?.label?.startsWith("Heart"));
    expect(hr!.opts.series).toHaveLength(2);
    expect(hr!.data).toHaveLength(2);

    // The painters run against a live plot: every gradient's stops are
    // finite and monotonic (a NaN offset would throw in addColorStop and
    // take the chart down), and the pitch puts real transitions along x —
    // the x mapping is exercised, not just the two end stops.
    for (const painter of [fillOnly.fill, withLine.fill, withLine.stroke] as Painter[]) {
      const { u, stops } = fakePlot();
      painter(u);
      expect(stops.length).toBeGreaterThan(2);
      const offsets = stops.map((s) => s.offset);
      expect(offsets.every((o) => Number.isFinite(o) && o >= 0 && o <= 1)).toBe(true);
      expect(offsets).toEqual([...offsets].sort((a, b) => a - b));
    }
    const { u, stops } = fakePlot();
    (withLine.fill as Painter)(u);
    // Transparent on the flats, the 8–12% color under the pitch, the
    // transitions strictly inside the plot.
    expect(stops[0].color).toBe("rgba(0, 0, 0, 0)");
    expect(stops[stops.length - 1].color).toBe("rgba(0, 0, 0, 0)");
    const inner = stops.filter((s) => s.offset > 0 && s.offset < 1);
    expect(inner.length).toBeGreaterThan(0);
    expect(inner.some((s) => s.color === GRADE_COLORS[3])).toBe(true);

    // The zone bars' painter colors each bar by its value.
    expect(barCfgs.length).toBeGreaterThan(0);
    const colors = barCfgs[barCfgs.length - 1].disp.fill.values(fakePlot().u);
    expect(colors).toHaveLength(2);
    expect(colors.every((c) => /^#[0-9a-f]{6}e0$/i.test(c))).toBe(true);
  });

  it("labels a painted climb with one average for the whole band, elsewhere the point's grade", () => {
    const N = 20;
    const climb = columns(N, {
      distance_m: seq((i) => i * 100),
      altitude_m: seq((i) => (i < 6 ? 100 : i < 14 ? 100 + (i - 6) * 10 : 180)),
    });
    const { container } = renderPanel(climb, vi.fn());
    const elevation = built.find((b) => b.opts.series[1]?.label?.startsWith("Elevation"))!;
    const onCursor = elevation.opts.hooks!.setCursor![0];
    const hover = (idx: number | null) => {
      act(() => onCursor({ cursor: { idx, left: 0 }, over: document.createElement("div") }));
      return container.textContent ?? "";
    };
    // Two points of the pitch answer with the same number — the band's
    // average (the 10 % it is built from, eased by the 5 % the smoothing
    // window reads at either end) — not their own.
    const mid = hover(9).match(/\+9\.4%/);
    expect(mid, "band average over the pitch").toBeTruthy();
    expect(hover(12)).toContain(mid![0]);
    // On the flat the point's own grade, and none once the cursor leaves.
    expect(hover(2)).not.toContain(mid![0]);
    expect(hover(2)).toMatch(/0\.0%/);
    expect(hover(null)).not.toMatch(/%/);
  });

  it("fills an inverted chart down from the line toward the slow edge", () => {
    // A run with speed: the pace chart's y axis is inverted, so its fill
    // must start at the data max (the slow edge), not the 0 baseline.
    const run = columns(N, { t: seq((i) => i * 30), distance_m: seq((i) => i * 100), speed_mps: seq(() => 3) });
    renderPanel(run, vi.fn(), "run");
    const pace = built.find((b) => b.opts.series[1]?.label?.startsWith("Pace"));
    expect(pace, "a pace chart").toBeTruthy();
    const fillTo = (pace!.opts.series[1] as { fillTo?: (u: unknown, si: number, min: number, max: number) => number }).fillTo;
    expect(fillTo?.({}, 1, 4, 9)).toBe(9);
  });

  it("stays one series on a dead-flat track and on one that never reaches the climb threshold", () => {
    const flat = columns(N, { distance_m: seq((i) => i * 100), altitude_m: seq(() => 100) });
    renderPanel(flat, vi.fn());
    let elevation = built.find((b) => b.opts.series[1]?.label?.startsWith("Elevation"));
    expect(elevation!.opts.series).toHaveLength(2);
    expect(elevation!.data).toHaveLength(2);

    built.splice(0);
    // A 1% rise: the line colors by grade, but there is nothing to fill.
    const rolling = columns(N, { distance_m: seq((i) => i * 100), altitude_m: seq((i) => 100 + i) });
    renderPanel(rolling, vi.fn());
    elevation = built.find((b) => b.opts.series[1]?.label?.startsWith("Elevation"));
    expect(elevation!.opts.series).toHaveLength(2);
    expect(typeof elevation!.opts.series[1].stroke).toBe("function");
    expect(elevation!.data).toHaveLength(2);
  });
});

describe("ChartPanel filler slot", () => {
  it("drops the filler into the lone last slot and reports it, and takes it back", () => {
    const placed = vi.fn();
    const { container, rerenderWith, unmount } = renderPanel(twoCharts, placed);
    const grid = container.querySelector(".grid.grid-cols-2")!;
    expect(grid.querySelectorAll("[data-chart-key]")).toHaveLength(2);
    // The filler is the grid's last child, after the charts.
    expect(grid.lastElementChild!.getAttribute("data-testid")).toBe("filler");
    expect(placed).toHaveBeenLastCalledWith(true);

    // Three charts pair up: no slot, no filler.
    rerenderWith(threeCharts);
    expect(grid.querySelectorAll("[data-chart-key]")).toHaveLength(3);
    expect(container.querySelector('[data-testid="filler"]')).toBeNull();
    expect(placed).toHaveBeenLastCalledWith(false);

    rerenderWith(twoCharts);
    expect(placed).toHaveBeenLastCalledWith(true);
    unmount();
    expect(placed).toHaveBeenLastCalledWith(false);
  });

  it("never places anything without a filler or without chart data", () => {
    const placed = vi.fn();
    const qc = new QueryClient();
    const { container } = render(
      <QueryClientProvider client={qc}>
        <ChartPanel trackpoints={twoCharts} sport="ride" onFillerPlaced={placed} />
      </QueryClientProvider>,
    );
    expect(container.querySelectorAll("[data-chart-key]")).toHaveLength(2);
    expect(placed).toHaveBeenLastCalledWith(false);

    const empty = render(
      <QueryClientProvider client={qc}>
        <ChartPanel
          trackpoints={columns(N)}
          sport="ride"
          filler={<div data-testid="filler" />}
          onFillerPlaced={placed}
        />
      </QueryClientProvider>,
    );
    expect(empty.container.innerHTML).toBe("");
    expect(placed).toHaveBeenLastCalledWith(false);
  });
});

describe("first-slot grip", () => {
  const N = 20;
  const track = () =>
    columns(N, { distance_m: seq((i) => i * 100), altitude_m: seq((i) => 100 + i), hr: seq((i) => 120 + i) });
  const grip = (c: HTMLElement) => c.querySelector('[data-testid="wide-chart-grip"]') as HTMLElement;

  beforeEach(() => {
    built.splice(0);
    sizes.splice(0);
    vi.mocked(api.setSetting).mockClear();
    // happy-dom has no pointer capture; the handlers call it unconditionally
    // (WKWebView needs it or the drag dies at the handle's edge).
    HTMLElement.prototype.setPointerCapture = vi.fn();
    HTMLElement.prototype.hasPointerCapture = vi.fn(() => true);
    HTMLElement.prototype.releasePointerCapture = vi.fn();
  });
  const proto = HTMLElement.prototype as unknown as Record<string, unknown>;
  const original = {
    set: proto.setPointerCapture,
    has: proto.hasPointerCapture,
    release: proto.releasePointerCapture,
  };
  afterEach(() => {
    proto.setPointerCapture = original.set;
    proto.hasPointerCapture = original.has;
    proto.releasePointerCapture = original.release;
    // A failed waitFor must not leak a per-test getSetting into the next.
    vi.mocked(api.getSetting).mockImplementation(async () => null);
  });

  it("clamps a dragged or persisted height into [default, +50 %], NaN to the default", () => {
    expect(clampWideChartHeight(250)).toBe(250);
    expect(clampWideChartHeight(250.4)).toBe(250);
    expect(clampWideChartHeight(10)).toBe(CHART_HEIGHT_PX);
    expect(clampWideChartHeight(10_000)).toBe(WIDE_CHART_MAX_HEIGHT_PX);
    expect(WIDE_CHART_MAX_HEIGHT_PX).toBe(CHART_HEIGHT_PX * 1.5);
    expect(clampWideChartHeight(NaN)).toBe(CHART_HEIGHT_PX);
    expect(clampWideChartHeight(Infinity)).toBe(CHART_HEIGHT_PX);
  });

  it("only the full-width first card has the grip, and dragging it resizes that plot in place", () => {
    const { container } = renderPanel(track(), vi.fn());
    const cards = container.querySelectorAll("[data-chart-key]");
    expect(cards.length).toBeGreaterThan(1);
    expect(cards[0].querySelector('[data-testid="wide-chart-grip"]')).not.toBeNull();
    expect(cards[1].querySelector('[data-testid="wide-chart-grip"]')).toBeNull();
    // Every plot starts at the shared default height, and none is resized
    // on mount: uPlot's setSize is a full recommit even for equal sizes.
    expect(built.every((b) => b.opts.height === CHART_HEIGHT_PX)).toBe(true);
    expect(sizes).toHaveLength(0);
    const plotsBefore = built.length;

    const g = grip(container);
    fireEvent.pointerDown(g, { pointerId: 1, clientY: 10 });
    // The grip's pointerdown stops before the card's reorder drag: no
    // card turns into the dragging ghost.
    expect(container.querySelector(".opacity-40")).toBeNull();
    fireEvent.pointerMove(g, { pointerId: 1, clientY: 70 });
    // +60 px on the first plot, no rebuild, the others untouched.
    expect(sizes[sizes.length - 1].height).toBe(CHART_HEIGHT_PX + 60);
    expect(built.length).toBe(plotsBefore);
    fireEvent.pointerMove(g, { pointerId: 1, clientY: 900 });
    expect(sizes[sizes.length - 1].height).toBe(WIDE_CHART_MAX_HEIGHT_PX);
    expect(api.setSetting).not.toHaveBeenCalled();
    fireEvent.pointerUp(g, { pointerId: 1, clientY: 900 });
    expect(api.setSetting).toHaveBeenCalledWith("chart_wide_height", String(WIDE_CHART_MAX_HEIGHT_PX));
    // A move after release is not a drag.
    const n = sizes.length;
    fireEvent.pointerMove(g, { pointerId: 1, clientY: 10 });
    expect(sizes.length).toBe(n);
  });

  it("refits to the container on window resize at the slot's current height, and ignores a release with no drag", () => {
    const { container } = renderPanel(track(), vi.fn());
    const g = grip(container);
    fireEvent.pointerDown(g, { pointerId: 1, clientY: 0 });
    fireEvent.pointerMove(g, { pointerId: 1, clientY: 40 });
    fireEvent.pointerUp(g, { pointerId: 1, clientY: 40 });
    const n = sizes.length;
    // The resize listener reads the height through the ref, not a stale
    // closure from when the plot was built.
    act(() => {
      window.dispatchEvent(new Event("resize"));
    });
    expect(sizes.length).toBeGreaterThan(n);
    expect(sizes.slice(n).some((s) => s.height === CHART_HEIGHT_PX + 40)).toBe(true);
    // A pointer-up that no drag started writes nothing.
    vi.mocked(api.setSetting).mockClear();
    fireEvent.pointerUp(g, { pointerId: 2, clientY: 400 });
    expect(api.setSetting).not.toHaveBeenCalled();
  });

  it("binds the height to the slot, not to the chart: another order puts it on the chart now first", async () => {
    vi.mocked(api.getSetting).mockImplementation(async (key: string) =>
      key === "chart_wide_height" ? "300" : key === "chart_order" ? JSON.stringify(["hr", "elevation"]) : null,
    );
    renderPanel(track(), vi.fn());
    // Heart rate is a bar chart: moving into the wide slot changes its bar
    // window, so it is rebuilt there rather than resized — and the rebuild
    // reads the slot's height. Either way the plot ends up at 300.
    const heightOf = (prefix: string) => {
      const plots = built.filter((b) => b.opts.series[1]?.label?.startsWith(prefix));
      const last = plots[plots.length - 1];
      const resized = sizes.filter((s) => s.label?.startsWith(prefix));
      return resized.length > 0 ? resized[resized.length - 1].height : last?.opts.height;
    };
    await waitFor(() => expect(heightOf("Heart")).toBe(300));
    expect(heightOf("Elevation")).toBe(CHART_HEIGHT_PX);
    expect(sizes.some((s) => s.label?.startsWith("Elevation") && s.height !== CHART_HEIGHT_PX)).toBe(false);
  });

  it("restores the persisted height, clamped", async () => {
    vi.mocked(api.getSetting).mockImplementation(async (key: string) => (key === "chart_wide_height" ? "9999" : null));
    renderPanel(track(), vi.fn());
    await waitFor(() => expect(sizes.some((s) => s.height === WIDE_CHART_MAX_HEIGHT_PX)).toBe(true));
  });
});
