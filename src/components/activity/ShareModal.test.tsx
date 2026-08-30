// @vitest-environment happy-dom
import { describe, it, expect, beforeAll, beforeEach, afterEach, vi } from "vitest";
import { render, cleanup, waitFor, fireEvent } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { useToastStore } from "../../stores/toastStore";
import type { Activity, Photo, TrackPointColumns } from "../../lib/types";

const { photo } = vi.hoisted(() => ({
  photo: {
    id: "ph-1",
    activity_id: "act-1",
    path_in_vault: "photos/act-1/ph-1.jpg",
    thumbnail_path: "photos/act-1/ph-1.thumb.jpg",
    original_path: null,
    mime_type: "image/jpeg",
    width: 1920,
    height: 1080,
    size_bytes: 1000,
    hash_sha256: "h",
    taken_at: null,
    caption: null,
    sort_order: 0,
    created_at: "",
  } as Photo,
}));

// A second photo (different stored dims) for the photo-switching tests.
const photoB: Photo = { ...photo, id: "ph-2", width: 1600, height: 1000 };

vi.mock("../../lib/tauri", () => ({
  api: {
    getPhotos: vi.fn().mockResolvedValue([photo]),
    getPhotoDataUrl: vi.fn(),
    saveShareImage: vi.fn(),
  },
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ save: vi.fn() }));
// Isolate the preview <img> under test from the overlay/canvas logic.
vi.mock("./DraggableShareBlocks", () => ({ DraggableShareBlocks: () => null, BrandMark: () => null }));

import { ShareModal } from "./ShareModal";
import { api } from "../../lib/tauri";

// The preview wrapper's measured size (clientWidth/clientHeight stubbed below).
const WRAP = { w: 800, h: 600 };
// Browser-natural (EXIF-applied) dims the Image mock reports — the transpose of
// the stored 1920×1080, like a phone photo with EXIF orientation 6.
const NATURAL = { w: 1080, h: 1920 };

// ShareModal loads the full photo via `new Image()` for the compose preview;
// happy-dom never fires load events for photo:// URLs, so emulate per photo id:
// load with EXIF-swapped naturals (default), stay pending forever, or fail.
let imageMode: Record<string, "pending" | "error"> = {};

class MockImage {
  onload: (() => void) | null = null;
  onerror: (() => void) | null = null;
  naturalWidth = 0;
  naturalHeight = 0;
  set src(url: string) {
    const id = /localhost\/([^?]+)/.exec(url)?.[1] ?? "";
    const mode = imageMode[id];
    if (mode === "pending") return;
    queueMicrotask(() => {
      if (mode === "error") {
        this.onerror?.();
        return;
      }
      this.naturalWidth = NATURAL.w;
      this.naturalHeight = NATURAL.h;
      this.onload?.();
    });
  }
}

beforeAll(() => {
  // happy-dom has no ResizeObserver; ShareModal creates one for its preview.
  globalThis.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  } as unknown as typeof ResizeObserver;
  globalThis.Image = MockImage as unknown as typeof Image;
  // happy-dom reports 0×0 for every element; the preview-box effect needs a
  // measurable wrapper to fit the photo into.
  Object.defineProperty(HTMLElement.prototype, "clientWidth", { configurable: true, get: () => WRAP.w });
  Object.defineProperty(HTMLElement.prototype, "clientHeight", { configurable: true, get: () => WRAP.h });
  // No canvas impl in happy-dom — the compose effect bails out on a null ctx.
  HTMLCanvasElement.prototype.getContext = (() => null) as never;
});

beforeEach(() => {
  imageMode = {};
  vi.mocked(api.getPhotos).mockResolvedValue([photo]);
  useToastStore.setState({ toasts: [] });
});

afterEach(cleanup);

const activity = {
  id: "act-1",
  start_time: "2026-05-01T08:00:00+00:00",
  sport_type: "run",
  distance_m: 5000,
  duration_s: 1800,
  avg_speed_mps: 2.7,
  elev_gain_m: 50,
} as unknown as Activity;

const trackpoints = { lat: [], lon: [], altitude_m: [] } as unknown as TrackPointColumns;

function renderModal() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <ShareModal
        activity={activity}
        trackpoints={trackpoints}
        initialPhoto={photo}
        onClose={() => {}}
      />
    </QueryClientProvider>
  );
}

/** The crop-editor box: the positioned div holding the full-photo <img>. */
function cropEditorBox(container: HTMLElement) {
  const img = container.querySelector('img[src$="size=full"]');
  if (!img?.parentElement) throw new Error("crop editor box not rendered yet");
  return img.parentElement;
}

const px = (v: string) => parseFloat(v);

/** Aspect (w/h, photo px) of the crop frame as rendered in the crop editor. */
function frameAspect(container: HTMLElement) {
  const box = cropEditorBox(container);
  const knob = container.querySelector('[title^="Drag to rotate"]');
  const frame = knob?.parentElement;
  if (!frame) throw new Error("crop frame not rendered yet");
  return (
    ((px(frame.style.width) / px(box.style.width)) * NATURAL.w) /
    ((px(frame.style.height) / px(box.style.height)) * NATURAL.h)
  );
}

describe("ShareModal preview", () => {
  it("compose mode renders the de-rotated crop on a canvas (no <img>)", async () => {
    const { container } = renderModal();
    // Default (compose) mode paints the cropped result into a canvas; the full
    // <img> only exists in crop-edit mode.
    await waitFor(() => {
      if (!container.querySelector("canvas")) throw new Error("compose canvas not rendered yet");
    });
    expect(container.querySelector("img")).toBeNull();
  });

  it("crop-edit shows the photo without crossOrigin (regression: black preview)", async () => {
    const { container, getByLabelText } = renderModal();

    // Enter crop-edit mode to reveal the full-photo <img>.
    await waitFor(() => getByLabelText("Adjust crop"));
    fireEvent.click(getByLabelText("Adjust crop"));

    const img = await waitFor(() => {
      const el = container.querySelector("img");
      if (!el) throw new Error("preview image not rendered yet");
      return el;
    });

    expect(img.getAttribute("src")).toBe("photo://localhost/ph-1?size=full");
    // crossOrigin would force a CORS request the photo:// handler no longer
    // satisfies, producing a black preview.
    expect(img.hasAttribute("crossorigin")).toBe(false);
  });
});

describe("ShareModal EXIF orientation (natural ≠ stored dims)", () => {
  it("crop editor box follows the browser-natural aspect, not the stored one", async () => {
    const { container, getByLabelText } = renderModal();

    await waitFor(() => getByLabelText("Adjust crop"));
    fireEvent.click(getByLabelText("Adjust crop"));

    // Stored dims say landscape (1920×1080) but the browser orients the photo
    // to portrait (1080×1920); the editor box must match what the <img> shows,
    // otherwise the user crops a different region than the export samples.
    await waitFor(() => {
      const box = cropEditorBox(container);
      const w = px(box.style.width);
      const h = px(box.style.height);
      expect(h).toBe(WRAP.h);
      expect(w).toBe(Math.floor(WRAP.h * (NATURAL.w / NATURAL.h)));
    });
  });

  it("aspect presets compute the crop in natural coordinates, oriented to the photo", async () => {
    const { container, getByText } = renderModal();

    await waitFor(() => getByText("16:9"));
    fireEvent.click(getByText("16:9"));

    // The stored dims are exactly 16:9, so the old stored-dims math would return
    // the full frame; in natural (portrait) coordinates the preset must both
    // use the natural pixels AND flip to vertical (#46): a portrait photo gets
    // a 9:16 band with no manual quarter turn.
    await waitFor(() => {
      expect(frameAspect(container)).toBeCloseTo(9 / 16, 4);
    });
  });

  it("a preset on a quarter-turned frame keeps its LOCAL aspect (photo-oriented)", async () => {
    const { container, getByText, getByLabelText } = renderModal();

    await waitFor(() => getByLabelText("Adjust crop"));
    fireEvent.click(getByLabelText("Adjust crop"));
    await waitFor(() => {
      expect(px(cropEditorBox(container).style.height)).toBe(WRAP.h);
    });

    // Straighten to the slider bound: 45° folds into the next quarter
    // (autoQuarterOrientation → orientation 90), then apply a preset. The
    // fresh crop's LOCAL aspect depends only on the photo (portrait → 9:16),
    // not on the quarter, so it always matches the resize lock — the turn
    // shows through the quarter fold, not through a re-derived ratio.
    fireEvent.change(getByLabelText("Straighten crop frame"), { target: { value: "45" } });
    fireEvent.click(getByText("16:9"));

    await waitFor(() => {
      expect(frameAspect(container)).toBeCloseTo(9 / 16, 4);
    });
  });

  it("straighten slider edits the residual tilt — no frame flip after a knob quarter-turn", async () => {
    const { container, getByLabelText } = renderModal();

    await waitFor(() => getByLabelText("Adjust crop"));
    fireEvent.click(getByLabelText("Adjust crop"));
    await waitFor(() => {
      expect(px(cropEditorBox(container).style.height)).toBe(WRAP.h);
    });

    // Turn the frame a full quarter with the rotation knob. The knob starts on
    // the frame's local -Y axis, 328px above the center (h/2 + 28 stalk);
    // dragging so it lands due right of the center reads as 90° (snapAngle
    // squares it near cardinals).
    const knob = container.querySelector('[title^="Drag to rotate"]')!;
    fireEvent.pointerDown(knob, { clientX: 0, clientY: 0 });
    fireEvent.pointerMove(window, { clientX: 100, clientY: 328 });
    fireEvent.pointerUp(window);

    const frame = knob.parentElement as HTMLElement;
    await waitFor(() => expect(frame.style.transform).toBe("rotate(90deg)"));
    // The slider shows the residual tilt (0), not the raw 90 on a ±45 control.
    const slider = getByLabelText("Straighten crop frame") as HTMLInputElement;
    expect(slider.value).toBe("0");

    // Leveling by 10° tilts on TOP of the quarter — the flip bug reset the
    // frame to a bare 10° landscape instead.
    fireEvent.change(slider, { target: { value: "10" } });
    await waitFor(() => expect(frame.style.transform).toBe("rotate(100deg)"));
    expect(slider.value).toBe("10");
    // The quarter is still folded into the orientation: the output wrapper
    // keeps counter-rotating by -90.
    const wrapper = Array.from(frame.querySelectorAll("div")).find((el) =>
      el.style.transform.startsWith("rotate(-")
    );
    expect(wrapper?.style.transform).toBe("rotate(-90deg)");
  });

  it("switching photos drops the previous image's natural dims (stale imgEl race)", async () => {
    vi.mocked(api.getPhotos).mockResolvedValue([photo, photoB]);
    imageMode = { "ph-2": "pending" };
    const { container, getByLabelText } = renderModal();

    // Photo A loaded: the editor box uses its portrait naturals.
    await waitFor(() => getByLabelText("Adjust crop"));
    fireEvent.click(getByLabelText("Adjust crop"));
    await waitFor(() => {
      expect(px(cropEditorBox(container).style.height)).toBe(WRAP.h);
    });

    // Switch to photo B, whose full image never finishes loading. The stale
    // imgEl of photo A must be dropped — the box falls back to B's stored dims
    // (1600×1000, landscape) instead of keeping A's portrait naturals.
    const thumbB = container.querySelector('img[src*="ph-2"]')?.closest("button");
    if (!thumbB) throw new Error("photo B thumbnail not found");
    fireEvent.click(thumbB);

    await waitFor(() => getByLabelText("Adjust crop"));
    fireEvent.click(getByLabelText("Adjust crop"));
    await waitFor(() => {
      const box = cropEditorBox(container);
      expect(px(box.style.width)).toBe(WRAP.w);
      expect(px(box.style.height)).toBe(Math.floor(WRAP.w * (1000 / 1600)));
    });
  });

  it("shows an error toast when the full photo fails to load", async () => {
    imageMode = { "ph-1": "error" };
    renderModal();

    await waitFor(() => {
      const toasts = useToastStore.getState().toasts;
      expect(toasts.some((t) => t.type === "error" && t.message.includes("Failed to load photo preview"))).toBe(true);
    });
  });
});
