// @vitest-environment happy-dom
import { describe, it, expect, afterEach, beforeEach, vi } from "vitest";
import { render, cleanup, fireEvent, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, Route, Routes } from "react-router";
import { api } from "../lib/tauri";
import type { SegmentLeaderboardRow, SegmentSummaryRow } from "../lib/types";
import { Segments, filterSegments } from "./Segments";

vi.mock("../lib/tauri", () => ({
  api: {
    listSegments: vi.fn(),
    renameSegment: vi.fn(),
    deleteSegment: vi.fn(),
    getSegmentEfforts: vi.fn(),
  },
}));

const confirmMock = vi.fn<(opts: unknown) => Promise<boolean>>();
vi.mock("../stores/confirmStore", () => ({
  confirmDialog: (opts: unknown) => confirmMock(opts),
}));

const mocked = vi.mocked(api);

function summary(over: Partial<SegmentSummaryRow> = {}): SegmentSummaryRow {
  return {
    id: "seg-1",
    name: "Siedra from Damlataş",
    sport: "ride",
    distance_m: 3145.1,
    avg_grade_pct: 6.94,
    elev_delta_m: 218.2,
    created_at: "2026-08-24T10:00:00Z",
    effort_count: 2,
    best_elapsed_s: 1406,
    best_effort_power_w: null,
    ...over,
  };
}

function lb(over: Partial<SegmentLeaderboardRow> = {}): SegmentLeaderboardRow {
  return {
    id: 1,
    activity_id: "act-1",
    activity_title: "Siedra",
    start_time: "2024-09-19T07:59:22+03:00",
    distance_m: 3111.9,
    elapsed_s: 1406,
    avg_power_w: null,
    rank: 1,
    ...over,
  };
}

afterEach(cleanup);
beforeEach(() => {
  vi.clearAllMocks();
  mocked.listSegments.mockResolvedValue([summary()]);
  mocked.getSegmentEfforts.mockResolvedValue([]);
  mocked.renameSegment.mockResolvedValue(undefined);
  mocked.deleteSegment.mockResolvedValue(undefined);
  confirmMock.mockResolvedValue(true);
});

async function renderPage() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={qc}>
      <MemoryRouter>
        <Segments />
      </MemoryRouter>
    </QueryClientProvider>,
  );
  await screen.findByText("Segments");
}

describe("filterSegments", () => {
  const rows = [
    summary({ id: "a", name: "Rock - Stream climb" }),
    summary({ id: "b", name: "D400-Sapadere" }),
    summary({ id: "c", name: "Siedra from Damlataş" }),
  ];

  it("matches case-insensitive substrings and trims the query", () => {
    expect(filterSegments(rows, "sapa").map((s) => s.id)).toEqual(["b"]);
    expect(filterSegments(rows, "  ROCK ").map((s) => s.id)).toEqual(["a"]);
  });

  it("empty query returns the list untouched", () => {
    expect(filterSegments(rows, "")).toBe(rows);
    expect(filterSegments(rows, "   ")).toBe(rows);
  });

  it("handles non-ASCII names", () => {
    expect(filterSegments(rows, "damlataş").map((s) => s.id)).toEqual(["c"]);
  });
});

describe("Segments page", () => {
  it("lists segments with distance, grade, effort count and best time", async () => {
    await renderPage();
    expect(await screen.findByText("Siedra from Damlataş")).toBeTruthy();
    expect(screen.getByText("3.15 km")).toBeTruthy();
    expect(screen.getByText("+6.9%")).toBeTruthy();
    expect(screen.getByText("23:26")).toBeTruthy();
  });

  it("search filters the list and clears back", async () => {
    mocked.listSegments.mockResolvedValue([
      summary({ id: "a", name: "Rock - Stream climb" }),
      summary({ id: "b", name: "D400-Sapadere" }),
    ]);
    await renderPage();
    await screen.findByText("Rock - Stream climb");

    fireEvent.change(screen.getByLabelText("Search segments"), {
      target: { value: "sapadere" },
    });
    expect(screen.queryByText("Rock - Stream climb")).toBeNull();
    screen.getByText("D400-Sapadere");

    fireEvent.click(screen.getByTitle("Clear search"));
    await screen.findByText("Rock - Stream climb");
  });

  it("a query matching nothing explains itself instead of a bare table", async () => {
    await renderPage();
    await screen.findByText("Siedra from Damlataş");
    fireEvent.change(screen.getByLabelText("Search segments"), {
      target: { value: "everest" },
    });
    screen.getByText("No segments match “everest”.");
    expect(screen.queryByText("Siedra from Damlataş")).toBeNull();
  });

  it("shows the how-to empty state without segments", async () => {
    mocked.listSegments.mockResolvedValue([]);
    await renderPage();
    expect(await screen.findByText(/No segments yet/)).toBeTruthy();
  });

  it("renames through the inline editor (Enter submits, trimmed)", async () => {
    await renderPage();
    await screen.findByText("Siedra from Damlataş");
    fireEvent.click(screen.getByLabelText("Rename segment"));
    const input = screen.getByDisplayValue("Siedra from Damlataş");
    fireEvent.change(input, { target: { value: "  Siedra climb  " } });
    fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() =>
      expect(mocked.renameSegment).toHaveBeenCalledWith("seg-1", "Siedra climb"),
    );
  });

  it("Escape cancels a rename without calling the backend", async () => {
    await renderPage();
    await screen.findByText("Siedra from Damlataş");
    fireEvent.click(screen.getByLabelText("Rename segment"));
    fireEvent.keyDown(screen.getByDisplayValue("Siedra from Damlataş"), {
      key: "Escape",
    });
    expect(mocked.renameSegment).not.toHaveBeenCalled();
    expect(screen.getByText("Siedra from Damlataş")).toBeTruthy();
  });

  it("deletes only after the danger confirm", async () => {
    confirmMock.mockResolvedValueOnce(false);
    await renderPage();
    await screen.findByText("Siedra from Damlataş");
    fireEvent.click(screen.getByLabelText("Delete segment"));
    await waitFor(() => expect(confirmMock).toHaveBeenCalled());
    expect(mocked.deleteSegment).not.toHaveBeenCalled();

    confirmMock.mockResolvedValueOnce(true);
    fireEvent.click(screen.getByLabelText("Delete segment"));
    await waitFor(() => expect(mocked.deleteSegment).toHaveBeenCalledWith("seg-1"));
  });

  it("shows the loading state while the list is in flight", async () => {
    mocked.listSegments.mockReturnValue(new Promise(() => {}));
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(
      <QueryClientProvider client={qc}>
        <MemoryRouter>
          <Segments />
        </MemoryRouter>
      </QueryClientProvider>,
    );
    expect(screen.getByText("Loading segments…")).toBeTruthy();
  });

  it("submitting an empty rename is a no-op", async () => {
    await renderPage();
    await screen.findByText("Siedra from Damlataş");
    fireEvent.click(screen.getByLabelText("Rename segment"));
    const input = screen.getByDisplayValue("Siedra from Damlataş");
    fireEvent.change(input, { target: { value: "   " } });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(mocked.renameSegment).not.toHaveBeenCalled();
    // The editor stays open — clicking the row must NOT toggle the
    // leaderboard while editing.
    fireEvent.click(input.closest("tr")!);
    expect(mocked.getSegmentEfforts).not.toHaveBeenCalled();
  });

  it("surfaces a backend rename error inline", async () => {
    mocked.renameSegment.mockRejectedValue("segment not found");
    await renderPage();
    await screen.findByText("Siedra from Damlataş");
    fireEvent.click(screen.getByLabelText("Rename segment"));
    fireEvent.keyDown(screen.getByDisplayValue("Siedra from Damlataş"), { key: "Enter" });
    expect(await screen.findByText(/segment not found/)).toBeTruthy();
  });

  it("an expanded segment without efforts explains itself", async () => {
    await renderPage();
    fireEvent.click(await screen.findByText("Siedra from Damlataş"));
    expect(await screen.findByText(/No efforts yet/)).toBeTruthy();
  });

  it("renders untimed leaderboard rows with placeholders", async () => {
    mocked.getSegmentEfforts.mockResolvedValue([
      lb({ activity_title: null, elapsed_s: null, rank: null }),
    ]);
    await renderPage();
    fireEvent.click(await screen.findByText("Siedra from Damlataş"));
    expect(await screen.findByText("Untitled")).toBeTruthy();
    expect(screen.getByText("—")).toBeTruthy();
    expect(screen.getAllByText("--").length).toBeGreaterThanOrEqual(1);
  });

  it("clicking a leaderboard effort navigates to its activity", async () => {
    mocked.getSegmentEfforts.mockResolvedValue([lb()]);
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(
      <QueryClientProvider client={qc}>
        <MemoryRouter initialEntries={["/segments"]}>
          <Routes>
            <Route path="/segments" element={<Segments />} />
            <Route path="/activity/:id" element={<div>ACTIVITY PAGE</div>} />
          </Routes>
        </MemoryRouter>
      </QueryClientProvider>,
    );
    fireEvent.click(await screen.findByText("Siedra from Damlataş"));
    fireEvent.click(await screen.findByText("Siedra"));
    expect(await screen.findByText("ACTIVITY PAGE")).toBeTruthy();
  });

  it("shows the Power column when a segment has watts, dash for meterless", async () => {
    mocked.listSegments.mockResolvedValue([summary({ best_effort_power_w: 245.4 })]);
    mocked.getSegmentEfforts.mockResolvedValue([
      lb({ avg_power_w: 245.4 }),
      lb({ id: 2, activity_id: "act-2", elapsed_s: 1511, rank: 2 }),
    ]);
    await renderPage();
    await screen.findByText("Siedra from Damlataş");
    // Summary row: the segment's best effort-average power.
    screen.getByText("Power");
    screen.getByText("245 W");
    fireEvent.click(screen.getByText("Siedra from Damlataş"));
    // Leaderboard: the powered pass repeats the value, the meterless dashes.
    // findAll resolves at ≥1 match (the summary cell) — wait for both.
    await waitFor(() => expect(screen.getAllByText("245 W")).toHaveLength(2));
    expect(screen.getByText("--")).toBeTruthy();
  });

  it("a fully meterless library has no Power column at all", async () => {
    mocked.getSegmentEfforts.mockResolvedValue([lb(), lb({ id: 2, rank: 2 })]);
    await renderPage();
    fireEvent.click(await screen.findByText("Siedra from Damlataş"));
    await screen.findByText("#1");
    expect(screen.queryByText("Power")).toBeNull();
    expect(screen.queryByText(/\d+ W$/)).toBeNull();
  });

  it("every row spans the same column count in both power modes", async () => {
    // Mutation guard for the table-fixed layout (#41/#42): a dropped or extra
    // <td> shifts every value one column over without failing a getByText.
    const rowWidths = (container: HTMLElement) =>
      Array.from(container.querySelectorAll("tr")).map((tr) =>
        Array.from(tr.querySelectorAll("th, td")).reduce(
          (sum, cell) => sum + (cell as HTMLTableCellElement).colSpan,
          0,
        ),
      );

    // Powered mode: 8 columns everywhere.
    mocked.listSegments.mockResolvedValue([summary({ best_effort_power_w: 245.4 })]);
    mocked.getSegmentEfforts.mockResolvedValue([lb({ avg_power_w: 245.4 }), lb({ id: 2, rank: 2 })]);
    const first = render(
      <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
        <MemoryRouter>
          <Segments />
        </MemoryRouter>
      </QueryClientProvider>,
    );
    fireEvent.click(await screen.findByText("Siedra from Damlataş"));
    await screen.findByText("#1");
    expect(new Set(rowWidths(first.container))).toEqual(new Set([8]));
    cleanup();

    // Meterless mode: 7 columns everywhere.
    mocked.listSegments.mockResolvedValue([summary()]);
    mocked.getSegmentEfforts.mockResolvedValue([lb(), lb({ id: 2, rank: 2 })]);
    const second = render(
      <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
        <MemoryRouter>
          <Segments />
        </MemoryRouter>
      </QueryClientProvider>,
    );
    fireEvent.click(await screen.findByText("Siedra from Damlataş"));
    await screen.findByText("#1");
    expect(new Set(rowWidths(second.container))).toEqual(new Set([7]));
  });

  it("expands a row into the leaderboard, best first with a trophy", async () => {
    mocked.getSegmentEfforts.mockResolvedValue([
      lb(),
      lb({ id: 2, activity_id: "act-2", activity_title: "Siedra 2", elapsed_s: 1511, rank: 2 }),
    ]);
    await renderPage();
    fireEvent.click(await screen.findByText("Siedra from Damlataş"));
    expect(await screen.findByText("#1")).toBeTruthy();
    expect(screen.getByText("#2")).toBeTruthy();
    expect(screen.getByLabelText("Best effort")).toBeTruthy();
    expect(mocked.getSegmentEfforts).toHaveBeenCalledWith("seg-1");
    // 25:11 for the slower ride.
    expect(screen.getByText("25:11")).toBeTruthy();
  });
});
