// @vitest-environment happy-dom
import { describe, it, expect, afterEach, vi } from "vitest";
import { render, cleanup, waitFor } from "@testing-library/react";
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
  },
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("../../stores/confirmStore", () => ({ confirmDialog: vi.fn() }));
import { confirmDialog } from "../../stores/confirmStore";
import { api } from "../../lib/tauri";
import { PhotoGallery } from "./PhotoGallery";

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
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
});
