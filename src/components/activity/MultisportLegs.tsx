import { useNavigate } from "react-router";
import { ChevronRight } from "lucide-react";
import { isFocusableLeg } from "./legFocus";
import type { MultisportLeg } from "../../lib/types";
import { SPORT_LABELS, type SportType } from "../../lib/types";
import { SportGlyph } from "../brand/SportIcon";
import { getSportColor } from "../../lib/sportColors";
import {
  formatDistance,
  formatDuration,
  formatPaceOrSpeed,
  formatElevation,
  formatHR,
} from "../../lib/format";
import { useUnits } from "../../lib/units";

/** One display row of the legs table — extracted pure for tests. */
export interface LegRow {
  key: number;
  /** "Swim" / "Ride" / "Run" — or "T1", "T2" for transitions. */
  label: string;
  sport: string;
  isTransition: boolean;
  /** Standalone activity to open on click (merged legs); null otherwise. */
  link: string | null;
  /** FIT-native sport leg that can be focused in place (map/charts window). */
  focusable: boolean;
  time: string;
  distance: string;
  /** Pace for pace sports, speed otherwise; empty for transitions. */
  effort: string;
  hr: string;
  ascent: string;
}

/** Transitions are labeled T1, T2… by their order among transitions; sport
 * legs get their sport label. Times prefer the leg's timer time (moving)
 * with elapsed as the fallback. */
export function legRows(legs: MultisportLeg[]): LegRow[] {
  let transitions = 0;
  return legs.map((leg) => {
    const time = leg.total_timer_time_s ?? leg.total_elapsed_time_s;
    if (leg.is_transition) {
      transitions += 1;
      return {
        key: leg.leg_number,
        label: `T${transitions}`,
        sport: leg.sport_type,
        isTransition: true,
        link: null,
        focusable: false,
        time: time != null ? formatDuration(time) : "—",
        distance: "",
        effort: "",
        hr: "",
        ascent: "",
      };
    }
    return {
      key: leg.leg_number,
      label: SPORT_LABELS[leg.sport_type as SportType] ?? leg.sport_type,
      sport: leg.sport_type,
      isTransition: false,
      link: leg.source_activity_id,
      focusable: isFocusableLeg(leg),
      time: time != null ? formatDuration(time) : "—",
      distance: leg.total_distance_m != null ? formatDistance(leg.total_distance_m) : "—",
      effort:
        leg.avg_speed_mps != null
          ? formatPaceOrSpeed(leg.sport_type, leg.avg_speed_mps)
          : "—",
      hr: leg.avg_hr != null ? formatHR(leg.avg_hr) : "—",
      ascent: leg.total_ascent_m != null ? formatElevation(leg.total_ascent_m) : "—",
    };
  });
}

/** The multisport (triathlon) per-leg breakdown card. Renders nothing for
 * single-sport activities — they have no legs stored.
 *
 * Two kinds of clickable rows: merged legs NAVIGATE to their standalone
 * activity; FIT-native legs FOCUS in place (`onFocusLeg`) — there is no
 * standalone page to go to, the parent windows its map/charts instead. */
export function MultisportLegs({
  legs,
  focusedLeg,
  onFocusLeg,
}: {
  legs: MultisportLeg[];
  /** leg_number currently focused by the parent, if any. */
  focusedLeg?: number | null;
  /** Called with the leg_number of a clicked FIT-native sport leg. */
  onFocusLeg?: (legNumber: number) => void;
}) {
  useUnits();
  const navigate = useNavigate();
  if (legs.length === 0) return null;
  const rows = legRows(legs);
  const anyAction = rows.some((r) => r.link || (r.focusable && onFocusLeg));

  return (
    <div className="dash-card">
      <h3 className="mb-3">Legs</h3>
      <div className="overflow-x-auto">
        <table className="w-full text-sm">
          <thead>
            <tr className="text-faint text-xs uppercase tracking-wide">
              <th className="font-semibold pb-2 text-left">Leg</th>
              <th className="font-semibold pb-2 text-right">Time</th>
              <th className="font-semibold pb-2 text-right">Distance</th>
              <th className="font-semibold pb-2 text-right">Pace / Speed</th>
              <th className="font-semibold pb-2 text-right">Avg HR</th>
              <th className="font-semibold pb-2 text-right">Elev</th>
              {anyAction && <th className="pb-2 w-4" />}
            </tr>
          </thead>
          <tbody>
            {rows.map((r) => {
              const focus = r.focusable && onFocusLeg ? () => onFocusLeg(r.key) : undefined;
              const onClick = r.link ? () => navigate(`/activity/${r.link}`) : focus;
              return (
              <tr
                key={r.key}
                onClick={onClick}
                data-tip={focus ? "Focus this leg's map & charts" : undefined}
                className={`border-t border-border ${r.isTransition ? "text-faint" : ""} ${
                  onClick ? "cursor-pointer hover:bg-card-2" : ""
                } ${r.key === focusedLeg ? "bg-accent-soft" : ""}`}
              >
                <td className="py-2 pr-3">
                  <span className="flex items-center gap-2 font-medium">
                    {!r.isTransition && (
                      <span
                        className="inline-grid place-items-center w-6 h-6 rounded-md text-white"
                        style={{ background: getSportColor(r.sport) }}
                      >
                        <SportGlyph sport={r.sport} size={13} />
                      </span>
                    )}
                    {r.label}
                  </span>
                </td>
                <td className="py-2 text-right font-num tabular-nums">{r.time}</td>
                <td className="py-2 text-right font-num tabular-nums">{r.distance}</td>
                <td className="py-2 text-right font-num tabular-nums">{r.effort}</td>
                <td className="py-2 text-right font-num tabular-nums">{r.hr}</td>
                <td className="py-2 text-right font-num tabular-nums">{r.ascent}</td>
                {anyAction && (
                  <td className="py-2 pl-2 text-faint">
                    {onClick && <ChevronRight size={14} />}
                  </td>
                )}
              </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </div>
  );
}
