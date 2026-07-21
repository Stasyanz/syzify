import { useEffect, useRef } from "react";
import { normalizeStraighten, normalizeOrientation, type CropRect } from "./shareLayout";
import {
  type Corner,
  type Rect,
  clampSizeRotated,
  containRotated,
  resizeRotatedCorner,
  rotateKnobAngle,
} from "./cropOverlayMath";

const ROT_OFFSET = 28; // distance of the rotation knob above the box top edge

/** CSS has no native "rotate" cursor — a data-URI circular-arrow icon (white
 * with a dark outline so it reads on any photo), hotspot at its center.
 * Falls back to `grab` where custom cursors are unsupported. */
const ROTATE_CURSOR = `url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='24' height='24' viewBox='0 0 24 24' fill='none' stroke-linecap='round' stroke-linejoin='round'%3E%3Cpath d='M21 12a9 9 0 1 1-9-9c2.52 0 4.93 1 6.74 2.74L21 8' stroke='black' stroke-width='4.5' opacity='.85'/%3E%3Cpath d='M21 3v5h-5' stroke='black' stroke-width='4.5' opacity='.85'/%3E%3Cpath d='M21 12a9 9 0 1 1-9-9c2.52 0 4.93 1 6.74 2.74L21 8' stroke='white' stroke-width='2'/%3E%3Cpath d='M21 3v5h-5' stroke='white' stroke-width='2'/%3E%3C/svg%3E") 12 12, grab`;

const CORNERS: Corner[] = ["nw", "ne", "sw", "se"];

interface Props {
  /** Display size of the full photo in the preview. */
  boxW: number;
  boxH: number;
  crop: CropRect;
  /** Locked pixel aspect ratio (w/h), or null for free resize. */
  ratio: number | null;
  onChange: (crop: CropRect) => void;
  /** Rendered in the EXPORT OUTPUT's coordinate system, laid over the frame
   * (e.g. the brand watermark): the wrapper is output-sized (W/H swapped for a
   * 90°/270° orientation), centered and counter-rotated, so content pinned to
   * its right/bottom lands exactly where it will on the exported image — for
   * any straighten AND orientation. */
  children?: React.ReactNode;
}

const deg2rad = (d: number) => (d * Math.PI) / 180;

interface DragState {
  kind: "move" | "rotate" | Corner;
  sx: number;
  sy: number;
  start: Rect; // un-rotated rect at drag start (box px)
  cx: number; // center at drag start (box px)
  cy: number;
  ang: number; // angle at drag start (rad)
  bw0: number; // frozen box size at drag start (guards mid-drag resize)
  bh0: number;
}

export function CropOverlay({ boxW, boxH, crop, ratio, onChange, children }: Props) {
  const rect: Rect = { x: crop.x * boxW, y: crop.y * boxH, w: crop.w * boxW, h: crop.h * boxH };
  const angle = crop.straighten; // degrees — the crop box tilts by this

  // Output-coordinate wrapper for `children`: the export de-rotates the frame
  // and turns it by `orientation`, so mapping output space back onto the frame
  // is the inverse quarter-turn. An output-sized box (swapped for 90/270)
  // shares the frame's center; counter-rotating it makes its area coincide
  // with the frame while its local axes are the output's.
  const q = normalizeOrientation(crop.orientation);
  const outW = q === 90 || q === 270 ? rect.h : rect.w;
  const outH = q === 90 || q === 270 ? rect.w : rect.h;
  const drag = useRef<DragState | null>(null);
  // Latest crop, so emit can preserve fields (orientation) without a stale closure.
  const cropRef = useRef(crop);
  cropRef.current = crop;

  const emit = (r: Rect, ang: number, bw: number, bh: number, resize = false) => {
    if (bw <= 0 || bh <= 0) return; // preview not measured yet — avoid NaN crop
    const norm = normalizeStraighten(ang);
    const rad = deg2rad(norm);
    // Corner resize clamps the ROTATED bbox to the photo (ratio-aware: hitting
    // an edge shrinks the other side, so a locked aspect survives dragging past
    // the boundary). A tilted box may legitimately span more than bw along one
    // local side — a 90° frame on a portrait photo grows to the photo height.
    // Move/rotate never change the size, so they must not clamp it: re-clamping
    // here is what used to snap a resized rotated frame back on the next drag.
    const { w, h } = resize ? clampSizeRotated(r.w, r.h, rad, bw, bh, ratio) : { w: r.w, h: r.h };
    // Keep the rotated box inside the photo when it fits (avoids blank corners).
    const { x, y } = containRotated(r.x, r.y, w, h, rad, bw, bh);
    onChange({ ...cropRef.current, x: x / bw, y: y / bh, w: w / bw, h: h / bh, straighten: norm });
  };

  useEffect(() => {
    function onMove(e: PointerEvent) {
      const d = drag.current;
      if (!d) return;
      const bw = d.bw0;
      const bh = d.bh0;
      const dx = e.clientX - d.sx;
      const dy = e.clientY - d.sy;
      const angDeg = d.ang * (180 / Math.PI);

      if (d.kind === "move") {
        const cx = d.cx + dx;
        const cy = d.cy + dy;
        emit({ x: cx - d.start.w / 2, y: cy - d.start.h / 2, w: d.start.w, h: d.start.h }, angDeg, bw, bh);
        return;
      }

      if (d.kind === "rotate") {
        const deg = rotateKnobAngle(d.start, d.cx, d.cy, d.ang, ROT_OFFSET, dx, dy, e.shiftKey);
        emit(d.start, deg, bw, bh);
        return;
      }

      emit(resizeRotatedCorner(d.kind, d.start, d.cx, d.cy, d.ang, dx, dy, ratio), angDeg, bw, bh, true);
    }
    function onUp() {
      drag.current = null;
    }
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    return () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
    };
  }, [ratio]); // eslint-disable-line react-hooks/exhaustive-deps

  const startDrag = (kind: DragState["kind"]) => (e: React.PointerEvent) => {
    e.preventDefault();
    e.stopPropagation();
    drag.current = {
      kind,
      sx: e.clientX,
      sy: e.clientY,
      start: rect,
      cx: rect.x + rect.w / 2,
      cy: rect.y + rect.h / 2,
      ang: deg2rad(angle),
      bw0: boxW,
      bh0: boxH,
    };
  };

  return (
    <div className="absolute inset-0" style={{ touchAction: "none" }}>
      {/* selection rect (rotated around its center), with a dimmed surround */}
      <div
        onPointerDown={startDrag("move")}
        style={{
          position: "absolute",
          left: rect.x,
          top: rect.y,
          width: rect.w,
          height: rect.h,
          transform: `rotate(${angle}deg)`,
          transformOrigin: "center",
          boxShadow: "0 0 0 9999px rgba(0,0,0,0.5)",
          outline: "1px solid rgba(255,255,255,0.9)",
          cursor: "move",
        }}
      >
        {/* rule-of-thirds guides */}
        <div style={{ position: "absolute", inset: 0, pointerEvents: "none" }}>
          {[1, 2].map((i) => (
            <div key={`v${i}`} style={{ position: "absolute", top: 0, bottom: 0, left: `${(i / 3) * 100}%`, width: 1, background: "rgba(255,255,255,0.35)" }} />
          ))}
          {[1, 2].map((i) => (
            <div key={`h${i}`} style={{ position: "absolute", left: 0, right: 0, top: `${(i / 3) * 100}%`, height: 1, background: "rgba(255,255,255,0.35)" }} />
          ))}
        </div>
        {/* frame content (watermark preview) — under the handles, above the guides */}
        {children && (
          <div
            style={{
              position: "absolute",
              width: outW,
              height: outH,
              left: (rect.w - outW) / 2,
              top: (rect.h - outH) / 2,
              transform: `rotate(${-q}deg)`,
              // The quarter-fold at the 45° boundary is a genuinely discrete
              // change of the output (orientation jumps by 90°) — a short ease
              // turns the visual "click" of the watermark into a quick spin.
              // The same transition makes the wrapper trail smoothly during
              // resize; content inside is unaffected.
              transition:
                "transform 160ms ease, width 160ms ease, height 160ms ease, left 160ms ease, top 160ms ease",
              pointerEvents: "none",
            }}
          >
            {children}
          </div>
        )}
        {/* rotation handle: a stalk above the top edge ending in a knob */}
        <div style={{ position: "absolute", left: "50%", top: -ROT_OFFSET, width: 1, height: ROT_OFFSET, background: "rgba(255,255,255,0.9)", pointerEvents: "none" }} />
        <div
          onPointerDown={startDrag("rotate")}
          title="Drag to rotate (hold Shift to snap to 15°)"
          style={{
            position: "absolute",
            left: "50%",
            top: -ROT_OFFSET,
            width: 16,
            height: 16,
            marginLeft: -8,
            marginTop: -8,
            background: "#fff",
            borderRadius: "50%",
            border: "1px solid rgba(0,0,0,0.3)",
            cursor: ROTATE_CURSOR,
          }}
        />
        {CORNERS.map((c) => (
          <div
            key={c}
            onPointerDown={startDrag(c)}
            style={{
              position: "absolute",
              width: 14,
              height: 14,
              background: "#fff",
              borderRadius: 3,
              border: "1px solid rgba(0,0,0,0.3)",
              left: c.includes("w") ? -7 : undefined,
              right: c.includes("e") ? -7 : undefined,
              top: c.includes("n") ? -7 : undefined,
              bottom: c.includes("s") ? -7 : undefined,
              cursor: `${c}-resize`,
            }}
          />
        ))}
      </div>
    </div>
  );
}
