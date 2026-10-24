// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

vi.mock("../../lib/tauri", () => ({
  api: { getCalendarData: vi.fn() },
}));

import { CalendarView } from "./CalendarView";
import { api } from "../../lib/tauri";
import { monthTitle } from "../../lib/calendar";
import { useLanguageStore } from "../../lib/i18n";

function renderView() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter>
        <CalendarView />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

// The view's month title is in the interface language — build the
// expectation the same way instead of hardcoding English.
const label = (y: number, m: number) => monthTitle(y, m);

describe("CalendarView in the interface language (#232)", () => {
  beforeEach(() => vi.mocked(api.getCalendarData).mockResolvedValue([]));
  afterEach(() => {
    cleanup();
    act(() => useLanguageStore.getState().setPref("system"));
  });

  it("titles the month and names the weekdays in Italian", () => {
    act(() => useLanguageStore.getState().setPref("it"));
    const { container, getByText } = renderView();
    const now = new Date();
    getByText(monthTitle(now.getFullYear(), now.getMonth() + 1));
    expect(Array.from(container.querySelectorAll(".cal-dow"), (el) => el.textContent)).toEqual([
      "Lun", "Mar", "Mer", "Gio", "Ven", "Sab", "Dom",
    ]);
  });
});

describe("CalendarView day rollover", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date(2026, 8, 30, 23, 59, 30));
    vi.mocked(api.getCalendarData).mockResolvedValue([]);
  });
  afterEach(() => {
    cleanup();
    vi.useRealTimers();
  });
  // The today cell carries the accent ring and its number in accent text.
  const todayNumber = (c: HTMLElement) =>
    c.querySelector(".ring-accent .text-accent-2")?.textContent ?? null;

  it("moves the today ring and follows the month at midnight", () => {
    const { container, getByText } = renderView();
    getByText(label(2026, 9));
    expect(todayNumber(container)).toBe("30");

    act(() => {
      vi.advanceTimersByTime(60_000);
    });
    getByText(label(2026, 10));
    expect(todayNumber(container)).toBe("1");
  });

  it("leaves a month the user paged to alone", () => {
    const { container, getByText } = renderView();
    fireEvent.keyDown(document.body, { key: "ArrowLeft" });
    getByText(label(2026, 8));

    act(() => {
      vi.advanceTimersByTime(60_000);
    });
    getByText(label(2026, 8));
    expect(todayNumber(container)).toBeNull();
  });
});
