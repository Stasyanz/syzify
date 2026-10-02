import { useEffect, useRef, useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { Bike, Check, ChevronDown, Footprints, Package } from "lucide-react";
import { api } from "../../lib/tauri";
import { gearChoicesFor, kindsForSport } from "../../lib/gear";
import type { GearItem, GearKind } from "../../lib/types";
import { useToastStore } from "../../stores/toastStore";
import { errorText } from "../../lib/errors";

/** The kind's icon, shared with the Garage cards. */
export function GearKindIcon({ kind, size = 16, className }: { kind: GearKind; size?: number; className?: string }) {
  if (kind === "bike") return <Bike size={size} className={className} />;
  if (kind === "shoes") return <Footprints size={size} className={className} />;
  return <Package size={size} className={className} />;
}

/** "Road (Canyon Ultimate)", or just the name. */
export function gearTip(item: GearItem): string {
  const detail = [item.brand, item.model].filter(Boolean).join(" ");
  return detail ? `${item.name} (${detail})` : item.name;
}

/** What an activity without gear asks for: the one kind its sport takes
 * ("Bike" for a ride, "Shoes" for a run), or plain "Gear" where the sport
 * takes anything. */
export function gearPlaceholder(sport: string): { kind: GearKind; label: string } {
  const kinds = kindsForSport(sport).filter((k) => k !== "other");
  if (kinds.length === 1) {
    const kind = kinds[0];
    return { kind, label: kind === "bike" ? "Bike" : "Shoes" };
  }
  return { kind: "other", label: "Gear" };
}

/** Whether the chip has anything to show: the item it is on, or (for an
 * activity without gear) something to pick. The header reads this too, so
 * its layout only makes room for a chip that is there. */
export function gearChipVisible(items: GearItem[], gearId: string | null, sport: string, locked: boolean): boolean {
  if (locked) return false;
  if (gearId != null && items.some((g) => g.id === gearId)) return true;
  return gearChoicesFor(items, sport, gearId).length > 0;
}

/**
 * The gear an activity was done on, as a chip for the activity header
 * next to the device chip: the kind's icon and the item's name, brand and
 * model on hover. A click opens a menu of the items in use of a kind that
 * fits the sport (and "None" to take the activity off its gear); a pick
 * is written at once. An activity without gear shows a muted "Gear" chip
 * as the entry point. Nothing for a multisport whole, which carries no
 * gear, and nothing while the registry has not loaded or has no items
 * to offer.
 */
export function GearChip({
  activityId,
  gearId,
  sport,
  locked = false,
  onChanged,
}: {
  activityId: string;
  gearId: string | null;
  sport: string;
  /** A multisport whole: no chip at all. */
  locked?: boolean;
  onChanged: () => void;
}) {
  const addToast = useToastStore((s) => s.addToast);
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const { data: items = [] } = useQuery({
    queryKey: ["gear"],
    queryFn: () => api.listGear(),
    enabled: !locked,
  });

  useEffect(() => {
    if (!open) return;
    const onDoc = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    };
    // Capture phase, and the event stops here: the activity page listens
    // on the document too, where Escape leaves for the library and the
    // arrows step to the next activity — none of that while a menu is
    // open on it.
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        setOpen(false);
      } else if (e.key === "ArrowLeft" || e.key === "ArrowRight") {
        e.stopPropagation();
      }
    };
    document.addEventListener("mousedown", onDoc);
    document.addEventListener("keydown", onKey, true);
    return () => {
      document.removeEventListener("mousedown", onDoc);
      document.removeEventListener("keydown", onKey, true);
    };
  }, [open]);

  const change = useMutation({
    mutationFn: (next: string | null) => api.setActivityGear(activityId, next),
    onSuccess: () => {
      setOpen(false);
      onChanged();
    },
    // A Tauri command's refusal arrives as a bare string, not an Error.
    onError: (e: unknown) => addToast("error", `Gear not changed: ${errorText(e)}`),
  });

  if (!gearChipVisible(items, gearId, sport, locked)) return null;
  const current = gearId != null ? items.find((g) => g.id === gearId) : undefined;
  // An id the registry no longer has (the item was deleted, the detail is
  // still cached) reads as no gear.
  const currentId = current?.id ?? null;
  const choices = gearChoicesFor(items, sport, gearId);
  const placeholder = gearPlaceholder(sport);
  const tip = current ? gearTip(current) : `No ${placeholder.label.toLowerCase()} yet — pick one`;

  return (
    <div className="relative shrink-0" ref={ref}>
      <button
        type="button"
        // Without gear the whole chip is muted; `!` because .device-chip
        // sets the ink color itself.
        className={`device-chip cursor-pointer ${current ? "" : "!text-faint"}`}
        data-tip={tip}
        aria-label={current ? `Gear: ${tip}` : `Pick ${placeholder.label.toLowerCase()}`}
        aria-haspopup="menu"
        aria-expanded={open}
        data-testid="gear-chip"
        onClick={() => setOpen((o) => !o)}
      >
        {/* The class sits on the svg itself: .device-chip svg colors it,
            and a wrapper's color would not reach it. */}
        {current ? <GearKindIcon kind={current.kind} /> : <GearKindIcon kind={placeholder.kind} className="!text-faint" />}
        <span>{current ? current.name : placeholder.label}</span>
        <ChevronDown size={13} className="!text-faint" />
      </button>
      {open && (
        <div
          role="menu"
          aria-label="Gear"
          className="absolute top-full left-0 mt-1.5 bg-card border border-border rounded-card shadow-xl py-1 z-20 w-56 max-h-80 overflow-y-auto scroll-themed"
        >
          {[null, ...choices].map((choice) => {
            const id = choice?.id ?? null;
            const isSel = id === currentId;
            return (
              <button
                key={id ?? "none"}
                type="button"
                role="menuitemradio"
                aria-checked={isSel}
                disabled={change.isPending}
                onClick={() => (isSel ? setOpen(false) : change.mutate(id))}
                className={`flex w-full items-center gap-2.5 px-3 py-1.5 text-left text-[13px] disabled:opacity-50 ${
                  isSel ? "bg-accent-soft text-accent-2 font-medium" : "text-ink hover:bg-card-2"
                }`}
              >
                <span className="grid h-6 w-6 shrink-0 place-items-center text-accent-2">
                  {choice ? <GearKindIcon kind={choice.kind} size={14} /> : null}
                </span>
                <span className="truncate">{choice ? (choice.retired_at ? `${choice.name} (retired)` : choice.name) : "None"}</span>
                {isSel && <Check size={14} className="ml-auto shrink-0" />}
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}
