import { useCallback, useEffect, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../lib/tauri";

/** The pointer handlers a resize grip spreads onto its 20×20 handle. */
export type GripHandlers = {
  onPointerDown: (e: React.PointerEvent<HTMLElement>) => void;
  onPointerMove: (e: React.PointerEvent<HTMLElement>) => void;
  onPointerUp: (e: React.PointerEvent<HTMLElement>) => void;
  onPointerCancel: (e: React.PointerEvent<HTMLElement>) => void;
};

/**
 * A panel height dragged by a bottom-right grip and remembered in a
 * setting — the map and the first chart card share it. `clamp` is the
 * panel's pure bounds ([default, default × 1.5], NaN → default) and must be
 * a stable function; `settingKey` is where the height persists.
 *
 * Pointer capture keeps the drag alive when the cursor leaves the handle
 * (WKWebView needs `touch-none` on it too, or the gesture is hijacked).
 * The drag starts from the height in a ref rather than from state, so the
 * handlers never go stale; the release persists from the same ref instead
 * of reading through a state updater (StrictMode runs updaters twice in
 * dev). `pointercancel` may arrive after the pointer is gone, and
 * releasing an uncaptured pointer is a NotFoundError by spec, hence the
 * `hasPointerCapture` check.
 */
export function useResizeGrip(
  defaultPx: number,
  clamp: (px: number) => number,
  settingKey: string,
): { height: number; grip: GripHandlers } {
  const { data: saved } = useQuery({
    queryKey: ["setting", settingKey],
    queryFn: () => api.getSetting(settingKey),
  });
  const [height, setHeight] = useState(defaultPx);
  const heightRef = useRef(height);
  heightRef.current = height;
  const dragRef = useRef<{ startY: number; startHeight: number } | null>(null);
  // A setting that resolves after the user already grabbed the grip must
  // not yank the height out from under the drag.
  useEffect(() => {
    if (saved != null && !dragRef.current) setHeight(clamp(Number(saved)));
  }, [saved, clamp]);
  const onPointerDown = useCallback((e: React.PointerEvent<HTMLElement>) => {
    e.preventDefault();
    // The grip may sit inside a card that is itself drag-reorderable.
    e.stopPropagation();
    e.currentTarget.setPointerCapture(e.pointerId);
    dragRef.current = { startY: e.clientY, startHeight: heightRef.current };
  }, []);
  const onPointerMove = useCallback(
    (e: React.PointerEvent<HTMLElement>) => {
      const drag = dragRef.current;
      if (!drag) return;
      setHeight(clamp(drag.startHeight + (e.clientY - drag.startY)));
    },
    [clamp],
  );
  const onPointerUp = useCallback(
    (e: React.PointerEvent<HTMLElement>) => {
      if (!dragRef.current) return;
      dragRef.current = null;
      if (e.currentTarget.hasPointerCapture(e.pointerId)) e.currentTarget.releasePointerCapture(e.pointerId);
      api.setSetting(settingKey, String(heightRef.current)).catch(() => {});
    },
    [settingKey],
  );
  return { height, grip: { onPointerDown, onPointerMove, onPointerUp, onPointerCancel: onPointerUp } };
}
