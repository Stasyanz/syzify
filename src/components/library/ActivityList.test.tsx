// @vitest-environment happy-dom
import { describe, it, expect, afterEach, vi } from "vitest";
import { render, cleanup, fireEvent, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ActivitySummary } from "../../lib/types";
import { api } from "../../lib/tauri";
import { ActivityList } from "./ActivityList";

vi.mock("../../lib/tauri", () => ({
  api: {
    getActivities: vi.fn(),
    getSetting: vi.fn().mockResolvedValue(null),
  },
  isTauri: () => false,
}));

afterEach(cleanup);

const summary = (i: number): ActivitySummary => ({
  id: `a-${i}`,
  start_time: "2026-07-01T08:00:00+00:00",
  sport_type: "run",
  title: `Run #${i}`,
  distance_m: 5000,
  duration_s: 1800,
  elev_gain_m: 50,
  avg_speed_mps: 2.8,
  avg_hr: 150,
  location_name: null,
  tags: [],
});

const page = (n: number) => Array.from({ length: n }, (_, i) => summary(i));

function renderList() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter>
        <ActivityList />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("ActivityList Load more", () => {
  it("keeps the loaded rows mounted while the next page fetches", async () => {
    // First page resolves immediately; the Load more page hangs until we
    // release it — the window where the list used to unmount into
    // "Loading activities..." and dump the scroll position.
    let releaseSecondPage!: (v: ActivitySummary[]) => void;
    vi.mocked(api.getActivities)
      .mockResolvedValueOnce(page(20))
      .mockImplementationOnce(
        () => new Promise((resolve) => (releaseSecondPage = resolve)),
      );

    renderList();
    await waitFor(() => expect(screen.getByText("Run #0")).toBeTruthy());

    fireEvent.click(screen.getByRole("button", { name: "Load more" }));

    // Mid-fetch: rows are STILL in the DOM (no loading placeholder), and the
    // button stays put — disabled, relabeled — instead of blinking away.
    expect(screen.getByText("Run #0")).toBeTruthy();
    expect(screen.queryByText(/Loading activities/)).toBeNull();
    const button = screen.getByRole("button", { name: "Loading…" });
    expect((button as HTMLButtonElement).disabled).toBe(true);

    // A short page (25 < 40 requested) is the LAST page: button disappears.
    releaseSecondPage(page(25));
    await waitFor(() => expect(screen.getByText("Run #24")).toBeTruthy());
    expect(screen.queryByRole("button", { name: /Load more|Loading…/ })).toBeNull();
  });

  it("shows Load more only while a full page came back", async () => {
    vi.mocked(api.getActivities).mockResolvedValue(page(7));
    renderList();
    await waitFor(() => expect(screen.getByText("Run #0")).toBeTruthy());
    // 7 < 20: everything is already loaded, no button.
    expect(screen.queryByRole("button", { name: "Load more" })).toBeNull();
  });
});

describe("ActivityList merge mode (Ctrl+M)", () => {
  it("has no visible Select button; Ctrl+M reveals the merge toolbar", async () => {
    vi.mocked(api.getActivities).mockResolvedValue(page(3));
    renderList();
    await waitFor(() => expect(screen.getByText("Run #0")).toBeTruthy());

    expect(screen.queryByRole("button", { name: "Select" })).toBeNull();
    expect(screen.queryByText(/selected/)).toBeNull();

    fireEvent.keyDown(document.body, { key: "m", ctrlKey: true });
    expect(screen.getByText("0 selected")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Merge into triathlon" })).toBeTruthy();
  });

  it("Ctrl+M again leaves the mode and drops the selection", async () => {
    vi.mocked(api.getActivities).mockResolvedValue(page(3));
    renderList();
    await waitFor(() => expect(screen.getByText("Run #0")).toBeTruthy());

    fireEvent.keyDown(document.body, { key: "m", ctrlKey: true });
    fireEvent.click(screen.getByText("Run #1"));
    expect(screen.getByText("1 selected")).toBeTruthy();

    fireEvent.keyDown(document.body, { key: "m", ctrlKey: true });
    expect(screen.queryByText(/selected/)).toBeNull();
    // Re-entering starts from a clean selection.
    fireEvent.keyDown(document.body, { key: "m", ctrlKey: true });
    expect(screen.getByText("0 selected")).toBeTruthy();
  });

  it("plain M or Cmd+M does not enter the mode", async () => {
    vi.mocked(api.getActivities).mockResolvedValue(page(3));
    renderList();
    await waitFor(() => expect(screen.getByText("Run #0")).toBeTruthy());

    fireEvent.keyDown(document.body, { key: "m" });
    fireEvent.keyDown(document.body, { key: "m", metaKey: true });
    fireEvent.keyDown(document.body, { key: "m", ctrlKey: true, metaKey: true });
    expect(screen.queryByText(/selected/)).toBeNull();
  });
});
