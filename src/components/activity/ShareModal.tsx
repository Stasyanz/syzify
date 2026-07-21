import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { save } from "@tauri-apps/plugin-dialog";
import { renderShareCanvas, drawCrop, cropOutputSize, cropBackdrop } from "./shareCanvas";
import { fitRotatedScale, clampCropRotated } from "./cropOverlayMath";
import { X, Download, Check, Undo2, Crop as CropIcon } from "lucide-react";
import { api } from "../../lib/tauri";
import { useToastStore } from "../../stores/toastStore";
import type { Activity, Photo, TrackPointColumns } from "../../lib/types";
import {
  formatDistance,
  formatDuration,
  formatPaceOrSpeed,
  paceOrSpeedLabel,
  formatElevation,
  formatHR,
} from "../../lib/format";
import { useUnits } from "../../lib/units";
import { photoUrl } from "./photoUrl";
import { DraggableShareBlocks, BrandMark } from "./DraggableShareBlocks";
import { Checkbox } from "../ui/Checkbox";
import { CropOverlay } from "./CropOverlay";
import {
  defaultPositions,
  defaultScales,
  fullCrop,
  centeredCrop,
  normalizeStraighten,
  straightenQuarter,
  autoQuarterOrientation,
  MAX_STRAIGHTEN,
  CROP_PRESETS,
  type BlockKind,
  type BlockPositions,
  type BlockScales,
  type CropRect,
} from "./shareLayout";

interface Props {
  activity: Activity;
  trackpoints: TrackPointColumns;
  initialPhoto: Photo | null;
  onClose: () => void;
}

type Theme = "dark" | "light";

interface Field {
  key: string;
  label: string;
  value: string;
  group: "basic" | "advanced";
}

function buildFields(activity: Activity): Field[] {
  const fields: Field[] = [
    { key: "distance", label: "Distance", value: formatDistance(activity.distance_m), group: "basic" },
    { key: "duration", label: "Duration", value: formatDuration(activity.duration_s), group: "basic" },
    {
      key: "pace",
      label: `Avg ${paceOrSpeedLabel(activity.sport_type)}`,
      value: formatPaceOrSpeed(activity.sport_type, activity.avg_speed_mps),
      group: "basic",
    },
    { key: "elev_gain", label: "Elev Gain", value: formatElevation(activity.elev_gain_m), group: "basic" },
  ];

  if (activity.avg_hr != null) {
    fields.push({ key: "avg_hr", label: "Avg HR", value: formatHR(activity.avg_hr), group: "advanced" });
  }
  if (activity.max_hr != null) {
    fields.push({ key: "max_hr", label: "Max HR", value: formatHR(activity.max_hr), group: "advanced" });
  }
  if (activity.avg_power_w != null) {
    fields.push({
      key: "avg_power",
      label: "Avg Power",
      value: `${Math.round(activity.avg_power_w)} W`,
      group: "advanced",
    });
  }
  if (activity.max_power_w != null) {
    fields.push({
      key: "max_power",
      label: "Max Power",
      value: `${Math.round(activity.max_power_w)} W`,
      group: "advanced",
    });
  }
  if (activity.avg_cadence != null) {
    fields.push({
      key: "avg_cadence",
      label: "Avg Cadence",
      value: `${Math.round(activity.avg_cadence)} spm`,
      group: "advanced",
    });
  }
  if (activity.calories != null) {
    fields.push({
      key: "calories",
      label: "Calories",
      value: `${Math.round(activity.calories)} kcal`,
      group: "advanced",
    });
  }
  if (activity.avg_temperature_c != null) {
    fields.push({
      key: "temp",
      label: "Avg Temp",
      value: `${Math.round(activity.avg_temperature_c)}°C`,
      group: "advanced",
    });
  }
  if (activity.training_stress_score != null) {
    fields.push({
      key: "tss",
      label: "TSS",
      value: `${Math.round(activity.training_stress_score)}`,
      group: "advanced",
    });
  }

  return fields;
}

export function ShareModal({ activity, trackpoints, initialPhoto, onClose }: Props) {
  const units = useUnits();
  const addToast = useToastStore((s) => s.addToast);
  const previewWrapRef = useRef<HTMLDivElement>(null);
  const composeCanvasRef = useRef<HTMLCanvasElement>(null);
  const [previewBox, setPreviewBox] = useState<{ w: number; h: number }>({ w: 0, h: 0 });
  const [wrapSize, setWrapSize] = useState<{ w: number; h: number }>({ w: 0, h: 0 });
  // The full photo loaded as an <img>, used to paint the de-rotated crop into the
  // compose-preview canvas with the exact same code path as the export.
  const [imgEl, setImgEl] = useState<HTMLImageElement | null>(null);
  const [exporting, setExporting] = useState(false);
  const [theme, setTheme] = useState<Theme>("dark");
  const [transparentBg, setTransparentBg] = useState(true);
  const [showTitle, setShowTitle] = useState(true);
  const [showMap, setShowMap] = useState(true);
  const [showElevation, setShowElevation] = useState(true);
  const [positions, setPositions] = useState<BlockPositions>(defaultPositions());
  const [scales, setScales] = useState<BlockScales>(defaultScales());
  const [crop, setCrop] = useState<CropRect>(fullCrop());
  const [cropMode, setCropMode] = useState(false);
  const [cropPreset, setCropPreset] = useState("free");

  const updatePosition = useCallback(
    (kind: BlockKind, pos: { x: number; y: number }) =>
      setPositions((prev) => ({ ...prev, [kind]: pos })),
    []
  );
  const updateScale = useCallback(
    (kind: BlockKind, scale: number) => setScales((prev) => ({ ...prev, [kind]: scale })),
    []
  );

  // buildFields runs the unit-aware formatters, so it must recompute when the
  // metric/imperial preference changes, not only when the activity does.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  const allFields = useMemo(() => buildFields(activity), [activity, units]);
  const [selectedFields, setSelectedFields] = useState<Set<string>>(
    () => new Set(allFields.filter((f) => f.group === "basic").map((f) => f.key))
  );
  // Stable identity for the chosen fields, so a block drag's re-renders don't
  // hand DraggableShareBlocks a fresh array every mousemove.
  const shareFields = useMemo(
    () => allFields.filter((f) => selectedFields.has(f.key)),
    [allFields, selectedFields]
  );

  const { data: photos = [] } = useQuery({
    queryKey: ["photos", activity.id],
    queryFn: () => api.getPhotos(activity.id),
  });

  const [photoId, setPhotoId] = useState<string | null>(initialPhoto?.id ?? null);

  useEffect(() => {
    if (!photoId && photos.length > 0) setPhotoId(photos[0].id);
  }, [photos, photoId]);

  const photo = useMemo(() => photos.find((p) => p.id === photoId) ?? null, [photos, photoId]);

  // Reset the crop when switching photos (a different photo has a different aspect).
  useEffect(() => {
    setCrop(fullCrop());
    setCropPreset("free");
    setCropMode(false);
  }, [photoId]);

  // Prefer the browser's natural (EXIF-applied) dimensions so the crop math matches the
  // exported <img> exactly; fall back to the stored dims until the image has loaded.
  const photoW = imgEl?.naturalWidth || photo?.width || 1920;
  const photoH = imgEl?.naturalHeight || photo?.height || 1080;

  // Single normalized crop used everywhere downstream (preview + export), so a
  // bad value in state can't reach the renderer as a NaN/empty region. The
  // rotation-aware clamp keeps tilted frames legal (their normalized w/h may
  // exceed 1 — see clampCropRotated) instead of snapping them to the photo box.
  const safeCrop = useMemo(() => clampCropRotated(crop, photoW, photoH), [crop, photoW, photoH]);

  // Presets shape the frame's LOCAL sides. With no ±90° buttons, orientation
  // comes solely from autoQuarterOrientation folds, so the frame's local aspect
  // and the exported aspect only differ by the quarter the user physically
  // turned the frame — a 16:9 preset on a turned frame is a turned 16:9.
  const activeRatio = CROP_PRESETS.find((p) => p.key === cropPreset)?.ratio ?? null;

  function applyCropPreset(key: string) {
    setCropPreset(key);
    setCropMode(true);
    const ratio = CROP_PRESETS.find((p) => p.key === key)?.ratio;
    if (ratio && photo) {
      // keep the current rotation when switching aspect; shrink the fresh crop so a
      // tilted frame's rotated bbox still fits the photo (no instant blank corners)
      setCrop((c) => {
        const base = centeredCrop(photoW, photoH, ratio);
        const s = fitRotatedScale(base.w * photoW, base.h * photoH, (c.straighten * Math.PI) / 180, photoW, photoH);
        const w = base.w * s;
        const h = base.h * s;
        return { ...c, x: (1 - w) / 2, y: (1 - h) / 2, w, h };
      });
    }
  }

  // The Straighten slider edits only the RESIDUAL tilt on top of whatever
  // quarter the knob put the frame in: the knob is unbounded (±180) while the
  // slider spans ±45. Binding the slider to the raw angle lied past 45° (a
  // vertical frame read "90°" on a ±45 control) and the first touch snapped
  // the quarter away, flipping the whole frame.
  const residualTilt = normalizeStraighten(
    safeCrop.straighten - straightenQuarter(safeCrop.straighten)
  );

  function setResidualTilt(residual: number) {
    setCrop((c) => {
      const next = straightenQuarter(c.straighten) + residual;
      return autoQuarterOrientation(c, { ...c, straighten: normalizeStraighten(next) });
    });
  }

  // Crop edits from the overlay: fold quarter turns of the frame into the
  // output orientation, so a frame turned vertical exports a vertical image.
  const updateCrop = useCallback(
    (next: CropRect) => setCrop((prev) => autoQuarterOrientation(prev, next)),
    []
  );

  function resetCrop() {
    setCrop(fullCrop());
    setCropPreset("free");
    setCropMode(false);
  }

  const hasMap = trackpoints.lat.some((v) => v != null);
  const hasElevation = trackpoints.altitude_m.some((v) => v != null);

  // Compute the preview box that fits inside the wrapper while preserving the photo's aspect ratio.
  useEffect(() => {
    const el = previewWrapRef.current;
    if (!el || !photo) return;
    const ratio = photoW / photoH;
    const recompute = () => {
      const cw = el.clientWidth;
      const ch = el.clientHeight;
      if (cw <= 0 || ch <= 0) return;
      setWrapSize({ w: cw, h: ch });
      let w = cw;
      let h = w / ratio;
      if (h > ch) {
        h = ch;
        w = h * ratio;
      }
      setPreviewBox({ w: Math.floor(w), h: Math.floor(h) });
    };
    recompute();
    const ro = new ResizeObserver(recompute);
    ro.observe(el);
    return () => ro.disconnect();
  }, [photo, photoW, photoH]);

  // Final output width (accounts for the 90° orientation swap) — drives block scale.
  const outSize = useMemo(() => cropOutputSize(safeCrop, photoW, photoH), [safeCrop, photoW, photoH]);

  // Display size of the cropped output (compose preview), fit into the wrap.
  const cropDisp = useMemo(() => {
    const aspect = outSize.W / outSize.H;
    const { w: cw, h: ch } = wrapSize;
    if (cw <= 0 || ch <= 0 || !Number.isFinite(aspect) || aspect <= 0) return { w: 0, h: 0 };
    let w = cw;
    let h = w / aspect;
    if (h > ch) { h = ch; w = h * aspect; }
    return { w: Math.floor(w), h: Math.floor(h) };
  }, [outSize.W, outSize.H, wrapSize.w, wrapSize.h]);

  // Load the full photo as an <img> so the compose preview can paint the de-rotated
  // crop into a canvas with the exact export code path (drawCroppedPhoto).
  useEffect(() => {
    // Drop the previous photo's element right away so a slow or failed load can't
    // leave the compose preview painting a stale image for the new photo.
    setImgEl(null);
    if (!photoId) return;
    const im = new Image();
    im.onload = () => setImgEl(im);
    im.onerror = () => addToast("error", "Failed to load photo preview");
    im.src = photoUrl(photoId, "full");
    return () => {
      im.onload = null;
      im.onerror = null;
    };
  }, [photoId, addToast]);

  // Paint the cropped/straightened/oriented output into the compose-preview canvas
  // through the exact export code path (drawCrop) — WYSIWYG.
  useEffect(() => {
    if (cropMode) return;
    const cv = composeCanvasRef.current;
    if (!cv || !imgEl) return;
    const { w: Dw, h: Dh } = cropDisp;
    if (Dw <= 0 || Dh <= 0) return;
    cv.width = Dw;
    cv.height = Dh;
    const ctx = cv.getContext("2d");
    if (!ctx) return;
    // Opaque backdrop matching the export, so an overhanging rotated frame shows the
    // same corners here as in the saved PNG.
    ctx.fillStyle = cropBackdrop(theme);
    ctx.fillRect(0, 0, Dw, Dh);
    drawCrop(ctx, imgEl, safeCrop, imgEl.naturalWidth, imgEl.naturalHeight, Dw, Dh);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [imgEl, cropMode, cropDisp.w, cropDisp.h, theme, safeCrop.x, safeCrop.y, safeCrop.w, safeCrop.h, safeCrop.straighten, safeCrop.orientation]);

  function toggleField(key: string) {
    setSelectedFields((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  }

  async function handleExport() {
    if (!photo) return;
    setExporting(true);
    try {
      const photoDataUrl = await api.getPhotoDataUrl(photo.id, "full");
      const fields = shareFields;

      const dataUrl = await renderShareCanvas({
        photoDataUrl,
        activity,
        trackpoints,
        fields: fields.map((f) => ({ label: f.label, value: f.value })),
        theme,
        transparentBg,
        showTitle,
        showMap: showMap && hasMap,
        showElevation: showElevation && hasElevation,
        positions,
        scales,
        crop: safeCrop,
      });

      const defaultName = `${activity.start_time.slice(0, 10)}_${activity.sport_type}_share.png`;
      const dest = await save({
        defaultPath: defaultName,
        filters: [{ name: "PNG", extensions: ["png"] }],
      });
      if (!dest) {
        setExporting(false);
        return;
      }

      const base64 = dataUrl.split(",")[1] ?? "";
      await api.saveShareImage(dest, base64);

      addToast("success", "Share image saved");
      onClose();
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      addToast("error", `Export failed: ${msg}`);
    } finally {
      setExporting(false);
    }
  }

  return (
    // Deliberately NOT closing on backdrop click: a stray click while
    // arranging share blocks or cropping would silently discard the work.
    // Closing is explicit — the header's X.
    <div className="fixed inset-0 z-[9999] flex items-center justify-center bg-black/60 p-4">
      <div className="bg-card rounded-lg shadow-xl flex flex-col lg:flex-row max-w-[95vw] max-h-[95vh] w-full h-full overflow-hidden">
        {/* Preview */}
        <div
          ref={previewWrapRef}
          className="flex-1 bg-gray-900 flex items-center justify-center p-4 min-h-0 overflow-hidden"
        >
          {photo ? (
            cropMode ? (
              // Crop-edit mode: the photo with an interactive, rotatable crop box (drag
              // the top knob to rotate the frame; corners resize, body moves).
              <div
                style={{
                  position: "relative",
                  width: previewBox.w || 1,
                  height: previewBox.h || 1,
                  background: "#000",
                  visibility: previewBox.w > 0 ? "visible" : "hidden",
                }}
              >
                <img
                  src={photoUrl(photo.id, "full")}
                  alt=""
                  style={{ position: "absolute", inset: 0, width: "100%", height: "100%", objectFit: "cover" }}
                />
                <CropOverlay
                  boxW={previewBox.w}
                  boxH={previewBox.h}
                  crop={crop}
                  ratio={activeRatio}
                  onChange={updateCrop}
                >
                  {/* The watermark stays visible while cropping, at the size it
                      will have on the export: previewBox.w/photoW converts
                      export px → display px, so shrinking the frame grows the
                      mark relative to it, exactly like the final image. */}
                  <BrandMark
                    theme={theme}
                    previewWidth={(outSize.W * previewBox.w) / photoW}
                    exportWidth={outSize.W}
                  />
                </CropOverlay>
              </div>
            ) : (
              // Compose mode: the de-rotated crop painted into a canvas (exact export
              // code path) with upright overlay blocks — a pixel-faithful WYSIWYG.
              <div
                style={{
                  position: "relative",
                  width: cropDisp.w || 1,
                  height: cropDisp.h || 1,
                  background: "#000",
                  visibility: cropDisp.w > 0 ? "visible" : "hidden",
                }}
              >
                <canvas
                  ref={composeCanvasRef}
                  style={{ position: "absolute", inset: 0, width: "100%", height: "100%", display: "block" }}
                />
                {/* overflow:hidden clips a block dragged past the edge, like the export */}
                <div style={{ position: "absolute", inset: 0, overflow: "hidden" }}>
                  <DraggableShareBlocks
                    activity={activity}
                    trackpoints={trackpoints}
                    fields={shareFields}
                    theme={theme}
                    transparentBg={transparentBg}
                    showTitle={showTitle}
                    showMap={showMap && hasMap}
                    showElevation={showElevation && hasElevation}
                    positions={positions}
                    onPositionChange={updatePosition}
                    scales={scales}
                    onScaleChange={updateScale}
                    previewWidth={cropDisp.w}
                    previewHeight={cropDisp.h}
                    exportWidth={outSize.W}
                  />
                </div>
              </div>
            )
          ) : (
            <div className="text-faint text-sm">
              No photos attached. Add photos to this activity first.
            </div>
          )}
        </div>

        {/* Controls */}
        <div className="w-full lg:w-80 border-l border-border flex flex-col">
          <div className="flex items-center justify-between p-4 border-b border-border">
            <h2 className="font-semibold text-ink">Share image</h2>
            <div className="flex items-center gap-2">
              <button
                onClick={onClose}
                data-tip="Close"
                aria-label="Close"
                className="text-faint hover:text-muted"
              >
                <X size={18} />
              </button>
            </div>
          </div>

          <div className="flex-1 overflow-y-auto p-4 space-y-5">
            {photos.length > 1 && (
              <section>
                <h3 className="text-xs font-medium text-muted uppercase tracking-wide mb-2">Photo</h3>
                <div className="grid grid-cols-4 gap-1.5">
                  {photos.map((p) => (
                    <button
                      key={p.id}
                      onClick={() => setPhotoId(p.id)}
                      className={`aspect-square rounded overflow-hidden border-2 ${
                        photoId === p.id ? "border-accent" : "border-transparent"
                      }`}
                    >
                      <img
                        src={photoUrl(p.id, "thumb")}
                        alt=""
                        className="w-full h-full object-cover"
                      />
                    </button>
                  ))}
                </div>
              </section>
            )}

            <section>
              <h3 className="text-xs font-medium text-muted uppercase tracking-wide mb-2">Theme</h3>
              <div className="flex gap-2">
                {(["dark", "light"] as Theme[]).map((t) => (
                  <button
                    key={t}
                    onClick={() => setTheme(t)}
                    className={`flex-1 px-3 py-1.5 rounded text-sm capitalize border ${
                      theme === t
                        ? "bg-accent-soft border-border-2 text-accent-2"
                        : "border-border text-muted hover:bg-card-2"
                    }`}
                  >
                    {t}
                  </button>
                ))}
              </div>
              <label className="flex items-center gap-2 text-sm text-ink mt-2 cursor-pointer">
                <Checkbox checked={transparentBg} onChange={setTransparentBg} />
                Transparent block background
              </label>
            </section>

            <section>
              <h3 className="text-xs font-medium text-muted uppercase tracking-wide mb-2">Crop</h3>
              {/* One row: aspect presets on the left, crop actions (icons) on the right */}
              <div className="flex items-center gap-1.5 flex-wrap">
                {CROP_PRESETS.map((p) => (
                  <button
                    key={p.key}
                    onClick={() => applyCropPreset(p.key)}
                    className={`px-3 py-1.5 rounded text-sm border ${
                      cropPreset === p.key
                        ? "bg-accent-soft border-border-2 text-accent-2"
                        : "border-border text-muted hover:bg-card-2"
                    }`}
                  >
                    {p.label}
                  </button>
                ))}
                <div className="ml-auto flex items-center gap-1.5">
                  <button
                    onClick={() => setCropMode((m) => !m)}
                    title={cropMode ? "Apply crop" : "Adjust crop"}
                    aria-label={cropMode ? "Apply crop" : "Adjust crop"}
                    className={`p-1.5 rounded border ${
                      cropMode
                        ? "bg-accent border-accent-2 text-white hover:bg-accent-2"
                        : "border-border text-muted hover:bg-card-2"
                    }`}
                  >
                    {cropMode ? <Check size={16} /> : <CropIcon size={16} />}
                  </button>
                  <button
                    onClick={resetCrop}
                    data-tip="Reset crop"
                    aria-label="Reset crop"
                    className="p-1.5 rounded border border-border text-muted hover:bg-card-2"
                  >
                    <Undo2 size={16} />
                  </button>
                </div>
              </div>

              {/* Straighten: fine horizon leveling (±45°); double-click resets to 0.
                  Quarter turns are made with the frame's rotation knob — the
                  auto-fold turns them into the output orientation. */}
              <div className="flex items-center gap-2 mt-2">
                <span className="text-xs text-faint w-14">Straighten</span>
                <input
                  type="range"
                  min={-MAX_STRAIGHTEN}
                  max={MAX_STRAIGHTEN}
                  step={1}
                  value={Math.round(residualTilt)}
                  onChange={(e) => setResidualTilt(Number(e.target.value))}
                  onDoubleClick={() => setResidualTilt(0)}
                  className="flex-1"
                  aria-label="Straighten crop frame"
                />
                <span className="w-10 text-right text-xs tabular-nums text-muted">{Math.round(residualTilt)}°</span>
              </div>

              {cropMode ? (
                <p className="text-xs text-faint mt-1.5">
                  Drag to move, corners to resize{activeRatio ? " (locked ratio)" : ""}, the top knob to rotate the frame
                  freely (Shift snaps to 15°, snaps square near 0/90/180°). Turning the frame a quarter gives a vertical crop.
                </p>
              ) : (
                <p className="text-xs text-faint mt-1.5">Showing the cropped result. Press the crop button to change the region.</p>
              )}
            </section>

            <section>
              <h3 className="text-xs font-medium text-muted uppercase tracking-wide mb-2">Visuals</h3>
              <label className="flex items-center gap-2 text-sm text-ink mb-1.5 cursor-pointer">
                <Checkbox checked={showTitle} onChange={setShowTitle} />
                Title &amp; date
              </label>
              <label className={`flex items-center gap-2 text-sm mb-1.5 ${hasMap ? "text-ink cursor-pointer" : "text-muted"}`}>
                <Checkbox checked={showMap && hasMap} disabled={!hasMap} onChange={setShowMap} />
                Route map {!hasMap && <span className="text-faint">(no GPS)</span>}
              </label>
              <label className={`flex items-center gap-2 text-sm ${hasElevation ? "text-ink cursor-pointer" : "text-muted"}`}>
                <Checkbox
                  checked={showElevation && hasElevation}
                  disabled={!hasElevation}
                  onChange={setShowElevation}
                />
                Elevation profile {!hasElevation && <span className="text-faint">(no data)</span>}
              </label>
            </section>

            <section>
              <h3 className="text-xs font-medium text-muted uppercase tracking-wide mb-2">Metrics</h3>
              <div className="space-y-1.5">
                {allFields.map((f) => (
                  <label key={f.key} className="flex items-center gap-2 text-sm text-ink cursor-pointer">
                    <Checkbox checked={selectedFields.has(f.key)} onChange={() => toggleField(f.key)} />
                    <span>{f.label}</span>
                    <span className="ml-auto text-faint text-xs">{f.value}</span>
                  </label>
                ))}
              </div>
            </section>
          </div>

          <div className="border-t border-border p-4">
            <button
              onClick={handleExport}
              disabled={!photo || exporting}
              className="w-full flex items-center justify-center gap-2 bg-accent hover:bg-accent-2 text-white font-medium py-2 rounded disabled:opacity-50"
            >
              <Download size={16} />
              {exporting ? "Exporting..." : "Save PNG"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
