// @vitest-environment happy-dom
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, cleanup, waitFor, act } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { PowerCurveData } from "../../lib/types";

vi.mock("../../lib/tauri", () => ({
  api: { getPowerCurve: vi.fn() },
}));

// happy-dom has no canvas — swap uPlot for a shell that records its inputs
// so the test can assert what the chart would draw.
const constructed: Array<{ opts: unknown; data: unknown }> = [];
vi.mock("uplot", () => ({
  default: class {
    over = document.createElement("div");
    constructor(opts: unknown, data: unknown, target: HTMLElement) {
      constructed.push({ opts, data });
      target.appendChild(this.over);
    }
    setSize() {}
    destroy() {}
  },
}));
vi.mock("uplot/dist/uPlot.min.css", () => ({}));

import { PowerCurvePanel } from "./PowerCurvePanel";
import { api } from "../../lib/tauri";
import { useLanguageStore } from "../../lib/i18n";

function renderPanel() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter>
        <PowerCurvePanel activityId="act-1" />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  constructed.length = 0;
  globalThis.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  } as unknown as typeof ResizeObserver;
});

afterEach(cleanup);

describe("PowerCurvePanel", () => {
  it("renders nothing for an activity without power", async () => {
    vi.mocked(api.getPowerCurve).mockResolvedValue({
      points: [],
      envelope: [],
    } as PowerCurveData);

    const { container } = renderPanel();
    await waitFor(() => expect(api.getPowerCurve).toHaveBeenCalledWith("act-1"));
    expect(container.querySelector(".dash-card")).toBeNull();
    expect(constructed).toHaveLength(0);
  });

  it("draws the merged envelope + activity series when a curve exists", async () => {
    vi.mocked(api.getPowerCurve).mockResolvedValue({
      points: [
        { window_s: 5, watts: 615 },
        { window_s: 60, watts: 339 },
      ],
      envelope: [
        {
          window_s: 5,
          watts: 646,
          activity_id: "older",
          title: "Morning Ride",
          start_time: "2026-08-29T08:02:36+03:00",
        },
        {
          window_s: 60,
          watts: 346,
          activity_id: "older",
          title: "Morning Ride",
          start_time: "2026-08-29T08:02:36+03:00",
        },
        {
          window_s: 300,
          watts: 280,
          activity_id: "act-1",
          title: null,
          start_time: "2026-08-30T09:23:25+03:00",
        },
      ],
    } as PowerCurveData);

    const { getByText } = renderPanel();
    await waitFor(() => getByText("Power Curve"));
    expect(constructed).toHaveLength(1);

    // [x, envelope, activity] on the union grid; the activity has no 300 s
    // window, so its series carries a null there instead of losing the tick.
    const data = constructed[0].data as [number[], (number | null)[], (number | null)[]];
    expect(data[0]).toEqual([5, 60, 300]);
    expect(data[1]).toEqual([646, 346, 280]);
    expect(data[2]).toEqual([615, 339, null]);

    // The chart config IS the feature — pin the parts a refactor could
    // silently lose: log x scale, drag-zoom disabled (the documented
    // ChartPanel trap: default drag zooms AND the release click would
    // navigate away), and the axis filter that keeps our custom splits
    // (uPlot's log-scale default blanks them to literal "null" labels).
    const opts = constructed[0].opts as {
      scales: { x: { distr: number } };
      cursor: { drag: { x: boolean; y: boolean } };
      axes: Array<{ filter?: (u: unknown, s: (number | null)[]) => (number | null)[] }>;
    };
    expect(opts.scales.x.distr).toBe(3);
    expect(opts.cursor.drag).toEqual({ x: false, y: false });
    expect(opts.axes[0].filter?.(null, [5, null, 60])).toEqual([5, null, 60]);
  });

  it("names the card and the series in Russian (#234)", async () => {
    useLanguageStore.getState().setPref("ru");
    try {
      vi.mocked(api.getPowerCurve).mockResolvedValue({
        points: [{ window_s: 5, watts: 615 }],
        envelope: [{ window_s: 5, watts: 646, activity_id: "older", title: null, start_time: "2026-08-29T08:02:36+03:00" }],
      } as PowerCurveData);
      const { getByText } = renderPanel();
      await waitFor(() => getByText("Кривая мощности"));
      getByText("лучшая средняя мощность · серым — за всё время");
      const opts = constructed[0].opts as { series: { label?: string }[] };
      expect(opts.series.map((s) => s.label)).toEqual(["Окно", "За всё время (Вт)", "Эта тренировка (Вт)"]);
    } finally {
      useLanguageStore.getState().setPref("system");
    }
  });

  it("sizes the watts axis by its labels, so 1,000 is not cut (#219)", async () => {
    vi.mocked(api.getPowerCurve).mockResolvedValue({
      points: [
        { window_s: 1, watts: 1050 },
        { window_s: 60, watts: 420 },
      ],
      envelope: [],
    } as PowerCurveData);

    const { getByText } = renderPanel();
    await waitFor(() => getByText("Power Curve"));

    // "1,000" needs 43 px, the axis had a fixed 40, and the plot has no left
    // padding to absorb the rest. The axis must ask for what its labels need.
    const opts = constructed[0].opts as {
      axes: Array<{ size: unknown; font: string }>;
    };
    const size = opts.axes[1].size as (u: unknown, values: string[] | null) => number;
    expect(typeof size).toBe("function");
    const measuredIn: string[] = [];
    const ctx = {
      font: "22px sans-serif",
      measureText(s: string) {
        measuredIn.push(this.font);
        return { width: s.length * 5.5 };
      },
    };
    expect(size({ ctx }, ["0", "200", "800"])).toBeLessThan(40);
    expect(size({ ctx }, ["800", "1,000"])).toBe(43);
    // Measured in the font the axis is drawn with.
    expect(new Set(measuredIn)).toEqual(new Set([opts.axes[1].font]));
  });

  it("keeps the hover tip inside the card at both ends of the curve (#206)", async () => {
    vi.mocked(api.getPowerCurve).mockResolvedValue({
      points: [
        { window_s: 1, watts: 740 },
        { window_s: 3600, watts: 209 },
      ],
      envelope: [],
    } as PowerCurveData);
    // happy-dom lays nothing out — give the plot a width to be halved.
    const width = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "clientWidth");
    Object.defineProperty(HTMLElement.prototype, "clientWidth", {
      configurable: true,
      get: () => 800,
    });
    try {
      const { getByText, getByTestId } = renderPanel();
      await waitFor(() => getByText("Power Curve"));
      const opts = constructed[0].opts as {
        hooks: { setCursor: ((u: unknown) => void)[] };
      };
      const hover = (idx: number, left: number) => {
        act(() =>
          opts.hooks.setCursor[0]({ cursor: { idx, left }, over: document.createElement("div") }),
        );
        const tip = getByTestId("chart-tip");
        // Tailwind v4's translate utilities set `translate`, which stacks
        // with the inline transform — a stray one re-centres the tip.
        expect(tip.className).not.toMatch(/translate/);
        return tip.style.transform;
      };
      // Clamped into the 800 px card at both ends — the 1 h end included.
      expect(hover(1, 790)).toBe("translateX(clamp(-790px, -50%, calc(10px - 100%)))");
      expect(hover(0, 10)).toBe("translateX(clamp(-10px, -50%, calc(790px - 100%)))");
    } finally {
      if (width) Object.defineProperty(HTMLElement.prototype, "clientWidth", width);
      else delete (HTMLElement.prototype as { clientWidth?: number }).clientWidth;
    }
  });
});
