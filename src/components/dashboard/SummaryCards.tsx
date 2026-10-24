import type { DashboardData } from "../../lib/types";
import { formatDistance, formatElevation, formatHR } from "../../lib/format";
import { unitLabel, useUnits } from "../../lib/units";
import { formatNumber, useT } from "../../lib/i18n";
import { CornerRings } from "../brand/CornerRings";

interface Props {
  data: DashboardData;
}

/** Split a "51.2 km" formatted string into value + small unit suffix. */
function splitValueUnit(s: string): { value: string; unit?: string } {
  const i = s.indexOf(" ");
  return i === -1 ? { value: s } : { value: s.slice(0, i), unit: s.slice(i + 1) };
}

/** This-week summary cards — five white tiles matching the activity stats. */
export function SummaryCards({ data }: Props) {
  useUnits();
  const t = useT();
  const w = data.week;
  const hours = `${formatNumber(w.duration_s / 3600, 1)} ${unitLabel("h")}`;

  const cards = [
    { label: t("dashboard.summary.activities"), value: formatNumber(w.activities, 0) },
    { label: t("common.distance"), ...splitValueUnit(formatDistance(w.distance_m)) },
    { label: t("common.duration"), ...splitValueUnit(hours) },
    { label: t("common.elevation"), ...splitValueUnit(formatElevation(w.elev_gain_m)) },
    { label: t("dashboard.summary.avgHr"), ...splitValueUnit(formatHR(w.avg_hr)) },
  ];

  return (
    <div className="grid grid-cols-5 gap-3">
      {cards.map((c) => (
        <div
          key={c.label}
          className="relative overflow-hidden bg-card border border-border rounded-xl p-4"
        >
          <CornerRings />
          <div className="relative text-xs text-faint uppercase tracking-wide whitespace-nowrap">
            {c.label}
          </div>
          <div className="relative mt-1.5 text-lg font-num font-semibold text-ink leading-none whitespace-nowrap">
            {c.value}
            {c.unit && <span className="ml-1 text-sm font-normal text-muted">{c.unit}</span>}
          </div>
          <div className="relative mt-1.5 text-xs text-faint uppercase tracking-wide">
            {t("dashboard.summary.thisWeek")}
          </div>
        </div>
      ))}
    </div>
  );
}
