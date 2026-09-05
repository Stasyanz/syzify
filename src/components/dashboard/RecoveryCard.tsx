import { useLayoutEffect, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../lib/tauri";
import type { RecoveryCard as RecoveryCardData } from "../../lib/types";
import { useInvalidateOnNewDay } from "../../hooks/useToday";
import {
  BAND_LABEL,
  BAND_TOKEN,
  buildingText,
  describeAge,
  emptyState,
  formatDelta,
  shortDate,
  sparkline,
  type SparkPoint,
} from "../../lib/recovery";

/** Sparkline box: the width follows the space left in the row (measured),
 * the height is fixed so a wider row stretches the time axis only. */
const SPARK = { minWidth: 180, height: 56, pad: 5, days: 28 };

/** The dashboard's Recovery card (ADR 0002): last night's index with its
 * band, the three components behind it, and a sparse 28-day history. Its
 * own query — the card must not wait for (or fail with) the dashboard
 * data — refetched past midnight and after every import. */
export function RecoveryCard() {
  const { data, isLoading, isError } = useQuery({
    queryKey: ["recovery"],
    queryFn: () => api.getRecovery(),
  });
  useInvalidateOnNewDay(["recovery"]);

  return (
    <section className="dash-card" aria-label="Recovery">
      <div className="flex items-baseline justify-between gap-3">
        <h3>Recovery</h3>
        {data?.date != null && data.age_days != null && (
          <span className="eyebrow">{describeAge(data.date, data.age_days)}</span>
        )}
      </div>
      {isLoading ? (
        <div className="h-20 flex items-center text-sm text-faint">Loading…</div>
      ) : isError || !data ? (
        <div className="h-20 flex items-center text-sm text-faint">Recovery unavailable</div>
      ) : (
        <Body card={data} />
      )}
    </section>
  );
}

function Body({ card }: { card: RecoveryCardData }) {
  const empty = emptyState(card);
  if (empty) {
    const [title, detail] =
      empty.kind === "no_data"
        ? [
            "No sleep data yet",
            "Drop the watch's Monitor folder onto the app to start tracking recovery",
          ]
        : empty.kind === "no_nights"
          ? [
              "No full night recorded yet",
              `${card.days_recorded_90d} day${card.days_recorded_90d === 1 ? "" : "s"} of monitoring in the last 90 — wear the watch through the night`,
            ]
          : [
              buildingText(empty.nightsNeeded),
              `${nightsLine(card.nights_recorded_90d)} · the baseline needs ${
                card.nights_recorded_90d + card.nights_needed
              } nights in 90 days`,
            ];
    return (
      <div className="h-20 flex flex-col justify-center gap-1">
        <div className="text-sm text-muted">{title}</div>
        <div className="text-xs text-faint">{detail}</div>
      </div>
    );
  }

  // The backend always sends a band with an index; the neutral tint only
  // guards the type.
  const band = card.band;
  const tint = band ? `var(${BAND_TOKEN[band]})` : "var(--ink)";
  const staleNote = (card.age_days ?? 0) > 0 ? "Last night not recorded" : null;
  const warning =
    card.warning === "hr_above_baseline" && card.hr
      ? `Night HR well above baseline (${formatDelta(card.hr.delta)} bpm)`
      : null;

  return (
    <>
      <div className="flex flex-wrap items-center gap-x-8 gap-y-4 mt-3">
        <div className="flex items-center gap-4">
          <div
            className="font-num font-extrabold leading-none"
            style={{ fontSize: 44, color: tint }}
            data-testid="recovery-index"
          >
            {card.index}
          </div>
          <div className="flex flex-col gap-1">
            {band && (
              <span
                className="text-xs font-semibold px-2 py-0.5 rounded-md self-start"
                style={{ color: tint, background: `var(${BAND_TOKEN[band]}-soft)` }}
              >
                {BAND_LABEL[band]}
              </span>
            )}
            {card.advice && <span className="text-xs text-muted">{card.advice}</span>}
          </div>
        </div>

        <div className="flex gap-6 shrink-0">
          <Stat
            label="Night HR"
            value={card.hr ? `${Math.round(card.hr.night_median)}` : "—"}
            unit="bpm"
            sub={
              card.hr
                ? `base ${Math.round(card.hr.baseline)} · ${formatDelta(card.hr.delta)}`
                : null
            }
          />
          <Stat
            label="Night stress"
            value={card.stress ? `${Math.round(card.stress.night_avg)}` : "—"}
            sub={card.stress ? `score ${Math.round(card.stress.score)}` : "no stress data"}
          />
          <Stat
            label="Yesterday's load"
            value={card.load ? `${Math.round(card.load.tss_yesterday)}` : "—"}
            unit="TSS"
            sub={card.load ? `CTL ${Math.round(card.load.ctl)}` : "no training history"}
          />
        </div>

        <Spark history={card.history} computedFor={card.computed_for} />
      </div>

      <div className="flex flex-wrap items-center justify-between gap-2 mt-3 text-xs">
        <span className="text-faint">{nightsLine(card.nights_recorded_90d)}</span>
        {warning ? (
          <span className="font-medium" style={{ color: "var(--danger)" }}>
            {warning}
          </span>
        ) : (
          staleNote && <span className="text-faint">{staleNote}</span>
        )}
      </div>
    </>
  );
}

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

/** The sparse 28-day sparkline. It takes whatever the row has left (a
 * wider window stretches the time axis, never the dots) and WKWebView
 * shows neither native `<title>` tooltips nor the CSS data-tip one on SVG
 * elements (no ::after there), so the hovered point gets an HTML label
 * floated over the chart. */
function Spark({ history, computedFor }: { history: RecoveryCardData["history"]; computedFor: string }) {
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

function nightsLine(recorded: number): string {
  return `${recorded} of the last 90 nights recorded`;
}

function Stat({
  label,
  value,
  unit,
  sub,
}: {
  label: string;
  value: string;
  unit?: string;
  sub: string | null;
}) {
  return (
    <div className="min-w-0">
      <div className="text-xs text-faint uppercase tracking-wide whitespace-nowrap">{label}</div>
      <div className="mt-1 text-lg font-num font-semibold text-ink leading-none whitespace-nowrap">
        {value}
        {unit && value !== "—" && (
          <span className="ml-1 text-sm font-normal text-muted">{unit}</span>
        )}
      </div>
      {sub && <div className="mt-1 text-xs text-muted whitespace-nowrap">{sub}</div>}
    </div>
  );
}
