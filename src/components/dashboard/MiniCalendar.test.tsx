// @vitest-environment happy-dom
import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import { render, cleanup, fireEvent, waitFor, act } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { DaySummary, RecoveryNight } from "../../lib/types";

vi.mock("../../lib/tauri", () => ({
  api: { getCalendarData: vi.fn(), getRecoveryNights: vi.fn() },
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
beforeEach(() => {
  vi.mocked(api.getRecoveryNights).mockResolvedValue([]);
});

const night = (date: string, index: number): RecoveryNight => ({
  date,
  index,
  band: index >= 80 ? "intervals_ok" : index >= 60 ? "easy_day" : "rest",
  hr: { night_median: 53, baseline: 51, delta: 2, score: 84 },
  stress: { night_avg: 12.9, score: 93 },
  load: { tss_yesterday: 22, ctl: 48, score: 100 },
  warning: index < 60 ? "hr_above_baseline" : null,
});

describe("MiniCalendar recovery", () => {
  it("tints a night's cell border by band and opens its popup above the workouts", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    vi.setSystemTime(new Date(2026, 6, 30, 12, 0, 0));
    try {
      vi.mocked(api.getCalendarData).mockResolvedValue([day("2026-07-25", 100)]);
      vi.mocked(api.getRecoveryNights).mockResolvedValue([
        night("2026-06-30", 80), // another month: not in July's cells
        night("2026-07-25", 86),
        night("2026-07-27", 71),
        night("2026-07-29", 58),
        night("2026-08-01", 95),
      ]);
      const { container, getByText } = renderCal();
      await waitFor(() => expect(container.querySelectorAll(".cal-cell.rec")).toHaveLength(3));
      const cells = container.querySelectorAll<HTMLElement>(".cal-cell.rec");
      expect(cells[0].style.borderColor).toBe("var(--good)");
      expect(cells[1].style.borderColor).toBe("var(--warn)");
      expect(cells[2].style.borderColor).toBe("var(--danger)");
      expect(cells[2].getAttribute("data-band")).toBe("rest");

      // 25.07: workout AND night — the recovery block comes first.
      const pop = cells[0].querySelector(".cal-pop-card")!;
      expect(pop.firstElementChild?.getAttribute("data-testid")).toBe("cal-pop-rec");
      expect(pop.textContent).toContain("86");
      expect(pop.textContent).toContain("Intervals OK");
      expect(pop.textContent).toContain("Night HR 53 · base 51 (+2)");
      expect(pop.textContent).toContain("Stress 13 · Load yesterday 22 TSS, CTL 48");
      expect(pop.querySelectorAll(".cal-pop-row")).toHaveLength(1);
      expect(pop.textContent).not.toContain("well above");

      // 29.07: night only — a popup all the same, with the warning.
      const rest = cells[2].querySelector(".cal-pop-card")!;
      expect(rest.querySelectorAll(".cal-pop-row")).toHaveLength(0);
      expect(rest.textContent).toContain("Rest");
      expect(rest.textContent).toContain("Night HR well above baseline");
      // A day with neither has no popup at all.
      const plain = container.querySelectorAll(".cal-cell:not(.rec):not(.has):not(.out)");
      expect(plain.length).toBeGreaterThan(0);
      expect(plain[0].querySelector(".cal-pop")).toBeNull();

      // Flipping the month cuts the same answer differently — no refetch.
      fireEvent.keyDown(document.body, { key: "ArrowRight" });
      getByText("August 2026");
      expect(container.querySelectorAll(".cal-cell.rec")).toHaveLength(1);
      expect(container.querySelector(".cal-cell.rec")?.querySelector(".cal-num")?.textContent).toBe("1");
      expect(vi.mocked(api.getRecoveryNights)).toHaveBeenCalledTimes(1);
    } finally {
      vi.useRealTimers();
    }
  });

  it("says when a night has no stress or load data", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    vi.setSystemTime(new Date(2026, 6, 30, 12, 0, 0));
    try {
      vi.mocked(api.getCalendarData).mockResolvedValue([]);
      vi.mocked(api.getRecoveryNights).mockResolvedValue([
        { ...night("2026-07-25", 90), stress: null, load: null },
      ]);
      const { container } = renderCal();
      await waitFor(() => expect(container.querySelectorAll(".cal-cell.rec")).toHaveLength(1));
      const pop = container.querySelector(".cal-pop-rec")!;
      expect(pop.textContent).toContain("No stress data · no training history");
      expect(pop.textContent).not.toContain("CTL");
    } finally {
      vi.useRealTimers();
    }
  });
});

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

  it("asks for the nights again at midnight, so last night's index appears", async () => {
    // Real-time advance too, so React Query's refetch promise can settle.
    vi.useFakeTimers({ shouldAdvanceTime: true });
    vi.setSystemTime(new Date(2026, 8, 30, 23, 59, 30));
    vi.mocked(api.getRecoveryNights).mockClear().mockResolvedValue([]);
    renderCal();
    await waitFor(() => expect(vi.mocked(api.getRecoveryNights)).toHaveBeenCalledTimes(1));
    await act(async () => {
      vi.advanceTimersByTime(60_000);
    });
    await waitFor(() => expect(vi.mocked(api.getRecoveryNights)).toHaveBeenCalledTimes(2));
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
