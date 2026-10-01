import { useEffect } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { invalidateActivityData } from "../lib/activityInvalidation";

/** The event the plugin runtime emits after a contribution call changed
 * the vault: imported files through `host_import_file`, or put activities
 * on gear through `host_set_activity_gear`. Its payload (`{ imported,
 * monitoring_days, gear_writes }`) is diagnostic; any change refreshes
 * everything. */
export const PLUGINS_IMPORTED_EVENT = "plugins:imported";

/**
 * Refresh everything derived from the activity set when a plugin imports
 * files or moves gear — the same invalidation a drop import triggers, plus
 * the open activity pages: a plugin may have put the one on screen on gear,
 * which the shared list leaves to the caller (it knows which id changed; a
 * plugin does not say). The backend emits the event whether the plugin's
 * call then returned a view or failed: the files are in the vault either
 * way, so a widget's own result cannot be the trigger.
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
