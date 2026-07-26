import type { Activity } from "../../lib/types";
import { formatDuration } from "../../lib/format";

interface Props {
  activity: Activity;
}

/** Whether the activity carries anything this panel can show. */
export function hasCyclingDynamics(a: Activity): boolean {
  return [
    a.avg_left_right_balance,
    a.avg_left_pco_mm,
    a.avg_right_pco_mm,
    a.avg_left_power_phase_start_deg,
    a.avg_right_power_phase_start_deg,
    a.avg_power_seated_w,
    a.time_standing_s,
  ].some((v) => v != null);
}

function polar(cx: number, cy: number, r: number, deg: number): [number, number] {
  const a = ((deg - 90) * Math.PI) / 180; // 0° = top dead center
  return [cx + r * Math.cos(a), cy + r * Math.sin(a)];
}

/** Clockwise SVG arc from startDeg to endDeg on the pedal-stroke clock. */
function arcPath(cx: number, cy: number, r: number, startDeg: number, endDeg: number): string {
  const span = (((endDeg - startDeg) % 360) + 360) % 360;
  const [x1, y1] = polar(cx, cy, r, startDeg);
  const [x2, y2] = polar(cx, cy, r, endDeg);
  return `M ${x1.toFixed(2)} ${y1.toFixed(2)} A ${r} ${r} 0 ${span > 180 ? 1 : 0} 1 ${x2.toFixed(2)} ${y2.toFixed(2)}`;
}

/** Pedal-stroke clock: faint full ring, the power phase arc, and a thicker
 * peak-phase arc — the Garmin Connect gauge, reduced to house colors. */
function PhaseGauge({
  side,
  start,
  end,
  peakStart,
  peakEnd,
}: {
  side: "L" | "R";
  start: number | null;
  end: number | null;
  peakStart: number | null;
  peakEnd: number | null;
}) {
  const size = 84;
  const c = size / 2;
  const r = 34;
  return (
    <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`} aria-hidden="true">
      <circle cx={c} cy={c} r={r} fill="none" stroke="var(--border-2)" strokeWidth={3} />
      {start != null && end != null && (
        <path
          d={arcPath(c, c, r, start, end)}
          fill="none"
          stroke="var(--accent)"
          strokeWidth={4}
          strokeLinecap="round"
        />
      )}
      {peakStart != null && peakEnd != null && (
        <path
          d={arcPath(c, c, r, peakStart, peakEnd)}
          fill="none"
          stroke="var(--accent-2)"
          strokeWidth={7}
          strokeLinecap="round"
        />
      )}
      <text
        x={c}
        y={c + 4}
        textAnchor="middle"
        fill="var(--muted)"
        fontSize={13}
        fontWeight={600}
      >
        {side}
      </text>
    </svg>
  );
}

const fmtDeg = (v: number | null) => (v == null ? "--" : `${Math.round(v)}°`);
const fmtMm = (v: number | null) =>
  v == null ? "--" : `${v > 0 ? "+" : ""}${Math.round(v)} mm`;
const fmtAvgMax = (avg: number | null, max: number | null, unit: string) =>
  avg == null && max == null
    ? "--"
    : `${avg != null ? Math.round(avg) : "--"} / ${max != null ? Math.round(max) : "--"} ${unit}`;

/**
 * Cycling Dynamics card (dual-sided power meter data): L/R balance bar,
 * per-pedal platform center offset + power phase gauges, seated vs standing
 * split. Renders nothing when the activity has no such data (non-rides,
 * single-sided meters, GPX imports).
 */
export function CyclingDynamicsPanel({ activity: a }: Props) {
  if (!hasCyclingDynamics(a)) return null;

  const rightPct = a.avg_left_right_balance;
  const leftPct = rightPct != null ? 100 - rightPct : null;

  const hasPhase =
    a.avg_left_power_phase_start_deg != null || a.avg_right_power_phase_start_deg != null;
  const hasPco = a.avg_left_pco_mm != null || a.avg_right_pco_mm != null;

  // Seated time is derived: the timer total minus the recorded standing time.
  const seatedS =
    a.duration_s != null && a.time_standing_s != null && a.duration_s > a.time_standing_s
      ? a.duration_s - a.time_standing_s
      : null;
  const hasPosition =
    a.time_standing_s != null || a.avg_power_seated_w != null || a.avg_cadence_seated != null;

  return (
    <div className="dash-card" style={{ padding: "12px 14px" }}>
      <h3 className="!m-0 mb-3">Cycling Dynamics</h3>

      {leftPct != null && rightPct != null && (
        <div className="mb-4">
          <div className="flex items-center justify-between text-sm mb-1">
            <span className="text-muted">
              L <span className="font-num font-semibold text-ink">{leftPct.toFixed(1)}%</span>
            </span>
            <span className="text-xs text-faint uppercase tracking-wide">Balance</span>
            <span className="text-muted">
              <span className="font-num font-semibold text-ink">{rightPct.toFixed(1)}%</span> R
            </span>
          </div>
          <div
            className="relative h-2 rounded-full overflow-hidden flex"
            role="img"
            aria-label={`Balance: left ${leftPct.toFixed(1)}%, right ${rightPct.toFixed(1)}%`}
          >
            {/* Same hue at two weights — accent-soft vanishes on the dark
                card; opacity keeps both halves visible in either theme. */}
            <div className="bg-accent" style={{ width: `${leftPct}%` }} />
            <div className="bg-accent opacity-40" style={{ width: `${rightPct}%` }} />
            {/* 50% tick: the eye reads the offset from perfect balance. */}
            <div className="absolute left-1/2 top-0 bottom-0 w-px bg-surface" />
          </div>
        </div>
      )}

      {(hasPhase || hasPco) && (
        <div className="flex items-center justify-center gap-6 mb-4">
          {hasPhase && (
            <PhaseGauge
              side="L"
              start={a.avg_left_power_phase_start_deg}
              end={a.avg_left_power_phase_end_deg}
              peakStart={a.avg_left_power_phase_peak_start_deg}
              peakEnd={a.avg_left_power_phase_peak_end_deg}
            />
          )}
          <div className="grid grid-cols-[auto_1fr_1fr] gap-x-4 gap-y-1 text-sm">
            <span />
            <span className="text-xs text-faint uppercase tracking-wide text-right">Left</span>
            <span className="text-xs text-faint uppercase tracking-wide text-right">Right</span>
            {hasPco && (
              <>
                <span className="text-muted" data-tip="Platform center offset">PCO</span>
                <span className="font-num text-ink text-right">{fmtMm(a.avg_left_pco_mm)}</span>
                <span className="font-num text-ink text-right">{fmtMm(a.avg_right_pco_mm)}</span>
              </>
            )}
            {hasPhase && (
              <>
                <span className="text-muted">Power phase</span>
                <span className="font-num text-ink text-right">
                  {fmtDeg(a.avg_left_power_phase_start_deg)} → {fmtDeg(a.avg_left_power_phase_end_deg)}
                </span>
                <span className="font-num text-ink text-right">
                  {fmtDeg(a.avg_right_power_phase_start_deg)} → {fmtDeg(a.avg_right_power_phase_end_deg)}
                </span>
                <span className="text-muted">Peak phase</span>
                <span className="font-num text-ink text-right">
                  {fmtDeg(a.avg_left_power_phase_peak_start_deg)} – {fmtDeg(a.avg_left_power_phase_peak_end_deg)}
                </span>
                <span className="font-num text-ink text-right">
                  {fmtDeg(a.avg_right_power_phase_peak_start_deg)} – {fmtDeg(a.avg_right_power_phase_peak_end_deg)}
                </span>
              </>
            )}
          </div>
          {hasPhase && (
            <PhaseGauge
              side="R"
              start={a.avg_right_power_phase_start_deg}
              end={a.avg_right_power_phase_end_deg}
              peakStart={a.avg_right_power_phase_peak_start_deg}
              peakEnd={a.avg_right_power_phase_peak_end_deg}
            />
          )}
        </div>
      )}

      {hasPosition && (
        <div className="grid grid-cols-[auto_1fr_1fr_1fr] gap-x-4 gap-y-1 text-sm">
          <span />
          <span className="text-xs text-faint uppercase tracking-wide text-right">Time</span>
          <span className="text-xs text-faint uppercase tracking-wide text-right">
            Power <span className="normal-case">(avg/max)</span>
          </span>
          <span className="text-xs text-faint uppercase tracking-wide text-right">
            Cadence <span className="normal-case">(avg/max)</span>
          </span>
          <span className="text-muted">Seated</span>
          <span className="font-num text-ink text-right">{formatDuration(seatedS)}</span>
          <span className="font-num text-ink text-right">
            {fmtAvgMax(a.avg_power_seated_w, a.max_power_seated_w, "W")}
          </span>
          <span className="font-num text-ink text-right">
            {fmtAvgMax(a.avg_cadence_seated, a.max_cadence_seated, "rpm")}
          </span>
          <span className="text-muted">
            Standing{a.stand_count != null && (
              <span className="text-faint"> ×{a.stand_count}</span>
            )}
          </span>
          <span className="font-num text-ink text-right">{formatDuration(a.time_standing_s)}</span>
          <span className="font-num text-ink text-right">
            {fmtAvgMax(a.avg_power_standing_w, a.max_power_standing_w, "W")}
          </span>
          <span className="font-num text-ink text-right">
            {fmtAvgMax(a.avg_cadence_standing, a.max_cadence_standing, "rpm")}
          </span>
        </div>
      )}
    </div>
  );
}