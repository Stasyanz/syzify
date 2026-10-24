import { useQuery } from "@tanstack/react-query";
import { Trophy } from "lucide-react";
import { api } from "../../lib/tauri";
import type { SegmentEffortRow } from "../../lib/types";
import {
  formatDistance,
  formatDuration,
  formatGrade,
  formatPaceOrSpeed,
  formatPower,
  paceOrSpeedLabel,
} from "../../lib/format";
import { useActivityStore } from "../../stores/activityStore";
import { useT } from "../../lib/i18n";

/** Sport-aware speed/pace of one effort, from its own path and time. */
export function effortSpeedMps(e: {
  distance_m: number;
  elapsed_s: number | null;
}): number | null {
  return e.elapsed_s != null && e.elapsed_s > 0 ? e.distance_m / e.elapsed_s : null;
}

/** A best-among-several marker: rank 1 means nothing against zero rivals. */
export function isPersonalRecord(e: { rank: number | null; effort_count: number }): boolean {
  return e.rank === 1 && e.effort_count >= 2;
}

/** The activity page's "Segments" card: one row per detected pass, with the
 * standing among that segment's timed efforts. Clicking a row publishes the
 * effort's trackpoint range — the route map and the elevation chart
 * highlight it (same channel as the chart's own drag-selection). Renders
 * nothing while the activity has no efforts. */
export function SegmentEffortsPanel({
  activityId,
  sport,
}: {
  activityId: string;
  sport: string;
}) {
  const t = useT();
  const selectedRange = useActivityStore((s) => s.selectedRange);
  const setSelectedRange = useActivityStore((s) => s.setSelectedRange);
  const { data: efforts } = useQuery({
    queryKey: ["segment-efforts", activityId],
    queryFn: () => api.getActivitySegmentEfforts(activityId),
  });

  if (!efforts || efforts.length === 0) return null;

  // All-or-nothing: the column only exists when some pass has power, so
  // meterless libraries keep today's layout.
  const hasPower = efforts.some((e) => e.avg_power_w != null);

  const isActive = (e: SegmentEffortRow) =>
    selectedRange != null &&
    selectedRange[0] === e.start_idx &&
    selectedRange[1] === e.end_idx;

  return (
    <div className="dash-card">
      <h3 className="mb-3">{t("activity.segments.title")}</h3>
      <div className="overflow-x-auto">
        <table className="w-full text-sm">
          <thead>
            <tr className="text-faint text-xs uppercase tracking-wide">
              <th className="font-semibold pb-2 text-left">{t("activity.segments.segment")}</th>
              <th className="font-semibold pb-2 text-right">{t("common.time")}</th>
              {hasPower && <th className="font-semibold pb-2 text-right">{t("common.power")}</th>}
              <th className="font-semibold pb-2 text-right">{paceOrSpeedLabel(sport)}</th>
              <th className="font-semibold pb-2 text-right">{t("common.distance")}</th>
              <th className="font-semibold pb-2 text-right">{t("activity.segments.grade")}</th>
              <th className="font-semibold pb-2 text-right">{t("activity.segments.rank")}</th>
            </tr>
          </thead>
          <tbody>
            {efforts.map((e) => (
              <tr
                key={e.id}
                onClick={() => setSelectedRange(isActive(e) ? null : [e.start_idx, e.end_idx])}
                aria-selected={isActive(e)}
                className={`cursor-pointer border-t border-border transition-colors ${
                  isActive(e) ? "bg-accent-soft" : "hover:bg-card-2"
                }`}
                title={t("activity.segments.showTip")}
              >
                <td className="py-1.5 pr-2 font-medium">
                  <span className="inline-flex items-center gap-1.5">
                    {e.segment_name}
                    {isPersonalRecord(e) && (
                      <span
                        className="inline-flex items-center gap-0.5 rounded px-1 text-[10px] font-bold uppercase text-accent-2"
                        // "PR" is a badge, read out whole.
                        role="img"
                        data-tip={t("activity.segments.prTip")}
                        aria-label={t("activity.segments.prTip")}
                      >
                        <Trophy size={11} aria-hidden />
                        {t("activity.segments.pr")}
                      </span>
                    )}
                  </span>
                </td>
                <td className="py-1.5 text-right tabular-nums">
                  {formatDuration(e.elapsed_s)}
                </td>
                {hasPower && (
                  <td className="py-1.5 text-right tabular-nums">
                    {formatPower(e.avg_power_w)}
                  </td>
                )}
                <td className="py-1.5 text-right tabular-nums">
                  {formatPaceOrSpeed(sport, effortSpeedMps(e))}
                </td>
                <td className="py-1.5 text-right tabular-nums">
                  {formatDistance(e.distance_m)}
                </td>
                <td className="py-1.5 text-right tabular-nums">
                  {e.avg_grade_pct != null ? formatGrade(e.avg_grade_pct) : "--"}
                </td>
                <td className="py-1.5 text-right tabular-nums">
                  {e.rank != null ? t("activity.segments.rankOf", { rank: e.rank, total: e.effort_count }) : "--"}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
