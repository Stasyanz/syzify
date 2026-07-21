/**
 * Pure geometry for the rotatable crop box (CropOverlay). Kept framework-free so the
 * fiddly rotated-resize / containment / snapping math is unit-testable on its own.
 * All coordinates are in preview/display pixels; angles in radians unless noted.
 */
import {
  fullCrop,
  normalizeOrientation,
  normalizeStraighten,
  type CropRect,
} from "./shareLayout";

export const MIN_PX = 24; // smallest crop side, display px

export type Corner = "nw" | "ne" | "sw" | "se";

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

const clamp = (v: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, v));

/** Rotate a vector by `r` radians (screen / CSS convention: clockwise for +r). */
export const rot = (x: number, y: number, r: number) => ({
  x: x * Math.cos(r) - y * Math.sin(r),
  y: x * Math.sin(r) + y * Math.cos(r),
});

/** Snap a rotation (degrees): nearest 15° while Shift is held, else a cardinal angle
 * (0/±90/180) when within 4° so the frame lands square easily. */
export function snapAngle(deg: number, shift: boolean): number {
  if (shift) return Math.round(deg / 15) * 15;
  for (const c of [0, 90, 180, -90]) {
    if (Math.abs(normalizeStraighten(deg - c)) <= 4) return c;
  }
  return deg;
}

/**
 * Keep the rect's center so its rotated bounding box stays inside [0,bw]×[0,bh] when
 * it fits; otherwise center that axis (the box overhangs — shown to the user). Returns
 * the contained top-left {x,y}; w/h are unchanged.
 */
export function containRotated(
  x: number,
  y: number,
  w: number,
  h: number,
  angRad: number,
  bw: number,
  bh: number
): { x: number; y: number } {
  const hx = (Math.abs(Math.cos(angRad)) * w) / 2 + (Math.abs(Math.sin(angRad)) * h) / 2;
  const hy = (Math.abs(Math.sin(angRad)) * w) / 2 + (Math.abs(Math.cos(angRad)) * h) / 2;
  const cx = 2 * hx <= bw ? clamp(x + w / 2, hx, bw - hx) : bw / 2;
  const cy = 2 * hy <= bh ? clamp(y + h / 2, hy, bh - hy) : bh / 2;
  return { x: cx - w / 2, y: cy - h / 2 };
}

/**
 * New angle (degrees) from dragging the rotation knob. The knob sits on the box's
 * local −Y axis; its start position (relative to center) is added to the pointer delta
 * so the angle starts exactly at the current angle (no jump on grab).
 */
export function rotateKnobAngle(
  start: Rect,
  cx: number,
  cy: number,
  angRad: number,
  rotOffset: number,
  dx: number,
  dy: number,
  shift: boolean
): number {
  const k = rot(0, -(start.h / 2 + rotOffset), angRad);
  const px = cx + k.x + dx;
  const py = cy + k.y + dy;
  return snapAngle((Math.atan2(py - cy, px - cx) * 180) / Math.PI + 90, shift);
}

/**
 * Rotation-aware size clamp for corner resize: the largest (w,h) not above the
 * requested size whose ROTATED bounding box fits bw×bh. An axis-aligned clamp
 * (w ≤ bw, h ≤ bh) is wrong for a tilted frame — a 90° box on a portrait
 * photo may legitimately span the photo's HEIGHT with its local width, and
 * such a cap stopped the resize with room still left (and, conversely, let a
 * 45° box overflow its corners). At 0° this degrades to the independent
 * per-side clamp; with a ratio lock the request shrinks uniformly so the lock
 * stays exact.
 */
export function clampSizeRotated(
  w: number,
  h: number,
  angRad: number,
  bw: number,
  bh: number,
  ratio: number | null
): { w: number; h: number } {
  w = Math.max(MIN_PX, w);
  h = Math.max(MIN_PX, h);
  if (ratio) {
    if (w > h * ratio) w = h * ratio;
    else h = w / ratio;
    const k = fitRotatedScale(w, h, angRad, bw, bh);
    return { w: Math.max(MIN_PX, w * k), h: Math.max(MIN_PX, h * k) };
  }
  const eps = 1e-9;
  const c = Math.abs(Math.cos(angRad));
  const s = Math.abs(Math.sin(angRad));
  // Each side's cap given the other, from both bbox constraints
  // (w·c + h·s ≤ bw and w·s + h·c ≤ bh). Clamp w against the requested h,
  // then re-tighten h — growing one side past a wall lets the other give way.
  const maxW = (hh: number) =>
    Math.min(c > eps ? (bw - hh * s) / c : Infinity, s > eps ? (bh - hh * c) / s : Infinity);
  const maxH = (ww: number) =>
    Math.min(s > eps ? (bw - ww * c) / s : Infinity, c > eps ? (bh - ww * s) / c : Infinity);
  w = clamp(w, MIN_PX, Math.max(MIN_PX, maxW(h)));
  h = clamp(h, MIN_PX, Math.max(MIN_PX, maxH(w)));
  return { w, h };
}

/**
 * Rotation-aware sanitizer for a crop, for callers that know the photo
 * dimensions. A tilted frame legitimately breaks the old `w,h ≤ 1` /
 * `x ∈ [0,1-w]` assumptions — a 90° frame on a portrait photo spans the photo
 * HEIGHT with its local width, i.e. normalized w > 1 with a negative x.
 *
 * Deliberately does NOT shrink the size to fit the rotated bbox: a frame
 * tilted by the slider may overhang the photo by design (the editor shows it;
 * the export fills the corners with the backdrop). Shrink-to-fit here
 * butchered a full-photo crop at intermediate angles into a 1%-wide sliver
 * (the sequential clamp collapsed one side against the oversized other).
 * Sanity bounds only: non-finite → full photo, sides floored at 1% of the
 * photo and capped at its diagonal (no editor gesture can produce a longer
 * side), position re-contained (centered when the bbox can't fit).
 */
export function clampCropRotated(c: CropRect, natW: number, natH: number): CropRect {
  if (!(natW > 0) || !(natH > 0)) return fullCrop();
  const fin = (v: number, d: number) => (Number.isFinite(v) ? v : d);
  const straighten = normalizeStraighten(c.straighten);
  const orientation = normalizeOrientation(c.orientation);
  const rad = (straighten * Math.PI) / 180;
  const minPx = 0.01 * Math.min(natW, natH);
  const diag = Math.hypot(natW, natH);
  const w = clamp(fin(c.w, 1) * natW, minPx, diag);
  const h = clamp(fin(c.h, 1) * natH, minPx, diag);
  const { x, y } = containRotated(fin(c.x, 0) * natW, fin(c.y, 0) * natH, w, h, rad, natW, natH);
  return { x: x / natW, y: y / natH, w: w / natW, h: h / natH, straighten, orientation };
}

/**
 * Largest uniform scale (≤1) of a w×h rect whose bounding box, rotated by `angRad`,
 * still fits inside bw×bh. Used when applying an aspect preset onto an already
 * tilted frame, so the fresh crop doesn't overhang the photo from the start.
 */
export function fitRotatedScale(w: number, h: number, angRad: number, bw: number, bh: number): number {
  const c = Math.abs(Math.cos(angRad));
  const s = Math.abs(Math.sin(angRad));
  const bboxW = w * c + h * s;
  const bboxH = w * s + h * c;
  if (!(bboxW > 0) || !(bboxH > 0)) return 1;
  return Math.min(1, bw / bboxW, bh / bboxH);
}

/**
 * Resize from a rotated corner, keeping the opposite corner anchored in screen space.
 * Works in the box's local (un-rotated) frame. Returns the new un-rotated rect.
 */
export function resizeRotatedCorner(
  corner: Corner,
  start: Rect,
  cx: number,
  cy: number,
  angRad: number,
  dx: number,
  dy: number,
  ratio: number | null
): Rect {
  const sxd = corner.includes("e") ? 1 : -1;
  const syd = corner.includes("s") ? 1 : -1;
  const sxa = -sxd;
  const sya = -syd;
  // anchor (opposite corner) at drag start, in box px
  const a = rot((sxa * start.w) / 2, (sya * start.h) / 2, angRad);
  const ax = cx + a.x;
  const ay = cy + a.y;
  // the dragged corner moved by the pointer delta
  const startCorner = rot((sxd * start.w) / 2, (syd * start.h) / 2, angRad);
  const px = cx + startCorner.x + dx;
  const py = cy + startCorner.y + dy;
  // pointer relative to the anchor, projected into the local (un-rotated) frame
  const local = rot(px - ax, py - ay, -angRad);
  let w = Math.max(MIN_PX, sxd * local.x);
  let h = Math.max(MIN_PX, syd * local.y);
  if (ratio) {
    h = w / ratio;
    if (h < MIN_PX) { h = MIN_PX; w = h * ratio; }
  }
  // place the box so the anchor corner stays fixed
  const half = rot((sxa * w) / 2, (sya * h) / 2, angRad);
  const ncx = ax - half.x;
  const ncy = ay - half.y;
  return { x: ncx - w / 2, y: ncy - h / 2, w, h };
}
