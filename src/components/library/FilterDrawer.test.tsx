// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

vi.mock("../../lib/tauri", () => ({
  api: { getTags: vi.fn(), getActivityYearRange: vi.fn(), getDetectedDevices: vi.fn() },
}));

import { FilterDrawer } from "./FilterDrawer";
import { api } from "../../lib/tauri";
import { useActivityStore } from "../../stores/activityStore";

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
    vi.mocked(api.getDetectedDevices).mockResolvedValue([]);
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

describe("FilterDrawer device filter", () => {
  const stat = (device_name: string, activity_count: number) => ({ device_name, activity_count, last_activity: "2026-09-16T08:00:00+00:00" });
  beforeEach(() => {
    vi.mocked(api.getTags).mockResolvedValue([]);
    vi.mocked(api.getActivityYearRange).mockResolvedValue([2026, 2026]);
    useActivityStore.getState().resetFilters();
  });
  afterEach(cleanup);

  it("offers one option per model with its count, and sends the raw strings of a pick", async () => {
    vi.mocked(api.getDetectedDevices).mockResolvedValue([
      stat("Garmin fenix6x", 900),
      stat("Garmin fenix6x_asia", 300),
      stat("Garmin edge_840", 40),
      stat("", 12),
    ]);
    const { findByLabelText, getByRole, getAllByRole } = renderDrawer();
    const trigger = await findByLabelText("Device");
    expect(trigger.textContent).toContain("All devices");
    fireEvent.click(trigger);
    const labels = getAllByRole("option").map((o) => o.textContent);
    expect(labels).toEqual(["All devices", "fenix 6X Pro (1200)", "Edge 840 (40)", "No device (12)"]);
    expect(getByRole("listbox").querySelectorAll("svg[data-form]").length).toBe(3);

    fireEvent.click(getAllByRole("option")[1]);
    expect(useActivityStore.getState().filters.devices).toEqual(["Garmin fenix6x", "Garmin fenix6x_asia"]);
    fireEvent.click(getAllByRole("option")[3]);
    expect(useActivityStore.getState().filters.devices).toEqual(["Garmin fenix6x", "Garmin fenix6x_asia", ""]);
    expect(trigger.textContent).toContain("fenix 6X Pro (1200), No device (12)");

    // Un-ticking the last option clears the facet entirely.
    fireEvent.click(getAllByRole("option")[1]);
    fireEvent.click(getAllByRole("option")[3]);
    expect(useActivityStore.getState().filters.devices).toBeUndefined();
  });

  it("is hidden when the library has a single device group — nothing to choose", async () => {
    vi.mocked(api.getDetectedDevices).mockResolvedValue([stat("Garmin fenix6x", 5)]);
    const { findByLabelText, queryByLabelText } = renderDrawer();
    await findByLabelText("Sport");
    expect(queryByLabelText("Device")).toBeNull();
  });
});
