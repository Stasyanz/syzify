import { useQuery } from "@tanstack/react-query";
import { Bike, Footprints, Package } from "lucide-react";
import { api } from "../../lib/tauri";
import type { GearKind } from "../../lib/types";

/** The kind's icon, shared with the Garage cards. */
export function GearKindIcon({ kind, size = 16 }: { kind: GearKind; size?: number }) {
  if (kind === "bike") return <Bike size={size} />;
  if (kind === "shoes") return <Footprints size={size} />;
  return <Package size={size} />;
}

/**
 * The gear an activity was done on, as a chip for the activity header
 * next to the device chip: the kind's icon and the item's name, brand and
 * model on hover. A click opens the edit modal, where the item is
 * changed. Nothing for an activity without gear, and nothing while the
 * registry has not loaded.
 */
export function GearChip({ gearId, onClick }: { gearId: string | null; onClick: () => void }) {
  const { data: items = [] } = useQuery({
    queryKey: ["gear"],
    queryFn: () => api.listGear(),
    enabled: gearId != null,
  });
  const item = gearId != null ? items.find((g) => g.id === gearId) : undefined;
  if (!item) return null;
  const detail = [item.brand, item.model].filter(Boolean).join(" ");
  const tip = detail ? `${item.name} (${detail})` : item.name;
  return (
    <button
      type="button"
      className="device-chip cursor-pointer"
      data-tip={tip}
      aria-label={`Gear: ${tip}`}
      data-testid="gear-chip"
      onClick={onClick}
    >
      <GearKindIcon kind={item.kind} />
      <span>{item.name}</span>
    </button>
  );
}
