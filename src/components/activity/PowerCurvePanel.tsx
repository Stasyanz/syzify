import { useEffect, useMemo, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useNavigate } from "react-router";
import uPlot from "uplot";
import "uplot/dist/uPlot.min.css";
import { api } from "../../lib/tauri";
import { chartTextColor, chartGridColor } from "../../lib/chartTheme";
import { useResolvedDark } from "../../lib/theme";
import { AXIS_SPLITS, formatWindow, mergeCurveData, tooltipText } from "./powerCurve";

const HEIGHT = 220;
// Matches the Power metric in ChartPanel — the curve is the same quantity.
const CURVE_COLOR = "#7c3aed";
const CURVE_FILL = "rgba(124,58,237,0.10)";

/** The activity page's "Power Curve" card: this ride's mean-max power line
 * over the library-wide all-time envelope (log time axis, 1 s → 1 h).
 * Hovering shows the window's values and which activity holds the record;
 * clicking while a foreign record is hovered navigates to that activity.
 * Renders nothing for activities without a stored curve. */
export function PowerCurvePanel({ activityId }: { activityId: string }) {
  const navigate = useNavigate();
  const dark = useResolvedDark();
  const containerRef = useRef<HTMLDivElement | null>(null);
  const plotRef = useRef<uPlot | null>(null);
  const [tip, setTip] = useState<{ left: number; text: string } | null>(null);
  // The record under the cursor, for the click-through; a ref because the
  // click handler is registered once at plot creation.
  const hoveredRecord = useRef<string | null>(null);

  const { data } = useQuery({
    queryKey: ["power-curve", activityId],
    queryFn: () => api.getPowerCurve(activityId),
  });

  const merged = useMemo(
    () => (data && data.points.length > 0 ? mergeCurveData(data.points, data.envelope) : null),
    [data],
  );

  useEffect(() => {
    const cont = containerRef.current;
    if (!cont || !merged) return;
    // Navigating /activity/A → /activity/B remounts the plot but not the
    // component — a stale tooltip would float over the new chart.
    setTip(null);
    hoveredRecord.current = null;

    const tickColor = chartTextColor();
    const gridColor = chartGridColor();

    const opts: uPlot.Options = {
      width: cont.clientWidth,
      height: HEIGHT,
      padding: [8, 12, 0, 0],
      scales: {
        // Log time axis: the curve's whole story lives in its left decade.
        // Pin the range to the data — uPlot's log default rounds the max up
        // to the next power of ten (3600 → 10000), wasting a dead right
        // margin past the 1 h tick.
        x: {
          time: false,
          distr: 3,
          range: () => [merged.x[0], merged.x[merged.x.length - 1]],
        },
        y: { range: (_u, _min, max) => [0, max * 1.05] },
      },
      axes: [
        {
          size: 30,
          font: "11px sans-serif",
          stroke: tickColor,
          grid: { stroke: gridColor, width: 1 },
          ticks: { stroke: gridColor, width: 1 },
          splits: () => AXIS_SPLITS.filter((s) => s <= merged.x[merged.x.length - 1]),
          // uPlot's default log-scale filter blanks most custom splits
          // (labels literally render "null") — keep every split we chose.
          filter: (_u, splits) => splits,
          values: (_u, ticks) => ticks.map((v) => (v == null ? "" : formatWindow(v))),
        },
        {
          size: 40,
          font: "11px sans-serif",
          stroke: tickColor,
          grid: { stroke: gridColor, width: 1 },
          ticks: { stroke: gridColor, width: 1 },
        },
      ],
      series: [
        { label: "Window" },
        {
          label: "All-time (W)",
          stroke: gridColor,
          fill: dark ? "rgba(255,255,255,0.06)" : "rgba(0,0,0,0.05)",
          width: 1,
          spanGaps: true,
          points: { show: false },
        },
        {
          label: "This activity (W)",
          stroke: CURVE_COLOR,
          fill: CURVE_FILL,
          width: 2,
          spanGaps: true,
          points: { show: false },
        },
      ],
      legend: { show: false },
      cursor: {
        y: false,
        points: { show: true },
        // The default drag would ZOOM the x scale on release (the documented
        // ChartPanel lesson) — this chart has nothing to zoom into.
        drag: { x: false, y: false },
      },
      hooks: {
        setCursor: [
          (u) => {
            const idx = u.cursor.idx;
            if (idx == null) {
              hoveredRecord.current = null;
              setTip(null);
              return;
            }
            const rec = merged.record[idx];
            hoveredRecord.current =
              rec && rec.activity_id !== activityId ? rec.activity_id : null;
            // The pointer cursor is the only affordance for the click-through.
            u.over.style.cursor = hoveredRecord.current ? "pointer" : "default";
            const left =
              u.over.getBoundingClientRect().left -
              (containerRef.current?.getBoundingClientRect().left ?? 0) +
              (u.cursor.left ?? 0);
            setTip({
              left,
              text: tooltipText(merged.x[idx], merged.activity[idx], rec, activityId),
            });
          },
        ],
      },
    };

    const plotData: uPlot.AlignedData = [
      merged.x,
      merged.envelope,
      merged.activity,
    ];

    const plot = new uPlot(opts, plotData, cont);
    plotRef.current = plot;

    // Click-through to the activity holding the hovered record. A drag that
    // happens to end on the plot also fires click — ignore anything that
    // moved more than a jitter between press and release.
    let downAt: { x: number; y: number } | null = null;
    const onDown = (e: MouseEvent) => {
      downAt = { x: e.clientX, y: e.clientY };
    };
    const onClick = (e: MouseEvent) => {
      const dragged =
        downAt != null &&
        (Math.abs(e.clientX - downAt.x) > 4 || Math.abs(e.clientY - downAt.y) > 4);
      if (!dragged && hoveredRecord.current) navigate(`/activity/${hoveredRecord.current}`);
    };
    plot.over.addEventListener("mousedown", onDown);
    plot.over.addEventListener("click", onClick);

    const ro = new ResizeObserver(() => {
      if (plotRef.current && cont.clientWidth > 0) {
        plotRef.current.setSize({ width: cont.clientWidth, height: HEIGHT });
      }
    });
    ro.observe(cont);

    return () => {
      ro.disconnect();
      plot.over.removeEventListener("mousedown", onDown);
      plot.over.removeEventListener("click", onClick);
      plot.destroy();
      plotRef.current = null;
    };
  }, [merged, dark, activityId, navigate]);

  if (!merged) return null;

  return (
    <div className="dash-card">
      <div className="mb-3 flex items-baseline justify-between">
        <h3>Power Curve</h3>
        <span className="text-faint text-xs">best average power · all-time in grey</span>
      </div>
      <div className="relative w-full">
        <div ref={containerRef} className="w-full" />
        {tip && (
          <div
            className="pointer-events-none absolute z-10 -translate-x-1/2 whitespace-nowrap rounded-md px-2 py-0.5 text-[11px] font-semibold tabular-nums"
            style={{ top: 2, left: tip.left, background: "var(--ink)", color: "var(--surface)" }}
          >
            {tip.text}
          </div>
        )}
      </div>
    </div>
  );
}
