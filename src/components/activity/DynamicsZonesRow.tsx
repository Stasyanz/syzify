import type { Activity, TimeInZone } from "../../lib/types";
import { CyclingDynamicsPanel, hasCyclingDynamics } from "./CyclingDynamicsPanel";
import { TimeInZonesPanel } from "./TimeInZonesPanel";
import { hasTimeInZones } from "./timeInZones";

interface Props {
  activity: Activity;
  timeInZones: TimeInZone[];
  /** Zones are stored per whole activity — a focused triathlon leg shows
   * only the dynamics half. */
  hideZones?: boolean;
}

/**
 * The two-card row under the power curve: Cycling Dynamics on the left,
 * Time in Zones on the right. Whichever is alone spans the row; with
 * neither the row is absent.
 */
export function DynamicsZonesRow({ activity, timeInZones, hideZones = false }: Props) {
  const dynamics = hasCyclingDynamics(activity);
  const zones = !hideZones && hasTimeInZones(timeInZones, activity.duration_s);
  if (!dynamics && !zones) return null;
  return (
    <div className="grid grid-cols-2 gap-4" data-testid="dynamics-zones-row">
      {dynamics && (
        <CyclingDynamicsPanel activity={activity} className={zones ? "" : "col-span-2"} />
      )}
      {zones && (
        <TimeInZonesPanel
          // A new activity opens on power again — the type pick belongs to
          // the card, not the session.
          key={activity.id}
          timeInZones={timeInZones}
          durationS={activity.duration_s}
          className={dynamics ? "" : "col-span-2"}
        />
      )}
    </div>
  );
}
