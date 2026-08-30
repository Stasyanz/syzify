// @vitest-environment happy-dom
import { describe, it, expect, vi, afterEach } from "vitest";
import { render, cleanup, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { DaySummary } from "../../lib/types";

vi.mock("../../lib/tauri", () => ({
  api: { getCalendarData: vi.fn() },
}));

import { MiniCalendar } from "./MiniCalendar";
import { api } from "../../lib/tauri";

const day = (date: string, elev: number, distance = 10000): DaySummary => ({
  date,
  activity_count: 1,
  total_distance_m: distance,
  total_duration_s: 3600,
  total_elev_gain_m: elev,
  sport_types: ["ride"],
  activities: [
    {
      id: `a-${date}`,
      sport_type: "ride",
      title: null,
      distance_m: distance,
      duration_s: 3600,
    },
  ],
});

function renderCal() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter>
        <MiniCalendar />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

afterEach(cleanup);

describe("MiniCalendar month stats", () => {
  it("sums elevation gain across the month's days", async () => {
    vi.mocked(api.getCalendarData).mockResolvedValue([
      day("2026-08-29", 375),
      day("2026-08-30", 839),
    ]);

    const { getByText } = renderCal();
    await waitFor(() => getByText("Elev gain"));
    // 375 + 839, metric units, thousands-separated like the other stats.
    getByText("1,214");
  });

  it("hides the row for a month with zero climbing", async () => {
    vi.mocked(api.getCalendarData).mockResolvedValue([day("2026-08-29", 0)]);

    const { queryByText, getByText } = renderCal();
    await waitFor(() => getByText("Sessions"));
    expect(queryByText("Elev gain")).toBeNull();
  });
});
