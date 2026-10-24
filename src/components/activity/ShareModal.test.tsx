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
// No canvas in happy-dom: the export's drawing is stood in for, the rest of
// the module (the preview's backdrop and crop) stays real.
vi.mock("./shareCanvas", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./shareCanvas")>()),
  renderShareCanvas: vi.fn(async () => "data:image/png;base64,UE5H"),
}));
import { save } from "@tauri-apps/plugin-dialog";

import { ShareModal, buildFields } from "./ShareModal";
import { api } from "../../lib/tauri";
import { t, useLanguageStore } from "../../lib/i18n";
import { commandError } from "../../test/commandError";

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

  /// A Tauri command refuses with a CommandError, not an Error: the toast
  /// must carry its words, not "undefined" (#174, #236).
  it("says a refused export in the backend's words", async () => {
    useToastStore.setState({ toasts: [] });
    vi.mocked(api.getPhotoDataUrl).mockRejectedValueOnce(commandError(null, "The vault is locked"));
    const { getByText } = renderModal();
    await waitFor(() => expect((getByText("Save PNG").closest("button") as HTMLButtonElement).disabled).toBe(false));
    fireEvent.click(getByText("Save PNG"));
    await waitFor(() => {
      const toasts = useToastStore.getState().toasts;
      expect(toasts.map((t) => `${t.type}: ${t.message}`)).toContain("error: Export failed: The vault is locked");
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

describe("buildFields (#232)", () => {
  const full = (sport: string) =>
    ({
      sport_type: sport,
      distance_m: 42195,
      duration_s: 5400,
      avg_speed_mps: 7.8,
      elev_gain_m: 1214,
      avg_hr: 150,
      max_hr: 181,
      avg_power_w: 212.6,
      max_power_w: 1013,
      avg_cadence: 88.4,
      calories: 1240.4,
      avg_temperature_c: 21.4,
      training_stress_score: 96.2,
    }) as unknown as Activity;
  const values = (a: Activity) => Object.fromEntries(buildFields(a, t).map((f) => [f.key, f.value]));
  const labels = (a: Activity) => buildFields(a, t).map((f) => f.label);

  afterEach(() => useLanguageStore.getState().setPref("system"));

  it("formats every stat a full ride carries, cadence in crank rpm", () => {
    expect(values(full("ride"))).toMatchObject({
      distance: "42.20 km",
      elev_gain: "1,214 m",
      avg_power: "213 W",
      max_power: "1,013 W",
      avg_cadence: "88 rpm",
      calories: "1,240 kcal",
      temp: "21°C",
      tss: "96",
    });
  });

  it("writes them in Russian, a run's cadence in steps", () => {
    useLanguageStore.getState().setPref("ru");
    expect(values(full("ride"))).toMatchObject({
      distance: "42,20 км",
      elev_gain: "1214 м",
      avg_power: "213 Вт",
      max_power: "1013 Вт",
      avg_cadence: "88 об/мин",
      calories: "1240 ккал",
    });
    expect(values(full("run")).avg_cadence).toBe("88 шаг/мин");
  });

  it("counts a swim's cadence in strokes", () => {
    expect(values(full("swim")).avg_cadence).toBe("88 spm");
    useLanguageStore.getState().setPref("ru");
    expect(values(full("open_water")).avg_cadence).toBe("88 гр/мин");
  });

  it("names the stats in the language, a ride's pace as its speed (#234)", () => {
    expect(labels(full("ride"))).toEqual([
      "Distance", "Duration", "Avg Speed", "Elev Gain", "Avg HR", "Max HR",
      "Avg Power", "Max Power", "Avg Cadence", "Calories", "Avg Temp", "TSS",
    ]);
    useLanguageStore.getState().setPref("ru");
    expect(labels(full("ride"))).toEqual([
      "Дистанция", "Время", "Ср. скорость", "Набор высоты", "Ср. пульс", "Макс. пульс",
      "Ср. мощность", "Макс. мощность", "Ср. каденс", "Калории", "Ср. температура", "TSS",
    ]);
    expect(labels(full("run"))[2]).toBe("Ср. темп");
    useLanguageStore.getState().setPref("it");
    expect(labels(full("swim"))[2]).toBe("Passo medio");
  });
});

describe("ShareModal in the interface language (#234)", () => {
  afterEach(() => useLanguageStore.getState().setPref("system"));

  it("writes its controls in Russian", async () => {
    useLanguageStore.getState().setPref("ru");
    const { getByText, getByLabelText, container } = renderModal();
    await waitFor(() => getByLabelText("Настроить кадрирование"));
    getByText("Поделиться изображением");
    getByLabelText("Закрыть");
    getByText("Тёмная");
    getByText("Светлая");
    getByText("Прозрачный фон блоков");
    getByText("Свободно");
    getByLabelText("Сбросить кадрирование");
    getByText("Показан кадрированный результат. Нажмите кнопку кадрирования, чтобы изменить область.");
    // No GPS and no altitude on this activity: both say why they are off.
    getByText("(нет GPS)");
    getByText("(нет данных)");
    getByText("Сохранить PNG");

    fireEvent.click(getByLabelText("Настроить кадрирование"));
    await waitFor(() => getByLabelText("Применить кадрирование"));
    expect(container.querySelector('[title^="Потяните, чтобы повернуть"]')).not.toBeNull();
    getByText(/^Перетаскивайте рамку, чтобы сдвинуть, углы — чтобы изменить размер, верхнюю/);
    // A ratio locks the frame, and the help says so.
    fireEvent.click(getByText("16:9"));
    await waitFor(() => getByText(/\(пропорции зафиксированы\)/));
  });

  it("says a failed export in Russian", async () => {
    useLanguageStore.getState().setPref("ru");
    vi.mocked(api.getPhotoDataUrl).mockRejectedValueOnce("Хранилище заблокировано");
    const { getByText } = renderModal();
    await waitFor(() => expect((getByText("Сохранить PNG").closest("button") as HTMLButtonElement).disabled).toBe(false));
    fireEvent.click(getByText("Сохранить PNG"));
    await waitFor(() =>
      expect(useToastStore.getState().toasts.map((x) => x.message)).toContain(
        "Не удалось экспортировать: Хранилище заблокировано",
      ),
    );
  });

  it("saves the picked theme's image and says so in Russian", async () => {
    useLanguageStore.getState().setPref("ru");
    vi.mocked(api.getPhotoDataUrl).mockResolvedValueOnce("data:image/jpeg;base64,SlBH");
    vi.mocked(save).mockResolvedValueOnce("/tmp/share.png");
    vi.mocked(api.saveShareImage).mockResolvedValueOnce(undefined);
    const { getByText } = renderModal();
    fireEvent.click(getByText("Светлая"));
    await waitFor(() => expect((getByText("Сохранить PNG").closest("button") as HTMLButtonElement).disabled).toBe(false));
    fireEvent.click(getByText("Сохранить PNG"));
    await waitFor(() => expect(api.saveShareImage).toHaveBeenCalledWith("/tmp/share.png", "UE5H"));
    const { renderShareCanvas } = await import("./shareCanvas");
    const drawn = vi.mocked(renderShareCanvas).mock.lastCall![0];
    expect(drawn).toMatchObject({ theme: "light" });
    // The image carries the stats in the interface language too.
    expect(drawn.fields).toEqual([
      { label: "Дистанция", value: "5,00 км" },
      { label: "Время", value: "30:00" },
      { label: "Ср. темп", value: expect.stringMatching(/^\d+:\d\d \/км$/) },
      { label: "Набор высоты", value: "50 м" },
    ]);
    await waitFor(() =>
      expect(useToastStore.getState().toasts.map((x) => x.message)).toContain("Изображение сохранено"),
    );
  });

  /** The drawing's own failures, quoted by the toast, in the language too. */
  it.each([
    ["the canvas", undefined, "Не удалось экспортировать: Холст 2D недоступен"],
    ["the photo", "error" as const, "Не удалось экспортировать: Не удалось загрузить изображение"],
  ])("says in Russian that %s could not be drawn", async (_what, photoMode, message) => {
    useLanguageStore.getState().setPref("ru");
    // The data URL carries no photo id: its mode is the empty id's.
    if (photoMode) imageMode[""] = photoMode;
    vi.mocked(api.getPhotoDataUrl).mockResolvedValueOnce("data:image/jpeg;base64,SlBH");
    const real = await vi.importActual<typeof import("./shareCanvas")>("./shareCanvas");
    const { renderShareCanvas } = await import("./shareCanvas");
    vi.mocked(renderShareCanvas).mockImplementationOnce(real.renderShareCanvas);
    const { getByText } = renderModal();
    await waitFor(() => expect((getByText("Сохранить PNG").closest("button") as HTMLButtonElement).disabled).toBe(false));
    fireEvent.click(getByText("Сохранить PNG"));
    await waitFor(() => expect(useToastStore.getState().toasts.map((x) => x.message)).toContain(message));
  });

  it("says in Russian that there is no photo to share", async () => {
    useLanguageStore.getState().setPref("ru");
    vi.mocked(api.getPhotos).mockResolvedValue([]);
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const { findByText } = render(
      <QueryClientProvider client={qc}>
        <ShareModal activity={activity} trackpoints={trackpoints} initialPhoto={null} onClose={() => {}} />
      </QueryClientProvider>,
    );
    await findByText("К тренировке не прикреплено фото. Сначала добавьте фото.");
  });
});
