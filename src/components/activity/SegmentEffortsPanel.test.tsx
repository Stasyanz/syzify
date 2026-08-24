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
    expect(screen.getByText("PR")).toBeTruthy();
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
    // Second click deselects.
    fireEvent.click(row);
    expect(useActivityStore.getState().selectedRange).toBeNull();
  });

  it("shows untimed efforts without rank or pace", async () => {
    await renderPanel([effort({ elapsed_s: null, rank: null })]);
    expect(screen.getByText("3.15 km")).toBeTruthy();
    // Time, pace and rank all fall back to placeholders.
    expect(screen.getAllByText("--").length).toBeGreaterThanOrEqual(3);
  });
});
