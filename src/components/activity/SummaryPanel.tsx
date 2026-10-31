import { elevationIsNoise, type Activity } from "../../lib/types";
import {
  formatDistance,
  formatDuration,
  formatPaceOrSpeed,
  avgPaceOrSpeedLabel,
  formatElevation,
  formatHR,
  formatWhole,
} from "../../lib/format";
import { useUnits } from "../../lib/units";
import { useT } from "../../lib/i18n";
import { SatelliteDish } from "lucide-react";
import { CornerRings } from "../brand/CornerRings";

interface Props {
  activity: Activity;
  /** Time the track spent without a GPS fix (`secondsWithoutGps`). */
  noGpsSeconds?: number;
}

/** Below a minute without GPS the hole in the map is too short to mention. */
const NO_GPS_NOTE_MIN_S = 60;

/** Split a "5.0 km" formatted string into value + small unit suffix. */
function splitValueUnit(s: string): { value: string; unit?: string } {
  const i = s.indexOf(" ");
  return i === -1 ? { value: s } : { value: s.slice(0, i), unit: s.slice(i + 1) };
}

export function SummaryPanel({ activity, noGpsSeconds = 0 }: Props) {
  useUnits();
  const t = useT();

  // Order + labels mirror the design's activity summary (Distance, Duration,
  // Pace/Speed, Elev Gain, Avg/Max HR, Cadence, Calories). HR / cadence /
  // calories tiles are dropped when the activity has no such data.
  const metrics: { label: string; value: string }[] = [
    { label: t("common.distance"), value: formatDistance(activity.distance_m) },
    { label: t("common.duration"), value: formatDuration(activity.duration_s) },
    {
      label: avgPaceOrSpeedLabel(activity.sport_type, t),
      value: formatPaceOrSpeed(activity.sport_type, activity.avg_speed_mps),
    },
  ];
  // Swim "elevation gain" is GPS/pressure noise, a trainer's is barometer
  // drift — no tile for either.
  if (!elevationIsNoise(activity.sport_type)) {
    metrics.push({ label: t("common.elevGain"), value: formatElevation(activity.elev_gain_m) });
  }
  if (activity.avg_hr != null) {
    metrics.push({ label: t("common.avgHr"), value: formatHR(activity.avg_hr) });
  }
  if (activity.calories != null) {
    metrics.push({ label: t("common.calories"), value: formatWhole(activity.calories, "kcal") });
  }

  const tiles = (
    <div className="grid grid-flow-col auto-cols-fr gap-3">
      {metrics.map((m) => {
        const { value, unit } = splitValueUnit(m.value);
        return (
          <div
            key={m.label}
            className="relative overflow-hidden bg-card border border-border rounded-xl p-4"
          >
            <CornerRings />
            <div className="relative text-xs text-faint uppercase tracking-wide whitespace-nowrap">{m.label}</div>
            <div className="relative mt-1.5 text-lg font-num font-semibold text-ink leading-none whitespace-nowrap">
              {value}
              {unit && <span className="ml-1 text-sm font-normal text-muted">{unit}</span>}
            </div>
          </div>
        );
      })}
    </div>
  );
  if (noGpsSeconds < NO_GPS_NOTE_MIN_S) return tiles;
  return (
    <div className="space-y-2">
      {tiles}
      <p className="flex items-center gap-1.5 text-xs text-muted">
        <SatelliteDish size={14} aria-hidden className="shrink-0" />
        {t("activity.summary.noGps", { count: Math.round(noGpsSeconds / 60) })}
      </p>
    </div>
  );
}
