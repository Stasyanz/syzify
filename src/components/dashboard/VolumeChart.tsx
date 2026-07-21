import { useMemo, useState } from "react";
import type { VolumeBucket } from "../../lib/types";
import { getSportColor } from "../../lib/sportColors";
import { SPORT_LABELS, type SportType } from "../../lib/types";
import { useDashboardStore } from "../../stores/dashboardStore";
import { useUnits, toDistance, distanceUnit } from "../../lib/units";

interface Props {
  /** Daily buckets for the last 7 days (only days with activity). */
  weekVolume: VolumeBucket[];
}

function ymd(d: Date): string {
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${y}-${m}-${day}`;
}

/** Format a hovered bar segment value. Time (value in minutes) shows whole
 * minutes under an hour ("25 min") and hours+minutes above ("1h 25m", "2h").
 * Distance uses whole numbers from 10 up, one decimal below ("42 km", "9.4 km"). */
export function formatVolumeValue(value: number, unit: "km" | "mi" | "min"): string {
  if (unit === "min") {
    const total = Math.round(value);
    if (total < 60) return `${total} min`;
    const h = Math.floor(total / 60);
    const m = total % 60;
    return m ? `${h}h ${m}m` : `${h}h`;
  }
  return `${value >= 10 ? Math.round(value) : value.toFixed(1)} ${unit}`;
}

/** Sports appearing anywhere in the week — by EITHER metric. The legend must
 * not depend on the Dist/Time toggle: a distance-less strength session would
 * drop out of the Dist legend, rewrapping it and jumping the card height. */
export function weekSports(weekVolume: VolumeBucket[]): string[] {
  return Array.from(
    new Set(
      weekVolume.flatMap((b) =>
        Object.entries(b.by_sport)
          .filter(([, sb]) => sb.distance_m > 0 || sb.duration_s > 0)
          .map(([sport]) => sport),
      ),
    ),
  ).sort();
}

/** Dashboard volume: 7 daily stacked bars (last 7 days), Distance or Time. */
export function VolumeChart({ weekVolume }: Props) {
  const volumeMetric = useDashboardStore((s) => s.volumeMetric);
  const setVolumeMetric = useDashboardStore((s) => s.setVolumeMetric);
  const units = useUnits();
  // Segment under the cursor: { day index, segment index }.
  const [hover, setHover] = useState<{ d: number; s: number } | null>(null);

  const present = useMemo(() => weekSports(weekVolume), [weekVolume]);
  const { days, max } = useMemo(() => {
    const byDate = new Map(weekVolume.map((b) => [b.start_date, b]));
    const metric = (sb: { distance_m: number; duration_s: number }) =>
      volumeMetric === "distance" ? toDistance(sb.distance_m) : sb.duration_s / 60;

    // Build the 7-day grid: 6 days ago → today.
    const days = Array.from({ length: 7 }, (_, i) => {
      const d = new Date();
      d.setDate(d.getDate() - (6 - i));
      const bucket = byDate.get(ymd(d));
      const segs = bucket
        ? Object.entries(bucket.by_sport)
            .map(([sport, sb]) => ({ sport, value: metric(sb) }))
            .filter((s) => s.value > 0)
        : [];
      const total = segs.reduce((sum, s) => sum + s.value, 0);
      return {
        label: d.toLocaleDateString(undefined, { weekday: "short" }),
        total,
        segs,
      };
    });

    const max = Math.max(...days.map((d) => d.total), 0.0001);
    return { days, max };
    // `units` feeds toDistance() inside metric().
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [weekVolume, volumeMetric, units]);

  const unit = volumeMetric === "distance" ? distanceUnit() : "min";
  const fmt = (v: number) => formatVolumeValue(v, unit);

  return (
    <div className="dash-card">
      <div className="flex items-center justify-between gap-2 flex-wrap">
        <div>
          <h3>Volume</h3>
          <div className="text-[11px] font-medium text-faint mt-0.5">
            Last 7 days · {unit}
          </div>
        </div>
        <div className="seg">
          <button
            onClick={() => setVolumeMetric("distance")}
            className={volumeMetric === "distance" ? "on" : ""}
          >
            Dist
          </button>
          <button
            onClick={() => setVolumeMetric("duration")}
            className={volumeMetric === "duration" ? "on" : ""}
          >
            Time
          </button>
        </div>
      </div>

      <div className="bars" onMouseLeave={() => setHover(null)}>
        {days.map((d, i) => {
          const hovered = hover?.d === i ? d.segs[hover.s] : undefined;
          return (
            <div className={`col${d.total <= 0 ? " dim" : ""}`} key={i}>
              {/* Always rendered (transparent placeholder when not hovered) so
                  showing the value never shifts the bar's size or position. */}
              <b
                className="vval"
                style={{ color: hovered ? getSportColor(hovered.sport) : "transparent" }}
              >
                {hovered ? fmt(hovered.value) : " "}
              </b>
              {d.total <= 0 ? (
                <i style={{ height: "3px" }} />
              ) : (
                <div className="vbar" style={{ height: `${(d.total / max) * 100}%` }}>
                  {d.segs.map((s, j) => (
                    <span
                      key={j}
                      style={{ flexGrow: s.value, background: getSportColor(s.sport) }}
                      onMouseEnter={() => setHover({ d: i, s: j })}
                    />
                  ))}
                </div>
              )}
              <span>{d.label}</span>
            </div>
          );
        })}
      </div>

      {present.length > 0 && (
        <div className="vol-legend">
          {present.map((sp) => (
            <span key={sp}>
              <i style={{ background: getSportColor(sp) }} />
              {SPORT_LABELS[sp as SportType] ?? sp}
            </span>
          ))}
        </div>
      )}
    </div>
  );
}
