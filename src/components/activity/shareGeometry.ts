/**
 * Single source of truth for the map / elevation block layout, computed in
 * EXPORT pixel coordinates. Both renderers consume the same result:
 *  - shareCanvas.ts draws it 1:1 onto the export canvas (`new Path2D(...)`);
 *  - DraggableShareBlocks.tsx renders the same paths in an SVG whose `viewBox`
 *    is in export-px and whose on-screen width is scaled by `k = preview/export`.
 * Because the geometry is identical and only the SVG `viewBox` scales it down,
 * the preview is a pixel-exact replica of the export (WYSIWYG) regardless of crop.
 */

export interface LatLon {
  lat: number;
  lon: number;
}

export interface MapLayout {
  /** SVG/viewBox inner size, in export-px. */
  innerW: number;
  innerH: number;
  /** Chip size including padding, in export-px. */
  blockW: number;
  blockH: number;
  /** Chip padding, in export-px. */
  padding: number;
  /** Route polyline path, coordinates inside the viewBox. */
  d: string;
  /** Start/end markers, coordinates inside the viewBox. */
  start: { x: number; y: number };
  end: { x: number; y: number };
}

export interface ElevationLayout {
  innerW: number;
  innerH: number;
  blockW: number;
  blockH: number;
  padding: number;
  /** Elevation profile line, inside the viewBox. */
  lineD: string;
  /** Closed area under the profile, inside the viewBox. */
  fillD: string;
}

/** Route layout in export-px. Returns null when there is nothing to draw. */
export function computeMapLayout(exportW: number, points: LatLon[]): MapLayout | null {
  if (points.length < 2) return null;
  const padding = 8;
  const innerW = Math.round(exportW * 0.28);
  const innerH = Math.round(innerW * 0.6);
  const innerPad = innerW * 0.04;

  let minLat = Infinity, maxLat = -Infinity, minLon = Infinity, maxLon = -Infinity;
  for (const p of points) {
    if (p.lat < minLat) minLat = p.lat;
    if (p.lat > maxLat) maxLat = p.lat;
    if (p.lon < minLon) minLon = p.lon;
    if (p.lon > maxLon) maxLon = p.lon;
  }
  const dLat = Math.max(maxLat - minLat, 1e-6);
  const dLon = Math.max(maxLon - minLon, 1e-6);
  const scale = Math.min((innerW - innerPad * 2) / dLon, (innerH - innerPad * 2) / dLat);
  const offX = (innerW - dLon * scale) / 2;
  const offY = (innerH - dLat * scale) / 2;
  const px = (p: LatLon) => offX + (p.lon - minLon) * scale;
  const py = (p: LatLon) => innerH - (offY + (p.lat - minLat) * scale);

  const d = points.map((p, i) => `${i === 0 ? "M" : "L"}${px(p).toFixed(1)},${py(p).toFixed(1)}`).join(" ");
  return {
    innerW,
    innerH,
    blockW: innerW + padding * 2,
    blockH: innerH + padding * 2,
    padding,
    d,
    start: { x: px(points[0]), y: py(points[0]) },
    end: { x: px(points[points.length - 1]), y: py(points[points.length - 1]) },
  };
}

/** Elevation profile layout in export-px. Returns null when there is nothing to draw. */
export function computeElevationLayout(exportW: number, alts: number[]): ElevationLayout | null {
  if (alts.length < 2) return null;
  const padding = 8;
  const innerW = Math.round(exportW * 0.28);
  const innerH = Math.round(innerW * 0.35);
  const innerPad = innerH * 0.08;

  let minA = Infinity, maxA = -Infinity;
  for (const a of alts) { if (a < minA) minA = a; if (a > maxA) maxA = a; }
  const dA = Math.max(maxA - minA, 1);
  const stepX = (innerW - innerPad * 2) / (alts.length - 1);

  let lineD = "";
  for (let i = 0; i < alts.length; i++) {
    const x = innerPad + i * stepX;
    const y = innerH - innerPad - ((alts[i] - minA) / dA) * (innerH - innerPad * 2);
    lineD += `${i === 0 ? "M" : "L"}${x.toFixed(1)},${y.toFixed(1)} `;
  }
  const fillD = `${lineD}L${(innerW - innerPad).toFixed(1)},${(innerH - innerPad).toFixed(1)} L${innerPad.toFixed(1)},${(innerH - innerPad).toFixed(1)} Z`;
  return {
    innerW,
    innerH,
    blockW: innerW + padding * 2,
    blockH: innerH + padding * 2,
    padding,
    lineD,
    fillD,
  };
}
