import { cadenceUnit, isCyclingSport, type Lap, type TrackPointColumns } from "../../lib/types";
import {
  formatDistance,
  formatDuration,
  formatPaceOrSpeed,
  avgPaceOrSpeedLabel,
  formatHR,
  formatElevation,
  formatPower,
  formatWhole,
} from "../../lib/format";
import { isImperial, unitLabel, useUnits, M_PER_MILE } from "../../lib/units";
import { useT } from "../../lib/i18n";

interface Props {
  laps: Lap[];
  trackpoints: TrackPointColumns;
  sport: string;
}

/** Normalized lap row consumed by the table (from real laps or auto-split). */
interface Row {
  lap: number;
  time: number | null;
  distance: number | null;
  speed: number | null;
  hr: number | null;
  maxHr: number | null;
  ascent: number | null;
  cadence: number | null;
  power: number | null;
}

function lapDistanceFor(sport: string): number {
  const imperial = isImperial();
  if (isCyclingSport(sport)) return imperial ? 5 * M_PER_MILE : 5000;
  // Pool lengths stay metric even for imperial users.
  if (sport === "swim" || sport === "open_water") return 100;
  return imperial ? M_PER_MILE : 1000;
}

/** "1 km" / "5 km" / "100 m" / "1 mi" — the auto-split length for the header. */
function lapLengthLabel(sport: string): string {
  const d = lapDistanceFor(sport);
  if (isImperial() && d >= M_PER_MILE) return `${Math.round(d / M_PER_MILE)} ${unitLabel("mi")}`;
  return d >= 1000 ? `${d / 1000} ${unitLabel("km")}` : `${d} ${unitLabel("m")}`;
}

/** Split a GPS track into fixed-distance laps when the source has no laps. */
export function autoLaps(tp: TrackPointColumns, sport: string): Row[] {
  const lapDist = lapDistanceFor(sport);
  const pts: { d: number; t: number; alt: number | null; hr: number | null }[] = [];
  for (let i = 0; i < tp.distance_m.length; i++) {
    const d = tp.distance_m[i];
    const t = tp.t[i];
    if (d == null || t == null) continue;
    pts.push({ d, t, alt: tp.altitude_m[i] ?? null, hr: tp.hr[i] ?? null });
  }
  if (pts.length < 2) return [];

  const rows: Row[] = [];
  let boundary = lapDist;
  let startT = pts[0].t;
  let ascent = 0;
  let hrSum = 0;
  let hrCount = 0;
  let prevAlt = pts[0].alt;
  let lap = 1;

  const push = (endT: number, dist: number) => {
    const time = endT - startT;
    rows.push({
      lap,
      time,
      distance: dist,
      speed: time > 0 ? dist / time : null,
      hr: hrCount ? hrSum / hrCount : null,
      maxHr: null,
      ascent,
      cadence: null,
      power: null,
    });
    lap++;
    startT = endT;
    ascent = 0;
    hrSum = 0;
    hrCount = 0;
  };

  for (let i = 1; i < pts.length; i++) {
    const s = pts[i];
    if (s.alt != null && prevAlt != null && s.alt > prevAlt) ascent += s.alt - prevAlt;
    if (s.alt != null) prevAlt = s.alt;
    if (s.hr != null) {
      hrSum += s.hr;
      hrCount++;
    }
    while (s.d >= boundary) {
      const prev = pts[i - 1];
      const span = s.d - prev.d || 1;
      const crossT = prev.t + ((boundary - prev.d) / span) * (s.t - prev.t);
      push(crossT, lapDist);
      boundary += lapDist;
    }
  }
  // Trailing partial lap, if it's a meaningful fraction.
  const last = pts[pts.length - 1];
  const remDist = last.d - (boundary - lapDist);
  if (remDist > lapDist * 0.1) push(last.t, remDist);

  return rows;
}

export function LapsTable({ laps, trackpoints, sport }: Props) {
  useUnits();
  const t = useT();

  const real = laps.length >= 2;
  const rows: Row[] = real
    ? laps.map((l) => ({
        lap: l.lap_number,
        time: l.total_timer_time_s ?? l.total_elapsed_time_s,
        distance: l.total_distance_m,
        speed: l.avg_speed_mps,
        hr: l.avg_hr,
        maxHr: l.max_hr,
        ascent: l.total_ascent_m,
        cadence: l.avg_cadence,
        power: l.avg_power_w,
      }))
    : autoLaps(trackpoints, sport);

  if (rows.length < 2) return null;

  const some = (f: (r: Row) => number | null) => rows.some((r) => f(r) != null);
  const hasDist = some((r) => r.distance);
  const hasSpeed = some((r) => r.speed);
  const hasHr = some((r) => r.hr);
  const hasMaxHr = some((r) => r.maxHr);
  const hasAsc = some((r) => r.ascent);
  const hasCad = some((r) => r.cadence);
  const hasPower = some((r) => r.power);

  const cols: { key: string; label: string; cell: (r: Row) => string; num?: boolean }[] = [
    { key: "lap", label: t("activity.laps.lap"), cell: (r) => String(r.lap) },
    { key: "time", label: t("common.time"), cell: (r) => formatDuration(r.time), num: true },
  ];
  if (hasDist)
    cols.push({ key: "dist", label: t("common.distance"), cell: (r) => formatDistance(r.distance), num: true });
  if (hasSpeed)
    cols.push({
      key: "pace",
      label: avgPaceOrSpeedLabel(sport, t),
      cell: (r) => formatPaceOrSpeed(sport, r.speed),
      num: true,
    });
  if (hasHr) cols.push({ key: "hr", label: t("common.avgHr"), cell: (r) => formatHR(r.hr), num: true });
  if (hasMaxHr) cols.push({ key: "maxhr", label: t("common.maxHr"), cell: (r) => formatHR(r.maxHr), num: true });
  if (hasAsc)
    cols.push({ key: "asc", label: t("activity.laps.ascent"), cell: (r) => formatElevation(r.ascent), num: true });
  if (hasCad)
    cols.push({
      key: "cad",
      label: t("common.cadence"),
      cell: (r) => (r.cadence != null ? formatWhole(r.cadence, cadenceUnit(sport)) : "--"),
      num: true,
    });
  if (hasPower)
    cols.push({
      key: "power",
      label: t("common.power"),
      cell: (r) => formatPower(r.power),
      num: true,
    });

  return (
    <div className="dash-card">
      <h3 className="mb-3">
        {t("activity.laps.title")}
        {!real && (
          <span className="ml-2 text-xs font-normal text-faint">
            {t("activity.laps.auto", { length: lapLengthLabel(sport) })}
          </span>
        )}
      </h3>
      <div className="overflow-x-auto">
        <table className="w-full text-sm">
          <thead>
            <tr className="text-faint text-xs uppercase tracking-wide">
              {cols.map((c) => (
                <th
                  key={c.key}
                  className={`font-semibold pb-2 ${c.num ? "text-right" : "text-left"}`}
                >
                  {c.label}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {rows.map((r) => (
              <tr
                key={r.lap}
                className="border-t border-border transition-colors hover:bg-card-2"
              >
                {cols.map((c) => (
                  <td
                    key={c.key}
                    className={`py-1.5 ${c.num ? "text-right tabular-nums text-ink" : "text-left text-muted"}`}
                  >
                    {c.cell(r)}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
