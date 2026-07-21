export type BlockKind = "title" | "metrics" | "map" | "elevation";

/** Position of a block, in fractions of the photo size (0..1, top-left corner). */
export interface BlockPos {
  x: number;
  y: number;
}

export type BlockPositions = Record<BlockKind, BlockPos>;

/** Default positions assume bottom-left for text, bottom-right for visuals. */
export function defaultPositions(): BlockPositions {
  return {
    title: { x: 0.04, y: 0.62 },
    metrics: { x: 0.04, y: 0.78 },
    map: { x: 0.66, y: 0.58 },
    elevation: { x: 0.66, y: 0.84 },
  };
}

/** Per-block size multiplier (1 = the default size). */
export type BlockScales = Record<BlockKind, number>;

export function defaultScales(): BlockScales {
  return { title: 1, metrics: 1, map: 1, elevation: 1 };
}

export const MIN_BLOCK_SCALE = 0.4;
export const MAX_BLOCK_SCALE = 3;

/** A reserved region in normalized [0..1] coords (e.g. the brand watermark). */
export interface NormZone {
  x: number;
  y: number;
  w: number;
  h: number;
}

/**
 * Keep a dragged block (top-left x/y, size w/h — all normalized) out of a
 * reserved zone: on overlap the block shifts left or up, whichever clears it
 * with the smaller move (the zone sits in the bottom-right corner), but never
 * past the canvas edge. Degenerate sizes are a no-op — happy-dom measures
 * every element as 0×0, which keeps this out of unit-test renders by design.
 */
export function avoidZone(
  x: number,
  y: number,
  w: number,
  h: number,
  zone: NormZone
): { x: number; y: number } {
  if (!(w > 0) || !(h > 0) || !(zone.w > 0) || !(zone.h > 0)) return { x, y };
  const overlaps =
    x < zone.x + zone.w && x + w > zone.x && y < zone.y + zone.h && y + h > zone.y;
  if (!overlaps) return { x, y };
  const pushLeft = x + w - zone.x;
  const pushUp = y + h - zone.y;
  if (pushLeft <= pushUp) return { x: Math.max(0, zone.x - w), y };
  return { x, y: Math.max(0, zone.y - h) };
}

/** Keep a block scale sane: non-finite → 1, else clamped to the slider range. */
export function clampBlockScale(s: number): number {
  if (!Number.isFinite(s)) return 1;
  return Math.min(MAX_BLOCK_SCALE, Math.max(MIN_BLOCK_SCALE, s));
}

/** Straighten-slider bound (degrees), each way — fine horizon leveling only;
 * quarter turns are made with the frame's rotation knob (the auto-fold turns
 * them into the output orientation). The knob itself is unbounded. */
export const MAX_STRAIGHTEN = 45;

/**
 * Crop region, in fractions of the photo size (0..1). w/h are the selection size,
 * x/y its top-left (the un-rotated rect). The output is the selection:
 *  - `straighten` (degrees, wrapped to (-180, 180]): the crop frame is freely tilted
 *    by this angle around its own centre; the export de-rotates that tilted region to
 *    an upright image;
 *  - `orientation` (0/90/180/270, clockwise quarter-turns): rotates the cropped
 *    output, so 90/270 swap width↔height (a landscape crop → a vertical image).
 */
export interface CropRect {
  x: number;
  y: number;
  w: number;
  h: number;
  straighten: number;
  orientation: number;
}

/** The whole photo (no crop, no rotation). */
export function fullCrop(): CropRect {
  return { x: 0, y: 0, w: 1, h: 1, straighten: 0, orientation: 0 };
}

/** Wrap the free crop-frame rotation into (-180, 180]; non-finite → 0. */
export function normalizeStraighten(deg: number): number {
  if (!Number.isFinite(deg)) return 0;
  let a = deg % 360;
  if (a > 180) a -= 360;
  if (a <= -180) a += 360;
  return a;
}

/** Snap orientation to the nearest quarter-turn in {0, 90, 180, 270}; non-finite → 0. */
export function normalizeOrientation(deg: number): number {
  if (!Number.isFinite(deg)) return 0;
  return ((Math.round(deg / 90) % 4) + 4) % 4 * 90;
}

/** Quarter-turn component of a straighten angle: the nearest multiple of 90
 * to the wrapped angle (…, -90, 0, 90, 180). Halfway points round away from
 * zero on BOTH sides (Math.round alone folds +45 but not -45), so the two
 * ends of the straighten slider behave symmetrically. */
export function straightenQuarter(deg: number): number {
  const a = normalizeStraighten(deg);
  return Math.sign(a) * Math.round(Math.abs(a) / 90) * 90;
}

/**
 * Keep the exported image matching what the frame shows on the photo while it
 * is rotated through quarter turns: the export de-rotates the tilted frame to
 * upright, so turning a 16:9 frame to 90° would otherwise land the photo on
 * its side. When straighten crosses into a new quarter, fold the difference
 * into orientation — a frame turned vertical yields a vertical image without
 * touching the rotate buttons (which still adjust on top of this).
 */
export function autoQuarterOrientation(prev: CropRect, next: CropRect): CropRect {
  const dq = straightenQuarter(next.straighten) - straightenQuarter(prev.straighten);
  if (dq === 0) return next;
  return { ...next, orientation: normalizeOrientation(next.orientation + dq) };
}

// NOTE: crop sanitizing lives in cropOverlayMath.clampCropRotated — an
// axis-aligned clamp is wrong for tilted frames (their normalized w/h may
// legitimately exceed 1), so there is deliberately no photo-space clamp here.

/** Aspect-ratio presets for cropping. `ratio` null = free / no lock. */
export interface CropPreset {
  key: string;
  label: string;
  ratio: number | null;
}

export const CROP_PRESETS: CropPreset[] = [
  { key: "free", label: "Free", ratio: null },
  { key: "1:1", label: "1:1", ratio: 1 },
  { key: "16:9", label: "16:9", ratio: 16 / 9 },
];

/**
 * Largest centered crop of the given pixel aspect that fits `imageW × imageH`,
 * returned in normalized [0..1] coords. `targetRatio` is width/height in pixels.
 */
export function centeredCrop(imageW: number, imageH: number, targetRatio: number): CropRect {
  if (imageW <= 0 || imageH <= 0 || targetRatio <= 0) return fullCrop();
  const imgRatio = imageW / imageH;
  let w: number;
  let h: number;
  if (targetRatio > imgRatio) {
    // crop is wider than the image → full width, reduced height
    w = 1;
    h = imgRatio / targetRatio;
  } else {
    h = 1;
    w = targetRatio / imgRatio;
  }
  return { x: (1 - w) / 2, y: (1 - h) / 2, w, h, straighten: 0, orientation: 0 };
}
