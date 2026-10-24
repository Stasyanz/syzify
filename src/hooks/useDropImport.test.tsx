// @vitest-environment happy-dom
import { describe, it, expect, afterEach, beforeEach, vi } from "vitest";
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
        .mockResolvedValue({
          imported: 1,
          skipped: 0,
          failed: [],
          monitoring_files: 0,
          monitoring_days: 0,
          monitoring_range: null,
          monitoring_night: false,
        }),
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
import { useLanguageStore } from "../lib/i18n";
import { commandError } from "../test/commandError";

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

  it("passes a dropped folder through for the backend to expand", async () => {
    const { fire } = await renderDropImport("/library");
    fire({ type: "drop", paths: ["/Users/me/garmin/Monitor", "/a/notes.txt"], position: pos });
    await waitFor(() =>
      expect(importFiles).toHaveBeenCalledWith(["/Users/me/garmin/Monitor"]),
    );
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

describe("useDropImport subscription", () => {
  /** The cleanup ran while `onDragDropEvent` was on its IPC round trip: the
   * handler that lands afterwards would import every drop a second time
   * after a remount, so it is let go at once (#194). */
  it("drops a subscription that arrives after unmount", async () => {
    const unlisten = vi.fn();
    let resolveSub: ((u: () => void) => void) | undefined;
    onDragDropEvent.mockImplementationOnce(
      () => new Promise<() => void>((resolve) => { resolveSub = resolve; }),
    );
    const qc = new QueryClient();
    const wrapper = ({ children }: { children: ReactNode }) => (
      <QueryClientProvider client={qc}>
        <MemoryRouter initialEntries={["/"]}>{children}</MemoryRouter>
      </QueryClientProvider>
    );
    const { unmount } = renderHook(() => useDropImport(), { wrapper });
    await waitFor(() => expect(resolveSub).toBeDefined());
    unmount();
    expect(unlisten).not.toHaveBeenCalled();
    await act(async () => {
      resolveSub!(unlisten);
    });
    expect(unlisten).toHaveBeenCalledTimes(1);
  });

  it("unsubscribes on unmount", async () => {
    const unlisten = vi.fn();
    onDragDropEvent.mockImplementationOnce(async (cb: DragDropHandler) => {
      captured.handler = cb;
      return unlisten;
    });
    const { unmount } = await renderDropImport("/");
    expect(unlisten).not.toHaveBeenCalled();
    unmount();
    expect(unlisten).toHaveBeenCalledTimes(1);
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

  it("says a refused attach in the backend's words — a bare string, not an Error (#174)", async () => {
    attachPhotos.mockRejectedValueOnce("The vault is locked");
    const { fire } = await renderDropImport("/activity/act-1");
    fire({ type: "drop", paths: ["/a/1.jpg"], position: pos });
    await waitFor(() =>
      expect(addToast).toHaveBeenCalledWith("error", "Failed to attach photos: The vault is locked")
    );
  });

  it("says a refused import in the backend's words", async () => {
    importFiles.mockRejectedValueOnce(commandError(null, "The vault is locked"));
    const { fire } = await renderDropImport("/");
    fire({ type: "drop", paths: ["/a/ride.fit"], position: pos });
    await waitFor(() =>
      expect(addToast).toHaveBeenCalledWith("error", "Import failed: The vault is locked")
    );
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

  it("counts the photos it added, as the gallery's button does", async () => {
    const { fire } = await renderDropImport("/activity/act-1");
    fire({ type: "drop", paths: ["/a/1.jpg"], position: pos });
    await waitFor(() => expect(addToast).toHaveBeenCalledWith("success", "Added 1 photo"));
  });

  it("says nothing when a drop attached, skipped and failed nothing", async () => {
    attachPhotos.mockResolvedValueOnce({ attached: [], skipped: [], failed: [] });
    const { fire } = await renderDropImport("/activity/act-1");
    fire({ type: "drop", paths: ["/a/1.jpg"], position: pos });
    await waitFor(() => expect(attachPhotos).toHaveBeenCalled());
    await act(async () => {});
    expect(addToast).not.toHaveBeenCalled();
  });
});

describe("useDropImport in Russian (#235)", () => {
  beforeEach(() => useLanguageStore.getState().setPref("ru"));
  afterEach(() => useLanguageStore.getState().setPref("system"));

  it("says every warning in the interface language", async () => {
    const page = await renderDropImport("/activity/act-1");
    page.fire({ type: "drop", paths: ["/a/ride.fit"], position: pos });
    page.fire({ type: "drop", paths: ["/a/notes.txt"], position: pos });
    cleanup();
    const library = await renderDropImport("/library");
    library.fire({ type: "drop", paths: ["/a/1.jpg"], position: pos });
    library.fire({ type: "drop", paths: ["/a/notes.txt"], position: pos });
    expect(addToast.mock.calls).toEqual([
      ["warning", "Чтобы импортировать тренировки, перетащите их за пределами страницы тренировки"],
      ["warning", "К тренировке можно добавить только изображения JPG, PNG, WebP или HEIC"],
      ["warning", "Чтобы добавить фото, перетащите их на страницу тренировки"],
      ["warning", "Файлы тренировок не найдены (GPX, FIT, TCX)"],
    ]);
  });

  it("sums up an import and an attach, and a refusal, in the interface language", async () => {
    const library = await renderDropImport("/library");
    library.fire({ type: "drop", paths: ["/a/ride.fit"], position: pos });
    await waitFor(() => expect(addToast).toHaveBeenCalledWith("success", "Импортировано: 1 тренировка"));
    importFiles.mockRejectedValueOnce("The vault is locked");
    library.fire({ type: "drop", paths: ["/a/ride.fit"], position: pos });
    await waitFor(() => expect(addToast).toHaveBeenCalledWith("error", "Импорт не удался: The vault is locked"));
    cleanup();
    attachPhotos.mockResolvedValueOnce({ attached: ["p1", "p2"], skipped: ["d"], failed: [] });
    const page = await renderDropImport("/activity/act-1");
    page.fire({ type: "drop", paths: ["/a/1.jpg"], position: pos });
    await waitFor(() =>
      expect(addToast).toHaveBeenCalledWith("success", "Добавлено 2 фото, пропущен 1 дубликат"),
    );
    attachPhotos.mockRejectedValueOnce("The vault is locked");
    page.fire({ type: "drop", paths: ["/a/1.jpg"], position: pos });
    await waitFor(() =>
      expect(addToast).toHaveBeenCalledWith("error", "Не удалось добавить фото: The vault is locked"),
    );
  });
});