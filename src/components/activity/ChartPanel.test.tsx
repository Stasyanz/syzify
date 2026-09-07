// @vitest-environment happy-dom
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, cleanup } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { TrackPointColumns } from "../../lib/types";

vi.mock("../../lib/tauri", () => ({
  api: { getSetting: vi.fn(async () => null), setSetting: vi.fn(async () => undefined) },
}));

// happy-dom has no canvas — a shell in uPlot's place is enough for the
// grid layout this test is about.
vi.mock("uplot", () => ({
  default: class {
    static pxRatio = 1;
    static paths = { bars: () => () => null };
    over = document.createElement("div");
    cursor = { idx: null };
    constructor(_opts: unknown, _data: unknown, target: HTMLElement) {
      target.appendChild(this.over);
    }
    setSize() {}
    setCursor() {}
    valToPos() {
      return 0;
    }
    destroy() {}
  },
}));
vi.mock("uplot/dist/uPlot.min.css", () => ({}));

import { ChartPanel } from "./ChartPanel";

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

function renderPanel(tp: TrackPointColumns, onFillerPlaced: (p: boolean) => void) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const filler = <div data-testid="filler">zones</div>;
  const ui = (cols: TrackPointColumns) => (
    <QueryClientProvider client={qc}>
      <ChartPanel trackpoints={cols} sport="ride" filler={filler} onFillerPlaced={onFillerPlaced} />
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
