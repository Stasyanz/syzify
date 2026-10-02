import { useState, useRef, useEffect } from "react";
import { useMutation } from "@tanstack/react-query";
import { Check } from "lucide-react";
import { api } from "../../lib/tauri";
import { errorText } from "../../lib/errors";
import { useToastStore } from "../../stores/toastStore";
import { SportIcon, SportGlyph } from "../brand/SportIcon";
import { getSportColor } from "../../lib/sportColors";
import { SPORT_LABELS, SPORT_TYPES, type SportType } from "../../lib/types";

/** The sport icon in the activity header, and the menu to change the
 * sport behind it. Its own file so the refusal path has a test (#174). */
export function SportBadge({ activityId, sportType, onChanged }: { activityId: string; sportType: string; onChanged: () => void }) {
  const addToast = useToastStore((s) => s.addToast);
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const label = SPORT_LABELS[sportType as SportType] ?? sportType;

  useEffect(() => {
    if (!open) return;
    function handleClick(e: MouseEvent) {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    }
    document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, [open]);

  const mutation = useMutation({
    mutationFn: (newSport: string) =>
      api.updateActivity(activityId, { sport_type: newSport }),
    onSuccess: () => {
      addToast("success", "Sport type updated");
      onChanged();
      setOpen(false);
    },
    onError: (err: unknown) => addToast("error", errorText(err)),
  });

  return (
    <div className="relative shrink-0" ref={ref}>
      <button
        onClick={() => setOpen(!open)}
        title={`${label} — change sport`}
        className="block rounded-xl hover:opacity-90 transition-opacity"
      >
        <SportIcon sport={sportType} size={40} />
      </button>
      {open && (
        <div className="absolute top-full left-0 mt-1.5 bg-card border border-border rounded-card shadow-xl py-1 z-20 w-52 max-h-80 overflow-y-auto scroll-themed">
          {SPORT_TYPES.map((st) => {
            const isSel = st === sportType;
            return (
              <button
                key={st}
                onClick={() => { if (!isSel) mutation.mutate(st); else setOpen(false); }}
                className={`flex w-full items-center gap-2.5 px-3 py-1.5 text-left text-[13px] ${
                  isSel ? "bg-accent-soft text-accent-2 font-medium" : "text-ink hover:bg-card-2"
                }`}
              >
                <span
                  className="grid h-6 w-6 shrink-0 place-items-center rounded-md text-white"
                  style={{ background: getSportColor(st) }}
                >
                  <SportGlyph sport={st} size={13} />
                </span>
                <span className="truncate">{SPORT_LABELS[st]}</span>
                {isSel && <Check size={14} className="ml-auto shrink-0" />}
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}
