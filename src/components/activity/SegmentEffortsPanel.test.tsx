// @vitest-environment happy-dom
import { describe, it, expect, afterEach, beforeEach, vi } from "vitest";
import { render, cleanup, fireEvent, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { api } from "../../lib/tauri";
import type { SegmentEffortRow } from "../../lib/types";
import {
  SegmentEffortsPanel,
  effortSpeedMps,
  isPersonalRecord,
} from "./SegmentEffortsPanel";
import { useActivityStore } from "../../stores/activityStore";
import { useLanguageStore } from "../../lib/i18n";

vi.mock("../../lib/tauri", () => ({
  api: { getActivitySegmentEfforts: vi.fn() },
}));

const mocked = vi.mocked(api);

function effort(over: Partial<SegmentEffortRow> = {}): SegmentEffortRow {
  return {
    id: 1,
    segment_id: "seg-1",
    segment_name: "Siedra from Damlataş",
    start_idx: 746,
    end_idx: 1269,
    distance_m: 3145.1,
    elapsed_s: 1511,
    avg_power_w: null,
    avg_grade_pct: 6.94,
    rank: 2,
    effort_count: 2,
    ...over,
  };
}

afterEach(cleanup);
beforeEach(() => {
  vi.clearAllMocks();
  useActivityStore.setState({ selectedRange: null });
});

async function renderPanel(rows: SegmentEffortRow[]) {
  mocked.getActivitySegmentEfforts.mockResolvedValue(rows);
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const view = render(
    <QueryClientProvider client={qc}>
      <SegmentEffortsPanel activityId="act-1" sport="ride" />
    </QueryClientProvider>,
  );
  if (rows.length > 0) await screen.findByText("Segments");
  return view;
}

describe("effortSpeedMps / isPersonalRecord", () => {
  it("derives speed only from timed efforts", () => {
    expect(effortSpeedMps({ distance_m: 3000, elapsed_s: 1500 })).toBe(2);
    expect(effortSpeedMps({ distance_m: 3000, elapsed_s: null })).toBeNull();
    expect(effortSpeedMps({ distance_m: 3000, elapsed_s: 0 })).toBeNull();
  });

  it("PR needs rank 1 AND at least one rival", () => {
    expect(isPersonalRecord({ rank: 1, effort_count: 2 })).toBe(true);
    // A lone effort is trivially first — no badge.
    expect(isPersonalRecord({ rank: 1, effort_count: 1 })).toBe(false);
    expect(isPersonalRecord({ rank: 2, effort_count: 5 })).toBe(false);
    expect(isPersonalRecord({ rank: null, effort_count: 3 })).toBe(false);
  });
});

describe("SegmentEffortsPanel", () => {
  it("renders one row per effort with time, pace and standing", async () => {
    await renderPanel([effort()]);
    expect(screen.getByText("Siedra from Damlataş")).toBeTruthy();
    expect(screen.getByText("25:11")).toBeTruthy();
    // 3145.1 m / 1511 s = 2.08 m/s = 7.5 km/h.
    expect(screen.getByText("7.5 km/h")).toBeTruthy();
    expect(screen.getByText("+6.9%")).toBeTruthy();
    expect(screen.getByText("2 of 2")).toBeTruthy();
    expect(screen.queryByText("PR")).toBeNull();
  });

  it("marks the best effort with a PR badge", async () => {
    await renderPanel([effort({ rank: 1 })]);
    expect(screen.getByText("PR").getAttribute("data-tip")).toBe("Personal record");
    expect(screen.getByRole("img", { name: "Personal record" })).toBe(screen.getByText("PR"));
  });

  it("renders nothing when the activity has no efforts", async () => {
    const { container } = await renderPanel([]);
    // Give the resolved-empty query a beat to settle.
    await new Promise((r) => setTimeout(r, 0));
    expect(container.innerHTML).toBe("");
  });

  it("clicking a row toggles the shared highlight range", async () => {
    await renderPanel([effort()]);
    const row = screen.getByText("Siedra from Damlataş").closest("tr")!;
    fireEvent.click(row);
    expect(useActivityStore.getState().selectedRange).toEqual([746, 1269]);
    expect(row.getAttribute("aria-selected")).toBe("true");
    // Second click deselects.
    fireEvent.click(row);
    expect(useActivityStore.getState().selectedRange).toBeNull();
    expect(row.getAttribute("aria-selected")).toBe("false");
  });

  it("a foreign chart selection does not activate any row", async () => {
    await renderPanel([effort()]);
    // A hand-dragged chart range that only overlaps the effort.
    useActivityStore.setState({ selectedRange: [800, 1200] });
    const row = screen.getByText("Siedra from Damlataş").closest("tr")!;
    expect(row.getAttribute("aria-selected")).toBe("false");
  });

  it("adds a Power column only when some pass carries watts", async () => {
    await renderPanel([
      effort({ avg_power_w: 233.6 }),
      effort({ id: 2, start_idx: 30, end_idx: 40 }),
    ]);
    expect(screen.getByText("Power")).toBeTruthy();
    screen.getByText("234 W");
    screen.getByText("--");
  });

  it("keeps the meterless layout without any power data", async () => {
    await renderPanel([effort(), effort({ id: 2, start_idx: 30, end_idx: 40 })]);
    expect(screen.queryByText("Power")).toBeNull();
  });

  it("shows untimed efforts without rank or pace", async () => {
    await renderPanel([effort({ elapsed_s: null, rank: null })]);
    expect(screen.getByText("3.15 km")).toBeTruthy();
    // Time, pace and rank all fall back to placeholders.
    expect(screen.getAllByText("--").length).toBeGreaterThanOrEqual(3);
  });

  it("speaks Russian: the headers, the standing and the PR badge (#234)", async () => {
    useLanguageStore.getState().setPref("ru");
    try {
      mocked.getActivitySegmentEfforts.mockResolvedValue([
        effort({ rank: 1, avg_power_w: 240 }),
        effort({ id: 2, start_idx: 30, end_idx: 40, rank: 2, effort_count: 12 }),
      ]);
      const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
      render(
        <QueryClientProvider client={qc}>
          <SegmentEffortsPanel activityId="act-1" sport="ride" />
        </QueryClientProvider>,
      );
      await screen.findByText("Сегменты");
      expect(Array.from(document.querySelectorAll("th"), (th) => th.textContent)).toEqual([
        "Сегмент", "Время", "Мощность", "Скорость", "Дистанция", "Уклон", "Место",
      ]);
      const pr = screen.getByText("ЛР");
      expect(pr.getAttribute("data-tip")).toBe("Личный рекорд");
      expect(screen.getByRole("img", { name: "Личный рекорд" })).toBe(pr);
      screen.getByText("1 из 2");
      screen.getByText("2 из 12");
      expect(document.querySelector("tbody tr")!.getAttribute("title")).toBe("Показать на карте и графике");
    } finally {
      useLanguageStore.getState().setPref("system");
    }
  });
});
