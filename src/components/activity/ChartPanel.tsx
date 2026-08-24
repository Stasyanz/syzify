import { useRef, useEffect, useState, useMemo, useCallback } from "react";
import { useQuery } from "@tanstack/react-query";
import { GripVertical } from "lucide-react";
import uPlot from "uplot";
import "uplot/dist/uPlot.min.css";
import { isPaceSport, isSwimSport, type TimeInZone, type TrackPointColumns } from "../../lib/types";
import {
  bandGradientStops,
  bucketMaxBars,
  cadenceZoneRanges,
  ELEVATION_BANDS_M,
  gradeGradientStops,
  gradeSeries,
  nearestChartIdx,
  nearestIdx,
  selectionGrade,
  hrVisRange,
  hrZoneRanges,
  powerVisRange,
  powerZoneRanges,
  speedVisRange,
  speedZoneRanges,
  zoneBarCount,
  zoneColorFor,
  type ElevationBand,
  type ZoneRange,
} from "./chartZones";
import { api } from "../../lib/tauri";
import { formatGrade, formatSelectionStats } from "../../lib/format";
import { useActivityStore } from "../../stores/activityStore";
import { chartTextColor, chartGridColor } from "../../lib/chartTheme";
import { SaveSegmentPopover } from "./SaveSegmentPopover";
import { useResolvedDark } from "../../lib/theme";
import {
  useUnits,
  isImperial,
  distanceUnit,
  elevationUnit,
  speedUnit,
  M_PER_MILE,
  M_PER_100YD,
  FT_PER_M,
  MPH_PER_MPS,
} from "../../lib/units";

interface Props {
  trackpoints: TrackPointColumns;
  sport: string;
  /** The activity's stored time-in-zone rows — HR boundaries paint the
   * zone bands behind the heart-rate chart (FIT only; absent elsewhere). */
  timeInZones?: TimeInZone[];
  /** FTP (threshold_power from the FIT session) — fallback source for Coggan
   * power zones when the device wrote no power boundaries. */
  ftpW?: number | null;
  /** Enables "Save segment" on the selection's right-click menu. Only passed
   * when the charted columns are the activity's full track — a focused leg's
   * slice drops points and rebases indices, so its selection indices don't
   * address the stored trackpoints. */
  segmentSource?: { activityId: string };
}

type ChartType = "elevation" | "hr" | "pace" | "speed" | "cadence" | "power";

interface ChartConfig {
  key: ChartType;
  label: string;
  unit: string;
  color: string;
  fill: string;
  getData: (tp: TrackPointColumns) => (number | null)[];
  /** Optional y-axis tick formatter (e.g. pace mm:ss). */
  valueFmt?: (v: number) => string;
  /** Flip the y-axis so smaller values read as "higher" — pace is
   * faster the smaller the number, and faster belongs at the top. */
  invertY?: boolean;
  /** Hypsometric fill bands (display units) — elevation's atlas coloring. */
  elevationBands?: ElevationBand[];
}

// Hypsometric band definitions live in chartZones.ts (ELEVATION_BANDS_M),
// next to the gradient math and its ordering guard test.

/** Pace (min/km decimal) → "m:ss". */
function fmtPace(minPerKm: number): string {
  if (!isFinite(minPerKm) || minPerKm <= 0) return "";
  const m = Math.floor(minPerKm);
  const s = Math.round((minPerKm - m) * 60);
  return s === 60 ? `${m + 1}:00` : `${m}:${String(s).padStart(2, "0")}`;
}

// Elevation / pace / speed depend on the units setting — built via factories
// so a units change yields fresh configs (see the useMemo in ChartPanel).
const ELEVATION = (): ChartConfig => ({
  key: "elevation",
  label: "Elevation",
  unit: elevationUnit(),
  color: "#0e7490",
  fill: "rgba(14,116,144,0.12)",
  getData: (tp) =>
    isImperial()
      ? tp.altitude_m.map((v) => (v != null ? v * FT_PER_M : null))
      : tp.altitude_m,
  // Band ceilings stay at honest meter marks in any display unit
  // (Infinity * FT_PER_M is still Infinity).
  elevationBands: ELEVATION_BANDS_M.map((b) => ({
    to: isImperial() ? b.to * FT_PER_M : b.to,
    color: b.color,
  })),
});
const HR: ChartConfig = {
  key: "hr",
  label: "Heart rate",
  unit: "bpm",
  color: "#dc2626",
  fill: "rgba(220,38,38,0.10)",
  getData: (tp) => tp.hr,
};
const PACE = (): ChartConfig => ({
  key: "pace",
  label: "Pace",
  unit: `min/${distanceUnit()}`,
  color: "#c2410c",
  fill: "rgba(194,65,12,0.10)",
  // Pace from speed; ignore near-stops so the line doesn't spike to infinity.
  getData: (tp) => {
    const perUnit = isImperial() ? M_PER_MILE : 1000;
    return speedSeries(tp).map((v) => (v != null && v > 0.5 ? perUnit / v / 60 : null));
  },
  valueFmt: fmtPace,
  invertY: true,
});
const SWIM_PACE = (): ChartConfig => ({
  key: "pace",
  label: "Pace",
  unit: `min/100${isImperial() ? "yd" : "m"}`,
  color: "#c2410c",
  fill: "rgba(194,65,12,0.10)",
  // Swim pace per 100 m/yd; the near-stop cutoff sits lower than running's
  // (0.2 m/s ≈ 8:20 /100m) — swim speeds live well below running speeds.
  getData: (tp) => {
    const per100 = isImperial() ? M_PER_100YD : 100;
    return speedSeries(tp).map((v) => (v != null && v > 0.2 ? per100 / v / 60 : null));
  },
  valueFmt: fmtPace,
  invertY: true,
});
const SPEED = (): ChartConfig => ({
  key: "speed",
  label: "Speed",
  unit: speedUnit(),
  color: "#3f7d4e",
  fill: "rgba(63,125,78,0.10)",
  getData: (tp) => {
    const factor = isImperial() ? MPH_PER_MPS : 3.6;
    return speedSeries(tp).map((v) => (v != null ? v * factor : null));
  },
});
const CADENCE: ChartConfig = {
  key: "cadence",
  label: "Cadence",
  unit: "spm",
  color: "#ca8a04",
  fill: "rgba(202,138,4,0.10)",
  getData: (tp) => tp.cadence,
};
const POWER: ChartConfig = {
  key: "power",
  label: "Power",
  unit: "W",
  color: "#7c3aed",
  fill: "rgba(124,58,237,0.10)",
  getData: (tp) => tp.power_w,
};

type XAxis = "distance" | "time";
const SYNC_KEY = "activity-charts";

/** Per-point speed (m/s): use the recorded series, else derive it from the
 * cumulative distance + time deltas (GPS-only tracks carry no speed field). */
function speedSeries(tp: TrackPointColumns): (number | null)[] {
  // Use the recorded field only when it carries real signal; some devices
  // populate speed_mps with all-zeros, which would otherwise suppress the
  // speed/pace chart that is perfectly derivable from distance + time.
  if (tp.speed_mps.some((v) => v != null && v !== 0)) return tp.speed_mps;
  const n = tp.distance_m.length;
  const out: (number | null)[] = new Array(n).fill(null);
  let prev = -1;
  for (let i = 0; i < n; i++) {
    if (tp.distance_m[i] == null || tp.t[i] == null) continue;
    if (prev >= 0) {
      const dd = tp.distance_m[i]! - tp.distance_m[prev]!;
      const dt = tp.t[i]! - tp.t[prev]!;
      if (dt > 0 && dd >= 0) out[i] = dd / dt;
    }
    prev = i;
  }
  return out;
}

/** A metric is worth a chart only if it carries a real signal — at least one
 * non-null AND non-zero sample. An all-zero series (e.g. speed/distance on an
 * indoor strength session) is a flat line at 0, not data worth a card. */
function hasData(values: (number | null)[]): boolean {
  return values.some((v) => v != null && v !== 0);
}

/** Fit the y-axis to its actual tick labels instead of reserving a fixed
 * 48px: "80" needs half of what "20:00" does, and the difference is plot
 * width. measureText is transform-independent, so the plain 11px font gives
 * CSS-px widths; 15 covers the tick marks (10) + label gap. */
function yAxisSize(u: uPlot, values: string[] | null): number {
  if (!values || values.length === 0) return 40;
  u.ctx.font = "11px sans-serif";
  const widest = Math.max(...values.map((v) => u.ctx.measureText(v).width));
  return Math.max(28, Math.ceil(widest) + 15);
}

function SingleChart({
  config,
  xValues,
  xLabel,
  xUnit,
  values,
  indexMap,
  reverseMap,
  height,
  barZones,
  barRange,
  barStep,
  gradeValues,
  selectionStats,
  onSelectionMenu,
}: {
  config: ChartConfig;
  xValues: number[];
  xLabel: string;
  xUnit: string;
  values: number[];
  indexMap: number[];
  reverseMap: Map<number, number>;
  height: number;
  /** Render as zone-colored bars (design HRChart) instead of a line. */
  barZones?: ZoneRange[];
  /** Y-range for bar mode (hrVisRange / powerVisRange). */
  barRange?: (min: number, max: number) => [number, number];
  /** Bar window width in x units (bar mode only) — pads the x scale by half
  1 * a window per side so the edge bars aren't clipped to half-width. */
  barStep?: number;
  /** Smoothed grade (%) per chart point (elevation only) — colors the line
   * by climb steepness and adds the percentage to the hover popup. */
  gradeValues?: (number | null)[];
  /** Stats line for a drag-selected trackpoint range (elevation only) —
   * enables x-drag selection and the selection badge. */
  selectionStats?: (tpA: number, tpB: number) => string | null;
  /** Right-click handler for the active selection box (client coords) —
   * opens the "Save segment" form. */
  onSelectionMenu?: (x: number, y: number) => void;
}) {
  const containerRef = useRef<HTMLDivElement>(null);
  const plotRef = useRef<uPlot | null>(null);
  // The contextmenu listener lives as long as the plot, which outlives any
  // single render — read the callback through a ref so it can never go
  // stale, independent of the plot effect's dependency list.
  const onSelectionMenuRef = useRef(onSelectionMenu);
  onSelectionMenuRef.current = onSelectionMenu;
  const setHoveredPointIndex = useActivityStore((s) => s.setHoveredPointIndex);
  const hoveredPointIndex = useActivityStore((s) => s.hoveredPointIndex);
  const setSelectedRange = useActivityStore((s) => s.setSelectedRange);
  const selectedRange = useActivityStore((s) => s.selectedRange);
  const isLocalCursor = useRef(false);
  // The trackpoint range this chart itself last published — a store range
  // that differs came from OUTSIDE (a segment-effort click) and must be
  // drawn onto the plot instead of being treated as our own echo.
  const publishedRange = useRef<[number, number] | null>(null);
  const [tip, setTip] = useState<{ left: number; text: string } | null>(null);
  // Drag-selection badge: the selected span's stats, centered over the box.
  const [sel, setSel] = useState<{ left: number; width: number; text: string } | null>(
    null,
  );
  // Recreate the plot on theme change — uPlot bakes axis/grid colors in.
  const dark = useResolvedDark();

  // Format the value under the cursor for the hover popup.
  const fmtVal = (v: number) => {
    if (config.valueFmt) {
      const suffix = config.unit.startsWith("min/") ? ` /${config.unit.slice(4)}` : "";
      return `${config.valueFmt(v)}${suffix}`;
    }
    const dec = config.key === "speed" ? 1 : 0;
    return `${v.toFixed(dec)} ${config.unit}`;
  };

  useEffect(() => {
    if (!containerRef.current || xValues.length === 0) return;

    const tickColor = chartTextColor();
    const gridColor = chartGridColor();

    const opts: uPlot.Options = {
      width: containerRef.current.clientWidth,
      height,
      cursor: {
        // setScale:false keeps drag as pure selection — the default would
        // ZOOM the x scale on release.
        drag: selectionStats
          ? { x: true, y: false, setScale: false }
          : { x: false, y: false },
        sync: { key: SYNC_KEY, setSeries: false },
      },
      // Non-selecting charts must not mirror the synced drag: the cursor
      // sync REPLAYS elevation's drag here, and on release uPlot applies
      // the receiver's own drag.setScale default (true) with the source's
      // drag flags — zooming this chart's x scale to the selection (bars
      // ballooned to fill the card). select.show:false makes every synced
      // select/zoom path a no-op.
      ...(selectionStats
        ? {}
        : { select: { show: false, left: 0, top: 0, width: 0, height: 0 } }),
      hooks: {
        ...(selectionStats
          ? {
              setSelect: [
                (u: uPlot) => {
                  const cont = containerRef.current;
                  if (u.select.width <= 0 || !cont) {
                    setSel(null);
                    publishedRange.current = null;
                    setSelectedRange(null);
                    return;
                  }
                  const iA = nearestIdx(xValues, u.posToVal(u.select.left, "x"));
                  const iB = nearestIdx(
                    xValues,
                    u.posToVal(u.select.left + u.select.width, "x"),
                  );
                  const text = selectionStats(indexMap[iA], indexMap[iB]);
                  if (!text) {
                    setSel(null);
                    publishedRange.current = null;
                    setSelectedRange(null);
                    return;
                  }
                  // Publish the trackpoint range — the route map thickens
                  // this segment. Ordered: a time-axis drag maps 1:1, and
                  // indexMap is ascending either way.
                  const range: [number, number] = [
                    Math.min(indexMap[iA], indexMap[iB]),
                    Math.max(indexMap[iA], indexMap[iB]),
                  ];
                  publishedRange.current = range;
                  setSelectedRange(range);
                  // select.left is relative to the plot area (u.over).
                  const overLeft =
                    u.over.getBoundingClientRect().left -
                    cont.getBoundingClientRect().left;
                  setSel({ left: overLeft + u.select.left, width: u.select.width, text });
                },
              ],
            }
          : {}),
        setCursor: [
          (u) => {
            const chartIdx = u.cursor.idx;
            const cont = containerRef.current;
            if (chartIdx == null) {
              isLocalCursor.current = false;
              setHoveredPointIndex(null);
              setTip(null);
              return;
            }
            // Position the value popup over the cursor (account for the
            // y-axis offset between the plot area and the container).
            if (cont) {
              const left =
                u.over.getBoundingClientRect().left -
                cont.getBoundingClientRect().left +
                (u.cursor.left ?? 0);
              const g = gradeValues?.[chartIdx];
              // Signed grade so descents read as such.
              const gradeTxt = g != null && isFinite(g) ? ` · ${formatGrade(g)}` : "";
              setTip({ left, text: fmtVal(values[chartIdx]) + gradeTxt });
            }
            const tpIdx = indexMap[chartIdx] ?? null;
            isLocalCursor.current = true;
            setHoveredPointIndex(tpIdx);
          },
        ],
      },
      // Kill uPlot's auto side padding: the card supplies the breathing room,
      // and every extra pixel here shrinks the plot itself. Top keeps a sliver
      // so the topmost y label and the line's peak don't clip; right just
      // covers the overhang of the last x tick label.
      padding: [10, 8, 0, 0],
      scales: {
        x: {
          time: false,
          // Bars are centered on window midpoints; without half a window of
          // slack on each side the first/last bar hang past the scale's
          // min/max and get clipped by the plot area.
          ...(barZones && barStep && barStep > 0
            ? {
                range: (_u: uPlot, min: number, max: number): [number, number] => [
                  min - barStep / 2,
                  max + barStep / 2,
                ],
              }
            : {}),
        },
        // dir:-1 puts small values (fast pace) at the top.
        ...(config.invertY ? { y: { dir: -1 as const } } : {}),
        // Bars use the design's padded, tens-rounded range (per metric).
        ...(barZones && barRange
          ? { y: { range: (_u: uPlot, min: number, max: number) => barRange(min, max) } }
          : {}),
      },
      axes: [
        {
          label: xUnit,
          size: 30,
          labelSize: 16,
          font: "11px sans-serif",
          labelFont: "11px sans-serif",
          stroke: tickColor,
          grid: { stroke: gridColor, width: 1 },
          ticks: { stroke: gridColor, width: 1 },
        },
        {
          size: yAxisSize,
          font: "11px sans-serif",
          stroke: tickColor,
          grid: { stroke: gridColor, width: 1 },
          ticks: { stroke: gridColor, width: 1 },
          values: config.valueFmt
            ? (_u, ticks) => ticks.map((v) => config.valueFmt!(v))
            : undefined,
        },
      ],
      series: [
        { label: xLabel },
        {
          label: `${config.label} (${config.unit})`,
          stroke: config.color,
          fill: config.fill,
          width: 1.5,
          // Inverted axis: fill DOWN from the line toward the slow edge —
          // the default 0-baseline sits above the data there and would
          // paint the fill overhead.
          fillTo: config.invertY ? (_u, _si, _dataMin, dataMax) => dataMax : undefined,
          // Hypsometric bands: a sharp-stop vertical gradient paints the
          // fill by altitude, atlas-style. Offsets clamp to the visible
          // range, so sea-level rides are simply green.
          ...(config.elevationBands
            ? {
                fill: (u: uPlot) => {
                  const { top, height } = u.bbox;
                  const grad = u.ctx.createLinearGradient(0, top, 0, top + height);
                  const stops = bandGradientStops(
                    config.elevationBands!,
                    (v) => u.valToPos(v, "y", true),
                    top,
                    height,
                  );
                  for (const s of stops) grad.addColorStop(s.offset, s.color);
                  return grad;
                },
              }
            : {}),
          // Grade coloring: a sharp-stop HORIZONTAL gradient paints the
          // line by climb steepness (flat teal → warm ramp), the sibling of
          // the vertical fill gradient above. A hair wider than the default
          // 1.5 — the color IS the information here.
          ...(gradeValues
            ? {
                width: 2,
                stroke: (u: uPlot) => {
                  const { left, width } = u.bbox;
                  const grad = u.ctx.createLinearGradient(left, 0, left + width, 0);
                  const stops = gradeGradientStops(
                    xValues,
                    gradeValues,
                    (x) => u.valToPos(x, "x", true),
                    left,
                    width,
                  );
                  for (const s of stops) grad.addColorStop(s.offset, s.color);
                  return grad;
                },
              }
            : {}),
          // Design HRChart: rounded bars, each colored by the zone its
          // value falls into, at the design's 0.88 opacity ("e0" alpha).
          ...(barZones
            ? {
                width: 0,
                points: { show: false },
                paths: uPlot.paths.bars!({
                  size: [0.8, 100],
                  radius: 0.25,
                  disp: {
                    fill: {
                      unit: 3,
                      // Array.from, NOT .map: u.data[1] is a Float64Array,
                      // whose .map would coerce the color strings to NaN.
                      values: (u) =>
                        Array.from(
                          u.data[1] as ArrayLike<number>,
                          (v) => `${zoneColorFor(v ?? 0, barZones)}e0`,
                        ),
                    },
                  },
                }),
              }
            : {}),
        },
      ],
      legend: { show: false },
    };

    const data: uPlot.AlignedData = [
      new Float64Array(xValues),
      new Float64Array(values),
    ];

    plotRef.current = new uPlot(opts, data, containerRef.current);

    // uPlot's double-click reset hides its select box WITHOUT firing the
    // setSelect hook (internal hideSelect passes fire=false) — clear the
    // badge and the published map range ourselves or they outlive the box.
    if (selectionStats) {
      plotRef.current.over.addEventListener("dblclick", () => {
        setSel(null);
        publishedRange.current = null;
        setSelectedRange(null);
      });
    }

    // Right-click inside the active selection box → the save-segment menu.
    // Outside the box (or with no selection) the browser menu stays.
    if (selectionStats) {
      plotRef.current.over.addEventListener("contextmenu", (e) => {
        const cb = onSelectionMenuRef.current;
        const u = plotRef.current;
        if (!cb || !u || u.select.width <= 0) return;
        const px = e.clientX - u.over.getBoundingClientRect().left;
        if (px < u.select.left || px > u.select.left + u.select.width) return;
        e.preventDefault();
        cb(e.clientX, e.clientY);
      });
    }

    const onResize = () => {
      if (plotRef.current && containerRef.current) {
        plotRef.current.setSize({ width: containerRef.current.clientWidth, height });
      }
    };
    window.addEventListener("resize", onResize);
    // Re-fit when the container width changes (e.g. a chart moves into or out
    // of the full-width first slot on reorder), not just on window resize.
    const ro = new ResizeObserver(onResize);
    ro.observe(containerRef.current);

    return () => {
      window.removeEventListener("resize", onResize);
      ro.disconnect();
      plotRef.current?.destroy();
      setTip(null);
      setSel(null);
      // The plot recreate drops uPlot's select box — retract the map
      // highlight with it (no-op for chart types that never set it).
      if (selectionStats) setSelectedRange(null);
    };
    // `dark` re-bakes axis/grid colors from the live CSS tokens.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [xValues, values, height, config, setHoveredPointIndex, indexMap, dark, barZones, barStep, gradeValues]);

  // Sync selection FROM external sources (segment-effort click) → chart:
  // draw uPlot's select box over the published trackpoint range. The box is
  // set with fire=false and the badge is rendered here directly, so the
  // store keeps the EXACT effort range (a fired hook would republish it
  // re-snapped to chart points, deselecting the panel row).
  useEffect(() => {
    if (!selectionStats) return;
    const own = publishedRange.current;
    const same =
      selectedRange === own ||
      (selectedRange != null &&
        own != null &&
        selectedRange[0] === own[0] &&
        selectedRange[1] === own[1]);
    if (same) return;
    // valToPos is NaN in the tick the plot is constructed — defer a beat.
    const t = setTimeout(() => {
      const u = plotRef.current;
      const cont = containerRef.current;
      if (!u || !cont) return;
      if (selectedRange == null) {
        publishedRange.current = null;
        u.setSelect({ left: 0, top: 0, width: 0, height: 0 }, false);
        setSel(null);
        return;
      }
      const maxTp = indexMap.length > 0 ? indexMap[indexMap.length - 1] : 0;
      const cA = nearestChartIdx(reverseMap, selectedRange[0], maxTp);
      const cB = nearestChartIdx(reverseMap, selectedRange[1], maxTp);
      if (cA == null || cB == null || cA === cB) return;
      const left = u.valToPos(xValues[Math.min(cA, cB)], "x");
      const right = u.valToPos(xValues[Math.max(cA, cB)], "x");
      if (!Number.isFinite(left) || !Number.isFinite(right)) return;
      publishedRange.current = selectedRange;
      u.setSelect(
        { left, top: 0, width: right - left, height: u.over.clientHeight },
        false,
      );
      const text = selectionStats(selectedRange[0], selectedRange[1]);
      if (text) {
        const overLeft =
          u.over.getBoundingClientRect().left - cont.getBoundingClientRect().left;
        setSel({ left: overLeft + left, width: right - left, text });
      }
    }, 0);
    return () => clearTimeout(t);
    // selectionStats is recreated per parent render; the `same` guard above
    // makes those re-runs no-ops.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedRange, reverseMap, xValues, indexMap]);

  // Sync cursor FROM external source (map hover) → chart
  useEffect(() => {
    const plot = plotRef.current;
    if (!plot) return;
    if (isLocalCursor.current) return;

    if (hoveredPointIndex == null) {
      plot.setCursor({ left: -1, top: -1 }, false);
      return;
    }
    const chartIdx = reverseMap.get(hoveredPointIndex);
    if (chartIdx == null) return;
    if (plot.cursor.idx === chartIdx) return;

    const left = plot.valToPos(xValues[chartIdx], "x");
    const top = plot.valToPos(values[chartIdx], "y");
    plot.setCursor({ left, top }, false);
  }, [hoveredPointIndex, reverseMap, xValues, values, config.key]);

  return (
    <div className="relative w-full">
      <div ref={containerRef} className="w-full" />
      {sel && (
        <div
          className="pointer-events-none absolute z-10 -translate-x-1/2 whitespace-nowrap rounded-md px-2 py-0.5 text-[11px] font-semibold tabular-nums"
          style={{
            top: 2,
            left: sel.left + sel.width / 2,
            background: "var(--ink)",
            color: "var(--surface)",
          }}
        >
          {sel.text}
        </div>
      )}
      {tip && !sel && (
        <div
          className="pointer-events-none absolute z-10 -translate-x-1/2 whitespace-nowrap rounded-md px-2 py-0.5 text-[11px] font-semibold tabular-nums"
          style={{ top: 2, left: tip.left, background: "var(--ink)", color: "var(--surface)" }}
        >
          {tip.text}
        </div>
      )}
    </div>
  );
}

const DEFAULT_ORDER: ChartType[] = ["elevation", "hr", "pace", "speed", "cadence", "power"];
const ORDER_KEY = "chart_order";

/** Ensure every metric key is present (append any missing in default order). */
function normalizeOrder(order: ChartType[]): ChartType[] {
  const seen = new Set(order);
  return [...order.filter((k) => DEFAULT_ORDER.includes(k)), ...DEFAULT_ORDER.filter((k) => !seen.has(k))];
}

export function ChartPanel({ trackpoints, sport, timeInZones, ftpW, segmentSource }: Props) {
  const showPace = isPaceSport(sport);
  // Right-click on the elevation selection → "Save segment" form at the
  // click point. The range snapshot is taken at open so a later selection
  // change can't retarget an already-open form.
  const [segMenu, setSegMenu] = useState<{ x: number; y: number; range: [number, number] } | null>(
    null,
  );
  const openSegmentMenu = useCallback(
    (x: number, y: number) => {
      const range = useActivityStore.getState().selectedRange;
      if (!range) return;
      // Mirror the backend's minimum (build_segment needs ≥2 GPS points) —
      // otherwise the form opens on a selection that can only error.
      let gpsPoints = 0;
      for (let i = range[0]; i <= range[1] && gpsPoints < 2; i++) {
        if (trackpoints.lat[i] != null && trackpoints.lon[i] != null) gpsPoints++;
      }
      if (gpsPoints >= 2) setSegMenu({ x, y, range });
    },
    [trackpoints],
  );
  const showSwimPace = isSwimSport(sport);
  const hrRanges = useMemo(() => hrZoneRanges(timeInZones ?? []), [timeInZones]);
  const powerRanges = useMemo(
    () => powerZoneRanges(timeInZones ?? [], ftpW),
    [timeInZones, ftpW],
  );

  // One card per available metric (pace for foot sports, min/100 pace for
  // swim sports, speed otherwise).
  const units = useUnits();
  const configs = useMemo(
    () => [
      ELEVATION(),
      HR,
      showSwimPace ? SWIM_PACE() : showPace ? PACE() : SPEED(),
      CADENCE,
      POWER,
    ],
    // `units` re-creates the factory configs with the right conversions.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [showPace, showSwimPace, units],
  );
  // Materialize each metric's series ONCE and share it with chartData below —
  // getData runs an O(N) pass (pace/speed also derive speedSeries), so calling
  // it here for the filter and again in chartData doubled the work per render.
  const seriesByKey = useMemo(() => {
    const m = new Map<ChartType, (number | null)[]>();
    for (const c of configs) m.set(c.key, c.getData(trackpoints));
    return m;
  }, [configs, trackpoints]);
  const available = useMemo(
    () => configs.filter((c) => hasData(seriesByKey.get(c.key) ?? [])),
    [configs, seriesByKey],
  );

  // Persisted, drag-reorderable global chart order.
  const { data: savedOrder } = useQuery({
    queryKey: ["setting", ORDER_KEY],
    queryFn: () => api.getSetting(ORDER_KEY),
  });
  const [order, setOrder] = useState<ChartType[]>(DEFAULT_ORDER);
  useEffect(() => {
    if (!savedOrder) return;
    try {
      const parsed = JSON.parse(savedOrder);
      if (Array.isArray(parsed)) setOrder(normalizeOrder(parsed));
    } catch {
      /* ignore malformed setting */
    }
  }, [savedOrder]);

  const dragKey = useRef<ChartType | null>(null);
  const [overKey, setOverKey] = useState<ChartType | null>(null);
  const [draggingKey, setDraggingKey] = useState<ChartType | null>(null);
  const [ghost, setGhost] = useState<{ x: number; y: number; label: string } | null>(null);

  const persistOrder = (next: ChartType[]) => {
    setOrder(next);
    api.setSetting(ORDER_KEY, JSON.stringify(next)).catch(() => {});
  };

  const reorder = (from: ChartType, target: ChartType) => {
    if (from === target) return;
    const next = [...order];
    next.splice(next.indexOf(from), 1);
    next.splice(next.indexOf(target), 0, from);
    persistOrder(next);
  };

  // Pointer-based drag (not HTML5 DnD): the Tauri webview's native file
  // drag-drop would otherwise hijack it and trigger the import overlay.
  const startDrag = (e: React.PointerEvent, key: ChartType) => {
    e.preventDefault();
    dragKey.current = key;
    const label = configs.find((c) => c.key === key)?.label ?? "";
    document.body.style.userSelect = "none";
    document.body.style.cursor = "grabbing";

    const keyAt = (x: number, y: number): ChartType | null => {
      const card = (document.elementFromPoint(x, y) as HTMLElement | null)?.closest(
        "[data-chart-key]",
      );
      return (card?.getAttribute("data-chart-key") as ChartType | null) ?? null;
    };
    const move = (ev: PointerEvent) => {
      setDraggingKey(key);
      setGhost({ x: ev.clientX, y: ev.clientY, label });
      const k = keyAt(ev.clientX, ev.clientY);
      setOverKey(k && k !== dragKey.current ? k : null);
    };
    const up = (ev: PointerEvent) => {
      document.removeEventListener("pointermove", move);
      document.removeEventListener("pointerup", up);
      document.body.style.userSelect = "";
      document.body.style.cursor = "";
      const target = keyAt(ev.clientX, ev.clientY);
      const from = dragKey.current;
      dragKey.current = null;
      setOverKey(null);
      setDraggingKey(null);
      setGhost(null);
      if (from && target) reorder(from, target);
    };
    document.addEventListener("pointermove", move);
    document.addEventListener("pointerup", up);
  };

  // Available metrics sorted by the saved order.
  const ordered = useMemo(() => {
    const rank = (k: ChartType) => {
      const i = order.indexOf(k);
      return i === -1 ? 999 : i;
    };
    return [...available].sort((a, b) => rank(a.key) - rank(b.key));
  }, [available, order]);

  const hasTimeData = useMemo(() => trackpoints.t.some((v) => v != null), [trackpoints]);
  // A strength/indoor session records no distance, so a distance X-axis is a
  // degenerate flat line — default (and restrict) to time there.
  const hasDistanceData = useMemo(
    () => trackpoints.distance_m.some((v) => v != null && v !== 0),
    [trackpoints],
  );
  const [xAxis, setXAxis] = useState<XAxis>(() => (hasDistanceData ? "distance" : "time"));

  // Smoothed grade (%) per trackpoint — meters in, unitless out, so the
  // display-unit setting never touches it.
  const grades = useMemo(
    () => gradeSeries(trackpoints.distance_m, trackpoints.altitude_m),
    [trackpoints],
  );

  const chartData = useMemo(() => {
    if (available.length === 0) return null;

    const xValues: number[] = [];
    const indexMap: number[] = [];
    const reverseMap = new Map<number, number>();
    const chartValues = new Map<ChartType, number[]>();
    const gradeValues: (number | null)[] = [];
    for (const config of available) chartValues.set(config.key, []);

    // Reuse the series already materialized for the `available` filter — no
    // second getData pass (pace/speed re-derivation) per render.
    const series = available.map(
      (config) => [config, seriesByKey.get(config.key) ?? []] as const,
    );

    const firstT = trackpoints.t.find((v) => v != null) ?? 0;

    for (let i = 0; i < trackpoints.distance_m.length; i++) {
      let xVal: number | null;
      if (xAxis === "time") {
        const t = trackpoints.t[i];
        xVal = t != null ? (t - firstT) / 60 : null;
      } else {
        xVal =
          trackpoints.distance_m[i] != null
            ? trackpoints.distance_m[i]! / (isImperial() ? M_PER_MILE : 1000)
            : null;
      }
      if (xVal == null) continue;

      const chartIdx = xValues.length;
      indexMap.push(i);
      reverseMap.set(i, chartIdx);
      xValues.push(xVal);
      for (const [config, data] of series) {
        chartValues.get(config.key)!.push(data[i] ?? 0);
      }
      gradeValues.push(grades[i] ?? null);
    }

    if (xValues.length === 0) return null;
    return { xValues, indexMap, reverseMap, chartValues, gradeValues };
  }, [trackpoints, xAxis, available, seriesByKey, grades]);

  // Bar counts follow the panel's real width (~14px per bar, see
  // zoneBarCount), per SLOT: the full-width first card fits ~2× the bars of
  // a half-width one. The observer re-attaches when chartData flips
  // non-null — before that the panel isn't rendered and panelRef is empty.
  const panelRef = useRef<HTMLDivElement>(null);
  const [panelW, setPanelW] = useState(0);
  const hasChartData = chartData != null;
  useEffect(() => {
    const el = panelRef.current;
    if (!el) return;
    const update = () => setPanelW(Math.round(el.clientWidth));
    update();
    const ro = new ResizeObserver(update);
    ro.observe(el);
    return () => ro.disconnect();
  }, [hasChartData]);
  // gap-4 = 16px between the grid's two columns.
  const halfCount = zoneBarCount(panelW > 0 ? (panelW - 16) / 2 : 0);
  const fullCount = zoneBarCount(panelW);
  // The chart occupying the col-span-2 first slot (reorder moves it).
  const firstKey = ordered[0]?.key;

  // Zone-colored bar charts (the design HRChart): each bar the max of its
  // window, with both cursor-sync maps re-derived at bar resolution
  // (bar → its max sample's trackpoint; trackpoint → its window's bar).
  // HR always qualifies (design defaults exist); power only with real FIT
  // boundaries — without FTP-based zones it stays a line.
  const barCharts = useMemo(() => {
    const out = new Map<
      ChartType,
      {
        xValues: number[];
        values: number[];
        indexMap: number[];
        reverseMap: Map<number, number>;
        zones: ZoneRange[];
        range: (min: number, max: number) => [number, number];
        step: number;
      }
    >();
    if (!chartData) return out;
    // Cadence zones depend on the data itself (per-leg vs full-spm scale
    // detection), so they resolve here where the values are at hand.
    // Speed zones convert with the SAME m/s → display factor as the series.
    const mpsToUnit = isImperial() ? MPH_PER_MPS : 3.6;
    const zoned: [ChartType, (vals: number[]) => ZoneRange[] | null, typeof hrVisRange][] = [
      ["hr", () => hrRanges, hrVisRange],
      ["power", () => powerRanges, powerVisRange],
      ["cadence", (vals) => cadenceZoneRanges(timeInZones ?? [], sport, vals), powerVisRange],
      ["speed", () => speedZoneRanges(timeInZones ?? [], sport, mpsToUnit), speedVisRange],
    ];
    for (const [key, zonesFor, range] of zoned) {
      const vals = chartData.chartValues.get(key);
      if (!vals) continue;
      const zones = zonesFor(vals);
      if (!zones) continue;
      const b = bucketMaxBars(
        chartData.xValues,
        vals,
        key === firstKey ? fullCount : halfCount,
      );
      const indexMap = b.srcIdx.map((chartIdx) => chartData.indexMap[chartIdx]);
      const reverseMap = new Map<number, number>();
      chartData.indexMap.forEach((tpIdx, chartIdx) =>
        reverseMap.set(tpIdx, b.barOf[chartIdx]),
      );
      out.set(key, {
        xValues: b.xs,
        values: b.values,
        indexMap,
        reverseMap,
        zones,
        range,
        step: b.step,
      });
    }
    return out;
  }, [chartData, hrRanges, powerRanges, timeInZones, sport, firstKey, halfCount, fullCount]);

  if (!chartData) return null;

  const { xValues, indexMap, reverseMap, chartValues, gradeValues } = chartData;
  // Grade only makes sense when it actually varies — an indoor session with
  // constant (or absent) altitude keeps the plain teal line.
  const hasGrades = gradeValues.some((g) => g != null && g !== 0);

  // Drag-selection stats for the elevation chart: span distance, net climb,
  // average grade — "2.41 km · +183 m · +7.6%", all in display units.
  const elevationSelectionStats = (tpA: number, tpB: number): string | null => {
    const s = selectionGrade(
      trackpoints.distance_m,
      trackpoints.altitude_m,
      tpA,
      tpB,
      trackpoints.t,
    );
    return s && formatSelectionStats(s);
  };
  const xLabel = xAxis === "time" ? "Time (min)" : `Distance (${distanceUnit()})`;
  const xUnit = xAxis === "time" ? "min" : distanceUnit();

  return (
    <div ref={panelRef} className="space-y-3">
      {/* Offer the axis toggle only when both axes carry data — with no
          distance there is nothing to switch to. */}
      {hasTimeData && hasDistanceData && (
        <div className="flex justify-end">
          <div className="seg">
            <button
              onClick={() => setXAxis("distance")}
              className={xAxis === "distance" ? "on" : ""}
            >
              Distance
            </button>
            <button
              onClick={() => setXAxis("time")}
              className={xAxis === "time" ? "on" : ""}
            >
              Time
            </button>
          </div>
        </div>
      )}

      <div className="grid grid-cols-2 gap-4">
        {ordered.map((config, i) => (
          <div
            key={config.key}
            data-chart-key={config.key}
            style={{ padding: "12px 14px" }}
            className={`dash-card transition ${i === 0 ? "col-span-2" : ""} ${
              draggingKey === config.key ? "opacity-40" : ""
            } ${overKey === config.key ? "ring-2 ring-accent" : ""}`}
          >
            <div className="flex items-center justify-between mb-2">
              <h3 className="!m-0">{config.label}</h3>
              <span
                onPointerDown={(e) => startDrag(e, config.key)}
                className="cursor-grab text-faint hover:text-muted active:cursor-grabbing touch-none"
                title="Drag to reorder"
              >
                <GripVertical size={16} />
              </span>
            </div>
            <SingleChart
              key={`${config.key}-${xAxis}`}
              config={config}
              xValues={barCharts.get(config.key)?.xValues ?? xValues}
              xLabel={xLabel}
              xUnit={xUnit}
              values={barCharts.get(config.key)?.values ?? chartValues.get(config.key)!}
              indexMap={barCharts.get(config.key)?.indexMap ?? indexMap}
              reverseMap={barCharts.get(config.key)?.reverseMap ?? reverseMap}
              height={210}
              barZones={barCharts.get(config.key)?.zones}
              barRange={barCharts.get(config.key)?.range}
              barStep={barCharts.get(config.key)?.step}
              gradeValues={config.key === "elevation" && hasGrades ? gradeValues : undefined}
              selectionStats={
                config.key === "elevation" && hasGrades ? elevationSelectionStats : undefined
              }
              onSelectionMenu={
                config.key === "elevation" && hasGrades && segmentSource
                  ? openSegmentMenu
                  : undefined
              }
            />
          </div>
        ))}
      </div>

      {segMenu && segmentSource && (
        <SaveSegmentPopover
          x={segMenu.x}
          y={segMenu.y}
          activityId={segmentSource.activityId}
          range={segMenu.range}
          onClose={() => setSegMenu(null)}
        />
      )}

      {/* Drag ghost following the cursor */}
      {ghost && (
        <div
          className="pointer-events-none fixed z-50 flex items-center gap-1.5 rounded-lg border border-border bg-card px-2.5 py-1.5 text-sm font-semibold text-ink shadow-lg"
          style={{ left: ghost.x + 12, top: ghost.y + 12 }}
        >
          <GripVertical size={14} className="text-faint" />
          {ghost.label}
        </div>
      )}
    </div>
  );
}
