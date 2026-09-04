// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { DashboardData } from "../lib/types";

vi.mock("../lib/tauri", () => ({
  api: { getDashboardData: vi.fn(), getCalendarData: vi.fn() },
}));
// The page's other widgets pull in chart.js and the plugin host — not what
// this test is about. The mini calendar stays real: its month state is the
// thing a midnight remount would destroy.
vi.mock("../lib/chartSetup", () => ({}));
vi.mock("../components/dashboard/SummaryCards", () => ({ SummaryCards: () => null }));
vi.mock("../components/dashboard/VolumeChart", () => ({ VolumeChart: () => null }));
vi.mock("../components/dashboard/SportDistribution", () => ({
  SportDistribution: () => null,
}));
vi.mock("../components/dashboard/PersonalRecords", () => ({ PersonalRecords: () => null }));
vi.mock("../components/plugins/PluginContributions", () => ({
  PluginContributions: () => null,
}));

import { DashboardPage } from "./Dashboard";
import { api } from "../lib/tauri";

const dashboard = {
  total_activities: 0,
  total_distance_m: 0,
  total_duration_s: 0,
  total_elev_gain_m: 0,
  avg_hr: null,
  week: {},
  week_volume: [],
  volume_buckets: [],
  sport_distribution: [],
  week_sport_distribution: [],
  records_by_sport: [],
} as unknown as DashboardData;

function renderPage() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter>
        <DashboardPage />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("DashboardPage at midnight", () => {
  beforeEach(() => {
    // shouldAdvanceTime lets waitFor's own polling run under fake timers.
    vi.useFakeTimers({ shouldAdvanceTime: true });
    vi.setSystemTime(new Date(2026, 8, 30, 23, 59, 30));
    vi.mocked(api.getDashboardData).mockResolvedValue(dashboard);
    vi.mocked(api.getCalendarData).mockResolvedValue([]);
  });
  afterEach(() => {
    cleanup();
    vi.useRealTimers();
  });

  it("refetches for the new day without a loading flash or losing the paged month", async () => {
    const { getByText, queryByText } = renderPage();
    await waitFor(() => getByText("September 2026"));
    expect(api.getDashboardData).toHaveBeenCalledTimes(1);

    // The user paged the calendar back before midnight.
    fireEvent.keyDown(document.body, { key: "ArrowLeft" });
    getByText("August 2026");

    act(() => {
      vi.advanceTimersByTime(60_000);
    });
    // Data stayed on screen while refetching, the widgets were not remounted.
    expect(queryByText("Loading dashboard...")).toBeNull();
    getByText("August 2026");
    await waitFor(() => expect(api.getDashboardData).toHaveBeenCalledTimes(2));
  });
});
