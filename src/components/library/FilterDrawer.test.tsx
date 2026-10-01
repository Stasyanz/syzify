// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

vi.mock("../../lib/tauri", () => ({
  api: {
    getActivityYearRange: vi.fn(),
    getDetectedDevices: vi.fn(),
    listGear: vi.fn(),
    countGearTargets: vi.fn(),
    assignGearToFiltered: vi.fn(),
  },
}));
vi.mock("../../stores/confirmStore", () => ({ confirmDialog: vi.fn() }));
const addToast = vi.fn();
vi.mock("../../stores/toastStore", () => ({
  useToastStore: (sel: (s: { addToast: typeof addToast }) => unknown) => sel({ addToast }),
}));

import { FilterDrawer } from "./FilterDrawer";
import { api } from "../../lib/tauri";
import { confirmDialog } from "../../stores/confirmStore";
import { useActivityStore } from "../../stores/activityStore";
import type { GearItem } from "../../lib/types";

const gearItem = (id: string, name: string, kind: GearItem["kind"], activities: number, retired = false): GearItem => ({
  id,
  kind,
  name,
  brand: null,
  model: null,
  purchased_at: null,
  initial_distance_m: 0,
  distance_limit_m: null,
  retired_at: retired ? "2026-01-01T00:00:00" : null,
  notes: null,
  created_at: "2026-01-01T00:00:00",
  stats: { activities, distance_m: 0, duration_s: 0, elev_gain_m: 0, last_used: null },
  default_for: [],
});

let qcRef: QueryClient | null = null;
function renderDrawer() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  qcRef = qc;
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

describe("FilterDrawer gear", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(confirmDialog).mockReset();
    vi.mocked(api.getActivityYearRange).mockResolvedValue([2026, 2026]);
    vi.mocked(api.getDetectedDevices).mockResolvedValue([]);
    vi.mocked(api.listGear).mockResolvedValue([
      gearItem("g-road", "Road", "bike", 37),
      gearItem("g-old", "Old road", "bike", 5, true),
      gearItem("g-peg", "Pegasus", "shoes", 12),
    ]);
    useActivityStore.getState().resetFilters();
  });
  afterEach(cleanup);

  it("offers every item with its count, retired ones marked, and No gear; a pick sends the ids", async () => {
    const { findByLabelText, getAllByRole } = renderDrawer();
    const trigger = await findByLabelText("Gear");
    expect(trigger.textContent).toContain("All gear");
    fireEvent.click(trigger);
    expect(getAllByRole("option").map((o) => o.textContent)).toEqual([
      "All gear",
      "Road (37)",
      "Old road (retired) (5)",
      "Pegasus (12)",
      "No gear",
    ]);
    fireEvent.click(getAllByRole("option")[1]);
    expect(useActivityStore.getState().filters.gear_ids).toEqual(["g-road"]);
    fireEvent.click(getAllByRole("option")[4]);
    expect(useActivityStore.getState().filters.gear_ids).toEqual(["g-road", ""]);
    fireEvent.click(getAllByRole("option")[1]);
    fireEvent.click(getAllByRole("option")[4]);
    expect(useActivityStore.getState().filters.gear_ids).toBeUndefined();
  });

  it("is hidden while the garage is empty", async () => {
    vi.mocked(api.listGear).mockResolvedValue([]);
    const { findByLabelText, queryByLabelText, queryByTestId } = renderDrawer();
    await findByLabelText("Sport");
    expect(queryByLabelText("Gear")).toBeNull();
    expect(queryByTestId("bulk-gear")).toBeNull();
  });

  it("offers the bulk action only in the list, once a filter narrows the library, over the items in use", async () => {
    const { findByLabelText, queryByTestId, getByLabelText, getAllByRole } = renderDrawer();
    await findByLabelText("Gear");
    expect(queryByTestId("bulk-gear")).toBeNull();
    act(() => useActivityStore.getState().setFilters({ sport_types: ["ride"] }));
    await waitFor(() => expect(queryByTestId("bulk-gear")).toBeTruthy());
    // The calendar adds its month and the map drops what has no track:
    // there the filters do not say what is on screen, so no bulk action.
    act(() => useActivityStore.getState().setViewMode("calendar"));
    expect(queryByTestId("bulk-gear")).toBeNull();
    act(() => useActivityStore.getState().setViewMode("map"));
    expect(queryByTestId("bulk-gear")).toBeNull();
    act(() => useActivityStore.getState().setViewMode("list"));
    await waitFor(() => expect(queryByTestId("bulk-gear")).toBeTruthy());
    fireEvent.click(getByLabelText("Put the filtered activities on"));
    expect(getAllByRole("option").map((o) => o.textContent)).toEqual(["Put them on…", "Road", "Pegasus"]);
    fireEvent.click(getAllByRole("option")[0]);
    expect((getByLabelText("Put the filtered activities on").nextElementSibling as HTMLButtonElement).disabled).toBe(true);
  });

  it("confirms with the counts, assigns what the filters show and reports it; cancel does nothing", async () => {
    vi.mocked(api.countGearTargets).mockResolvedValue({ eligible: 40, moved_from: [{ name: "Gravel", count: 2 }, { name: "Old road", count: 1 }] });
    vi.mocked(confirmDialog).mockResolvedValueOnce(false).mockResolvedValueOnce(true);
    vi.mocked(api.assignGearToFiltered).mockResolvedValue(40);
    const { findByLabelText, getByLabelText, getByRole, getAllByRole } = renderDrawer();
    await findByLabelText("Gear");
    act(() => useActivityStore.getState().setFilters({ sport_types: ["ride"], date_from: "2024-01-01" }));
    await waitFor(() => getByLabelText("Put the filtered activities on"));
    fireEvent.click(getByLabelText("Put the filtered activities on"));
    fireEvent.click(getAllByRole("option").find((o) => o.textContent === "Road")!);
    const assign = getByRole("button", { name: "Assign" });
    expect((assign as HTMLButtonElement).disabled).toBe(false);

    fireEvent.click(assign);
    await waitFor(() => expect(confirmDialog).toHaveBeenCalledTimes(1));
    expect(api.countGearTargets).toHaveBeenCalledWith(expect.objectContaining({ sport_types: ["ride"], date_from: "2024-01-01" }), "g-road");
    expect(vi.mocked(confirmDialog).mock.calls[0][0]).toMatchObject({
      title: "Put 40 activities on Road?",
      message: expect.stringContaining("2 from Gravel, 1 from Old road will be moved"),
    });
    expect(vi.mocked(confirmDialog).mock.calls[0][0].message).toContain("all dates unless filtered");
    await new Promise((r) => setTimeout(r, 20));
    expect(api.assignGearToFiltered).not.toHaveBeenCalled();

    fireEvent.click(assign);
    await waitFor(() => expect(api.assignGearToFiltered).toHaveBeenCalledWith(expect.objectContaining({ sport_types: ["ride"] }), "g-road"));
    await waitFor(() => expect(addToast).toHaveBeenCalledWith("success", "Road assigned to 40 activities"));
  });

  it("says so when the picked item is gone by the time Assign is pressed", async () => {
    const { findByLabelText, getByLabelText, getByRole, getAllByRole } = renderDrawer();
    await findByLabelText("Gear");
    act(() => useActivityStore.getState().setFilters({ sport_types: ["ride"] }));
    await waitFor(() => getByLabelText("Put the filtered activities on"));
    fireEvent.click(getByLabelText("Put the filtered activities on"));
    fireEvent.click(getAllByRole("option").find((o) => o.textContent === "Road")!);
    // The registry refreshes without Road (retired elsewhere).
    vi.mocked(api.listGear).mockResolvedValue([gearItem("g-peg", "Pegasus", "shoes", 12)]);
    act(() => useActivityStore.getState().setFilters({ sport_types: ["ride", "run"] }));
    await waitFor(() => expect(api.listGear).toHaveBeenCalledTimes(1));
    // Force the registry refetch the way the app would (an import, a Garage edit).
    await act(async () => {
      await qcRef!.invalidateQueries({ queryKey: ["gear"] });
    });
    await waitFor(() => expect(api.listGear).toHaveBeenCalledTimes(2));
    fireEvent.click(getByRole("button", { name: "Assign" }));
    await waitFor(() => expect(addToast).toHaveBeenCalledWith("error", "Could not assign gear: That gear is no longer available — pick another"));
    expect(api.countGearTargets).not.toHaveBeenCalled();
    // The vanished pick is cleared: Assign waits for a new one.
    expect(getByLabelText("Put the filtered activities on").textContent).toContain("Put them on…");
    expect((getByRole("button", { name: "Assign" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("says when the filters show nothing to assign, and when the backend refuses", async () => {
    vi.mocked(api.countGearTargets).mockResolvedValueOnce({ eligible: 0, moved_from: [] });
    const { findByLabelText, getByLabelText, getByRole, getAllByRole } = renderDrawer();
    await findByLabelText("Gear");
    act(() => useActivityStore.getState().setFilters({ sport_types: ["swim"] }));
    await waitFor(() => getByLabelText("Put the filtered activities on"));
    fireEvent.click(getByLabelText("Put the filtered activities on"));
    fireEvent.click(getAllByRole("option").find((o) => o.textContent === "Pegasus")!);
    fireEvent.click(getByRole("button", { name: "Assign" }));
    await waitFor(() => expect(addToast).toHaveBeenCalledWith("info", expect.stringContaining("on Pegasus already")));
    expect(confirmDialog).not.toHaveBeenCalled();

    vi.mocked(api.countGearTargets).mockResolvedValueOnce({ eligible: 2, moved_from: [] });
    vi.mocked(confirmDialog).mockResolvedValueOnce(true);
    vi.mocked(api.assignGearToFiltered).mockRejectedValueOnce("Bring the gear back before assigning activities to it");
    fireEvent.click(getByRole("button", { name: "Assign" }));
    await waitFor(() =>
      expect(addToast).toHaveBeenCalledWith("error", "Could not assign gear: Bring the gear back before assigning activities to it"),
    );
    expect(vi.mocked(confirmDialog).mock.calls[0][0].message).not.toContain("moved");
  });
});
