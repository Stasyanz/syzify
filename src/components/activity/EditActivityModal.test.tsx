// @vitest-environment happy-dom
import { describe, it, expect, afterEach, vi } from "vitest";
import { render, cleanup, waitFor, fireEvent } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { Activity, Tag } from "../../lib/types";

const { tags } = vi.hoisted(() => ({
  tags: [
    { id: 1, name: "tag-a" },
    { id: 2, name: "tag-b" },
    { id: 3, name: "tag-c" },
    { id: 4, name: "tag-d" },
    { id: 5, name: "tag-e" },
  ] as Tag[],
}));

const addToast = vi.fn();

vi.mock("../../lib/tauri", () => ({
  api: {
    getTags: vi.fn().mockResolvedValue(tags),
    createTag: vi.fn(),
    updateActivity: vi.fn().mockResolvedValue(undefined),
    setActivityTags: vi.fn().mockResolvedValue(undefined),
    updateActivityLocation: vi.fn().mockResolvedValue({ geocoded: true, geocoding_off: false, location_name: "" }),
    searchLocations: vi.fn().mockResolvedValue([]),
    setActivityLocationNamed: vi.fn().mockResolvedValue({ geocoded: true, geocoding_off: false, location_name: "" }),
    deleteActivity: vi.fn().mockResolvedValue(undefined),
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
        currentTags={[]}
        onClose={() => {}}
        onSaved={() => {}}
        onDeleted={() => {}}
      />
    </QueryClientProvider>
  );
}

describe("EditActivityModal tag cap", () => {
  it("blocks selecting a 4th tag", async () => {
    const { getByRole, getByPlaceholderText } = renderModal();

    await waitFor(() => getByRole("button", { name: "tag-a" }));

    // Select three tags.
    fireEvent.click(getByRole("button", { name: "tag-a" }));
    fireEvent.click(getByRole("button", { name: "tag-b" }));
    fireEvent.click(getByRole("button", { name: "tag-c" }));

    // The remaining (unselected) tags are now disabled, and so is the new-tag
    // input — you can't add a 4th.
    await waitFor(() => {
      expect((getByRole("button", { name: "tag-d" }) as HTMLButtonElement).disabled).toBe(true);
      expect((getByRole("button", { name: "tag-e" }) as HTMLButtonElement).disabled).toBe(true);
    });
    expect((getByPlaceholderText("Up to 3 tags selected") as HTMLInputElement).disabled).toBe(true);

    // Clicking a disabled tag does nothing; deselecting one frees a slot again.
    fireEvent.click(getByRole("button", { name: "tag-d" }));
    expect((getByRole("button", { name: "tag-d" }) as HTMLButtonElement).disabled).toBe(true);

    fireEvent.click(getByRole("button", { name: "tag-a" })); // deselect a-tag
    await waitFor(() => {
      expect((getByRole("button", { name: "tag-d" }) as HTMLButtonElement).disabled).toBe(false);
    });
  });
});

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
        <EditActivityModal activity={saved} currentTags={[]} onClose={() => {}} onSaved={() => {}} onDeleted={() => {}} />
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
