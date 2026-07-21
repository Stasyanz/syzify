import { isWaterSport, type Activity } from "../../lib/types";
import {
  formatDistance,
  formatDuration,
  formatPaceOrSpeed,
  paceOrSpeedLabel,
  formatElevation,
  formatHR,
} from "../../lib/format";
import { useUnits } from "../../lib/units";
import { CornerRings } from "../brand/CornerRings";

interface Props {
  activity: Activity;
}

/** Split a "5.0 km" formatted string into value + small unit suffix. */
function splitValueUnit(s: string): { value: string; unit?: string } {
  const i = s.indexOf(" ");
  return i === -1 ? { value: s } : { value: s.slice(0, i), unit: s.slice(i + 1) };
}

export function SummaryPanel({ activity }: Props) {
  useUnits();

  // Order + labels mirror the design's activity summary (Distance, Duration,
  // Pace/Speed, Elev Gain, Avg/Max HR, Cadence, Calories). HR / cadence /
  // calories tiles are dropped when the activity has no such data.
  const metrics: { label: string; value: string }[] = [
    { label: "Distance", value: formatDistance(activity.distance_m) },
    { label: "Duration", value: formatDuration(activity.duration_s) },
    {
      label: `Avg ${paceOrSpeedLabel(activity.sport_type)}`,
      value: formatPaceOrSpeed(activity.sport_type, activity.avg_speed_mps),
    },
  ];
  // Swim "elevation gain" is GPS/pressure noise — no tile for water sports.
  if (!isWaterSport(activity.sport_type)) {
    metrics.push({ label: "Elev Gain", value: formatElevation(activity.elev_gain_m) });
  }
  if (activity.avg_hr != null) {
    metrics.push({ label: "Avg HR", value: formatHR(activity.avg_hr) });
  }
  if (activity.calories != null) {
    metrics.push({ label: "Calories", value: `${Math.round(activity.calories)} kcal` });
  }

  return (
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
}
