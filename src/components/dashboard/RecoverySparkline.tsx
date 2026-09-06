import { useLayoutEffect, useRef, useState } from "react";
import {
  BAND_TOKEN,
  shortDate,
  sparkline,
  type SparkHistoryPoint,
  type SparkPoint,
} from "../../lib/recovery";

/** Sparkline box: the width follows the space left in the row (measured),
 * the height is fixed so a wider row stretches the time axis only. */
const SPARK = { minWidth: 180, height: 56, pad: 5, days: 28 };

/** The width of an element, tracked through ResizeObserver; `initial`
 * until the first measurement (and in environments without the API). */
function useMeasuredWidth<T extends HTMLElement>(initial: number) {
  const ref = useRef<T>(null);
  const [width, setWidth] = useState(initial);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el || typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(([entry]) => {
      const w = Math.floor(entry.contentRect.width);
      if (w > 0) setWidth(w);
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  return { ref, width };
}

/** The sparse 28-day recovery sparkline (ADR 0002). Kept after the
 * dashboard card went (the calendar carries the rest): points on their
 * own day, lines only between consecutive nights, 80/60 guides, an HTML
 * hover label. It takes whatever the row has left (a
 * wider window stretches the time axis, never the dots) and WKWebView
 * shows neither native `<title>` tooltips nor the CSS data-tip one on SVG
 * elements (no ::after there), so the hovered point gets an HTML label
 * floated over the chart. */
export function RecoverySparkline({
  history,
  computedFor,
}: {
  history: SparkHistoryPoint[];
  computedFor: string;
}) {
  const [hover, setHover] = useState<SparkPoint | null>(null);
  const { ref, width } = useMeasuredWidth<HTMLDivElement>(260);
  const { points, segments, guides } = sparkline(history, computedFor, {
    width,
    height: SPARK.height,
    pad: SPARK.pad,
    days: SPARK.days,
  });
  const last = points[points.length - 1];
  if (!last) {
    return (
      <div className="ml-auto self-center text-xs text-faint" data-testid="spark-empty">
        No nights in the last 28 days
      </div>
    );
  }
  return (
    <div ref={ref} className="ml-auto flex-1" style={{ minWidth: SPARK.minWidth }}>
      <div className="relative">
        <svg
          width={width}
          height={SPARK.height}
          viewBox={`0 0 ${width} ${SPARK.height}`}
          className="block"
          role="img"
          aria-label={`Recovery index, last 28 days: ${points.length} night${
            points.length === 1 ? "" : "s"
          }, latest ${last.index} on ${shortDate(last.date)}`}
        >
          {[
            ["80", guides.y80],
            ["60", guides.y60],
          ].map(([key, y]) => (
            <line
              key={key}
              x1={SPARK.pad}
              x2={width - SPARK.pad}
              y1={y}
              y2={y}
              stroke="var(--border)"
              strokeDasharray="2 3"
            />
          ))}
          {segments.map(([a, b]) => (
            <line
              key={a.date}
              x1={a.x}
              y1={a.y}
              x2={b.x}
              y2={b.y}
              stroke="var(--border-2)"
              strokeWidth={1.5}
            />
          ))}
          {points.map((p) => (
            <circle
              key={p.date}
              cx={p.x}
              cy={p.y}
              r={hover?.date === p.date ? 4 : 3}
              fill={`var(${BAND_TOKEN[p.band]})`}
              style={{ cursor: "default" }}
              onMouseEnter={() => setHover(p)}
              onMouseLeave={() => setHover(null)}
            />
          ))}
        </svg>
        {hover && (
          <div
            className="absolute pointer-events-none whitespace-nowrap rounded-md px-2 py-0.5 text-[11px] font-medium"
            style={{
              left: hover.x,
              top: hover.y - 8,
              transform: "translate(-50%, -100%)",
              background: "var(--ink)",
              color: "var(--card)",
            }}
            data-testid="spark-tip"
          >
            {shortDate(hover.date)} · {hover.index}
          </div>
        )}
      </div>
      <div className="flex justify-between text-[10px] text-faint uppercase tracking-wide mt-1">
        <span>4 weeks ago</span>
        <span>Today</span>
      </div>
    </div>
  );
}
