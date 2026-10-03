import { useEffect } from "react";
import { useQueryClient } from "@tanstack/react-query";

/** The event the backend emits when background work (geocoding) changed
 * stored activities. */
export const ACTIVITIES_UPDATED_EVENT = "activities:updated";

/**
 * Refresh the activity list and the open activity pages when background
 * geocoding fills in a place name. The subscription arrives after an IPC
 * round trip; a cleanup that ran meanwhile lets it go the moment it lands
 * (#194), so a remount never leaves a second listener behind.
 */
export function useActivitiesUpdatedRefresh(): void {
  const queryClient = useQueryClient();
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    import("@tauri-apps/api/event")
      .then(({ listen }) => {
        if (cancelled) return;
        return listen(ACTIVITIES_UPDATED_EVENT, () => {
          void queryClient.invalidateQueries({ queryKey: ["activities"] });
          void queryClient.invalidateQueries({ queryKey: ["activity"] });
        }).then((fn) => {
          if (cancelled) fn();
          else unlisten = fn;
        });
      })
      .catch(() => {
        // Outside Tauri (tests, a browser preview) there is no event bus.
      });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [queryClient]);
}
