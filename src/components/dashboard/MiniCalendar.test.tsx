// @vitest-environment happy-dom
import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import { render, cleanup, fireEvent, waitFor, act } from "@testing-library/react";
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

describe("MiniCalendar keyboard paging", () => {
  it("arrow keys flip the month like the Library calendar", async () => {
    vi.mocked(api.getCalendarData).mockResolvedValue([]);
    const { getByText } = renderCal();
    const label = (d: Date) =>
      d.toLocaleString("en-US", { month: "long", year: "numeric" });
    const now = new Date();
    await waitFor(() => getByText(label(now)));

    fireEvent.keyDown(document.body, { key: "ArrowRight" });
    getByText(label(new Date(now.getFullYear(), now.getMonth() + 1)));

    fireEvent.keyDown(document.body, { key: "ArrowLeft" });
    fireEvent.keyDown(document.body, { key: "ArrowLeft" });
    getByText(label(new Date(now.getFullYear(), now.getMonth() - 1)));
  });

  it("ignores arrows while a form field is focused", async () => {
    vi.mocked(api.getCalendarData).mockResolvedValue([]);
    const { getByText } = renderCal();
    const label = new Date().toLocaleString("en-US", { month: "long", year: "numeric" });
    await waitFor(() => getByText(label));

    const input = document.createElement("input");
    document.body.appendChild(input);
    input.focus();
    fireEvent.keyDown(input, { key: "ArrowRight" });
    getByText(label); // month unchanged
    input.remove();
  });
});

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
    // Two 3600 s days → 2.0 decimal hours, the This Week Duration shape.
    getByText("Hours");
    getByText("2.0");
  });

  it("hides the row for a month with zero climbing", async () => {
    vi.mocked(api.getCalendarData).mockResolvedValue([day("2026-08-29", 0)]);

    const { queryByText, getByText } = renderCal();
    await waitFor(() => getByText("Sessions"));
    expect(queryByText("Elev gain")).toBeNull();
  });
});

describe("MiniCalendar day rollover", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    // 30 s before the month turns.
    vi.setSystemTime(new Date(2026, 8, 30, 23, 59, 30));
    vi.mocked(api.getCalendarData).mockResolvedValue([]);
  });
  afterEach(() => {
    cleanup();
    vi.useRealTimers();
  });
  const todayCell = (c: HTMLElement) =>
    c.querySelector(".cal-cell.today .cal-num")?.textContent ?? null;

  it("moves the today ring and follows the month at midnight", () => {
    const { container, getByText } = renderCal();
    getByText("September 2026");
    expect(todayCell(container)).toBe("30");

    act(() => {
      vi.advanceTimersByTime(60_000);
    });
    getByText("October 2026");
    expect(todayCell(container)).toBe("1");
  });

  it("leaves a month the user paged to alone", () => {
    const { container, getByText } = renderCal();
    fireEvent.keyDown(document.body, { key: "ArrowLeft" });
    getByText("August 2026");

    act(() => {
      vi.advanceTimersByTime(60_000);
    });
    getByText("August 2026");
    expect(todayCell(container)).toBeNull();
  });
});
