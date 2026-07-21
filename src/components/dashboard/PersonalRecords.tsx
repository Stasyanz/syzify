import { useState } from "react";
import { useNavigate } from "react-router";
import { Ruler, Clock, Mountain, Timer, Dumbbell, ChevronRight, type LucideIcon } from "lucide-react";
import type { SportRecords, PersonalRecord } from "../../lib/types";
import { SPORT_LABELS, type SportType } from "../../lib/types";
import { formatDistance, formatDurationHM, formatElevation, formatPace } from "../../lib/format";
import { useUnits, toWeight, weightUnit } from "../../lib/units";

interface Props {
  recordsBySport: SportRecords[];
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
          <span> · {record.title ?? record.date}</span>
        </div>
      </div>
      <ChevronRight size={16} className="text-faint shrink-0" />
    </div>
  );
}

export function PersonalRecords({ recordsBySport }: Props) {
  useUnits();
  const navigate = useNavigate();
  const goToActivity = (id: string) => navigate(`/activity/${id}`);
  const [sel, setSel] = useState(0);

  if (recordsBySport.length === 0) {
    return (
      <div className="dash-card">
        <h3 className="mb-2">Personal records</h3>
        <div className="flex h-32 items-center justify-center text-sm text-faint">
          No records yet
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
      <h3 className="mb-2">Personal records</h3>
      <div className="rec-filter">
        {recordsBySport.map((s, i) => (
          <span
            key={s.sport_type}
            className={`chip${i === idx ? " on" : ""}`}
            onClick={() => setSel(i)}
          >
            {SPORT_LABELS[s.sport_type as SportType] ?? s.sport_type}
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
                <div className="rl">{pb.label}</div>
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
              label="Heaviest set"
              icon={Dumbbell}
              record={r.heaviest_set}
              formatter={(v) => {
                const w = toWeight(v);
                return `${Number.isInteger(w) ? w : w.toFixed(1)} ${weightUnit()}`;
              }}
              onClick={goToActivity}
            />
            <RecordRow
              label="Longest distance"
              icon={Ruler}
              record={r.longest_distance}
              formatter={(v) => formatDistance(v)}
              onClick={goToActivity}
            />
            <RecordRow
              label="Longest duration"
              icon={Clock}
              record={r.longest_duration}
              formatter={(v) => formatDurationHM(v)}
              onClick={goToActivity}
            />
            <RecordRow
              label="Highest elevation"
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
