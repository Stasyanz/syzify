import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { Activity, TrackPointColumns } from "../../lib/types";
import { SPORT_LABELS, type SportType } from "../../lib/types";
import { formatDate } from "../../lib/format";
import { avoidZone, clampBlockScale, type BlockKind, type BlockPositions, type BlockScales } from "./shareLayout";
import { computeMapLayout, computeElevationLayout } from "./shareGeometry";
import {
  BRAND_MARK_PATH,
  BRAND_MARK_DOT,
  BRAND_ACCENT,
  BRAND_WORDMARK,
  BRAND_WORDMARK_TYPESET,
  BRAND_IDOT_PREVIEW_BOTTOM_EM,
  BRAND_IDOT_R_EM,
  BRAND_MARK_FS,
  BRAND_WORD_FS,
} from "./shareCanvas";

export type Theme = "dark" | "light";

export interface BlockField {
  key: string;
  label: string;
  value: string;
}

interface Props {
  activity: Activity;
  trackpoints: TrackPointColumns;
  fields: BlockField[];
  theme: Theme;
  showTitle: boolean;
  showMap: boolean;
  showElevation: boolean;
  positions: BlockPositions;
  onPositionChange: (kind: BlockKind, pos: { x: number; y: number }) => void;
  /** Per-block size multipliers, mirrored 1:1 by the export. */
  scales: BlockScales;
  onScaleChange: (kind: BlockKind, scale: number) => void;
  previewWidth: number;
  previewHeight: number;
  /** Width (px) of the exported canvas region, so floored sizes (baseFs, stroke)
   * are computed identically to shareCanvas and just scaled down for the preview
   * — keeps the preview a pixel-exact replica of the export. */
  exportWidth: number;
  /** Mirror of the export option: no chip plate, halo behind glyphs instead. */
  transparentBg?: boolean;
  /** Set by the composite: the watermark element dragged blocks must keep off. */
  avoidRef?: React.RefObject<HTMLDivElement | null>;
}

/** A size computed from the export dimensions, then scaled to the preview, so a
 * `Math.max(floor, …)` clamp behaves identically in preview and export. */
function exportScaled(floor: number, exportValue: number, previewWidth: number, exportWidth: number) {
  const k = exportWidth > 0 ? previewWidth / exportWidth : 1;
  return Math.max(floor, exportValue) * k;
}

function chipStyle(theme: Theme, transparentBg?: boolean): React.CSSProperties {
  return {
    position: "absolute",
    color: theme === "dark" ? "#ffffff" : "#0a0a0a",
    borderRadius: 12,
    cursor: "grab",
    userSelect: "none",
    fontFamily: "ui-sans-serif, system-ui, -apple-system, Segoe UI, Roboto, sans-serif",
    ...(transparentBg
      ? {}
      : {
          background: theme === "dark" ? "rgba(0,0,0,0.55)" : "rgba(255,255,255,0.78)",
          backdropFilter: "blur(8px)",
          WebkitBackdropFilter: "blur(8px)",
        }),
  };
}

function subtextColor(theme: Theme) {
  return theme === "dark" ? "rgba(255,255,255,0.7)" : "rgba(0,0,0,0.55)";
}

interface DragState {
  kind: BlockKind;
  startX: number;
  startY: number;
  startPosX: number;
  startPosY: number;
}

function useDraggable(
  kind: BlockKind,
  positions: BlockPositions,
  onChange: Props["onPositionChange"],
  previewWidth: number,
  previewHeight: number,
  avoid?: Props["avoidRef"]
) {
  const ref = useRef<HTMLDivElement>(null);
  const dragRef = useRef<DragState | null>(null);

  const onMouseDown = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      dragRef.current = {
        kind,
        startX: e.clientX,
        startY: e.clientY,
        startPosX: positions[kind].x,
        startPosY: positions[kind].y,
      };
      if (ref.current) ref.current.style.cursor = "grabbing";
    },
    [kind, positions]
  );

  useEffect(() => {
    function onMove(ev: MouseEvent) {
      const drag = dragRef.current;
      if (!drag || previewWidth <= 0 || previewHeight <= 0) return;
      const dx = (ev.clientX - drag.startX) / previewWidth;
      const dy = (ev.clientY - drag.startY) / previewHeight;
      let x = Math.min(1, Math.max(0, drag.startPosX + dx));
      let y = Math.min(1, Math.max(0, drag.startPosY + dy));
      // Keep dragged blocks off the brand watermark, so it stays readable on
      // the PNG (it draws last there, but content under it looks broken).
      const el = ref.current;
      const wm = avoid?.current;
      if (el && wm) {
        ({ x, y } = avoidZone(
          x,
          y,
          el.offsetWidth / previewWidth,
          el.offsetHeight / previewHeight,
          {
            x: wm.offsetLeft / previewWidth,
            y: wm.offsetTop / previewHeight,
            w: wm.offsetWidth / previewWidth,
            h: wm.offsetHeight / previewHeight,
          }
        ));
      }
      onChange(drag.kind, { x, y });
    }
    function onUp() {
      if (dragRef.current && ref.current) ref.current.style.cursor = "grab";
      dragRef.current = null;
    }
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
    return () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
  }, [onChange, previewWidth, previewHeight, avoid]);

  return { ref, onMouseDown };
}

/** Corner-grip drag → per-block scale. The block's on-screen width at grab
 * time anchors the mapping, so the size follows the pointer ~1:1. */
function useScalable(
  kind: BlockKind,
  scales: BlockScales,
  onChange: Props["onScaleChange"],
  blockRef: React.RefObject<HTMLDivElement | null>
) {
  const dragRef = useRef<{ sx: number; sy: number; s0: number; w0: number } | null>(null);
  const [active, setActive] = useState(false);

  const onHandleDown = useCallback(
    (e: React.PointerEvent) => {
      e.preventDefault();
      e.stopPropagation(); // a grip drag must not start the block move
      dragRef.current = {
        sx: e.clientX,
        sy: e.clientY,
        s0: scales[kind],
        w0: blockRef.current?.offsetWidth || 0,
      };
      setActive(true);
    },
    [kind, scales, blockRef]
  );

  useEffect(() => {
    function onMove(ev: PointerEvent) {
      const d = dragRef.current;
      if (!d || d.w0 <= 0) return;
      const delta = (ev.clientX - d.sx + ev.clientY - d.sy) / 2;
      onChange(kind, clampBlockScale((d.s0 * (d.w0 + delta)) / d.w0));
    }
    function onUp() {
      dragRef.current = null;
      setActive(false);
    }
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    return () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
    };
  }, [kind, onChange]);

  return { onHandleDown, active };
}

/** Resize frame around a block: dashed outline + bottom-right grip. Editing
 * chrome only — it never appears in the export. */
function ScaleChrome({
  active,
  onHandleDown,
}: {
  active: boolean;
  onHandleDown: (e: React.PointerEvent) => void;
}) {
  return (
    <>
      <div
        style={{
          position: "absolute",
          inset: -2,
          border: `1px dashed rgba(255,255,255,${active ? 0.9 : 0.35})`,
          borderRadius: "inherit",
          pointerEvents: "none",
        }}
      />
      <div
        onPointerDown={onHandleDown}
        title="Drag to resize"
        style={{
          position: "absolute",
          right: -6,
          bottom: -6,
          width: 12,
          height: 12,
          background: "#fff",
          borderRadius: 3,
          border: "1px solid rgba(0,0,0,0.3)",
          cursor: "nwse-resize",
        }}
      />
    </>
  );
}

function TitleBlock(props: Props) {
  const { activity, theme, positions, onPositionChange, scales, onScaleChange, previewWidth, previewHeight, exportWidth, transparentBg } = props;
  const { ref, onMouseDown } = useDraggable("title", positions, onPositionChange, previewWidth, previewHeight, props.avoidRef);
  const { onHandleDown, active } = useScalable("title", scales, onScaleChange, ref);
  const sportLabel = SPORT_LABELS[activity.sport_type as SportType] ?? "Activity";
  const title = activity.title ?? sportLabel;
  const baseFs = exportScaled(14, exportWidth * 0.018, previewWidth, exportWidth) * scales.title;

  return (
    <div
      ref={ref}
      onMouseDown={onMouseDown}
      style={{
        ...chipStyle(theme, transparentBg),
        left: `${positions.title.x * 100}%`,
        top: `${positions.title.y * 100}%`,
        padding: `${baseFs * 0.7}px ${baseFs * 1}px`,
        maxWidth: `${previewWidth * 0.7}px`,
      }}
    >
      <div style={{ fontSize: baseFs * 0.75, color: subtextColor(theme), letterSpacing: 1, fontWeight: 500 }}>
        {sportLabel.toUpperCase()} · {formatDate(activity.start_time).toUpperCase()}
      </div>
      <div style={{ fontSize: baseFs * 1.7, fontWeight: 700, marginTop: baseFs * 0.15, lineHeight: 1.1 }}>{title}</div>
      {activity.location_name && (
        <div style={{ fontSize: baseFs * 0.85, color: subtextColor(theme), marginTop: baseFs * 0.15 }}>
          {activity.location_name}
        </div>
      )}
      <ScaleChrome active={active} onHandleDown={onHandleDown} />
    </div>
  );
}

function MetricsBlock(props: Props) {
  const { fields, theme, positions, onPositionChange, scales, onScaleChange, previewWidth, previewHeight, exportWidth, transparentBg } = props;
  const { ref, onMouseDown } = useDraggable("metrics", positions, onPositionChange, previewWidth, previewHeight, props.avoidRef);
  const { onHandleDown, active } = useScalable("metrics", scales, onScaleChange, ref);
  if (fields.length === 0) return null;
  const baseFs = exportScaled(14, exportWidth * 0.018, previewWidth, exportWidth) * scales.metrics;
  const cols = Math.min(fields.length, 4);

  return (
    <div
      ref={ref}
      onMouseDown={onMouseDown}
      style={{
        ...chipStyle(theme, transparentBg),
        left: `${positions.metrics.x * 100}%`,
        top: `${positions.metrics.y * 100}%`,
        padding: `${baseFs * 0.7}px ${baseFs * 1}px`,
        display: "grid",
        gridTemplateColumns: `repeat(${cols}, minmax(0, auto))`,
        columnGap: baseFs * 1.2,
        rowGap: baseFs * 0.6,
      }}
    >
      {fields.map((f) => (
        <div key={f.key}>
          <div style={{ fontSize: baseFs * 0.65, color: subtextColor(theme), letterSpacing: 0.5, fontWeight: 500 }}>
            {f.label.toUpperCase()}
          </div>
          <div style={{ fontSize: baseFs * 1.35, fontWeight: 700, marginTop: 2 }}>{f.value}</div>
        </div>
      ))}
      <ScaleChrome active={active} onHandleDown={onHandleDown} />
    </div>
  );
}

function MapBlock(props: Props) {
  const { trackpoints, theme, positions, onPositionChange, scales, onScaleChange, previewWidth, previewHeight, exportWidth, transparentBg } = props;
  const { ref, onMouseDown } = useDraggable("map", positions, onPositionChange, previewWidth, previewHeight, props.avoidRef);
  const { onHandleDown, active } = useScalable("map", scales, onScaleChange, ref);

  // Rebuilding the route path over every trackpoint on each render would run
  // on every mousemove of a drag; the layout depends only on these three.
  const L = useMemo(() => {
    const points: { lat: number; lon: number }[] = [];
    for (let i = 0; i < trackpoints.lat.length; i++) {
      const lat = trackpoints.lat[i];
      const lon = trackpoints.lon[i];
      if (lat != null && lon != null) points.push({ lat, lon });
    }
    return computeMapLayout(exportWidth * scales.map, points);
  }, [trackpoints, exportWidth, scales.map]);
  if (!L) return null;

  // The SVG viewBox is in export-px; the on-screen size is scaled by k, so every
  // length (stroke, radius) is given in export-px and renders identically to the
  // canvas export downscaled by the same k. See shareGeometry.ts.
  const k = exportWidth > 0 ? previewWidth / exportWidth : 1;
  const stroke = theme === "dark" ? "#fff" : "#0a0a0a";
  const lineW = Math.max(2, L.innerW * 0.012);
  const dotR = Math.max(3, L.innerW * 0.016);
  const dotStroke = Math.max(1, L.innerW * 0.006);

  return (
    <div
      ref={ref}
      onMouseDown={onMouseDown}
      style={{
        ...chipStyle(theme, transparentBg),
        left: `${positions.map.x * 100}%`,
        top: `${positions.map.y * 100}%`,
        padding: L.padding * k,
        borderRadius: 12 * k,
      }}
    >
      <svg
        width={L.innerW * k}
        height={L.innerH * k}
        viewBox={`0 0 ${L.innerW} ${L.innerH}`}
        style={{ display: "block" }}
      >
        <path d={L.d} fill="none" stroke={stroke} strokeWidth={lineW} strokeLinejoin="round" strokeLinecap="round" />
        <circle cx={L.start.x} cy={L.start.y} r={dotR} fill="#22c55e" stroke={stroke} strokeWidth={dotStroke} />
        <circle cx={L.end.x} cy={L.end.y} r={dotR} fill="#ef4444" stroke={stroke} strokeWidth={dotStroke} />
      </svg>
      <ScaleChrome active={active} onHandleDown={onHandleDown} />
    </div>
  );
}

function ElevationBlock(props: Props) {
  const { trackpoints, theme, positions, onPositionChange, scales, onScaleChange, previewWidth, previewHeight, exportWidth, transparentBg } = props;
  const { ref, onMouseDown } = useDraggable("elevation", positions, onPositionChange, previewWidth, previewHeight, props.avoidRef);
  const { onHandleDown, active } = useScalable("elevation", scales, onScaleChange, ref);

  // Memoized like MapBlock — the profile path over every altitude sample must
  // not be rebuilt on each mousemove of a drag.
  const L = useMemo(() => {
    const alts: number[] = [];
    for (const a of trackpoints.altitude_m) if (a != null) alts.push(a);
    return computeElevationLayout(exportWidth * scales.elevation, alts);
  }, [trackpoints, exportWidth, scales.elevation]);
  if (!L) return null;

  const k = exportWidth > 0 ? previewWidth / exportWidth : 1;
  const stroke = theme === "dark" ? "#fff" : "#0a0a0a";
  const lineW = Math.max(2, L.innerW * 0.008);

  return (
    <div
      ref={ref}
      onMouseDown={onMouseDown}
      style={{
        ...chipStyle(theme, transparentBg),
        left: `${positions.elevation.x * 100}%`,
        top: `${positions.elevation.y * 100}%`,
        padding: L.padding * k,
        borderRadius: 12 * k,
      }}
    >
      <svg
        width={L.innerW * k}
        height={L.innerH * k}
        viewBox={`0 0 ${L.innerW} ${L.innerH}`}
        style={{ display: "block" }}
      >
        {!transparentBg && <path d={L.fillD} fill={stroke} fillOpacity={0.18} />}
        <path d={L.lineD} fill="none" stroke={stroke} strokeWidth={lineW} strokeLinejoin="round" />
      </svg>
      <ScaleChrome active={active} onHandleDown={onHandleDown} />
    </div>
  );
}

/** Preview mirror of the export's drawBrandMark: fixed bottom-right.
 * Deliberately not draggable and not toggleable —
 * pointer events pass through so it never blocks a drag underneath.
 * Exported for the crop editor, which pins it inside the crop frame:
 * `previewWidth` is the on-screen span corresponding to `exportWidth`
 * export-px, so the mark always shows its final exported size. */
export function BrandMark({
  theme,
  previewWidth,
  exportWidth,
  innerRef,
}: Pick<Props, "theme" | "previewWidth" | "exportWidth"> & {
  /** Lets the composite measure the mark as the drag keep-out zone. */
  innerRef?: React.Ref<HTMLDivElement>;
}) {
  const baseFs = exportScaled(14, exportWidth * 0.018, previewWidth, exportWidth);
  const markH = baseFs * BRAND_MARK_FS;
  const color = theme === "dark" ? "#ffffff" : "#0a0a0a";

  return (
    <div
      ref={innerRef}
      style={{
        position: "absolute",
        right: baseFs,
        bottom: baseFs,
        display: "flex",
        alignItems: "flex-end",
        gap: markH * 0.3,
        pointerEvents: "none",
        color,
        fontFamily: "ui-sans-serif, system-ui, -apple-system, Segoe UI, Roboto, sans-serif",
      }}
    >
      <svg
        width={markH}
        height={markH}
        viewBox="0 0 32 32"
        style={{ display: "block" }}
      >
        <path d={BRAND_MARK_PATH} fill={color} strokeLinejoin="round" />
        <circle cx={BRAND_MARK_DOT.x} cy={BRAND_MARK_DOT.y} r={BRAND_MARK_DOT.r} fill={BRAND_ACCENT} />
      </svg>
      <span
        role="img"
        aria-label={BRAND_WORDMARK}
        style={{ fontSize: baseFs * BRAND_WORD_FS, fontWeight: 800, lineHeight: 1 }}
      >
        {/* Same trick as the canvas export and Logo.tsx: dotless ı, accent
            dot drawn by us. Geometry shared via BRAND_IDOT_* so the preview
            stays WYSIWYG with drawBrandMark. */}
        <span aria-hidden="true">
          {BRAND_WORDMARK_TYPESET.slice(0, 3)}
          <span style={{ position: "relative", display: "inline-block" }}>
            {"ı"}
            <span
              style={{
                position: "absolute",
                left: "50%",
                transform: "translateX(-50%)",
                bottom: `${BRAND_IDOT_PREVIEW_BOTTOM_EM}em`,
                width: `${BRAND_IDOT_R_EM * 2}em`,
                height: `${BRAND_IDOT_R_EM * 2}em`,
                borderRadius: "50%",
                background: BRAND_ACCENT,
              }}
            />
          </span>
          {BRAND_WORDMARK_TYPESET.slice(4)}
        </span>
      </span>
    </div>
  );
}

export function DraggableShareBlocks(props: Props) {
  // The watermark element doubles as the keep-out zone for block drags.
  const brandRef = useRef<HTMLDivElement>(null);
  return (
    <>
      {props.showTitle && <TitleBlock {...props} avoidRef={brandRef} />}
      <MetricsBlock {...props} avoidRef={brandRef} />
      {props.showMap && <MapBlock {...props} avoidRef={brandRef} />}
      {props.showElevation && <ElevationBlock {...props} avoidRef={brandRef} />}
      <BrandMark {...props} innerRef={brandRef} />
    </>
  );
}
