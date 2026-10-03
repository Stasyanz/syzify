// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { GearItem } from "../../lib/types";
import { useUnitsStore, M_PER_MILE } from "../../lib/units";

vi.mock("../../lib/tauri", () => ({
  api: {
    listGear: vi.fn(),
    createGear: vi.fn(),
    updateGear: vi.fn(),
    setGearRetired: vi.fn(),
    deleteGear: vi.fn(),
    assignGearHistory: vi.fn(),
    gearRuleCandidates: vi.fn(),
    applyGearRules: vi.fn(),
  },
}));
vi.mock("../../stores/confirmStore", () => ({ confirmDialog: vi.fn() }));
vi.mock("../../stores/toastStore", () => {
  const state = { addToast: vi.fn(() => "t1"), updateToast: vi.fn(), removeToast: vi.fn() };
  const useToastStore = Object.assign((sel: (s: typeof state) => unknown) => sel(state), {
    getState: () => state,
  });
  return { useToastStore };
});

import {
  Garage,
  distanceInputValue,
  kindSports,
  odometerM,
  readDistanceField,
  wearFraction,
  wearTip,
} from "./Garage";
import { api } from "../../lib/tauri";
import { confirmDialog } from "../../stores/confirmStore";
import { useToastStore } from "../../stores/toastStore";

const road: GearItem = {
  id: "g-road",
  kind: "bike",
  name: "Road",
  brand: "Canyon",
  model: "Ultimate",
  purchased_at: "2025-03-01",
  initial_distance_m: 1_000_000,
  distance_limit_m: null,
  retired_at: null,
  notes: null,
  created_at: "2026-10-01T10:00:00",
  stats: { activities: 2, distance_m: 50_000, duration_s: 6_000, elev_gain_m: 200, last_used: "2026-09-29T08:44:29+03:00" },
  default_for: ["ride"],
  rules: [],
};
const pegasus: GearItem = {
  id: "g-peg",
  kind: "shoes",
  name: "Pegasus",
  brand: null,
  model: null,
  purchased_at: null,
  initial_distance_m: 0,
  distance_limit_m: 800_000,
  retired_at: null,
  notes: null,
  created_at: "2026-10-01T10:00:00",
  stats: { activities: 12, distance_m: 700_000, duration_s: 0, elev_gain_m: 0, last_used: null },
  default_for: [],
  rules: [],
};
const oldShoes: GearItem = {
  ...pegasus,
  id: "g-old",
  name: "Old shoes",
  retired_at: "2026-09-01T00:00:00",
  distance_limit_m: null,
  initial_distance_m: 300_000,
  stats: { activities: 0, distance_m: 0, duration_s: 0, elev_gain_m: 0, last_used: null },
};

let qc: QueryClient;
function renderIt() {
  qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <Garage />
    </QueryClientProvider>,
  );
}

describe("Garage helpers", () => {
  afterEach(() => useUnitsStore.setState({ mode: "metric" }));

  it("offers bikes to rides, shoes to feet and anything else to every sport", () => {
    expect(kindSports("bike")).toEqual(["ride", "mountain_bike", "indoor_ride", "virtual_ride"]);
    expect(kindSports("shoes")).toContain("hike");
    expect(kindSports("shoes")).not.toContain("ride");
    expect(kindSports("other").length).toBeGreaterThan(10);
  });

  it("continues the odometer from the mileage before Syzify and reads the wear against the limit", () => {
    expect(odometerM(road)).toBe(1_050_000);
    expect(wearFraction(road)).toBeNull();
    expect(wearFraction(pegasus)).toBeCloseTo(0.875);
    expect(wearFraction({ ...pegasus, stats: { ...pegasus.stats, distance_m: 900_000 } })).toBe(1);
    expect(wearFraction({ ...pegasus, distance_limit_m: 0 })).toBeNull();
    expect(wearTip(pegasus, 0.875)).toBe("700.00 km of 800.00 km · 87 %");
    expect(wearTip({ ...pegasus, stats: { ...pegasus.stats, distance_m: 796_800 } }, 0.996)).toBe("796.80 km of 800.00 km · 99 %");
    expect(wearTip({ ...pegasus, stats: { ...pegasus.stats, distance_m: 900_000 } }, 1)).toBe("900.00 km of 800.00 km · past the limit");
  });

  it("reads a distance field in the display unit, hands back untouched meters as they were, and flags junk", () => {
    expect(readDistanceField("12.5", "", null)).toEqual({ meters: 12_500, invalid: false });
    expect(readDistanceField("12,5", "", null)).toEqual({ meters: 12_500, invalid: false });
    expect(readDistanceField("", "", null)).toEqual({ meters: null, invalid: false });
    expect(readDistanceField("  ", "", null)).toEqual({ meters: null, invalid: false });
    expect(readDistanceField("1ooo", "", null)).toEqual({ meters: null, invalid: true });
    expect(readDistanceField("-5", "", null)).toEqual({ meters: null, invalid: true });
    // The text the field opened with means the stored value, to the meter.
    expect(readDistanceField("621.37", "621.37", 1_000_000)).toEqual({ meters: 1_000_000, invalid: false });
    expect(readDistanceField("621.37", "621.38", 1_000_000).meters).toBeCloseTo(621_370);
    expect(distanceInputValue(12_500)).toBe("12.5");
    expect(distanceInputValue(null)).toBe("");
    useUnitsStore.setState({ mode: "imperial" });
    expect(readDistanceField("10", "", null).meters).toBeCloseTo(10 * M_PER_MILE);
    expect(distanceInputValue(M_PER_MILE * 3)).toBe("3");
    expect(distanceInputValue(1_000_000)).toBe("621.37");
  });
});

describe("Garage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // Queued once-answers must not leak from a test that stopped early.
    vi.mocked(confirmDialog).mockReset();
    vi.mocked(api.listGear).mockResolvedValue([road, pegasus, oldShoes]);
    vi.mocked(api.gearRuleCandidates).mockResolvedValue({
      profiles: [
        { value: "ROAD", count: 169 },
        { value: "Bike", count: 12 },
      ],
      sensors: [{ serial: "3632674300", device_type: "bike_power", manufacturer: "favero_electronics", product: "assioma_duo", count: 169 }],
    });
  });
  afterEach(() => {
    cleanup();
    useUnitsStore.setState({ mode: "metric" });
    vi.useRealTimers();
  });

  it("lists every item with its odometer, totals, defaults and wear", async () => {
    renderIt();
    await waitFor(() => expect(screen.getAllByTestId("gear-card")).toHaveLength(3));
    const [bike, shoes, retired] = screen.getAllByTestId("gear-card");
    expect(bike.textContent).toContain("Road");
    expect(bike.textContent).toContain("Canyon Ultimate");
    // 1 000 km before Syzify + 50 km in it.
    expect(bike.textContent).toContain("1050.00 km");
    expect(bike.textContent).toContain("(50.00 km in Syzify)");
    expect(bike.textContent).toContain("1h 40m");
    expect(bike.textContent).toContain("2 activities");
    expect(bike.textContent).toContain("Default for Ride");
    expect(bike.textContent).toContain("last used");
    expect(within(bike).queryByRole("progressbar")).toBeNull();

    expect(shoes.textContent).toContain("12 activities");
    expect(shoes.textContent).not.toContain("in Syzify");
    expect(shoes.textContent).not.toContain("last used");
    const bar = within(shoes).getByRole("progressbar");
    expect(bar.getAttribute("aria-valuenow")).toBe("88");
    expect(bar.getAttribute("aria-label")).toBe("700.00 km of 800.00 km · 87 %");
    expect(bar.getAttribute("data-tip")).toBe("700.00 km of 800.00 km · 87 %");
    // The tooltip's host does not clip (the track inside it does).
    expect(bar.className).not.toContain("overflow-hidden");
    expect(bar.firstElementChild!.className).toContain("overflow-hidden");

    // No activities: no row of zeros, just the fact (and the pre-Syzify
    // mileage when there is one).
    expect(retired.textContent).toContain("300.00 km before Syzify · No activities yet");
    expect(retired.textContent).not.toContain("0 activities");
    expect(retired.textContent).toContain("Retired");
    expect(within(retired).getByRole("button", { name: "Bring back Old shoes" })).toBeTruthy();
    expect(within(bike).getByRole("button", { name: "Retire Road" })).toBeTruthy();
  });

  it("says so when the garage is empty, and when it could not be read", async () => {
    vi.mocked(api.listGear).mockResolvedValue([]);
    renderIt();
    await waitFor(() => expect(screen.getByText(/Nothing here yet/)).toBeTruthy());
    cleanup();
    vi.mocked(api.listGear).mockRejectedValue("database is locked");
    renderIt();
    await waitFor(() => expect(screen.getByRole("alert").textContent).toContain("Could not load the garage: database is locked"));
    expect(screen.queryByText(/Nothing here yet/)).toBeNull();
  });

  it("paints the wear bar red once the limit is reached, and a brand-new item says so without zeros", async () => {
    vi.mocked(api.listGear).mockResolvedValue([
      { ...pegasus, stats: { ...pegasus.stats, distance_m: 800_000 } },
      { ...pegasus, id: "g-mid", stats: { ...pegasus.stats, distance_m: 640_000 } },
      { ...pegasus, id: "g-low", stats: { ...pegasus.stats, distance_m: 100_000 } },
      { ...pegasus, id: "g-new", name: "Fresh", distance_limit_m: null, stats: { ...pegasus.stats, activities: 0, distance_m: 0 } },
    ]);
    renderIt();
    await waitFor(() => expect(screen.getAllByRole("progressbar")).toHaveLength(3));
    const fills = screen.getAllByTestId("wear-fill").map((fill) => fill.style.background);
    expect(fills).toEqual(["var(--danger)", "var(--warn)", "var(--good)"]);
    expect(screen.getAllByRole("progressbar")[0].getAttribute("aria-valuenow")).toBe("100");
    expect(screen.getAllByRole("progressbar")[0].getAttribute("data-tip")).toBe("800.00 km of 800.00 km · past the limit");
    const fresh = screen.getAllByTestId("gear-card")[3];
    expect(fresh.textContent).toContain("No activities yet");
    expect(fresh.textContent).not.toContain("before Syzify");
    expect(fresh.textContent).not.toContain("0 m");
  });

  it("adds an item from the modal, in the display unit, and refreshes the list", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    vi.setSystemTime(new Date(2026, 9, 1, 12, 0, 0));
    useUnitsStore.setState({ mode: "imperial" });
    vi.mocked(api.createGear).mockResolvedValue({ ...pegasus, id: "g-new" });
    renderIt();
    await waitFor(() => expect(screen.getAllByTestId("gear-card")).toHaveLength(3));
    fireEvent.click(screen.getByRole("button", { name: "Add gear" }));
    const dialog = screen.getByRole("dialog", { name: "Add gear" });
    // Save needs a name.
    expect((within(dialog).getByRole("button", { name: "Save" }) as HTMLButtonElement).disabled).toBe(true);

    fireEvent.click(within(dialog).getByLabelText("Kind"));
    fireEvent.click(screen.getByRole("option", { name: "Shoes" }));
    fireEvent.change(within(dialog).getByLabelText("Name"), { target: { value: "  Pegasus 41 " } });
    fireEvent.change(within(dialog).getByLabelText("Brand"), { target: { value: "Nike" } });
    fireEvent.change(within(dialog).getByLabelText("Model"), { target: { value: "41" } });
    fireEvent.change(within(dialog).getByLabelText("Notes"), { target: { value: "road pair" } });
    // The app's calendar, opening downward in the modal: pick the 1st of
    // the month shown (today's), which is 2026-10-01 under the fake clock.
    fireEvent.click(within(dialog).getByRole("button", { name: /Purchased/ }));
    const picker = dialog.querySelector(".dp")!;
    expect(picker.className).toContain("dp-down");
    fireEvent.click(within(dialog).getByRole("button", { name: "1" }));
    expect(within(dialog).getByRole("button", { name: /Purchased/ }).textContent).toContain("01 Oct 2026");
    fireEvent.change(within(dialog).getByLabelText("Mileage before Syzify (mi)"), { target: { value: "10" } });
    fireEvent.change(within(dialog).getByLabelText("Replace at (mi)"), { target: { value: "500" } });
    fireEvent.click(within(dialog).getByLabelText("Default for"));
    fireEvent.click(screen.getByRole("option", { name: "Run" }));
    fireEvent.click(screen.getByRole("option", { name: "Hike" }));
    fireEvent.click(within(dialog).getByRole("button", { name: "Save" }));

    await waitFor(() => expect(api.createGear).toHaveBeenCalledTimes(1));
    const input = vi.mocked(api.createGear).mock.calls[0][0];
    expect(input.kind).toBe("shoes");
    expect(input.name).toBe("  Pegasus 41 ");
    expect(input.brand).toBe("Nike");
    expect(input.model).toBe("41");
    expect(input.notes).toBe("road pair");
    expect(input.purchased_at).toBe("2026-10-01");
    expect(input.initial_distance_m).toBeCloseTo(10 * M_PER_MILE);
    expect(input.distance_limit_m).toBeCloseTo(500 * M_PER_MILE);
    expect(input.default_for).toEqual(["run", "hike"]);
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(api.listGear).toHaveBeenCalledTimes(2);
    expect(useToastStore.getState().addToast).toHaveBeenCalledWith("success", "Gear added");
    vi.useRealTimers();
  });

  it("edits an item with its fields prefilled and drops defaults the new kind does not offer", async () => {
    vi.mocked(api.updateGear).mockResolvedValue(undefined);
    renderIt();
    await waitFor(() => expect(screen.getAllByTestId("gear-card")).toHaveLength(3));
    fireEvent.click(screen.getByRole("button", { name: "Edit Road" }));
    const dialog = screen.getByRole("dialog", { name: "Edit gear" });
    expect((within(dialog).getByLabelText("Name") as HTMLInputElement).value).toBe("Road");
    expect((within(dialog).getByLabelText("Mileage before Syzify (km)") as HTMLInputElement).value).toBe("1000");
    expect(within(dialog).getByLabelText("Default for").textContent).toContain("Ride");

    // Turning the bike into shoes cannot keep "ride" as its default.
    fireEvent.click(within(dialog).getByLabelText("Kind"));
    fireEvent.click(screen.getByRole("option", { name: "Shoes" }));
    fireEvent.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(() => expect(api.updateGear).toHaveBeenCalledTimes(1));
    const [id, input] = vi.mocked(api.updateGear).mock.calls[0];
    expect(id).toBe("g-road");
    expect(input.kind).toBe("shoes");
    expect(input.default_for).toEqual([]);
    expect(input.initial_distance_m).toBe(1_000_000);
  });

  it("keeps the stored meters to the meter when only something else changes, in miles too", async () => {
    useUnitsStore.setState({ mode: "imperial" });
    vi.mocked(api.updateGear).mockResolvedValue(undefined);
    renderIt();
    await waitFor(() => expect(screen.getAllByTestId("gear-card")).toHaveLength(3));
    fireEvent.click(screen.getByRole("button", { name: "Edit Road" }));
    const dialog = screen.getByRole("dialog", { name: "Edit gear" });
    // 1 000 000 m shows as 621.37 mi; a rename must not round-trip it.
    expect((within(dialog).getByLabelText("Mileage before Syzify (mi)") as HTMLInputElement).value).toBe("621.37");
    fireEvent.change(within(dialog).getByLabelText("Name"), { target: { value: "Road bike" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(() => expect(api.updateGear).toHaveBeenCalledTimes(1));
    expect(vi.mocked(api.updateGear).mock.calls[0][1].initial_distance_m).toBe(1_000_000);
  });

  it("refuses to save a distance that is not a number instead of storing zero", async () => {
    renderIt();
    await waitFor(() => expect(screen.getAllByTestId("gear-card")).toHaveLength(3));
    fireEvent.click(screen.getByRole("button", { name: "Edit Road" }));
    const dialog = screen.getByRole("dialog", { name: "Edit gear" });
    const field = within(dialog).getByLabelText("Mileage before Syzify (km)");
    fireEvent.change(field, { target: { value: "1ooo" } });
    expect(field.getAttribute("aria-invalid")).toBe("true");
    expect(within(dialog).getByText("Enter a number")).toBeTruthy();
    expect((within(dialog).getByRole("button", { name: "Save" }) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(within(dialog).getByRole("button", { name: "Save" }));
    expect(api.updateGear).not.toHaveBeenCalled();
    // A typo in the limit must not quietly remove it either.
    fireEvent.change(field, { target: { value: "1000" } });
    fireEvent.change(within(dialog).getByLabelText("Replace at (km)"), { target: { value: "8oo" } });
    expect((within(dialog).getByRole("button", { name: "Save" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("offers no defaults on a retired item and sends none", async () => {
    vi.mocked(api.updateGear).mockResolvedValue(undefined);
    renderIt();
    await waitFor(() => expect(screen.getAllByTestId("gear-card")).toHaveLength(3));
    fireEvent.click(screen.getByRole("button", { name: "Edit Old shoes" }));
    const dialog = screen.getByRole("dialog", { name: "Edit gear" });
    expect(within(dialog).queryByLabelText("Default for")).toBeNull();
    expect(within(dialog).getByText(/Bring it back first/)).toBeTruthy();
    fireEvent.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(() => expect(api.updateGear).toHaveBeenCalledTimes(1));
    expect(vi.mocked(api.updateGear).mock.calls[0][1].default_for).toEqual([]);
  });

  it("reports a refused save and keeps the modal open", async () => {
    vi.mocked(api.createGear).mockRejectedValue("Name is required");
    renderIt();
    await waitFor(() => expect(screen.getAllByTestId("gear-card")).toHaveLength(3));
    fireEvent.click(screen.getByRole("button", { name: "Add gear" }));
    const dialog = screen.getByRole("dialog", { name: "Add gear" });
    fireEvent.change(within(dialog).getByLabelText("Name"), { target: { value: "x" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(() =>
      expect(useToastStore.getState().addToast).toHaveBeenCalledWith("error", "Could not save gear: Name is required"),
    );
    expect(screen.getByRole("dialog")).toBeTruthy();
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("retires and brings back without a question", async () => {
    vi.mocked(api.setGearRetired).mockResolvedValue(undefined);
    renderIt();
    await waitFor(() => expect(screen.getAllByTestId("gear-card")).toHaveLength(3));
    fireEvent.click(screen.getByRole("button", { name: "Retire Road" }));
    await waitFor(() => expect(api.setGearRetired).toHaveBeenCalledWith("g-road", true));
    fireEvent.click(screen.getByRole("button", { name: "Bring back Old shoes" }));
    await waitFor(() => expect(api.setGearRetired).toHaveBeenCalledWith("g-old", false));
    expect(confirmDialog).not.toHaveBeenCalled();
    await waitFor(() => expect(api.listGear).toHaveBeenCalledTimes(3));
    // A refusal is said, not swallowed — and a Tauri command refuses with
    // a bare string, not an Error (#174).
    vi.mocked(api.setGearRetired).mockRejectedValueOnce("Gear not found: g-road");
    fireEvent.click(screen.getByRole("button", { name: "Retire Road" }));
    await waitFor(() =>
      expect(useToastStore.getState().addToast).toHaveBeenCalledWith("error", "Could not update gear: Gear not found: g-road"),
    );
  });

  it("offers to put the item on its history from the purchase on, and says how many it took", async () => {
    vi.mocked(confirmDialog).mockResolvedValueOnce(false).mockResolvedValueOnce(true).mockResolvedValueOnce(true);
    vi.mocked(api.assignGearHistory).mockResolvedValueOnce(37).mockResolvedValueOnce(0);
    renderIt();
    await waitFor(() => expect(screen.getAllByTestId("gear-card")).toHaveLength(3));
    const [bike, shoes, retired] = screen.getAllByTestId("gear-card");
    // Only an item with default sports offers it; a retired one never.
    const offer = within(bike).getByRole("button", { name: /Assign to all Ride activities since/ });
    expect(offer.textContent).toContain("2025");
    expect(within(shoes).queryByRole("button", { name: /Assign to all/ })).toBeNull();
    expect(within(retired).queryByRole("button", { name: /Assign to all/ })).toBeNull();

    // Cancelled: nothing happens.
    fireEvent.click(offer);
    await waitFor(() => expect(confirmDialog).toHaveBeenCalledTimes(1));
    expect(vi.mocked(confirmDialog).mock.calls[0][0].title).toMatch(/^Assign to all Ride activities since .*\?$/);
    expect(api.assignGearHistory).not.toHaveBeenCalled();

    fireEvent.click(offer);
    await waitFor(() => expect(api.assignGearHistory).toHaveBeenCalledWith("g-road"));
    await waitFor(() =>
      expect(useToastStore.getState().addToast).toHaveBeenCalledWith("success", "Road assigned to 37 activities"),
    );
    await waitFor(() => expect(api.listGear).toHaveBeenCalledTimes(2));

    // Nothing left to take is said as such, not as a success.
    fireEvent.click(offer);
    await waitFor(() =>
      expect(useToastStore.getState().addToast).toHaveBeenCalledWith("info", expect.stringContaining("Nothing to assign")),
    );

    // A refusal is said.
    vi.mocked(confirmDialog).mockResolvedValueOnce(true);
    vi.mocked(api.assignGearHistory).mockRejectedValueOnce("Bring the gear back");
    fireEvent.click(offer);
    await waitFor(() =>
      expect(useToastStore.getState().addToast).toHaveBeenCalledWith("error", "Could not assign gear: Bring the gear back"),
    );
  });

  it("offers the whole history when the item has no purchase date", async () => {
    vi.mocked(api.listGear).mockResolvedValue([{ ...road, purchased_at: null }]);
    renderIt();
    await waitFor(() => expect(screen.getAllByTestId("gear-card")).toHaveLength(1));
    expect(screen.getByRole("button", { name: "Assign to all Ride activities" })).toBeTruthy();
  });

  it("offers the profiles and sensors the files carried as rules, keeps a value the vault no longer has, and saves them", async () => {
    vi.mocked(api.updateGear).mockResolvedValue(undefined);
    vi.mocked(api.listGear).mockResolvedValue([
      { ...road, rules: [{ kind: "profile_name", value: "ROAD" }, { kind: "sensor_serial", value: "999" }] },
    ]);
    renderIt();
    await waitFor(() => expect(screen.getAllByTestId("gear-card")).toHaveLength(1));
    // The card names the rules, a known sensor by its name, an unknown by its serial.
    await waitFor(() => expect(screen.getByTestId("gear-rules").textContent).toBe("Auto-assign by profile ROAD · sensor 999"));
    fireEvent.click(screen.getByRole("button", { name: "Edit Road" }));
    const dialog = screen.getByRole("dialog", { name: "Edit gear" });
    await waitFor(() => expect(within(dialog).getByTestId("gear-rules-form")).toBeTruthy());
    fireEvent.click(within(dialog).getByLabelText("Profile is"));
    expect(screen.getAllByRole("option").map((o) => o.textContent)).toEqual(["ROAD (169)", "Bike (12)"]);
    fireEvent.click(screen.getByRole("option", { name: "Bike (12)" }));
    // A multi-select stays open after a toggle; close it before the next one.
    fireEvent.mouseDown(document.body);
    fireEvent.click(within(dialog).getByLabelText("Sensor is paired"));
    expect(screen.getAllByRole("option").map((o) => o.textContent)).toEqual([
      "Favero Electronics Assioma Duo (bike power) · 3632674300 (169)",
      "999",
    ]);
    fireEvent.click(screen.getByRole("option", { name: /Assioma/ }));
    fireEvent.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(() => expect(api.updateGear).toHaveBeenCalledTimes(1));
    expect(vi.mocked(api.updateGear).mock.calls[0][1].rules).toEqual([
      { kind: "profile_name", value: "ROAD" },
      { kind: "profile_name", value: "Bike" },
      { kind: "sensor_serial", value: "3632674300" },
      { kind: "sensor_serial", value: "999" },
    ]);
  });

  it("hides the rules form while the vault has nothing a rule could match", async () => {
    vi.mocked(api.gearRuleCandidates).mockResolvedValue({ profiles: [], sensors: [] });
    renderIt();
    await waitFor(() => expect(screen.getAllByTestId("gear-card")).toHaveLength(3));
    fireEvent.click(screen.getByRole("button", { name: "Add gear" }));
    await new Promise((r) => setTimeout(r, 20));
    expect(screen.queryByTestId("gear-rules-form")).toBeNull();
    expect(screen.queryByTestId("apply-rules")).toBeNull();
  });

  it("applies the rules to the history on request, with a confirmation, and says what happened", async () => {
    vi.mocked(api.listGear).mockResolvedValue([{ ...road, rules: [{ kind: "profile_name", value: "ROAD" }] }, pegasus]);
    vi.mocked(confirmDialog).mockResolvedValueOnce(false).mockResolvedValueOnce(true).mockResolvedValueOnce(true);
    vi.mocked(api.applyGearRules).mockResolvedValueOnce(140).mockResolvedValueOnce(0);
    renderIt();
    await waitFor(() => expect(screen.getByTestId("apply-rules")).toBeTruthy());
    fireEvent.click(screen.getByTestId("apply-rules"));
    await waitFor(() => expect(confirmDialog).toHaveBeenCalledTimes(1));
    expect(api.applyGearRules).not.toHaveBeenCalled();
    fireEvent.click(screen.getByTestId("apply-rules"));
    await waitFor(() => expect(useToastStore.getState().addToast).toHaveBeenCalledWith("success", "Rules put 140 activities on their gear"));
    await waitFor(() => expect(api.listGear).toHaveBeenCalledTimes(2));
    fireEvent.click(screen.getByTestId("apply-rules"));
    await waitFor(() => expect(useToastStore.getState().addToast).toHaveBeenCalledWith("info", expect.stringContaining("no unassigned activity matches")));
    expect(api.listGear).toHaveBeenCalledTimes(2);
    // A refusal is said in the backend's words — a bare string (#174).
    vi.mocked(confirmDialog).mockResolvedValueOnce(true);
    vi.mocked(api.applyGearRules).mockRejectedValueOnce("database is locked");
    fireEvent.click(screen.getByTestId("apply-rules"));
    await waitFor(() => expect(useToastStore.getState().addToast).toHaveBeenCalledWith("error", "Could not apply the rules: database is locked"));
    // A rule on a retired item alone does not offer the button.
    cleanup();
    vi.mocked(api.listGear).mockResolvedValue([{ ...oldShoes, rules: [{ kind: "profile_name", value: "Run" }] }]);
    renderIt();
    await waitFor(() => expect(screen.getAllByTestId("gear-card")).toHaveLength(1));
    expect(screen.queryByTestId("apply-rules")).toBeNull();
  });

  it("deletes only after a confirmation that counts the activities, and never on cancel", async () => {
    vi.mocked(confirmDialog).mockResolvedValueOnce(false).mockResolvedValueOnce(true);
    vi.mocked(api.deleteGear).mockResolvedValue(undefined);
    renderIt();
    await waitFor(() => expect(screen.getAllByTestId("gear-card")).toHaveLength(3));
    fireEvent.click(screen.getByRole("button", { name: "Delete Road" }));
    await waitFor(() => expect(confirmDialog).toHaveBeenCalledTimes(1));
    expect(vi.mocked(confirmDialog).mock.calls[0][0]).toMatchObject({
      title: "Delete Road?",
      message: expect.stringContaining("2 activities will keep their data"),
      danger: true,
    });
    expect(api.deleteGear).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "Delete Old shoes" }));
    await waitFor(() => expect(api.deleteGear).toHaveBeenCalledWith("g-old"));
    expect(vi.mocked(confirmDialog).mock.calls[1][0].message).toBe("This cannot be undone.");
    await waitFor(() => expect(api.listGear).toHaveBeenCalledTimes(2));

    // A refused delete is said.
    vi.mocked(confirmDialog).mockResolvedValueOnce(true);
    vi.mocked(api.deleteGear).mockRejectedValueOnce("database is locked");
    fireEvent.click(screen.getByRole("button", { name: "Delete Pegasus" }));
    await waitFor(() =>
      expect(useToastStore.getState().addToast).toHaveBeenCalledWith("error", "Could not delete gear: database is locked"),
    );
  });
});
