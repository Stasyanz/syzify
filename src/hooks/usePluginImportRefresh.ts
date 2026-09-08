import { useEffect } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { invalidateActivityData } from "../lib/activityInvalidation";

/** The event the plugin runtime emits after a contribution call imported
 * files through `host_import_file` (payload `{ imported }`). */
export const PLUGINS_IMPORTED_EVENT = "plugins:imported";

/**
 * Refresh everything derived from the activity set when a plugin imports
 * files — the same invalidation a drop import triggers. The backend emits
 * the event whether the plugin's call then returned a view or failed: the
 * files are in the vault either way, so a widget's own result cannot be
 * the trigger.
 */
export function usePluginImportRefresh(): void {
  const queryClient = useQueryClient();
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    import("@tauri-apps/api/event")
      .then(({ listen }) => {
        if (cancelled) return;
        return listen(PLUGINS_IMPORTED_EVENT, () => {
          void invalidateActivityData(queryClient);
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
