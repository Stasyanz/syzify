// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

vi.mock("../../lib/tauri", () => ({
  api: { getTags: vi.fn(), getActivityYearRange: vi.fn() },
}));

import { FilterDrawer } from "./FilterDrawer";
import { api } from "../../lib/tauri";

function renderDrawer() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <FilterDrawer />
    </QueryClientProvider>,
  );
}

describe("FilterDrawer date picker day rollover", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date(2026, 8, 30, 23, 59, 30));
    vi.mocked(api.getTags).mockResolvedValue([]);
    vi.mocked(api.getActivityYearRange).mockResolvedValue([2026, 2026]);
  });
  afterEach(() => {
    cleanup();
    vi.useRealTimers();
  });
  const todayCell = (c: HTMLElement) => c.querySelector(".dp-day.today")?.textContent ?? null;

  it("moves the today mark at midnight without jumping the opened month", () => {
    const { container, getByText, getByLabelText } = renderDrawer();
    fireEvent.click(getByText("From").closest("button")!);
    expect(todayCell(container)).toBe("30");

    act(() => {
      vi.advanceTimersByTime(60_000);
    });
    // The picker stays on the month it was opened on — the mark simply
    // leaves it — and shows up on the 1st once paged forward.
    expect(todayCell(container)).toBeNull();
    fireEvent.click(getByLabelText("Next month"));
    expect(todayCell(container)).toBe("1");
  });
});
