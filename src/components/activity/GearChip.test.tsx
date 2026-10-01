// @vitest-environment happy-dom
import { describe, it, expect, afterEach, beforeEach, vi } from "vitest";
import { render, cleanup, screen, waitFor, fireEvent } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { GearItem } from "../../lib/types";

vi.mock("../../lib/tauri", () => ({ api: { listGear: vi.fn(), setActivityGear: vi.fn() } }));
const addToast = vi.fn();
vi.mock("../../stores/toastStore", () => ({
  useToastStore: (sel: (s: { addToast: typeof addToast }) => unknown) => sel({ addToast }),
}));

import { GearChip, gearChipVisible, gearPlaceholder, gearTip } from "./GearChip";
import { api } from "../../lib/tauri";

const road: GearItem = {
  id: "g-road",
  kind: "bike",
  name: "Road",
  brand: "Canyon",
  model: "Ultimate",
  purchased_at: null,
  initial_distance_m: 0,
  distance_limit_m: null,
  retired_at: null,
  notes: null,
  created_at: "2026-10-01T10:00:00",
  stats: { activities: 1, distance_m: 0, duration_s: 0, elev_gain_m: 0, last_used: null },
  default_for: [],
  rules: [],
};
const gravel: GearItem = { ...road, id: "g-gravel", name: "Gravel", brand: null, model: null };
const oldRoad: GearItem = { ...road, id: "g-old", name: "Old road", retired_at: "2026-01-01T00:00:00" };
const shoes: GearItem = { ...road, id: "g-shoes", kind: "shoes", name: "Pegasus", brand: null, model: null };

function renderIt(props: { gearId: string | null; sport?: string; locked?: boolean }) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const onChanged = vi.fn();
  render(
    <QueryClientProvider client={qc}>
      <GearChip activityId="act-1" gearId={props.gearId} sport={props.sport ?? "ride"} locked={props.locked} onChanged={onChanged} />
    </QueryClientProvider>,
  );
  return onChanged;
}

const menuItems = () => screen.getAllByRole("menuitemradio").map((o) => o.textContent);

describe("GearChip", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(api.listGear).mockResolvedValue([road, gravel, oldRoad, shoes]);
    vi.mocked(api.setActivityGear).mockResolvedValue(undefined);
  });
  afterEach(cleanup);

  it("names the item with its kind's icon and the brand and model on hover", async () => {
    renderIt({ gearId: "g-road" });
    await waitFor(() => expect(screen.getByTestId("gear-chip").textContent).toBe("Road"));
    const chip = screen.getByTestId("gear-chip");
    expect(chip.className).not.toContain("text-faint");
    expect(chip.getAttribute("data-tip")).toBe("Road (Canyon Ultimate)");
    expect(chip.getAttribute("aria-label")).toBe("Gear: Road (Canyon Ultimate)");
    expect(gearTip(gravel)).toBe("Gravel");
  });

  it("opens a menu of the items that fit the sport, writes a pick at once and reports it", async () => {
    const onChanged = renderIt({ gearId: "g-road" });
    await waitFor(() => expect(screen.getByTestId("gear-chip").textContent).toBe("Road"));
    fireEvent.click(screen.getByTestId("gear-chip"));
    // None, the bikes in use and nothing else: the shoes do not fit a ride,
    // the retired bike is not on this activity.
    expect(menuItems()).toEqual(["None", "Road", "Gravel"]);
    expect(screen.getByRole("menuitemradio", { name: "Road" }).getAttribute("aria-checked")).toBe("true");

    fireEvent.click(screen.getByRole("menuitemradio", { name: "Gravel" }));
    await waitFor(() => expect(api.setActivityGear).toHaveBeenCalledWith("act-1", "g-gravel"));
    await waitFor(() => expect(onChanged).toHaveBeenCalledTimes(1));
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("takes the activity off its gear with None, and picking the current item only closes", async () => {
    const onChanged = renderIt({ gearId: "g-road" });
    await waitFor(() => expect(screen.getByTestId("gear-chip").textContent).toBe("Road"));
    fireEvent.click(screen.getByTestId("gear-chip"));
    fireEvent.click(screen.getByRole("menuitemradio", { name: "Road" }));
    expect(screen.queryByRole("menu")).toBeNull();
    expect(api.setActivityGear).not.toHaveBeenCalled();
    expect(onChanged).not.toHaveBeenCalled();

    fireEvent.click(screen.getByTestId("gear-chip"));
    fireEvent.click(screen.getByRole("menuitemradio", { name: "None" }));
    await waitFor(() => expect(api.setActivityGear).toHaveBeenCalledWith("act-1", null));
    await waitFor(() => expect(onChanged).toHaveBeenCalledTimes(1));
  });

  it("keeps a retired item it is on in the menu, marked", async () => {
    renderIt({ gearId: "g-old" });
    await waitFor(() => expect(screen.getByTestId("gear-chip").textContent).toBe("Old road"));
    fireEvent.click(screen.getByTestId("gear-chip"));
    expect(menuItems()).toEqual(["None", "Road", "Gravel", "Old road (retired)"]);
  });

  it("says a refusal and leaves the menu open", async () => {
    // A Tauri command refuses with a bare string, not an Error.
    vi.mocked(api.setActivityGear).mockRejectedValueOnce("Bring the gear back");
    const onChanged = renderIt({ gearId: "g-road" });
    await waitFor(() => expect(screen.getByTestId("gear-chip").textContent).toBe("Road"));
    fireEvent.click(screen.getByTestId("gear-chip"));
    fireEvent.click(screen.getByRole("menuitemradio", { name: "Gravel" }));
    await waitFor(() => expect(addToast).toHaveBeenCalledWith("error", "Gear not changed: Bring the gear back"));
    expect(screen.getByRole("menu")).toBeTruthy();
    expect(onChanged).not.toHaveBeenCalled();
  });

  it("offers a muted entry point on an activity without gear, named by the kind its sport takes, only when there is something to pick", async () => {
    renderIt({ gearId: null, sport: "run" });
    await waitFor(() => expect(screen.getByTestId("gear-chip").textContent).toBe("Shoes"));
    expect(screen.getByTestId("gear-chip").getAttribute("aria-label")).toBe("Pick shoes");
    expect(screen.getByTestId("gear-chip").getAttribute("data-tip")).toBe("No shoes yet — pick one");
    // Muted like the label: the class is on the svg itself, where the
    // chip's own svg color rule would otherwise win.
    const icon = screen.getByTestId("gear-chip").querySelector("svg")!;
    expect(icon.getAttribute("class")).toContain("!text-faint");
    expect(icon.parentElement).toBe(screen.getByTestId("gear-chip"));
    // And the label with it: the chip's own ink color must lose.
    expect(screen.getByTestId("gear-chip").className).toContain("!text-faint");
    fireEvent.click(screen.getByTestId("gear-chip"));
    expect(menuItems()).toEqual(["None", "Pegasus"]);
    expect(screen.getByRole("menuitemradio", { name: "None" }).getAttribute("aria-checked")).toBe("true");
    cleanup();
    // A swim has nothing of a fitting kind: no chip.
    renderIt({ gearId: null, sport: "swim" });
    await waitFor(() => expect(api.listGear).toHaveBeenCalledTimes(2));
    expect(screen.queryByTestId("gear-chip")).toBeNull();
  });

  it("closes on a click outside or Escape, and keeps Escape and the arrows from the page while open", async () => {
    // The activity page listens on the document: Escape leaves for the
    // library, the arrows step between activities. Neither may fire
    // while the menu is open — the page's listener was registered first.
    const page = vi.fn();
    document.addEventListener("keydown", page);
    renderIt({ gearId: "g-road" });
    await waitFor(() => expect(screen.getByTestId("gear-chip").textContent).toBe("Road"));
    fireEvent.click(screen.getByTestId("gear-chip"));
    expect(screen.getByRole("menu")).toBeTruthy();
    fireEvent.mouseDown(document.body);
    expect(screen.queryByRole("menu")).toBeNull();
    fireEvent.click(screen.getByTestId("gear-chip"));
    fireEvent.keyDown(document, { key: "ArrowRight" });
    expect(screen.getByRole("menu")).toBeTruthy();
    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByRole("menu")).toBeNull();
    expect(page).not.toHaveBeenCalled();
    // With the menu closed the page hears its keys again.
    fireEvent.keyDown(document, { key: "Escape" });
    expect(page).toHaveBeenCalledTimes(1);
    document.removeEventListener("keydown", page);
  });

  it("locks the menu while a pick is being written", async () => {
    let settle: () => void = () => {};
    vi.mocked(api.setActivityGear).mockImplementationOnce(() => new Promise((r) => (settle = () => r(undefined))));
    renderIt({ gearId: "g-road" });
    await waitFor(() => expect(screen.getByTestId("gear-chip").textContent).toBe("Road"));
    fireEvent.click(screen.getByTestId("gear-chip"));
    fireEvent.click(screen.getByRole("menuitemradio", { name: "Gravel" }));
    await waitFor(() => expect((screen.getByRole("menuitemradio", { name: "None" }) as HTMLButtonElement).disabled).toBe(true));
    fireEvent.click(screen.getByRole("menuitemradio", { name: "None" }));
    expect(api.setActivityGear).toHaveBeenCalledTimes(1);
    settle();
    await waitFor(() => expect(screen.queryByRole("menu")).toBeNull());
  });

  it("reads an id the registry no longer has as no gear", async () => {
    renderIt({ gearId: "g-gone" });
    await waitFor(() => expect(screen.getByTestId("gear-chip").textContent).toBe("Bike"));
    fireEvent.click(screen.getByTestId("gear-chip"));
    expect(screen.getByRole("menuitemradio", { name: "None" }).getAttribute("aria-checked")).toBe("true");
  });

  it("names the kind a sport takes, and plain gear where it takes anything", () => {
    expect(gearPlaceholder("ride")).toEqual({ kind: "bike", label: "Bike" });
    expect(gearPlaceholder("mountain_bike")).toEqual({ kind: "bike", label: "Bike" });
    expect(gearPlaceholder("run")).toEqual({ kind: "shoes", label: "Shoes" });
    expect(gearPlaceholder("hike")).toEqual({ kind: "shoes", label: "Shoes" });
    expect(gearPlaceholder("swim")).toEqual({ kind: "other", label: "Gear" });
    expect(gearPlaceholder("ski")).toEqual({ kind: "other", label: "Gear" });
  });

  it("tells the header whether it will show", () => {
    const items = [road, shoes];
    expect(gearChipVisible(items, "g-road", "ride", false)).toBe(true);
    expect(gearChipVisible(items, null, "ride", false)).toBe(true);
    expect(gearChipVisible(items, null, "swim", false)).toBe(false);
    expect(gearChipVisible(items, "g-road", "ride", true)).toBe(false);
    expect(gearChipVisible([], "g-road", "ride", false)).toBe(false);
    expect(gearChipVisible(items, "g-gone", "swim", false)).toBe(false);
  });

  it("renders nothing, and asks for nothing, for a multisport whole", async () => {
    renderIt({ gearId: "g-road", locked: true });
    await new Promise((r) => setTimeout(r, 20));
    expect(screen.queryByTestId("gear-chip")).toBeNull();
    expect(api.listGear).not.toHaveBeenCalled();
  });
});
