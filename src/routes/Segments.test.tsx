// @vitest-environment happy-dom
import { describe, it, expect, afterEach, beforeEach, vi } from "vitest";
import { render, cleanup, fireEvent, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router";
import { api } from "../lib/tauri";
import type { SegmentLeaderboardRow, SegmentSummaryRow } from "../lib/types";
import { Segments } from "./Segments";

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

describe("Segments page", () => {
  it("lists segments with distance, grade, effort count and best time", async () => {
    await renderPage();
    expect(await screen.findByText("Siedra from Damlataş")).toBeTruthy();
    expect(screen.getByText("3.15 km")).toBeTruthy();
    expect(screen.getByText("+6.9%")).toBeTruthy();
    expect(screen.getByText("23:26")).toBeTruthy();
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
