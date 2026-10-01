import { useQuery } from "@tanstack/react-query";
import { useNavigate } from "react-router";
import { ChevronRight } from "lucide-react";
import { api } from "../../lib/tauri";
import { settingsPath } from "../../lib/settingsTabs";
import { wearState, wearTip, wornItems, WEAR_COLORS } from "../../lib/gear";
import { useUnits } from "../../lib/units";
import { GearKindIcon } from "../activity/GearChip";

/**
 * The dashboard's gear reminder (ADR 0003, #168): the items in use that
 * are at or past WEAR_WARN of their "replace at" distance, most worn
 * first, each with its numbers and a bar; a row opens the Garage. Nothing
 * at all while no item is wearing out — a warning, not a service log, so
 * there is no dismiss: retire the item or raise its limit in the Garage.
 * A registry that cannot be read shows nothing either: the dashboard is
 * no place for a side widget's error, the Garage says it.
 */
export function GearWear() {
  useUnits();
  const navigate = useNavigate();
  const { data: items = [] } = useQuery({ queryKey: ["gear"], queryFn: () => api.listGear() });
  const worn = wornItems(items);
  if (worn.length === 0) return null;

  return (
    <div className="dash-card" data-testid="gear-wear">
      <h3 className="mb-2">Gear wear</h3>
      {worn.map(({ item, wear }) => {
        const over = wearState(wear) === "over";
        return (
          <div
            key={item.id}
            className="rec link"
            role="button"
            tabIndex={0}
            onClick={() => navigate(settingsPath("garage"))}
            onKeyDown={(e) => {
              if (e.key === "Enter" || e.key === " ") {
                e.preventDefault();
                navigate(settingsPath("garage"));
              }
            }}
            aria-label={`${item.name}: ${wearTip(item, wear)}. Open the Garage`}
          >
            <span className="ic" aria-hidden="true">
              <GearKindIcon kind={item.kind} size={18} />
            </span>
            <div className="min-w-0 flex-1">
              <div className="rl">{over ? "Worn out" : "Wearing out"}</div>
              <div className="flex items-baseline gap-2">
                <span className="rv truncate">{item.name}</span>
                <span className="text-xs text-muted font-num truncate">{wearTip(item, wear)}</span>
              </div>
              <div className="mt-1.5 h-1.5 w-full max-w-xs overflow-hidden rounded-full bg-border">
                <div
                  className="h-full rounded-full"
                  data-testid="wear-fill"
                  style={{ width: `${wear * 100}%`, background: WEAR_COLORS[wearState(wear)] }}
                />
              </div>
            </div>
            <ChevronRight size={16} className="text-faint shrink-0" aria-hidden="true" />
          </div>
        );
      })}
    </div>
  );
}
