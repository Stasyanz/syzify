// @vitest-environment happy-dom
import { describe, it, expect, afterEach, beforeEach, vi } from "vitest";
import { render, cleanup, fireEvent, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { api } from "../../lib/tauri";
import { SaveSegmentPopover, clampMenuPosition, MENU_W } from "./SaveSegmentPopover";

vi.mock("../../lib/tauri", () => ({
  api: {
    checkSimilarSegments: vi.fn(),
    saveSegment: vi.fn(),
  },
}));

const mocked = vi.mocked(api);

afterEach(cleanup);
beforeEach(() => {
  vi.clearAllMocks();
  mocked.checkSimilarSegments.mockResolvedValue([]);
  mocked.saveSegment.mockResolvedValue({ id: "seg-1" } as Awaited<
    ReturnType<typeof api.saveSegment>
  >);
});

// Mirror the app's QueryClient (App.tsx sets staleTime: 30_000) — the
// duplicate check must stay correct under production caching defaults.
function appQueryClient() {
  return new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: 30_000 } },
  });
}

function renderPopover(onClose = vi.fn(), qc = appQueryClient()) {
  const view = render(
    <QueryClientProvider client={qc}>
      <SaveSegmentPopover x={100} y={100} activityId="act-1" range={[10, 90]} onClose={onClose} />
    </QueryClientProvider>,
  );
  return { onClose, qc, view };
}

describe("clampMenuPosition", () => {
  it("keeps the popover inside the viewport", () => {
    expect(clampMenuPosition(100, 100, 1200, 800)).toEqual({ left: 100, top: 100 });
    // Near the right/bottom edges it retreats by its own size.
    const c = clampMenuPosition(1190, 790, 1200, 800);
    expect(c.left).toBe(1200 - MENU_W - 8);
    expect(c.top).toBeLessThan(790);
    // Never off the top-left either.
    expect(clampMenuPosition(-50, -50, 1200, 800)).toEqual({ left: 8, top: 8 });
  });
});

describe("SaveSegmentPopover", () => {
  it("saves the trimmed name for the selected range and closes", async () => {
    const { onClose } = renderPopover();
    fireEvent.change(screen.getByPlaceholderText("Segment name"), {
      target: { value: "  Big climb  " },
    });
    fireEvent.click(screen.getByText("Save"));
    await waitFor(() =>
      expect(mocked.saveSegment).toHaveBeenCalledWith("act-1", 10, 90, "Big climb"),
    );
    expect(await screen.findByText("Saved ✓")).toBeTruthy();
    await waitFor(() => expect(onClose).toHaveBeenCalled(), { timeout: 2000 });
  });

  it("disables Save while the name is empty", () => {
    renderPopover();
    expect((screen.getByText("Save") as HTMLButtonElement).disabled).toBe(true);
    expect(mocked.saveSegment).not.toHaveBeenCalled();
  });

  it("warns about a similar segment but still allows saving", async () => {
    mocked.checkSimilarSegments.mockResolvedValue([
      { id: "s0", name: "Old hill", distance_m: 2410 },
    ]);
    renderPopover();
    expect(await screen.findByText(/Old hill/)).toBeTruthy();
    expect(mocked.checkSimilarSegments).toHaveBeenCalledWith("act-1", 10, 90);

    fireEvent.change(screen.getByPlaceholderText("Segment name"), {
      target: { value: "New hill" },
    });
    fireEvent.click(screen.getByText("Save anyway"));
    await waitFor(() =>
      expect(mocked.saveSegment).toHaveBeenCalledWith("act-1", 10, 90, "New hill"),
    );
  });

  it("re-checks duplicates on reopen instead of replaying the cache", async () => {
    // First open: no duplicates yet.
    const qc = appQueryClient();
    const { view } = renderPopover(vi.fn(), qc);
    await waitFor(() => expect(mocked.checkSimilarSegments).toHaveBeenCalledTimes(1));
    view.unmount();

    // The user saved this very selection; reopening the form on the SAME
    // range must surface the fresh duplicate despite the app's staleTime.
    mocked.checkSimilarSegments.mockResolvedValue([
      { id: "s1", name: "Just saved", distance_m: 500 },
    ]);
    renderPopover(vi.fn(), qc);
    expect(await screen.findByText(/Just saved/)).toBeTruthy();
    expect(mocked.checkSimilarSegments).toHaveBeenCalledTimes(2);
  });

  it("shows the backend error and stays open", async () => {
    mocked.saveSegment.mockRejectedValue("selection has no GPS data");
    const { onClose } = renderPopover();
    fireEvent.change(screen.getByPlaceholderText("Segment name"), {
      target: { value: "x" },
    });
    fireEvent.click(screen.getByText("Save"));
    expect(await screen.findByText(/no GPS data/)).toBeTruthy();
    expect(onClose).not.toHaveBeenCalled();
  });

  it("closes on Escape and on an outside pointerdown", () => {
    const { onClose } = renderPopover();
    fireEvent.keyDown(window, { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(1);
    fireEvent.pointerDown(document.body);
    expect(onClose).toHaveBeenCalledTimes(2);
  });
});
