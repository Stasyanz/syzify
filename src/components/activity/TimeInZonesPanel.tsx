import { useState } from "react";
import type { TimeInZone } from "../../lib/types";
import { formatDuration } from "../../lib/format";
import { timeInZoneRows, type ZoneRow, type ZoneType } from "./timeInZones";

interface Props {
  timeInZones: TimeInZone[];
  durationS: number | null;
  className?: string;
}

const TAB_LABEL: Record<ZoneType, string> = { hr: "Heart rate", power: "Power" };

/** "53%", and "<1%" for a sliver that would otherwise read as nothing. */
function pctLabel(pct: number): string {
  if (pct > 0 && pct < 0.5) return "<1%";
  return `${Math.round(pct)}%`;
}

/**
 * Time in Zones card — the Garmin Connect zones panel: one bar per zone,
 * top zone first, with the zone's range, time and share of the timer.
 * Heart rate and power as a switch when the activity carries both (power
 * first — it is the rarer, richer signal); no switch with one type. The
 * pick lives with the mounted card: callers key it by activity so a new
 * activity opens on power again. Renders nothing without device zone data.
 */
export function TimeInZonesPanel({ timeInZones, durationS, className = "" }: Props) {
  const [picked, setPicked] = useState<ZoneType | null>(null);

  const byType: Partial<Record<ZoneType, ZoneRow[]>> = {};
  for (const type of ["power", "hr"] as const) {
    const rows = timeInZoneRows(timeInZones, type, durationS);
    if (rows) byType[type] = rows;
  }
  const types = Object.keys(byType) as ZoneType[];
  if (types.length === 0) return null;

  // A pick survives a data refresh only while that type exists here.
  const active = picked && byType[picked] ? picked : types[0];

  return (
    <div className={`dash-card ${className}`} style={{ padding: "12px 14px" }}>
      <div className="mb-3 flex items-center justify-between gap-3">
        <h3 className="!m-0">Time in Zones</h3>
        {types.length > 1 && (
          <div
            role="group"
            aria-label="Zone type"
            className="inline-flex rounded-lg border border-border-2 bg-card-2 p-0.5"
          >
            {types.map((type) => (
              <button
                key={type}
                type="button"
                aria-pressed={type === active}
                onClick={() => setPicked(type)}
                className={`rounded-md px-2.5 py-0.5 text-xs font-semibold transition-colors ${
                  type === active
                    ? "bg-card text-ink shadow-sm"
                    : "text-muted hover:text-ink"
                }`}
              >
                {TAB_LABEL[type]}
              </button>
            ))}
          </div>
        )}
      </div>

      {/* Every list is rendered into the same grid cell: the hidden ones
          keep their height, so the card does not jump between 5 HR rows and
          7 power rows, and the shorter list spreads over the same height. */}
      <div className="grid">
        {types.map((type) => {
          const shown = type === active;
          return (
            <ul
              key={type}
              aria-label={`Time in ${TAB_LABEL[type].toLowerCase()} zones`}
              aria-hidden={!shown}
              className={`col-start-1 row-start-1 m-0 list-none p-0 grid grid-cols-[auto_1fr_auto_auto] content-between items-center gap-x-3 gap-y-2 text-sm ${
                shown ? "" : "invisible"
              }`}
            >
              {[...byType[type]!].reverse().map((r) => (
                <li
                  key={r.zone}
                  className="contents"
                  data-zone={r.zone}
                  aria-label={`${r.label}${r.range ? `, ${r.range}` : ""}, ${formatDuration(
                    r.timeS,
                  )}, ${pctLabel(r.pct)}`}
                >
                  <div className="leading-tight" aria-hidden="true">
                    <div className="text-ink">{r.label}</div>
                    {r.range && <div className="text-xs text-faint font-num">{r.range}</div>}
                  </div>
                  <div className="h-2.5 overflow-hidden rounded-full bg-card-2" aria-hidden="true">
                    <div
                      className="h-full rounded-full"
                      data-testid="zone-bar"
                      style={{
                        width: `${Math.min(100, Math.max(0, r.pct))}%`,
                        background: r.color,
                      }}
                    />
                  </div>
                  <div className="font-num text-ink text-right" aria-hidden="true">
                    {formatDuration(r.timeS)}
                  </div>
                  <div className="font-num text-muted text-right w-10" aria-hidden="true">
                    {pctLabel(r.pct)}
                  </div>
                </li>
              ))}
            </ul>
          );
        })}
      </div>
    </div>
  );
}
