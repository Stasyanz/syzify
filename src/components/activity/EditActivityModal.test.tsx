// @vitest-environment happy-dom
import { describe, it, expect, afterEach, beforeEach, vi } from "vitest";
import { render, cleanup, waitFor, fireEvent, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { Activity, GearItem } from "../../lib/types";

const addToast = vi.fn();

vi.mock("../../lib/tauri", () => ({
  api: {
    updateActivity: vi.fn().mockResolvedValue(undefined),
    updateActivityLocation: vi.fn().mockResolvedValue({ geocoded: true, geocoding_off: false, location_name: "" }),
    searchLocations: vi.fn().mockResolvedValue([]),
    setActivityLocationNamed: vi.fn().mockResolvedValue({ geocoded: true, geocoding_off: false, location_name: "" }),
    setActivityFtp: vi.fn().mockResolvedValue({ threshold_power_w: 238, intensity_factor: 0.7, training_stress_score: 60 }),
    deleteActivity: vi.fn().mockResolvedValue(undefined),
    listGear: vi.fn().mockResolvedValue([]),
    setActivityGear: vi.fn().mockResolvedValue(undefined),
  },
}));
vi.mock("../../stores/toastStore", () => ({
  useToastStore: (sel: (s: { addToast: typeof addToast }) => unknown) => sel({ addToast }),
}));

import { EditActivityModal, stepHighlight, LOCATION_SEARCH_DEBOUNCE_MS } from "./EditActivityModal";
import { api } from "../../lib/tauri";
import type { LocationHit } from "../../lib/types";

afterEach(() => {
  cleanup();
  addToast.mockClear();
});

const activity = { id: "act-1", title: "Run", notes: null, sport_type: "run", location_name: null } as Activity;

function renderModal() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <EditActivityModal
        activity={activity}
        onClose={() => {}}
        onSaved={() => {}}
        onDeleted={() => {}}
      />
    </QueryClientProvider>
  );
}

describe("location suggestions", () => {
  const mahmutlar: LocationHit = { name: "Mahmutlar", detail: "Alanya, Türkiye", lat: 36.49, lon: 32.09, kind: "suburb" };
  const bursa: LocationHit = { name: "Mahmutlar", detail: "Bursa, Türkiye", lat: 40.1, lon: 29.2, kind: "village" };

  afterEach(() => {
    vi.useRealTimers();
    vi.mocked(api.searchLocations).mockReset().mockResolvedValue([]);
    vi.mocked(api.setActivityLocationNamed).mockClear();
    vi.mocked(api.updateActivityLocation).mockClear();
  });

  /** Type into the field and let the debounce elapse under fake timers. */
  function type(input: HTMLElement, value: string) {
    vi.useFakeTimers();
    fireEvent.change(input, { target: { value } });
    vi.advanceTimersByTime(LOCATION_SEARCH_DEBOUNCE_MS);
    vi.useRealTimers();
  }

  it("steps the highlight with wrap-around, from nothing to either end", () => {
    expect(stepHighlight(-1, 3, 1)).toBe(0);
    expect(stepHighlight(-1, 3, -1)).toBe(2);
    expect(stepHighlight(2, 3, 1)).toBe(0);
    expect(stepHighlight(0, 3, -1)).toBe(2);
    expect(stepHighlight(1, 3, 1)).toBe(2);
    expect(stepHighlight(0, 0, 1)).toBe(-1);
  });

  it("asks only after a pause and three characters, lists the hits, and a keyboard pick saves the coordinates", async () => {
    vi.mocked(api.searchLocations).mockResolvedValue([mahmutlar, bursa]);
    const { getByPlaceholderText, getByRole, queryByRole, getByText } = renderModal();
    const input = getByPlaceholderText("City, address...");

    // Two characters: no request, whatever the pause.
    type(input, "ma");
    expect(api.searchLocations).not.toHaveBeenCalled();
    expect(queryByRole("listbox")).toBeNull();

    // Three characters, no pause yet: still nothing; after the pause: one request.
    vi.useFakeTimers();
    fireEvent.change(input, { target: { value: "mah" } });
    vi.advanceTimersByTime(LOCATION_SEARCH_DEBOUNCE_MS - 1);
    expect(api.searchLocations).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    vi.useRealTimers();
    expect(api.searchLocations).toHaveBeenCalledTimes(1);
    expect(api.searchLocations).toHaveBeenCalledWith("mah");

    await waitFor(() => expect(getByRole("listbox")).toBeTruthy());
    const options = getByRole("listbox").querySelectorAll("[role=option]");
    expect(options).toHaveLength(2);
    expect(options[1].textContent).toContain("Bursa, Türkiye");
    expect(input.getAttribute("aria-expanded")).toBe("true");

    // Hovering highlights; a click outside the field closes the list, a
    // click inside keeps it.
    fireEvent.mouseEnter(options[0]);
    expect(options[0].getAttribute("aria-selected")).toBe("true");
    fireEvent.mouseDown(input);
    expect(queryByRole("listbox")).not.toBeNull();
    fireEvent.mouseDown(document.body);
    expect(queryByRole("listbox")).toBeNull();
    // Reopen by typing on, then drive it from the keyboard: Down twice
    // lands on the second namesake; Enter picks it.
    type(input, "mahm");
    await waitFor(() => expect(getByRole("listbox")).toBeTruthy());
    fireEvent.keyDown(input, { key: "ArrowDown" });
    fireEvent.keyDown(input, { key: "ArrowDown" });
    expect(getByRole("listbox").querySelectorAll("[aria-selected=true]")[0].textContent).toContain("Bursa");
    fireEvent.keyDown(input, { key: "Enter" });
    expect((input as HTMLInputElement).value).toBe("Mahmutlar");
    expect(queryByRole("listbox")).toBeNull();
    // The pick is not a new query.
    await new Promise((r) => setTimeout(r, LOCATION_SEARCH_DEBOUNCE_MS + 50));
    expect(api.searchLocations).toHaveBeenCalledTimes(2);

    // Save writes the picked coordinates and never geocodes the text again.
    fireEvent.click(getByText("Save"));
    await waitFor(() => expect(api.setActivityLocationNamed).toHaveBeenCalledWith("act-1", "Mahmutlar", 40.1, 29.2));
    expect(api.updateActivityLocation).not.toHaveBeenCalled();
  });

  it("drops a slow earlier answer, closes on Escape, and typing on after a pick geocodes the text instead", async () => {
    let resolveFirst: (v: LocationHit[]) => void = () => {};
    let resolveSecond: (v: LocationHit[]) => void = () => {};
    vi.mocked(api.searchLocations)
      .mockImplementationOnce(() => new Promise((r) => (resolveFirst = r)))
      .mockImplementationOnce(() => new Promise((r) => (resolveSecond = r)));
    const { getByPlaceholderText, getByRole, queryByRole, getByText } = renderModal();
    const input = getByPlaceholderText("City, address...");

    type(input, "mah");
    type(input, "mahm");
    expect(api.searchLocations).toHaveBeenCalledTimes(2);
    // The newer answer arrives first, then the stale one — which is ignored.
    resolveSecond([bursa]);
    await waitFor(() => expect(getByRole("listbox").textContent).toContain("Bursa"));
    resolveFirst([mahmutlar]);
    await new Promise((r) => setTimeout(r, 20));
    expect(getByRole("listbox").querySelectorAll("[role=option]")).toHaveLength(1);
    expect(getByRole("listbox").textContent).toContain("Bursa");

    fireEvent.keyDown(input, { key: "Escape" });
    expect(queryByRole("listbox")).toBeNull();

    // A mouse pick, then more typing: the pick no longer applies.
    vi.mocked(api.searchLocations).mockResolvedValueOnce([bursa]);
    type(input, "mahmu");
    await waitFor(() => expect(getByRole("listbox")).toBeTruthy());
    fireEvent.mouseDown(getByRole("listbox").querySelector("[role=option]")!);
    expect((input as HTMLInputElement).value).toBe("Mahmutlar");
    fireEvent.change(input, { target: { value: "Mahmutlar beach" } });
    vi.mocked(api.updateActivityLocation).mockResolvedValueOnce({
      geocoded: false,
      geocoding_off: false,
      location_name: "Mahmutlar beach",
    });
    fireEvent.click(getByText("Save"));
    await waitFor(() => expect(api.updateActivityLocation).toHaveBeenCalledWith("act-1", "Mahmutlar beach"));
    expect(api.setActivityLocationNamed).not.toHaveBeenCalled();
    // The old path's own warning when the text could not be geocoded.
    await waitFor(() => expect(addToast).toHaveBeenCalledWith("warning", expect.stringContaining("map point is unchanged")));
    // Geocoding switched off in Settings is a choice: no warning for it.
    addToast.mockClear();
    vi.mocked(api.updateActivityLocation).mockResolvedValueOnce({ geocoded: false, geocoding_off: true, location_name: "x" });
    fireEvent.change(input, { target: { value: "Mahmutlar beach 2" } });
    fireEvent.click(getByText("Save"));
    await waitFor(() => expect(api.updateActivityLocation).toHaveBeenCalledWith("act-1", "Mahmutlar beach 2"));
    await new Promise((r) => setTimeout(r, 20));
    expect(addToast).not.toHaveBeenCalledWith("warning", expect.anything());
  });

  it("retyping the saved name lists its namesakes, and picking one with the same name still saves the coordinates", async () => {
    vi.mocked(api.searchLocations).mockResolvedValue([mahmutlar, bursa]);
    const saved = { ...activity, location_name: "Mahmutlar", start_lat: 36.49, start_lon: 32.09 } as Activity;
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const { getByPlaceholderText, getByRole, queryByRole, getByText } = render(
      <QueryClientProvider client={qc}>
        <EditActivityModal activity={saved} onClose={() => {}} onSaved={() => {}} onDeleted={() => {}} />
      </QueryClientProvider>,
    );
    const input = getByPlaceholderText("City, address...") as HTMLInputElement;
    expect(input.value).toBe("Mahmutlar");
    // Opening the modal on a saved name asks nothing.
    await new Promise((r) => setTimeout(r, 30));
    expect(api.searchLocations).not.toHaveBeenCalled();
    expect(queryByRole("listbox")).toBeNull();
    // Retyping the very same name is a request — the point is its namesakes.
    type(input, "Mahmutla");
    type(input, "Mahmutlar");
    await waitFor(() => expect(getByRole("listbox")).toBeTruthy());
    fireEvent.keyDown(input, { key: "ArrowDown" });
    fireEvent.keyDown(input, { key: "ArrowDown" });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(input.value).toBe("Mahmutlar");
    fireEvent.click(getByText("Save"));
    await waitFor(() => expect(api.setActivityLocationNamed).toHaveBeenCalledWith("act-1", "Mahmutlar", 40.1, 29.2));
    expect(api.updateActivityLocation).not.toHaveBeenCalled();
  });

  it("a superseded query's failure warns nobody, and the spinner lives only while the current answer is awaited", async () => {
    let failFirst: (e: Error) => void = () => {};
    let resolveSecond: (v: LocationHit[]) => void = () => {};
    vi.mocked(api.searchLocations)
      .mockImplementationOnce(() => new Promise((_, reject) => (failFirst = reject)))
      .mockImplementationOnce(() => new Promise((r) => (resolveSecond = r)));
    const { getByPlaceholderText, getByRole, queryByLabelText } = renderModal();
    const input = getByPlaceholderText("City, address...");
    // Nothing spins during the debounce — only once the request is out.
    vi.useFakeTimers();
    fireEvent.change(input, { target: { value: "mah" } });
    expect(queryByLabelText("Searching locations")).toBeNull();
    vi.advanceTimersByTime(LOCATION_SEARCH_DEBOUNCE_MS);
    vi.useRealTimers();
    await waitFor(() => expect(queryByLabelText("Searching locations")).not.toBeNull());
    // Typing on supersedes the request: nothing is in flight for the new
    // text during its pause, so the spinner rests until the next request.
    vi.useFakeTimers();
    fireEvent.change(input, { target: { value: "mahm" } });
    expect(queryByLabelText("Searching locations")).toBeNull();
    vi.advanceTimersByTime(LOCATION_SEARCH_DEBOUNCE_MS);
    vi.useRealTimers();
    await waitFor(() => expect(api.searchLocations).toHaveBeenCalledTimes(2));
    expect(queryByLabelText("Searching locations")).not.toBeNull();
    expect(queryByLabelText("Searching locations")!.getAttribute("role")).toBe("status");
    resolveSecond([bursa]);
    await waitFor(() => expect(getByRole("listbox")).toBeTruthy());
    expect(queryByLabelText("Searching locations")).toBeNull();
    failFirst(new Error("late failure"));
    await new Promise((r) => setTimeout(r, 20));
    expect(addToast).not.toHaveBeenCalled();
    expect(getByRole("listbox").textContent).toContain("Bursa");
  });

  it("says 'No matches' under the field for an empty answer, and drops it on the next keystroke", async () => {
    vi.mocked(api.searchLocations).mockResolvedValueOnce([]).mockResolvedValueOnce([bursa]);
    const { getByPlaceholderText, queryByText, getByRole } = renderModal();
    const input = getByPlaceholderText("City, address...");
    type(input, "cebeci 6 sitesi");
    await waitFor(() => expect(queryByText(/No matches/)).not.toBeNull());
    expect(addToast).not.toHaveBeenCalled();
    // Typing on withdraws the hint at once; a hit replaces it with the list.
    vi.useFakeTimers();
    fireEvent.change(input, { target: { value: "cebeci" } });
    expect(queryByText(/No matches/)).toBeNull();
    vi.advanceTimersByTime(LOCATION_SEARCH_DEBOUNCE_MS);
    vi.useRealTimers();
    await waitFor(() => expect(getByRole("listbox")).toBeTruthy());
    expect(queryByText(/No matches/)).toBeNull();
  });

  it("warns once when the search fails, shows no list, and warns again only after a search got through", async () => {
    vi.mocked(api.searchLocations).mockRejectedValue(new Error("Nominatim request failed"));
    const { getByPlaceholderText, queryByRole, queryByText } = renderModal();
    const input = getByPlaceholderText("City, address...");
    type(input, "mah");
    await waitFor(() => expect(addToast).toHaveBeenCalledTimes(1));
    expect(addToast).toHaveBeenCalledWith("warning", expect.stringContaining("no network"));
    expect(queryByRole("listbox")).toBeNull();
    expect(queryByText(/No matches/)).toBeNull();
    // The next keystroke fails too: no second toast.
    type(input, "mahm");
    await waitFor(() => expect(api.searchLocations).toHaveBeenCalledTimes(2));
    await new Promise((r) => setTimeout(r, 20));
    expect(addToast).toHaveBeenCalledTimes(1);
    // A search that gets through re-arms the warning.
    vi.mocked(api.searchLocations).mockResolvedValueOnce([]).mockRejectedValue(new Error("down again"));
    type(input, "mahmu");
    await waitFor(() => expect(api.searchLocations).toHaveBeenCalledTimes(3));
    type(input, "mahmut");
    await waitFor(() => expect(addToast).toHaveBeenCalledTimes(2));
  });
});

describe("FTP correction", () => {
  afterEach(() => vi.mocked(api.setActivityFtp).mockClear());

  function renderWith(over: Partial<Activity>) {
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    return render(
      <QueryClientProvider client={qc}>
        <EditActivityModal
          activity={{ ...activity, ...over } as Activity}
          onClose={() => {}}
          onSaved={() => {}}
          onDeleted={() => {}}
        />
      </QueryClientProvider>,
    );
  }

  it("offers the field only for activities with normalized power, prefilled with the file's FTP", () => {
    const none = renderWith({ normalized_power_w: null, threshold_power_w: null });
    expect(none.queryByLabelText("FTP (W)")).toBeNull();
    cleanup();
    const ride = renderWith({ normalized_power_w: 159, threshold_power_w: 200, duration_s: 4800 });
    expect((ride.getByLabelText("FTP (W)") as HTMLInputElement).value).toBe("200");
    expect((ride.getByLabelText("FTP (W)") as HTMLInputElement).disabled).toBe(false);
    cleanup();
    // Power but no duration: shown, disabled, and it says why.
    const noDur = renderWith({ normalized_power_w: 159, threshold_power_w: 200, duration_s: null });
    expect((noDur.getByLabelText("FTP (W)") as HTMLInputElement).disabled).toBe(true);
    expect(noDur.getByText(/no recorded duration/)).toBeTruthy();
  });

  it("opens on the FTP field when asked to, but never focuses a disabled one", () => {
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const mount = (over: Partial<Activity>) =>
      render(
        <QueryClientProvider client={qc}>
          <EditActivityModal
            activity={{ ...activity, ...over } as Activity}
            onClose={() => {}}
            onSaved={() => {}}
            onDeleted={() => {}}
            focusFtp
          />
        </QueryClientProvider>,
      );
    const ok = mount({ normalized_power_w: 159, threshold_power_w: 200, duration_s: 4800 });
    expect(document.activeElement).toBe(ok.getByLabelText("FTP (W)"));
    cleanup();
    const off = mount({ normalized_power_w: 159, threshold_power_w: 200, duration_s: null });
    expect(document.activeElement).not.toBe(off.getByLabelText("FTP (W)"));
  });

  // A Tauri command refuses with a bare string, not an Error: the toast
  // must carry its words, not "undefined" (#174).
  it("says a refused save and a refused delete in the backend's words", async () => {
    vi.mocked(api.updateActivity).mockRejectedValueOnce("The vault is locked");
    const { getByText } = renderWith({});
    fireEvent.click(getByText("Save"));
    await waitFor(() => expect(addToast).toHaveBeenCalledWith("error", "Failed to update: The vault is locked"));

    vi.mocked(api.deleteActivity).mockRejectedValueOnce("The vault is locked");
    fireEvent.click(getByText("Delete"));
    fireEvent.click(getByText("Confirm delete"));
    await waitFor(() => expect(addToast).toHaveBeenCalledWith("error", "Failed to delete: The vault is locked"));
  });

  it("reports a refused FTP by name and still saves the rest", async () => {
    vi.mocked(api.setActivityFtp).mockRejectedValueOnce("this activity has no normalized power, so IF and TSS cannot be recomputed");
    const { getByLabelText, getByText } = renderWith({ normalized_power_w: 159, threshold_power_w: 200, duration_s: 4800 });
    fireEvent.change(getByLabelText("FTP (W)"), { target: { value: "238" } });
    fireEvent.click(getByText("Save"));
    await waitFor(() => expect(addToast).toHaveBeenCalledWith("error", expect.stringContaining("FTP not changed: this activity has no normalized power")));
    await waitFor(() => expect(addToast).toHaveBeenCalledWith("success", "Activity updated (FTP unchanged)"));
  });

  it("saves a changed FTP through the dedicated command, and an unchanged or empty one not at all", async () => {
    const { getByLabelText, getByText } = renderWith({ normalized_power_w: 159, threshold_power_w: 200, duration_s: 4800 });
    const input = getByLabelText("FTP (W)") as HTMLInputElement;
    // Unchanged: no call.
    fireEvent.click(getByText("Save"));
    await waitFor(() => expect(api.updateActivity).toHaveBeenCalled());
    expect(api.setActivityFtp).not.toHaveBeenCalled();
    // Changed: one call with the number.
    fireEvent.change(input, { target: { value: "238" } });
    fireEvent.click(getByText("Save"));
    await waitFor(() => expect(api.setActivityFtp).toHaveBeenCalledWith("act-1", 238));
    // Emptied: nothing written.
    vi.mocked(api.setActivityFtp).mockClear();
    fireEvent.change(input, { target: { value: "" } });
    fireEvent.click(getByText("Save"));
    await new Promise((r) => setTimeout(r, 30));
    expect(api.setActivityFtp).not.toHaveBeenCalled();
  });
});

describe("gear", () => {
  const item = (id: string, kind: GearItem["kind"], retired = false): GearItem => ({
    id,
    kind,
    name: id,
    brand: null,
    model: null,
    purchased_at: null,
    initial_distance_m: 0,
    distance_limit_m: null,
    retired_at: retired ? "2026-01-01T00:00:00" : null,
    notes: null,
    created_at: "2026-01-01T00:00:00",
    stats: { activities: 0, distance_m: 0, duration_s: 0, elev_gain_m: 0, last_used: null },
    default_for: [],
    rules: [],
  });
  const registry = [item("road", "bike"), item("old-road", "bike", true), item("pegasus", "shoes"), item("helmet", "other")];

  beforeEach(() => {
    vi.mocked(api.listGear).mockClear().mockResolvedValue(registry);
    vi.mocked(api.setActivityGear).mockClear();
    vi.mocked(api.updateActivity).mockClear();
  });

  function renderGear(props: { gearId?: string | null; gearLocked?: boolean; sport?: string }) {
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    return render(
      <QueryClientProvider client={qc}>
        <EditActivityModal
          activity={{ ...activity, sport_type: props.sport ?? "run" } as Activity}
          gearId={props.gearId ?? null}
          gearLocked={props.gearLocked}
          onClose={() => {}}
          onSaved={() => {}}
          onDeleted={() => {}}
        />
      </QueryClientProvider>,
    );
  }

  it("offers the items that fit the sport, the current one even if retired, and saves only a change", async () => {
    const { getByLabelText, getByRole, getAllByRole, getByText } = renderGear({ gearId: "old-road", sport: "ride" });
    await waitFor(() => expect(getByLabelText("Gear").textContent).toContain("old-road (retired)"));
    fireEvent.click(getByLabelText("Gear"));
    expect(getAllByRole("option").map((o) => o.textContent)).toEqual(["None", "road", "old-road (retired)", "helmet"]);
    // Picking the same item again is no write.
    fireEvent.click(getByRole("option", { name: "old-road (retired)" }));
    fireEvent.click(getByText("Save"));
    await waitFor(() => expect(api.updateActivity).toHaveBeenCalled());
    await new Promise((r) => setTimeout(r, 20));
    expect(api.setActivityGear).not.toHaveBeenCalled();

    fireEvent.click(getByLabelText("Gear"));
    fireEvent.click(getByRole("option", { name: "road" }));
    fireEvent.click(getByText("Save"));
    await waitFor(() => expect(api.setActivityGear).toHaveBeenCalledWith("act-1", "road"));
  });

  it("follows the sport being saved and takes the activity off its gear with None", async () => {
    const { getByLabelText, getByRole, getAllByRole, getByText } = renderGear({ gearId: "pegasus", sport: "run" });
    await waitFor(() => expect(getByLabelText("Gear").textContent).toContain("pegasus"));
    // Switching the sport to a ride offers bikes instead of shoes — the
    // shoes it is on stay offered so a plain save keeps them.
    fireEvent.click(getByLabelText("Sport type"));
    fireEvent.click(getByRole("option", { name: "Ride" }));
    fireEvent.click(getByLabelText("Gear"));
    expect(getAllByRole("option").map((o) => o.textContent)).toEqual(["None", "road", "pegasus", "helmet"]);
    fireEvent.click(getByRole("option", { name: "None" }));
    fireEvent.click(getByText("Save"));
    await waitFor(() => expect(api.setActivityGear).toHaveBeenCalledWith("act-1", null));
  });

  it("says why a multisport event has no gear field and never writes one", async () => {
    const { queryByLabelText, getByText } = renderGear({ gearLocked: true });
    expect(queryByLabelText("Gear")).toBeNull();
    expect(getByText(/A multisport event carries no gear/)).toBeTruthy();
    fireEvent.click(getByText("Save"));
    await waitFor(() => expect(api.updateActivity).toHaveBeenCalled());
    await new Promise((r) => setTimeout(r, 20));
    expect(api.setActivityGear).not.toHaveBeenCalled();
    expect(api.listGear).not.toHaveBeenCalled();
  });
});

describe("gear refusals and sport changes", () => {
  const item = (id: string, kind: GearItem["kind"]): GearItem => ({
    id,
    kind,
    name: id,
    brand: null,
    model: null,
    purchased_at: null,
    initial_distance_m: 0,
    distance_limit_m: null,
    retired_at: null,
    notes: null,
    created_at: "2026-01-01T00:00:00",
    stats: { activities: 0, distance_m: 0, duration_s: 0, elev_gain_m: 0, last_used: null },
    default_for: [],
    rules: [],
  });

  beforeEach(() => {
    vi.mocked(api.listGear).mockClear().mockResolvedValue([item("road", "bike"), item("pegasus", "shoes")]);
    vi.mocked(api.setActivityGear).mockClear();
    vi.mocked(api.updateActivity).mockClear();
    vi.mocked(api.setActivityFtp).mockClear();
  });

  function renderWith(over: Partial<Activity>, gearId: string | null, onSaved = vi.fn()) {
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(
      <QueryClientProvider client={qc}>
        <EditActivityModal
          activity={{ ...activity, ...over } as Activity}
          gearId={gearId}
          onClose={() => {}}
          onSaved={onSaved}
          onDeleted={() => {}}
        />
      </QueryClientProvider>,
    );
    return onSaved;
  }

  it("reports a refused gear by name and still finishes the save, FTP included", async () => {
    vi.mocked(api.setActivityGear).mockRejectedValueOnce("Bring the gear back");
    const onSaved = renderWith(
      { sport_type: "ride", normalized_power_w: 200, threshold_power_w: 200, duration_s: 3600 },
      null,
    );
    fireEvent.click(screen.getByLabelText("Gear"));
    fireEvent.click(await screen.findByRole("option", { name: "road" }));
    fireEvent.change(screen.getByLabelText("FTP (W)"), { target: { value: "238" } });
    fireEvent.click(screen.getByText("Save"));
    await waitFor(() => expect(onSaved).toHaveBeenCalledTimes(1));
    expect(api.updateActivity).toHaveBeenCalledTimes(1);
    expect(api.setActivityFtp).toHaveBeenCalledWith("act-1", 238);
    expect(addToast).toHaveBeenCalledWith("error", "Gear not changed: Bring the gear back");
    expect(addToast).toHaveBeenLastCalledWith("success", "Activity updated (gear unchanged)");
  });

  it("drops a pick the new sport does not offer instead of saving it unseen", async () => {
    renderWith({ sport_type: "ride" }, null);
    fireEvent.click(screen.getByLabelText("Gear"));
    fireEvent.click(await screen.findByRole("option", { name: "road" }));
    expect(screen.getByLabelText("Gear").textContent).toContain("road");
    fireEvent.click(screen.getByLabelText("Sport type"));
    fireEvent.click(screen.getByRole("option", { name: "Run" }));
    expect(screen.getByLabelText("Gear").textContent).toContain("None");
    fireEvent.click(screen.getByText("Save"));
    await waitFor(() => expect(api.updateActivity).toHaveBeenCalled());
    await new Promise((r) => setTimeout(r, 20));
    expect(api.setActivityGear).not.toHaveBeenCalled();
    // The shoes it could take are still a pick away.
    fireEvent.click(screen.getByLabelText("Gear"));
    fireEvent.click(screen.getByRole("option", { name: "pegasus" }));
    fireEvent.click(screen.getByLabelText("Sport type"));
    fireEvent.click(screen.getByRole("option", { name: "Trail Run" }));
    expect(screen.getByLabelText("Gear").textContent).toContain("pegasus");
  });
});
