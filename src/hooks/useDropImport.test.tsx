// @vitest-environment happy-dom
import { describe, it, expect, afterEach, vi } from "vitest";
import { renderHook, act, cleanup, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router";
import type { ReactNode } from "react";

type DragDropHandler = (event: { payload: unknown }) => void;

const { onDragDropEvent, importFiles, attachPhotos, addToast, captured } =
  vi.hoisted(() => {
    const captured: { handler: DragDropHandler | null } = { handler: null };
    return {
      captured,
      onDragDropEvent: vi.fn(async (cb: DragDropHandler) => {
        captured.handler = cb;
        return () => {};
      }),
      importFiles: vi
        .fn()
        .mockResolvedValue({ imported: 1, skipped: 0, failed: [] }),
      attachPhotos: vi
        .fn()
        .mockResolvedValue({ attached: ["p1"], skipped: [], failed: [] }),
      addToast: vi.fn(),
    };
  });

vi.mock("@tauri-apps/api/webview", () => ({
  getCurrentWebview: () => ({ onDragDropEvent }),
}));
vi.mock("../lib/tauri", () => ({ api: { importFiles, attachPhotos } }));
vi.mock("../stores/toastStore", () => ({
  useToastStore: (sel: (s: { addToast: typeof addToast }) => unknown) =>
    sel({ addToast }),
}));

import { useDropImport } from "./useDropImport";

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  captured.handler = null;
});

async function renderDropImport(route: string) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={qc}>
      <MemoryRouter initialEntries={[route]}>{children}</MemoryRouter>
    </QueryClientProvider>
  );
  const utils = renderHook(() => useDropImport(), { wrapper });
  await waitFor(() => {
    if (!captured.handler) throw new Error("drag-drop listener not attached");
  });
  const fire = (payload: unknown) => act(() => captured.handler!({ payload }));
  return { ...utils, fire };
}

const pos = { x: 10, y: 10 };

describe("useDropImport outside an activity page", () => {
  it("shows the workout overlay for a workout-file drag", async () => {
    const { result, fire } = await renderDropImport("/library");
    expect(result.current.kind).toBe("workout");
    fire({ type: "enter", paths: ["/a/ride.fit"], position: pos });
    expect(result.current.dragging).toBe(true);
    fire({ type: "leave" });
    expect(result.current.dragging).toBe(false);
  });

  it("keeps the overlay down for an images-only drag", async () => {
    const { result, fire } = await renderDropImport("/library");
    fire({ type: "enter", paths: ["/a/1.jpg", "/a/2.png"], position: pos });
    expect(result.current.dragging).toBe(false);
    fire({ type: "over", position: pos });
    expect(result.current.dragging).toBe(false);
  });

  it("imports dropped workout files, ignoring stray images", async () => {
    const { fire } = await renderDropImport("/library");
    fire({ type: "drop", paths: ["/a/ride.fit", "/a/photo.jpg"], position: pos });
    await waitFor(() => expect(importFiles).toHaveBeenCalledWith(["/a/ride.fit"]));
  });

  it("points an images-only drop at an activity page", async () => {
    const { fire } = await renderDropImport("/library");
    fire({ type: "drop", paths: ["/a/1.jpg"], position: pos });
    expect(importFiles).not.toHaveBeenCalled();
    expect(addToast).toHaveBeenCalledWith(
      "warning",
      expect.stringContaining("activity page")
    );
  });

  it("still warns about non-workout, non-image drops", async () => {
    const { fire } = await renderDropImport("/library");
    fire({ type: "drop", paths: ["/a/notes.txt"], position: pos });
    expect(addToast).toHaveBeenCalledWith(
      "warning",
      expect.stringContaining("GPX, FIT, TCX")
    );
  });
});

describe("useDropImport on an activity page", () => {
  it("shows the photo overlay when the drag carries images", async () => {
    const { result, fire } = await renderDropImport("/activity/act-1");
    expect(result.current.kind).toBe("photo");
    fire({ type: "enter", paths: ["/a/1.jpg", "/a/ride.fit"], position: pos });
    expect(result.current.dragging).toBe(true);
  });

  it("keeps the overlay down for a drag with no images", async () => {
    const { result, fire } = await renderDropImport("/activity/act-1");
    fire({ type: "enter", paths: ["/a/ride.fit"], position: pos });
    expect(result.current.dragging).toBe(false);
    fire({ type: "over", position: pos });
    expect(result.current.dragging).toBe(false);
  });

  it("attaches dropped images to the routed activity", async () => {
    const { fire } = await renderDropImport("/activity/act-1");
    fire({ type: "drop", paths: ["/a/1.jpg", "/a/notes.txt"], position: pos });
    await waitFor(() =>
      expect(attachPhotos).toHaveBeenCalledWith("act-1", ["/a/1.jpg"])
    );
    expect(importFiles).not.toHaveBeenCalled();
  });

  it("refuses workout files with a pointer back out", async () => {
    const { fire } = await renderDropImport("/activity/act-1");
    fire({ type: "drop", paths: ["/a/ride.fit"], position: pos });
    expect(importFiles).not.toHaveBeenCalled();
    expect(attachPhotos).not.toHaveBeenCalled();
    expect(addToast).toHaveBeenCalledWith(
      "warning",
      expect.stringContaining("outside the activity page")
    );
  });

  it("warns about drops with neither images nor workouts", async () => {
    const { fire } = await renderDropImport("/activity/act-1");
    fire({ type: "drop", paths: ["/a/notes.txt"], position: pos });
    expect(addToast).toHaveBeenCalledWith(
      "warning",
      expect.stringContaining("JPG, PNG, WebP or HEIC")
    );
  });
});