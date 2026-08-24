import { useEffect, useRef, useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { api } from "../../lib/tauri";
import { formatDistance } from "../../lib/format";

export const MENU_W = 280;
const MENU_H_EST = 170;

/** Clamp the popover's top-left corner so a right-click near a viewport edge
 * doesn't push it off-screen. Pure for tests. */
export function clampMenuPosition(
  x: number,
  y: number,
  vw: number,
  vh: number,
): { left: number; top: number } {
  return {
    left: Math.max(8, Math.min(x, vw - MENU_W - 8)),
    top: Math.max(8, Math.min(y, vh - MENU_H_EST - 8)),
  };
}

/** Right-click "Save segment" form for the elevation chart's selection.
 * Proactively checks the vault for a likely duplicate (same sport, both
 * endpoints within ~50 m, similar length) and warns — saving stays allowed. */
export function SaveSegmentPopover({
  x,
  y,
  activityId,
  range,
  onClose,
}: {
  x: number;
  y: number;
  activityId: string;
  /** Selected trackpoint range, full-activity indices (ordered). */
  range: [number, number];
  onClose: () => void;
}) {
  const boxRef = useRef<HTMLDivElement>(null);
  const [name, setName] = useState("");
  const [saved, setSaved] = useState(false);

  const { data: similar, error: similarError } = useQuery({
    queryKey: ["similar-segments", activityId, range[0], range[1]],
    queryFn: () => api.checkSimilarSegments(activityId, range[0], range[1]),
    // The app-wide staleTime would replay a cached "no duplicates" when the
    // form reopens on the same selection right after a save — this check
    // must hit the vault every time the form opens.
    staleTime: 0,
    gcTime: 0,
    refetchOnMount: "always",
  });

  const save = useMutation({
    mutationFn: () => api.saveSegment(activityId, range[0], range[1], name.trim()),
    onSuccess: () => setSaved(true),
  });

  // Auto-close after the "Saved ✓" beat — as an effect so unmounting (Esc,
  // navigation) cancels the timer instead of firing onClose into a gone tree.
  useEffect(() => {
    if (!saved) return;
    const t = setTimeout(onClose, 900);
    return () => clearTimeout(t);
  }, [saved, onClose]);

  // Outside click / Esc dismiss. pointerdown (not click) so dragging a new
  // chart selection also closes the stale form immediately.
  useEffect(() => {
    const onPointerDown = (e: PointerEvent) => {
      if (boxRef.current && !boxRef.current.contains(e.target as Node)) onClose();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("pointerdown", onPointerDown);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("pointerdown", onPointerDown);
      window.removeEventListener("keydown", onKey);
    };
  }, [onClose]);

  const pos = clampMenuPosition(x, y, window.innerWidth, window.innerHeight);
  const canSave = name.trim().length > 0 && !save.isPending && !saved;

  return (
    <div
      ref={boxRef}
      className="dash-card fixed z-[1500] shadow-2xl"
      style={{ left: pos.left, top: pos.top, width: MENU_W, padding: 14 }}
    >
      <h3 className="!m-0 mb-2 text-sm font-bold">Save segment</h3>

      {similar && similar.length > 0 && (
        <p className="mb-2 text-xs leading-snug text-amber-600">
          Similar segment already saved: “{similar[0].name}” (
          {formatDistance(similar[0].distance_m)}). You can still save this one.
        </p>
      )}

      <div className="field mb-2 flex">
        <input
          autoFocus
          value={name}
          placeholder="Segment name"
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && canSave) save.mutate();
          }}
        />
      </div>

      {(save.isError || similarError != null) && (
        <p className="mb-2 text-xs text-red-600">
          {String(save.isError ? save.error : similarError)}
        </p>
      )}

      <div className="flex items-center justify-end gap-2">
        {saved && <span className="mr-auto text-xs font-semibold text-muted">Saved ✓</span>}
        <button className="btn ghost" onClick={onClose}>
          Cancel
        </button>
        <button className="btn primary" disabled={!canSave} onClick={() => save.mutate()}>
          {similar && similar.length > 0 ? "Save anyway" : "Save"}
        </button>
      </div>
    </div>
  );
}
