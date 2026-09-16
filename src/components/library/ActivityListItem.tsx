import type { ActivitySummary } from "../../lib/types";
import { SPORT_LABELS, MAX_TAGS_PER_ACTIVITY, type SportType } from "../../lib/types";
import { SportIcon } from "../brand/SportIcon";
import { DeviceSilhouette } from "../brand/DeviceSilhouette";
import { describeDevice } from "../../lib/devices";
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

/** Vault setting: "0" hides the recording device in list rows; unset or
 * anything else shows it. No UI yet — it is meant for Settings (#147). */
export const LIBRARY_SHOW_DEVICE_KEY = "library_show_device";

/** Whether the stored setting value means "show the device". */
export const showDeviceFromSetting = (value: string | null | undefined): boolean => value !== "0";

interface Props {
  activity: ActivitySummary;
  onClick: () => void;
  /** Show the recording device line (default on). */
  showDevice?: boolean;
}

export function ActivityListItem({ activity, onClick, showDevice = true }: Props) {
  useUnits();
  const label = SPORT_LABELS[activity.sport_type as SportType] ?? "Activity";
  const device = showDevice ? describeDevice(activity.source_device) : null;

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
        {/* The meta line clips instead of spilling onto the numbers: the
            date and the device keep their width, a long location truncates. */}
        <div className="text-sm text-muted mt-0.5 flex items-center gap-2 min-w-0 overflow-hidden">
          <span className="shrink-0">{formatDate(activity.start_time)}</span>
          {activity.location_name && (
            <span className="flex items-center gap-0.5 text-faint min-w-0">
              <MapPin size={12} className="shrink-0" />
              <span className="truncate">{activity.location_name}</span>
            </span>
          )}
          {device && (
            <span
              className="flex items-center gap-1 text-faint whitespace-nowrap shrink-0"
              data-tip={`Recorded on ${device.full_name}`}
              aria-label={`Recorded on ${device.full_name}`}
              data-testid="row-device"
            >
              <DeviceSilhouette form={device.form} size={16} />
              {device.label}
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
