import type { ActivitySummary } from "../../lib/types";
import { SPORT_LABELS, MAX_TAGS_PER_ACTIVITY, type SportType } from "../../lib/types";
import { SportIcon } from "../brand/SportIcon";
import { MapPin } from "lucide-react";
import {
  formatDistance,
  formatDuration,
  formatElevation,
  formatPaceOrSpeed,
  paceOrSpeedLabel,
  formatDate,
} from "../../lib/format";
import { useUnits } from "../../lib/units";

interface Props {
  activity: ActivitySummary;
  onClick: () => void;
}

export function ActivityListItem({ activity, onClick }: Props) {
  useUnits();
  const label = SPORT_LABELS[activity.sport_type as SportType] ?? "Activity";

  return (
    <button
      onClick={onClick}
      className="w-full text-left px-4 py-3 hover:bg-card-2 transition-colors flex items-center gap-4 cursor-pointer"
    >
      <SportIcon sport={activity.sport_type} size={38} title={label} />
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2">
          <span className="font-medium text-ink truncate">
            {activity.title ?? label}
          </span>
          {activity.tags.slice(0, MAX_TAGS_PER_ACTIVITY).map((tag) => (
            <span
              key={tag}
              className="text-xs bg-accent-soft text-accent-2 px-1.5 py-0.5 rounded"
            >
              {tag}
            </span>
          ))}
          {activity.tags.length > MAX_TAGS_PER_ACTIVITY && (
            <span className="text-xs text-faint" title={activity.tags.slice(MAX_TAGS_PER_ACTIVITY).join(", ")}>
              +{activity.tags.length - MAX_TAGS_PER_ACTIVITY}
            </span>
          )}
        </div>
        <div className="text-sm text-muted mt-0.5 flex items-center gap-2">
          <span>{formatDate(activity.start_time)}</span>
          {activity.location_name && (
            <span className="flex items-center gap-0.5 text-faint">
              <MapPin size={12} />
              {activity.location_name}
            </span>
          )}
        </div>
      </div>
      <div className="flex items-center gap-6 text-sm text-muted shrink-0">
        <div className="text-right">
          <div className="font-num font-semibold text-ink">{formatDistance(activity.distance_m)}</div>
          <div className="text-xs text-faint">distance</div>
        </div>
        <div className="text-right">
          <div className="font-num font-semibold text-ink">{formatDuration(activity.duration_s)}</div>
          <div className="text-xs text-faint">time</div>
        </div>
        <div className="text-right">
          <div className="font-num font-semibold text-ink">
            {formatPaceOrSpeed(activity.sport_type, activity.avg_speed_mps)}
          </div>
          <div className="text-xs text-faint">
            {paceOrSpeedLabel(activity.sport_type).toLowerCase()}
          </div>
        </div>
        <div className="text-right">
          <div className="font-num font-semibold text-ink">{formatElevation(activity.elev_gain_m)}</div>
          <div className="text-xs text-faint">elev</div>
        </div>
      </div>
    </button>
  );
}
