// @vitest-environment happy-dom
import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import { render, cleanup, waitFor, fireEvent, act } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import L from "leaflet";

vi.mock("../../lib/tauri", () => ({
  isTauri: () => false,
  api: {
    getSetting: vi.fn(async () => null),
    setSetting: vi.fn(async () => undefined),
    setActivityLocationPoint: vi.fn(),
  },
}));
const addToast = vi.fn();
vi.mock("../../stores/toastStore", () => ({
  useToastStore: (sel: (s: { addToast: typeof addToast }) => unknown) => sel({ addToast }),
}));

import { api } from "../../lib/tauri";
import { RouteMap } from "./RouteMap";
import { useActivityStore } from "../../stores/activityStore";
import type { TrackPointColumns } from "../../lib/types";

// Three points ~111 m apart along a meridian; a right-click snaps within 50 m.
const track: TrackPointColumns = {
  t: [0, 1, 2],
  lat: [55.7, 55.701, 55.702],
  lon: [37.6, 37.6, 37.6],
  altitude_m: [null, null, null],
  distance_m: [null, null, null],
  speed_mps: [null, null, null],
  hr: [null, null, null],
  cadence: [null, null, null],
  power_w: [null, null, null],
  temperature_c: [null, null, null],
  vertical_oscillation_mm: [null, null, null],
  stance_time_ms: [null, null, null],
  stance_time_percent: [null, null, null],
  step_length_mm: [null, null, null],
  grade_percent: [null, null, null],
  left_right_balance: [null, null, null],
  left_torque_effectiveness: [null, null, null],
  left_pedal_smoothness: [null, null, null],
  right_torque_effectiveness: [null, null, null],
  right_pedal_smoothness: [null, null, null],
};

/** Mounts the map and hands back the Leaflet instance: the wrapper creates
 * it through `L.map`, so a spy on that is the only way in from outside. */
function mount(props: Partial<Parameters<typeof RouteMap>[0]> = {}) {
  const mapSpy = vi.spyOn(L, "map");
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  const invalidate = vi.spyOn(qc, "invalidateQueries");
  const utils = render(
    <QueryClientProvider client={qc}>
      <RouteMap trackpoints={track} sport="ride" activityId="act-1" {...props} />
    </QueryClientProvider>,
  );
  // spyOn on an already-spied method returns the same spy: take the map
  // of THIS mount, not of the first one in the test.
  const map = mapSpy.mock.results.at(-1)!.value as L.Map;
  return { ...utils, map, invalidate };
}

const markers = (map: L.Map) => {
  const out: L.Marker[] = [];
  map.eachLayer((l) => {
    if (l instanceof L.Marker) out.push(l);
  });
  return out;
};
const flagOf = (map: L.Map) =>
  markers(map).find((m) => (m.options.icon as L.DivIcon).options.className === "route-flag");

beforeEach(() => {
  vi.mocked(api.setActivityLocationPoint).mockReset();
  addToast.mockClear();
  useActivityStore.setState({ hoveredPointIndex: null });
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

/** The menu's one item, if the popup is open. Leaflet events come from
 * outside React, so every `map.fire` in here runs under `act`. */
const menuButton = () =>
  Array.from(document.querySelectorAll<HTMLButtonElement>(".leaflet-popup button")).find((b) =>
    /destination point|Saving/.test(b.textContent ?? ""),
  );

describe("RouteMap destination point (#179)", () => {
  it("a left click on the route opens the menu on the snapped point, and the item saves it", async () => {
    vi.mocked(api.setActivityLocationPoint).mockResolvedValue({
      geocoded: true,
      geocoding_off: false,
      location_name: "Mahmutlar, Alanya",
    });
    const { map, invalidate } = mount();
    expect(menuButton()).toBeUndefined();

    // ~11 m north of the second point: snaps to it, not to the click.
    act(() => void map.fire("click", { latlng: L.latLng(55.7011, 37.6) }));
    const button = menuButton();
    expect(button?.textContent).toContain("Set as destination point");
    expect(api.setActivityLocationPoint).not.toHaveBeenCalled();

    fireEvent.click(button!);
    await waitFor(() => expect(api.setActivityLocationPoint).toHaveBeenCalledTimes(1));
    expect(api.setActivityLocationPoint).toHaveBeenCalledWith("act-1", 55.701, 37.6);
    await waitFor(() => expect(addToast).toHaveBeenCalledWith("success", "Destination point: Mahmutlar, Alanya"));
    // The menu is gone once saved; the activity page, the library list and
    // its map all read the point.
    await waitFor(() => expect(menuButton()).toBeUndefined());
    const keys = invalidate.mock.calls.map((c) => JSON.stringify(c[0]?.queryKey));
    expect(keys).toEqual(
      expect.arrayContaining([
        JSON.stringify(["activity", "act-1"]),
        JSON.stringify(["activities"]),
        JSON.stringify(["activity-locations"]),
      ]),
    );
  });

  it("a right-click opens the same menu, as it always did", () => {
    const { map } = mount();
    act(() => void map.fire("contextmenu", { latlng: L.latLng(55.702, 37.6) }));
    expect(menuButton()?.textContent).toContain("Set as destination point");
    expect(api.setActivityLocationPoint).not.toHaveBeenCalled();
  });

  it("a click off the route opens nothing", () => {
    const { map } = mount();
    // ~1.1 km east of the route.
    act(() => void map.fire("click", { latlng: L.latLng(55.701, 37.62) }));
    act(() => void map.fire("contextmenu", { latlng: L.latLng(55.701, 37.62) }));
    expect(menuButton()).toBeUndefined();
  });

  it("hides the hovered point's readings while the menu is open — they would sit under it", () => {
    const { map } = mount({ trackpoints: { ...track, distance_m: [0, 111, 222], hr: [140, 150, 160] } });
    // The click lands the hover on the point and opens the menu there.
    act(() => void map.fire("click", { latlng: L.latLng(55.701, 37.6) }));
    expect(useActivityStore.getState().hoveredPointIndex).toBe(1);
    expect(menuButton()).toBeDefined();
    const readings = () =>
      Array.from(document.querySelectorAll(".leaflet-tooltip")).some((t) => t.textContent?.includes("150 bpm"));
    expect(readings()).toBe(false);
    // The dot itself stays.
    expect(document.querySelectorAll("path.leaflet-interactive").length).toBeGreaterThan(0);
    // Only under the menu: the hover elsewhere (from the chart, say) reads on.
    act(() => useActivityStore.setState({ hoveredPointIndex: 2 }));
    expect(Array.from(document.querySelectorAll(".leaflet-tooltip")).some((t) => t.textContent?.includes("160 bpm"))).toBe(true);
    act(() => useActivityStore.setState({ hoveredPointIndex: 1 }));
    expect(readings()).toBe(false);

    act(() => void map.closePopup());
    expect(menuButton()).toBeUndefined();
    expect(readings()).toBe(true);
  });

  it("a second click on the same point reopens the menu Leaflet closed on the click", () => {
    const { map } = mount();
    const at = L.latLng(55.7, 37.6);
    act(() => void map.fire("click", { latlng: at }));
    expect(menuButton()).toBeDefined();
    // What one real click does, synchronously in one DOM event: the map's
    // preclick closes the open popup (its onClose empties the menu state),
    // then the click handler opens it again — both in one React batch.
    act(() => {
      map.fire("preclick", { latlng: at });
      map.fire("click", { latlng: at });
    });
    expect(menuButton()).toBeDefined();
    expect(document.querySelectorAll(".leaflet-popup")).toHaveLength(1);
  });

  it("a menu opened on another point carries neither the last refusal nor the last save's close", async () => {
    // A refusal on A, then a menu on B: B's menu is clean.
    vi.mocked(api.setActivityLocationPoint).mockRejectedValueOnce("invalid coordinates: 55.7, 37.6");
    const { map } = mount();
    act(() => void map.fire("click", { latlng: L.latLng(55.7, 37.6) }));
    fireEvent.click(menuButton()!);
    await waitFor(() => expect(document.querySelector(".leaflet-popup")?.textContent).toContain("invalid coordinates"));
    act(() => {
      map.fire("preclick", { latlng: L.latLng(55.702, 37.6) });
      map.fire("click", { latlng: L.latLng(55.702, 37.6) });
    });
    expect(document.querySelector(".leaflet-popup")?.textContent).not.toContain("invalid coordinates");

    // A save of B in flight, then a menu on C: C stays open when B's save
    // lands, and its item comes back to life.
    let settle!: (r: { geocoded: boolean; geocoding_off: boolean; location_name: string }) => void;
    vi.mocked(api.setActivityLocationPoint).mockImplementationOnce(
      () => new Promise((resolve) => (settle = resolve)),
    );
    fireEvent.click(menuButton()!);
    await waitFor(() => expect(menuButton()?.textContent).toContain("Saving"));
    act(() => {
      map.fire("preclick", { latlng: L.latLng(55.701, 37.6) });
      map.fire("click", { latlng: L.latLng(55.701, 37.6) });
    });
    expect(menuButton()).toBeDefined();
    settle({ geocoded: true, geocoding_off: false, location_name: "B" });
    await waitFor(() => expect(addToast).toHaveBeenCalledWith("success", "Destination point: B"));
    expect(menuButton()).toBeDefined();
    await waitFor(() => expect(menuButton()!.disabled).toBe(false));
    expect(menuButton()!.textContent).toContain("Set as destination point");

    // A save of C in flight, a menu on D, and C is REFUSED: the refusal
    // stays out of D's menu (reset could not run — the save was in flight).
    let refuse!: (reason: string) => void;
    vi.mocked(api.setActivityLocationPoint).mockImplementationOnce(
      () => new Promise((_resolve, reject) => (refuse = reject)),
    );
    fireEvent.click(menuButton()!);
    await waitFor(() => expect(menuButton()?.textContent).toContain("Saving"));
    act(() => {
      map.fire("preclick", { latlng: L.latLng(55.7, 37.6) });
      map.fire("click", { latlng: L.latLng(55.7, 37.6) });
    });
    refuse("boom-refusal");
    await waitFor(() => expect(menuButton()!.disabled).toBe(false));
    expect(document.querySelector(".leaflet-popup")?.textContent).not.toContain("boom-refusal");
  });

  it("the item is disabled while the save is in flight", async () => {
    let settle!: (r: { geocoded: boolean; geocoding_off: boolean; location_name: string }) => void;
    vi.mocked(api.setActivityLocationPoint).mockImplementation(
      () => new Promise((resolve) => (settle = resolve)),
    );
    const { map } = mount();
    act(() => void map.fire("click", { latlng: L.latLng(55.7, 37.6) }));
    fireEvent.click(menuButton()!);
    await waitFor(() => expect(menuButton()?.textContent).toContain("Saving"));
    expect(menuButton()!.disabled).toBe(true);
    fireEvent.click(menuButton()!);
    expect(api.setActivityLocationPoint).toHaveBeenCalledTimes(1);

    settle({ geocoded: false, geocoding_off: true, location_name: "55.70000, 37.60000" });
    await waitFor(() => expect(addToast).toHaveBeenCalledWith("success", "Destination point: 55.70000, 37.60000"));
  });

  it("warns when the lookup found no name and the point was saved as coordinates", async () => {
    vi.mocked(api.setActivityLocationPoint).mockResolvedValue({
      geocoded: false,
      geocoding_off: false,
      location_name: "55.70000, 37.60000",
    });
    const { map } = mount();
    act(() => void map.fire("click", { latlng: L.latLng(55.7, 37.6) }));
    fireEvent.click(menuButton()!);
    await waitFor(() =>
      expect(addToast).toHaveBeenCalledWith(
        "warning",
        "Destination point saved as coordinates (55.70000, 37.60000): no place name found.",
      ),
    );
  });

  it("a click on the flag itself opens no menu — it is a mark, not a control", () => {
    const { map } = mount({ location: [55.701, 37.6], locationName: "Home" });
    const flag = flagOf(map)!;
    // Real DOM events on the flag's icon, through Leaflet's own dispatch:
    // the marker's handlers first, then — unless stopped — the map's, with
    // the marker's latlng, which would snap the menu onto the flag.
    for (const type of ["click", "contextmenu"]) {
      act(() => void flag.getElement()!.dispatchEvent(new MouseEvent(type, { bubbles: true })));
      expect(menuButton(), type).toBeUndefined();
    }
  });

  it("a refused save stays in the menu in the backend's words", async () => {
    // Tauri refusals arrive as bare strings.
    vi.mocked(api.setActivityLocationPoint).mockRejectedValue("invalid coordinates: 55.7, 37.6");
    const { map } = mount();
    act(() => void map.fire("click", { latlng: L.latLng(55.7, 37.6) }));
    fireEvent.click(menuButton()!);
    await waitFor(() =>
      expect(document.querySelector(".leaflet-popup")?.textContent).toContain("invalid coordinates: 55.7, 37.6"),
    );
    expect(addToast).not.toHaveBeenCalled();
  });

  it("shows the destination flag on the location point with its name, above the route", () => {
    const { map } = mount({ location: [55.702, 37.6], locationName: "Mahmutlar" });
    const flag = flagOf(map);
    expect(flag).toBeDefined();
    expect(flag!.getLatLng()).toEqual(L.latLng(55.702, 37.6));
    expect(flag!.options.zIndexOffset).toBe(1000);
    const tip = flag!.getTooltip()?.getContent() as HTMLElement;
    expect(tip.textContent).toBe("Destination · Mahmutlar");
    // Start and finish keep their own markers: the flag is a third one.
    expect(markers(map)).toHaveLength(3);
  });

  it("names a flag without a location name plainly", () => {
    const { map } = mount({ location: [55.7, 37.6], locationName: null });
    const tip = flagOf(map)!.getTooltip()?.getContent() as HTMLElement;
    expect(tip.textContent).toBe("Destination point");
  });

  it("shows no flag for a location off the route or no location at all", () => {
    expect(flagOf(mount({ location: [55.701, 37.62], locationName: "Alanya" }).map)).toBeUndefined();
    cleanup();
    // 75 m off a vertex: past the click's own snap radius.
    expect(flagOf(mount({ location: [55.701, 37.6012], locationName: "Café" }).map)).toBeUndefined();
    cleanup();
    // 31 m beside the track, as a road-level point sits (#185): flagged.
    expect(flagOf(mount({ location: [55.701, 37.6005], locationName: "Yol" }).map)).toBeDefined();
    cleanup();
    expect(flagOf(mount({ location: null }).map)).toBeUndefined();
  });

  it("moves the flag when the activity's point changes", () => {
    const { map, rerender } = mount({ location: [55.7, 37.6] });
    expect(flagOf(map)!.getLatLng()).toEqual(L.latLng(55.7, 37.6));
    const qc = new QueryClient();
    rerender(
      <QueryClientProvider client={qc}>
        <RouteMap trackpoints={track} sport="ride" activityId="act-1" location={[55.702, 37.6]} />
      </QueryClientProvider>,
    );
    expect(flagOf(map)!.getLatLng()).toEqual(L.latLng(55.702, 37.6));
  });
});

describe("a simulated course (#190)", () => {
  it("draws no map for a virtual ride, only the notice — the track would land on a real island", () => {
    const mapSpy = vi.spyOn(L, "map");
    const before = mapSpy.mock.calls.length;
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const { getByText } = render(
      <QueryClientProvider client={qc}>
        <RouteMap trackpoints={track} sport="virtual_ride" activityId="act-1" location={[-11.64, 166.95]} locationName="Watopia" />
      </QueryClientProvider>,
    );
    getByText(/Virtual ride — the course is simulated/);
    expect(mapSpy.mock.calls.length).toBe(before);
  });

  it("names the sport in the notice — a virtual run is no ride (#192)", () => {
    const mapSpy = vi.spyOn(L, "map");
    const before = mapSpy.mock.calls.length;
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const { getByText } = render(
      <QueryClientProvider client={qc}>
        <RouteMap trackpoints={track} sport="virtual_run" activityId="act-1" />
      </QueryClientProvider>,
    );
    getByText("Virtual run — the course is simulated, no map");
    expect(mapSpy.mock.calls.length).toBe(before);
  });

  it("survives the sport changing under it — ride to virtual ride and back, no remount", () => {
    const mapSpy = vi.spyOn(L, "map");
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const ui = (sport: string) => (
      <QueryClientProvider client={qc}>
        <RouteMap trackpoints={track} sport={sport} activityId="act-1" />
      </QueryClientProvider>
    );
    const { rerender, getByText, queryByText } = render(ui("ride"));
    const maps = mapSpy.mock.calls.length;
    expect(maps).toBeGreaterThan(0);
    rerender(ui("virtual_ride"));
    getByText(/Virtual ride — the course is simulated/);
    rerender(ui("ride"));
    expect(queryByText(/Virtual ride/)).toBeNull();
    expect(mapSpy.mock.calls.length).toBe(maps + 1);
  });

  it("an indoor ride without a track keeps the plain notice", () => {
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const empty = { ...track, lat: [null, null, null], lon: [null, null, null] } as typeof track;
    const { getByText } = render(
      <QueryClientProvider client={qc}>
        <RouteMap trackpoints={empty} sport="indoor_ride" activityId="act-1" />
      </QueryClientProvider>,
    );
    getByText(/Indoor activity — no route data/);
  });
});
