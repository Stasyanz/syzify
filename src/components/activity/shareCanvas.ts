import type { Activity, TrackPointColumns } from "../../lib/types";
import { SPORT_LABELS, type SportType } from "../../lib/types";
import { formatDate } from "../../lib/format";
import type { BlockPositions, BlockScales, CropRect } from "./shareLayout";
import { fullCrop, defaultScales, clampBlockScale, normalizeStraighten, normalizeOrientation } from "./shareLayout";
import { clampCropRotated } from "./cropOverlayMath";
import { computeMapLayout, computeElevationLayout } from "./shareGeometry";

export interface CanvasField {
  label: string;
  value: string;
}

export interface RenderOptions {
  photoDataUrl: string;
  activity: Activity;
  trackpoints: TrackPointColumns;
  fields: CanvasField[];
  theme: "dark" | "light";
  showTitle: boolean;
  showMap: boolean;
  showElevation: boolean;
  positions: BlockPositions;
  /** Per-block size multipliers; defaults to 1 for every block. */
  scales?: BlockScales;
  /** Optional crop in normalized [0..1] coords; defaults to the whole photo. */
  crop?: CropRect;
  /** Draw overlay blocks without their chip plate — the photo shows through;
   * glyphs get a soft theme-matched halo instead so they stay readable. */
  transparentBg?: boolean;
}

function loadImage(src: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => resolve(img);
    img.onerror = () => reject(new Error("Failed to load image"));
    img.src = src;
  });
}

function getRoutePoints(tps: TrackPointColumns): { lat: number; lon: number }[] {
  const pts: { lat: number; lon: number }[] = [];
  for (let i = 0; i < tps.lat.length; i++) {
    const lat = tps.lat[i];
    const lon = tps.lon[i];
    if (lat != null && lon != null) pts.push({ lat, lon });
  }
  return pts;
}

function getAltitudes(tps: TrackPointColumns): number[] {
  const out: number[] = [];
  for (const a of tps.altitude_m) if (a != null) out.push(a);
  return out;
}

/**
 * Output size (px) of the cropped image for a reference photo size `refW × refH`.
 * The 90°/270° orientation swaps width↔height (a landscape crop → a vertical image);
 * straighten keeps the aspect (auto-zoom).
 */
export function cropOutputSize(
  crop: CropRect,
  refW: number,
  refH: number
): { W: number; H: number } {
  const rawW = crop.w * refW;
  const rawH = crop.h * refH;
  if (!(rawW > 0) || !(rawH > 0)) return { W: 1, H: 1 };
  // drawCrop derives its single scale `k` from the output WIDTH, so the height must
  // be derived from the (rounded) width on the same ratio. Rounding W and H
  // independently would leave an empty/clipped strip along the long side of an
  // extreme-aspect crop (the same class as the old floor-constant drift).
  const baseW = Math.max(1, Math.round(rawW));
  const baseH = Math.max(1, Math.round((baseW * rawH) / rawW));
  const q = normalizeOrientation(crop.orientation);
  const swap = q === 90 || q === 270;
  return { W: swap ? baseH : baseW, H: swap ? baseW : baseH };
}

/**
 * Draws the crop into an `outW`×`outH` output, matching the crop editor: the crop
 * rect is tilted by `straighten` around its own centre (the rotatable frame), the
 * tilted region is de-rotated to upright, then `orientation` turns the result a clean
 * quarter-turn (90/270 → vertical). All coordinates are in the photo's natural
 * (EXIF-applied) `natW × natH` space — the only coordinate system the crop uses.
 * A tilted frame that overhangs the photo samples transparent there (the editor
 * shows the box, so it's visible). Exported for the tests.
 */
export function drawCrop(
  ctx: CanvasRenderingContext2D,
  img: CanvasImageSource,
  crop: CropRect,
  natW: number,
  natH: number,
  outW: number,
  outH: number
): void {
  if (!(crop.w > 0) || !(crop.h > 0) || natW <= 0 || natH <= 0 || outW <= 0 || outH <= 0) return;
  const theta = (normalizeStraighten(crop.straighten) * Math.PI) / 180;
  const q = normalizeOrientation(crop.orientation);
  if (theta === 0 && q === 0) {
    // Fast path, byte-identical to the original crop export (regression-safe).
    ctx.drawImage(img, crop.x * natW, crop.y * natH, crop.w * natW, crop.h * natH, 0, 0, outW, outH);
    return;
  }
  const swap = q === 90 || q === 270;
  const outBaseW = swap ? outH : outW; // pre-orientation output width
  const k = outBaseW / (crop.w * natW); // tilted-rect → output scale (uniform; aspect kept)
  const ccx = (crop.x + crop.w / 2) * natW; // crop centre (tilt pivot)
  const ccy = (crop.y + crop.h / 2) * natH;
  ctx.save();
  ctx.translate(outW / 2, outH / 2);
  ctx.rotate((q * Math.PI) / 180); // orientation quarter-turn
  ctx.scale(k, k); // tilted-rect → output scale
  ctx.rotate(-theta); // de-rotate the tilted crop rect to upright
  ctx.translate(-ccx, -ccy);
  ctx.drawImage(img, 0, 0);
  ctx.restore();
}

/**
 * Backdrop behind the cropped photo: the corners of an overhanging rotated frame show
 * this color. One source for the compose preview and the export keeps them WYSIWYG.
 */
export function cropBackdrop(theme: "dark" | "light"): string {
  return theme === "dark" ? "#000" : "#fff";
}

function roundRect(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, r: number) {
  const rr = Math.min(r, w / 2, h / 2);
  ctx.beginPath();
  ctx.moveTo(x + rr, y);
  ctx.lineTo(x + w - rr, y);
  ctx.arcTo(x + w, y, x + w, y + rr, rr);
  ctx.lineTo(x + w, y + h - rr);
  ctx.arcTo(x + w, y + h, x + w - rr, y + h, rr);
  ctx.lineTo(x + rr, y + h);
  ctx.arcTo(x, y + h, x, y + h - rr, rr);
  ctx.lineTo(x, y + rr);
  ctx.arcTo(x, y, x + rr, y, rr);
  ctx.closePath();
}

function fontStack(weight: number, size: number) {
  return `${weight} ${size}px ui-sans-serif, -apple-system, system-ui, "Segoe UI", Roboto, sans-serif`;
}

export interface DrawCtx {
  ctx: CanvasRenderingContext2D;
  W: number;
  H: number;
  theme: "dark" | "light";
  baseFs: number;
  transparentBg?: boolean;
}

/** Fill the chip plate behind a block — skipped in transparent-background mode. */
function drawChipBg(d: DrawCtx, x: number, y: number, w: number, h: number): void {
  if (d.transparentBg) return;
  d.ctx.fillStyle = chipColors(d.theme).bg;
  roundRect(d.ctx, x, y, w, h, 12);
  d.ctx.fill();
}

/** Brand mark geometry (32×32 viewBox) — single source for the canvas export
 * and the SVG preview (same contract as shareGeometry): the Syzify ridge line
 * with the "boulder" dot, see components/brand/Logo.tsx. */
export const BRAND_MARK_PATH = "M2 32 L10 16 L15 20 L23 8 L30 32 Z";
export const BRAND_MARK_DOT = { x: 17.6, y: 9.8, r: 3.2 };
/** The boulder keeps the brand terracotta on any photo/theme. */
export const BRAND_ACCENT = "#ef6a2c";
export const BRAND_WORDMARK = "Syzify";
/** What actually gets typeset: a dotless ı (U+0131) whose dot we draw in
 * the brand accent — the wordmark's own boulder, mirroring Logo.tsx. */
export const BRAND_WORDMARK_TYPESET = "Syzıfy";
/** i-dot geometry in wordFs units (system font, tuned on the harness):
 * center height above the baseline, and radius. One source for canvas and
 * the DOM preview so WYSIWYG holds. */
export const BRAND_IDOT_BASELINE_EM = 0.66;
export const BRAND_IDOT_R_EM = 0.075;
/** The same dot in the DOM preview: CSS `bottom` on an inline-block wrapper
 * measures from the line box, not the baseline, so the canvas value doesn't
 * transfer — this is its visual equivalent, matched side-by-side against
 * drawBrandMark on the harness. */
export const BRAND_IDOT_PREVIEW_BOTTOM_EM = 0.6;
/** Watermark sizing in baseFs units — one source for canvas and preview. */
export const BRAND_MARK_FS = 3.6;
export const BRAND_WORD_FS = 2.16;

function chipColors(theme: "dark" | "light") {
  return {
    bg: theme === "dark" ? "rgba(0,0,0,0.55)" : "rgba(255,255,255,0.78)",
    text: theme === "dark" ? "#ffffff" : "#0a0a0a",
    subtext: theme === "dark" ? "rgba(255,255,255,0.7)" : "rgba(0,0,0,0.55)",
    stroke: theme === "dark" ? "#ffffff" : "#0a0a0a",
  };
}

function drawTitleBlock(
  d: DrawCtx,
  pos: { x: number; y: number },
  activity: Activity,
  scale = 1
): void {
  const { ctx, W, theme } = d;
  const baseFs = d.baseFs * scale;
  const c = chipColors(theme);
  const sportLabel = SPORT_LABELS[activity.sport_type as SportType] ?? "Activity";
  const title = activity.title ?? sportLabel;
  const metaStr = `${sportLabel.toUpperCase()} · ${formatDate(activity.start_time).toUpperCase()}`;
  const padX = baseFs;
  const padY = baseFs * 0.7;

  const metaFs = baseFs * 0.75;
  const titleFs = baseFs * 1.7;
  const locFs = baseFs * 0.85;
  const gap = baseFs * 0.15;

  ctx.font = fontStack(500, metaFs);
  const metaW = ctx.measureText(metaStr).width;
  ctx.font = fontStack(700, titleFs);
  const titleW = ctx.measureText(title).width;
  let locW = 0;
  if (activity.location_name) {
    ctx.font = fontStack(400, locFs);
    locW = ctx.measureText(activity.location_name).width;
  }
  const contentW = Math.max(metaW, titleW, locW);
  const blockW = contentW + padX * 2;
  const blockH =
    padY * 2 + metaFs + gap + titleFs + (activity.location_name ? gap + locFs : 0);

  const x = pos.x * W;
  const y = pos.y * d.H;

  drawChipBg(d, x, y, blockW, blockH);

  ctx.save();
  let cy = y + padY + metaFs;
  ctx.font = fontStack(500, metaFs);
  ctx.fillStyle = c.subtext;
  ctx.fillText(metaStr, x + padX, cy);
  cy += gap + titleFs;
  ctx.font = fontStack(700, titleFs);
  ctx.fillStyle = c.text;
  ctx.fillText(title, x + padX, cy);
  if (activity.location_name) {
    cy += gap + locFs;
    ctx.font = fontStack(400, locFs);
    ctx.fillStyle = c.subtext;
    ctx.fillText(activity.location_name, x + padX, cy);
  }
  ctx.restore();
}

function drawMetricsBlock(
  d: DrawCtx,
  pos: { x: number; y: number },
  fields: CanvasField[],
  scale = 1
): void {
  if (fields.length === 0) return;
  const { ctx, W, theme } = d;
  const baseFs = d.baseFs * scale;
  const c = chipColors(theme);
  const padX = baseFs;
  const padY = baseFs * 0.7;
  const labelFs = baseFs * 0.65;
  const valueFs = baseFs * 1.35;
  const colGap = baseFs * 1.2;
  const rowGap = baseFs * 0.6;

  const cols = Math.min(fields.length, 4);
  const rows = Math.ceil(fields.length / cols);

  const colWidths: number[] = new Array(cols).fill(0);
  for (let i = 0; i < fields.length; i++) {
    const col = i % cols;
    ctx.font = fontStack(500, labelFs);
    const lw = ctx.measureText(fields[i].label.toUpperCase()).width;
    ctx.font = fontStack(700, valueFs);
    const vw = ctx.measureText(fields[i].value).width;
    colWidths[col] = Math.max(colWidths[col], lw, vw);
  }
  const cellH = labelFs + valueFs + 2;
  const contentW = colWidths.reduce((a, b) => a + b, 0) + colGap * (cols - 1);
  const blockW = contentW + padX * 2;
  const blockH = padY * 2 + cellH * rows + rowGap * (rows - 1);

  const x = pos.x * W;
  const y = pos.y * d.H;

  drawChipBg(d, x, y, blockW, blockH);

  ctx.save();
  for (let i = 0; i < fields.length; i++) {
    const row = Math.floor(i / cols);
    const col = i % cols;
    const colX = x + padX + colWidths.slice(0, col).reduce((a, b) => a + b, 0) + col * colGap;
    const cellY = y + padY + row * (cellH + rowGap);

    ctx.font = fontStack(500, labelFs);
    ctx.fillStyle = c.subtext;
    ctx.fillText(fields[i].label.toUpperCase(), colX, cellY + labelFs);

    ctx.font = fontStack(700, valueFs);
    ctx.fillStyle = c.text;
    ctx.fillText(fields[i].value, colX, cellY + labelFs + valueFs + 2);
  }
  ctx.restore();
}

/** Brand watermark, bottom-right. Deliberately unconditional — it has no
 * toggle and no plate; it sits straight on the photo regardless of the
 * transparent-background option. Exported for tests. */
export function drawBrandMark(d: DrawCtx): void {
  const { ctx, W, H, theme, baseFs } = d;
  const c = chipColors(theme);
  const margin = baseFs;
  const markH = baseFs * BRAND_MARK_FS;
  const gap = markH * 0.3;
  const wordFs = baseFs * BRAND_WORD_FS;

  ctx.save();

  ctx.font = fontStack(800, wordFs);
  const wordW = ctx.measureText(BRAND_WORDMARK_TYPESET).width;
  const x = W - margin - (markH + gap + wordW);
  const y = H - margin - markH;

  const wordX = x + markH + gap;
  const wordY = y + markH;
  ctx.fillStyle = c.text;
  ctx.fillText(BRAND_WORDMARK_TYPESET, wordX, wordY);
  // The i's accent dot over the dotless ı (see BRAND_WORDMARK_TYPESET).
  const dotX = wordX + ctx.measureText("Syz").width + ctx.measureText("ı").width / 2;
  ctx.fillStyle = BRAND_ACCENT;
  ctx.beginPath();
  ctx.arc(dotX, wordY - BRAND_IDOT_BASELINE_EM * wordFs, BRAND_IDOT_R_EM * wordFs, 0, Math.PI * 2);
  ctx.fill();

  ctx.translate(x, y);
  ctx.scale(markH / 32, markH / 32);
  // Back to the theme ink after the accent i-dot — the ridge must not
  // inherit the boulder's terracotta (its own dot would vanish into it).
  ctx.fillStyle = c.text;
  ctx.fill(new Path2D(BRAND_MARK_PATH));
  ctx.fillStyle = BRAND_ACCENT;
  ctx.beginPath();
  ctx.arc(BRAND_MARK_DOT.x, BRAND_MARK_DOT.y, BRAND_MARK_DOT.r, 0, Math.PI * 2);
  ctx.fill();
  ctx.restore();
}

/** Exported for the canvas↔SVG invariant test; draws the route block onto `d.ctx`. */
export function drawMapBlock(
  d: DrawCtx,
  pos: { x: number; y: number },
  trackpoints: TrackPointColumns,
  scale = 1
): void {
  const { ctx, W, theme } = d;
  // The layout is proportional to the width it's given — scaling the input
  // width scales the whole block identically in canvas and SVG.
  const L = computeMapLayout(W * scale, getRoutePoints(trackpoints));
  if (!L) return;
  const c = chipColors(theme);

  const x = pos.x * W;
  const y = pos.y * d.H;

  drawChipBg(d, x, y, L.blockW, L.blockH);

  ctx.save();
  ctx.translate(x + L.padding, y + L.padding);
  ctx.strokeStyle = c.stroke;
  ctx.lineWidth = Math.max(2, L.innerW * 0.012);
  ctx.lineJoin = "round";
  ctx.lineCap = "round";
  ctx.stroke(new Path2D(L.d));

  const r = Math.max(3, L.innerW * 0.016);
  ctx.lineWidth = Math.max(1, L.innerW * 0.006);
  ctx.fillStyle = "#22c55e";
  ctx.beginPath(); ctx.arc(L.start.x, L.start.y, r, 0, Math.PI * 2); ctx.fill(); ctx.stroke();
  ctx.fillStyle = "#ef4444";
  ctx.beginPath(); ctx.arc(L.end.x, L.end.y, r, 0, Math.PI * 2); ctx.fill(); ctx.stroke();
  ctx.restore();
}

/** Exported for the canvas↔SVG invariant test; draws the elevation block onto `d.ctx`. */
export function drawElevationBlock(
  d: DrawCtx,
  pos: { x: number; y: number },
  trackpoints: TrackPointColumns,
  scale = 1
): void {
  const { ctx, W, theme } = d;
  const L = computeElevationLayout(W * scale, getAltitudes(trackpoints));
  if (!L) return;
  const c = chipColors(theme);

  const x = pos.x * W;
  const y = pos.y * d.H;

  drawChipBg(d, x, y, L.blockW, L.blockH);

  ctx.save();
  ctx.translate(x + L.padding, y + L.padding);
  // The translucent area fill is a veil over the photo — with no plate it
  // reads as a "not quite transparent" background, so only the line stays.
  if (!d.transparentBg) {
    ctx.fillStyle = c.stroke;
    ctx.globalAlpha = 0.18;
    ctx.fill(new Path2D(L.fillD));
    ctx.globalAlpha = 1;
  }
  ctx.strokeStyle = c.stroke;
  ctx.lineWidth = Math.max(2, L.innerW * 0.008);
  ctx.lineJoin = "round";
  ctx.stroke(new Path2D(L.lineD));
  ctx.restore();
}

export async function renderShareCanvas(opts: RenderOptions): Promise<string> {
  const img = await loadImage(opts.photoDataUrl);
  // Use the browser's natural dimensions as the reference: the <img> already applies
  // EXIF orientation, so the crop math stays in one coordinate system and a stored
  // width/height that disagrees with the pixels (EXIF-rotated phone photos) can't
  // distort the export.
  const natW = img.naturalWidth;
  const natH = img.naturalHeight;

  // Crop is normalized to the photo size; the output canvas is the cropped
  // region, and all overlay blocks are positioned in 0..1 of that region
  // (matching the preview). The rotation-aware clamp guards against
  // NaN/out-of-bounds without snapping legal tilted crops (whose normalized
  // w/h may exceed 1) back into the photo box.
  const crop = clampCropRotated(opts.crop ?? fullCrop(), natW, natH);
  const { W: width, H: height } = cropOutputSize(crop, natW, natH);

  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("Canvas 2D context unavailable");

  ctx.textBaseline = "alphabetic";
  // Opaque backdrop so a rotated frame that overhangs the photo has defined (not
  // transparent) corners, matching the compose preview.
  ctx.fillStyle = cropBackdrop(opts.theme);
  ctx.fillRect(0, 0, width, height);
  drawCrop(ctx, img, crop, natW, natH, width, height);

  const baseFs = Math.max(14, width * 0.018);
  const d: DrawCtx = {
    ctx,
    W: width,
    H: height,
    theme: opts.theme,
    baseFs,
    transparentBg: opts.transparentBg,
  };

  const scales = opts.scales ?? defaultScales();
  const scaleOf = (k: keyof BlockScales) => clampBlockScale(scales[k]);

  if (opts.showTitle) drawTitleBlock(d, opts.positions.title, opts.activity, scaleOf("title"));
  drawMetricsBlock(d, opts.positions.metrics, opts.fields, scaleOf("metrics"));
  if (opts.showMap) drawMapBlock(d, opts.positions.map, opts.trackpoints, scaleOf("map"));
  if (opts.showElevation)
    drawElevationBlock(d, opts.positions.elevation, opts.trackpoints, scaleOf("elevation"));
  drawBrandMark(d);

  return canvas.toDataURL("image/png");
}
