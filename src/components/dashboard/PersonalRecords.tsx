import { useState } from "react";
import { useNavigate } from "react-router";
import { Ruler, Clock, Mountain, Timer, Dumbbell, ChevronRight, type LucideIcon } from "lucide-react";
import type { SportRecords, PersonalRecord } from "../../lib/types";
import { sportLabel } from "../../lib/types";
import { formatDistance, formatDurationHM, formatElevation, formatPace, formatWeight } from "../../lib/format";
import { unitLabel, useUnits } from "../../lib/units";
import { formatNumber, useT } from "../../lib/i18n";
import { displayTitle } from "../../lib/activityTitle";
import type { MessageKey } from "../../locales/types";

interface Props {
  recordsBySport: SportRecords[];
}

/** The distance classes the backend names a best by (db/dashboard.rs:
 * running distances, triathlon classes), each by its message. */
const PB_CLASSES = new Map<string, Extract<MessageKey, `dashboard.records.${string}`>>([
  ["Ultra", "dashboard.records.ultra"],
  ["Marathon", "dashboard.records.marathon"],
  ["Half marathon", "dashboard.records.halfMarathon"],
  ["Full distance", "dashboard.records.fullDistance"],
  ["Half distance", "dashboard.records.halfDistance"],
  ["Olympic", "dashboard.records.olympic"],
  ["Sprint", "dashboard.records.sprint"],
]);

/** A distance best's label in the interface language: a class by its
 * message, a race distance ("10 km" — kilometres whatever the units) with
 * the language's symbol, anything else as the backend sent it. */
function pbLabel(label: string, t: ReturnType<typeof useT>): string {
  const key = PB_CLASSES.get(label);
  if (key) return t(key);
  const km = /^(\d+) km$/.exec(label);
  return km ? `${formatNumber(Number(km[1]), 0)} ${unitLabel("km")}` : label;
}

function RecordRow({
  label,
  icon: Icon,
  record,
  formatter,
  onClick,
}: {
  label: string;
  icon: LucideIcon;
  record: PersonalRecord | null;
  formatter: (v: number) => string;
  onClick: (id: string) => void;
}) {
  const t = useT();
  if (!record) return null;
  return (
    <div className="rec link" onClick={() => onClick(record.activity_id)}>
      <span className="ic">
        <Icon size={18} />
      </span>
      <div className="min-w-0 flex-1">
        <div className="rl">{label}</div>
        <div className="rv truncate">
          {formatter(record.value)}
          <span> · {displayTitle(record.title, t) ?? record.date}</span>
        </div>
      </div>
      <ChevronRight size={16} className="text-faint shrink-0" />
    </div>
  );
}

export function PersonalRecords({ recordsBySport }: Props) {
  useUnits();
  const t = useT();
  const navigate = useNavigate();
  const goToActivity = (id: string) => navigate(`/activity/${id}`);
  const [sel, setSel] = useState(0);

  if (recordsBySport.length === 0) {
    return (
      <div className="dash-card">
        <h3 className="mb-2">{t("dashboard.records.title")}</h3>
        <div className="flex h-32 items-center justify-center text-sm text-faint">
          {t("dashboard.records.empty")}
        </div>
      </div>
    );
  }

  // Selection may go stale if the data shrinks; clamp to a valid index.
  const idx = Math.min(sel, recordsBySport.length - 1);
  const sr = recordsBySport[idx];
  const r = sr.records;

  return (
    <div className="dash-card">
      <h3 className="mb-2">{t("dashboard.records.title")}</h3>
      <div className="rec-filter">
        {recordsBySport.map((s, i) => (
          <span
            key={s.sport_type}
            className={`chip${i === idx ? " on" : ""}`}
            onClick={() => setSel(i)}
          >
            {sportLabel(s.sport_type)}
          </span>
        ))}
      </div>
      {/* Fixed height of ~3 rows (scrolls past that) so switching sport
          doesn't resize the dashboard row. */}
      <div className="flex flex-col h-[162px] overflow-y-auto scroll-themed">
        {sr.distance_pbs.length > 0 ? (
          // Running: best time on standard distances (longest first).
          sr.distance_pbs.map((pb) => (
            <div
              key={pb.label}
              className="rec link"
              onClick={() => goToActivity(pb.activity_id)}
            >
              <span className="ic">
                <Timer size={18} />
              </span>
              <div className="min-w-0 flex-1">
                <div className="rl">{pbLabel(pb.label, t)}</div>
                <div className="rv truncate">
                  {formatDurationHM(pb.duration_s)}
                  <span> · {formatPace(pb.distance_m / pb.duration_s)}</span>
                </div>
              </div>
              <ChevronRight size={16} className="text-faint shrink-0" />
            </div>
          ))
        ) : (
          <>
            <RecordRow
              label={t("dashboard.records.heaviestSet")}
              icon={Dumbbell}
              record={r.heaviest_set}
              formatter={formatWeight}
              onClick={goToActivity}
            />
            <RecordRow
              label={t("dashboard.records.longestDistance")}
              icon={Ruler}
              record={r.longest_distance}
              formatter={(v) => formatDistance(v)}
              onClick={goToActivity}
            />
            <RecordRow
              label={t("dashboard.records.longestDuration")}
              icon={Clock}
              record={r.longest_duration}
              formatter={(v) => formatDurationHM(v)}
              onClick={goToActivity}
            />
            <RecordRow
              label={t("dashboard.records.highestElevation")}
              icon={Mountain}
              record={r.highest_elevation}
              formatter={(v) => formatElevation(v)}
              onClick={goToActivity}
            />
          </>
        )}
      </div>
    </div>
  );
}
