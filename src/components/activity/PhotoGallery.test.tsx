// @vitest-environment happy-dom
import { describe, it, expect, afterEach, vi } from "vitest";
import { render, cleanup, waitFor, fireEvent } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { Photo } from "../../lib/types";

const { photo } = vi.hoisted(() => ({
  photo: {
    id: "ph-1",
    activity_id: "act-1",
    path_in_vault: "photos/act-1/ph-1.jpg",
    thumbnail_path: "photos/act-1/ph-1.thumb.jpg",
    original_path: null,
    mime_type: "image/jpeg",
    width: 800,
    height: 600,
    size_bytes: 1000,
    hash_sha256: "h",
    taken_at: null,
    caption: null,
    sort_order: 0,
    created_at: "",
  } as Photo,
}));

vi.mock("../../lib/tauri", () => ({
  api: {
    getPhotos: vi.fn().mockResolvedValue([photo]),
    deletePhoto: vi.fn().mockResolvedValue(undefined),
    attachPhotos: vi.fn(),
    updatePhotoCaption: vi.fn(),
    reorderPhotos: vi.fn(),
  },
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
import { open } from "@tauri-apps/plugin-dialog";
vi.mock("../../stores/confirmStore", () => ({ confirmDialog: vi.fn() }));
import { confirmDialog } from "../../stores/confirmStore";
import { api } from "../../lib/tauri";
import { useToastStore } from "../../stores/toastStore";
import { PhotoGallery } from "./PhotoGallery";
import { useLanguageStore } from "../../lib/i18n";

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  // clearAllMocks keeps implementations: a test that swapped the photo
  // list must not leak it into the next one.
  vi.mocked(api.getPhotos).mockResolvedValue([photo]);
});

function renderGallery() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <PhotoGallery activityId="act-1" onShare={() => {}} />
    </QueryClientProvider>
  );
}

describe("PhotoGallery", () => {
  it("renders attached photo thumbnails", async () => {
    const { container } = renderGallery();
    await waitFor(() => {
      const img = container.querySelector('img[src="photo://localhost/ph-1?size=thumb"]');
      if (!img) throw new Error("thumbnail not rendered yet");
    });
  });

  /// The dialog is ASYNC: the delete must wait for the answer — with
  /// window.confirm the (shimmed) Promise was always truthy and the photo
  /// was gone before the user clicked Cancel.
  it("deletes only after the confirm dialog resolves true", async () => {
    const { container } = renderGallery();
    const del = await waitFor(() => {
      const b = container.querySelector('button[title="Delete"]');
      if (!b) throw new Error("delete button not rendered yet");
      return b as HTMLButtonElement;
    });

    vi.mocked(confirmDialog).mockResolvedValue(false);
    del.click();
    // Give the async handler a tick — Cancel must not delete.
    await waitFor(() => expect(confirmDialog).toHaveBeenCalledTimes(1));
    expect(api.deletePhoto).not.toHaveBeenCalled();

    vi.mocked(confirmDialog).mockResolvedValue(true);
    del.click();
    await waitFor(() => expect(api.deletePhoto).toHaveBeenCalledWith("ph-1"));
  });

  /// A Tauri command refuses with a bare string, not an Error: the toast
  /// must carry its words, not "undefined" (#174).
  it("says a refused delete in the backend's words", async () => {
    useToastStore.setState({ toasts: [] });
    vi.mocked(api.deletePhoto).mockRejectedValueOnce("The vault is locked");
    vi.mocked(confirmDialog).mockResolvedValue(true);
    const { container } = renderGallery();
    const del = await waitFor(() => {
      const b = container.querySelector('button[title="Delete"]');
      if (!b) throw new Error("delete button not rendered yet");
      return b as HTMLButtonElement;
    });
    del.click();
    await waitFor(() =>
      expect(useToastStore.getState().toasts.map((t) => [t.type, t.message])).toEqual([["error", "The vault is locked"]]),
    );
  });

  it("says a refused attach, caption and reorder in the backend's words", async () => {
    useToastStore.setState({ toasts: [] });
    const toasts = () => useToastStore.getState().toasts.map((t) => `${t.type}: ${t.message}`);
    vi.mocked(api.getPhotos).mockResolvedValue([photo, { ...photo, id: "ph-2" }]);
    const { container } = renderGallery();
    await waitFor(() => expect(container.querySelectorAll("[draggable]").length).toBe(2));

    // Attach: the file dialog picks a path, the backend refuses.
    vi.mocked(open).mockResolvedValueOnce(["/a/1.jpg"]);
    vi.mocked(api.attachPhotos).mockRejectedValueOnce("The vault is locked");
    fireEvent.click(screenButton(container, "Add Photos"));
    await waitFor(() => expect(api.attachPhotos).toHaveBeenCalledWith("act-1", ["/a/1.jpg"]));
    await waitFor(() => expect(toasts()).toContain("error: Failed to attach photos: The vault is locked"));

    // Caption.
    vi.mocked(api.updatePhotoCaption).mockRejectedValueOnce("Photo not found: ph-1");
    fireEvent.click(container.querySelectorAll('button[title="Edit caption"]')[0]);
    const input = document.querySelector<HTMLInputElement>('input[type="text"]')!;
    fireEvent.change(input, { target: { value: "Summit" } });
    fireEvent.click(screenButton(document.body, "Save"));
    await waitFor(() => expect(api.updatePhotoCaption).toHaveBeenCalledWith("ph-1", "Summit"));
    await waitFor(() => expect(toasts()).toContain("error: Photo not found: ph-1"));
    // The caption modal stays up with the draft: the user can retry.
    expect(document.querySelector<HTMLInputElement>('input[type="text"]')?.value).toBe("Summit");
    fireEvent.click(screenButton(document.body, "Cancel"));

    // Reorder: a drag between two tiles, the backend refuses, the list is
    // re-read so the optimistic order does not stick.
    vi.mocked(api.reorderPhotos).mockRejectedValueOnce("The vault is locked");
    const tiles = container.querySelectorAll("[draggable]");
    const calls = vi.mocked(api.getPhotos).mock.calls.length;
    fireEvent.dragStart(tiles[1]);
    fireEvent.drop(tiles[0]);
    await waitFor(() => expect(api.reorderPhotos).toHaveBeenCalledWith(["ph-2", "ph-1"]));
    await waitFor(() => expect(toasts()).toContain("error: Failed to reorder: The vault is locked"));
    await waitFor(() => expect(vi.mocked(api.getPhotos).mock.calls.length).toBeGreaterThan(calls));
  });

  /** #234: the attach summary counts each part with the language's plural. */
  it("sums up an attach, the counts in the plural they take", async () => {
    useToastStore.setState({ toasts: [] });
    const toasts = () => useToastStore.getState().toasts.map((t) => `${t.type}: ${t.message}`);
    const { container } = renderGallery();
    await waitFor(() => expect(container.querySelectorAll("[draggable]").length).toBe(1));
    vi.mocked(open).mockResolvedValueOnce(["/a/1.jpg", "/a/2.jpg"]);
    vi.mocked(api.attachPhotos).mockResolvedValueOnce({ attached: [photo], skipped: ["/a/2.jpg"], failed: [] });
    fireEvent.click(screenButton(container, "Add Photos"));
    await waitFor(() => expect(toasts()).toEqual(["success: Added 1 photo, skipped 1 duplicate"]));
    vi.mocked(open).mockResolvedValueOnce(["/a/3.jpg"]);
    vi.mocked(api.attachPhotos).mockResolvedValueOnce({
      attached: [photo, photo],
      skipped: [],
      failed: [{ path: "/a/3.jpg", reason: "bad" }],
    });
    fireEvent.click(screenButton(container, "Add Photos"));
    await waitFor(() => expect(toasts()).toContain("warning: Added 2 photos, 1 failed"));
    // Nothing added: the skipped part opens the sentence.
    vi.mocked(open).mockResolvedValueOnce(["/a/1.jpg", "/a/2.jpg"]);
    vi.mocked(api.attachPhotos).mockResolvedValueOnce({ attached: [], skipped: ["/a/1.jpg", "/a/2.jpg"], failed: [] });
    fireEvent.click(screenButton(container, "Add Photos"));
    await waitFor(() => expect(toasts()).toContain("success: Skipped 2 duplicates"));
    // An empty answer says nothing at all.
    const before = toasts().length;
    vi.mocked(open).mockResolvedValueOnce(["/a/1.jpg"]);
    vi.mocked(api.attachPhotos).mockResolvedValueOnce({ attached: [], skipped: [], failed: [] });
    fireEvent.click(screenButton(container, "Add Photos"));
    await waitFor(() => expect(api.attachPhotos).toHaveBeenCalledTimes(4));
    await new Promise((r) => setTimeout(r, 20));
    expect(toasts()).toHaveLength(before);
  });

  it("speaks Russian: its words and the attach summary's plurals", async () => {
    useLanguageStore.getState().setPref("ru");
    try {
      useToastStore.setState({ toasts: [] });
      const toasts = () => useToastStore.getState().toasts.map((t) => t.message);
      vi.mocked(api.getPhotos).mockResolvedValue([]);
      const { container, findByText } = renderGallery();
      await findByText("Фото пока нет. Нажмите «Добавить фото» или перетащите изображения в любое место страницы.");
      let settle: (v: { attached: Photo[]; skipped: string[]; failed: { path: string; reason: string }[] }) => void =
        () => {};
      vi.mocked(open).mockResolvedValueOnce(["/a/1.jpg"]);
      vi.mocked(api.attachPhotos).mockImplementationOnce(() => new Promise((r) => (settle = r)));
      fireEvent.click(screenButton(container, "Добавить фото"));
      await findByText("Добавление…");
      expect(vi.mocked(open).mock.calls[0][0]).toMatchObject({ filters: [{ name: "Изображения" }] });
      settle({ attached: [], skipped: ["1", "2", "3", "4", "5"], failed: [{ path: "x", reason: "y" }] });
      // Nothing added: the sentence opens on the skipped part, capitalized.
      await waitFor(() => expect(toasts()).toEqual(["Пропущено 5 дубликатов, с ошибкой: 1"]));
      vi.mocked(open).mockResolvedValueOnce(["/a/1.jpg"]);
      vi.mocked(api.attachPhotos).mockResolvedValueOnce({ attached: [photo, photo], skipped: ["1", "2"], failed: [] });
      fireEvent.click(screenButton(container, "Добавить фото"));
      await waitFor(() => expect(toasts()).toContain("Добавлено 2 фото, пропущено 2 дубликата"));
    } finally {
      useLanguageStore.getState().setPref("system");
    }
  });
});

/** The button with exactly this text under `root`. */
function screenButton(root: ParentNode, text: string): HTMLButtonElement {
  const b = Array.from(root.querySelectorAll("button")).find((x) => x.textContent?.trim() === text);
  if (!b) throw new Error(`no button "${text}"`);
  return b;
}
