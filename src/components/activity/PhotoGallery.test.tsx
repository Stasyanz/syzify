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
  api: { getPhotos: vi.fn().mockResolvedValue([photo]) },
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/api/webview", () => ({
  getCurrentWebview: () => ({
    onDragDropEvent: vi.fn().mockResolvedValue(() => {}),
  }),
}));

import { PhotoGallery } from "./PhotoGallery";

afterEach(cleanup);

function renderGallery() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <PhotoGallery activityId="act-1" onShare={() => {}} />
    </QueryClientProvider>
  );
}

describe("PhotoGallery", () => {
  it("exposes a [data-photo-dropzone] so the import hook can skip its drops", () => {
    const { container } = renderGallery();
    expect(container.querySelector("[data-photo-dropzone]")).not.toBeNull();
  });

  it("renders attached photo thumbnails", async () => {
    const { container } = renderGallery();
    await waitFor(() => {
      const img = container.querySelector('img[src="photo://localhost/ph-1?size=thumb"]');
      if (!img) throw new Error("thumbnail not rendered yet");
    });
  });
});
