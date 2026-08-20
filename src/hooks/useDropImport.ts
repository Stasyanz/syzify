import { useState, useEffect, useRef } from "react";
import { useQueryClient, useMutation } from "@tanstack/react-query";
import { useLocation, matchPath } from "react-router";
import { api } from "../lib/tauri";
import { useToastStore } from "../stores/toastStore";
import { isImagePath, isWorkoutPath } from "../lib/fileTypes";
import { invalidateActivityData } from "../lib/activityInvalidation";

export type DropKind = "workout" | "photo";

/// Native OS file drops arrive through Tauri's webview-wide drag-drop events
/// (HTML5 file drops are suppressed when dragDropEnabled is on). The whole
/// window is one drop target whose meaning depends on the current route: an
/// activity page attaches photos to that activity, every other page imports
/// workout files.
export function useDropImport() {
  const [dragging, setDragging] = useState(false);
  const queryClient = useQueryClient();
  const addToast = useToastStore((s) => s.addToast);

  const activityId =
    matchPath("/activity/:id", useLocation().pathname)?.params.id ?? null;
  const kind: DropKind = activityId ? "photo" : "workout";

  const importMutation = useMutation({
    mutationFn: (paths: string[]) => api.importFiles(paths),
    onSuccess: (data) => {
      invalidateActivityData(queryClient);
      const parts = [
        `Imported ${data.imported} activit${data.imported === 1 ? "y" : "ies"}`,
      ];
      if (data.skipped > 0) parts.push(`skipped ${data.skipped} (duplicates)`);
      if (data.failed.length > 0) parts.push(`${data.failed.length} failed`);
      addToast(data.failed.length > 0 ? "warning" : "success", parts.join(", "));
    },
    onError: (error: Error) => {
      addToast("error", `Import failed: ${error.message ?? "Unknown error"}`);
    },
  });

  const attachMutation = useMutation({
    mutationFn: (args: { activityId: string; paths: string[] }) =>
      api.attachPhotos(args.activityId, args.paths),
    onSuccess: (res, args) => {
      queryClient.invalidateQueries({ queryKey: ["photos", args.activityId] });
      const parts: string[] = [];
      if (res.attached.length) parts.push(`Added ${res.attached.length}`);
      if (res.skipped.length) parts.push(`skipped ${res.skipped.length} duplicates`);
      if (res.failed.length) parts.push(`${res.failed.length} failed`);
      if (parts.length)
        addToast(res.failed.length > 0 ? "warning" : "success", parts.join(", "));
    },
    onError: (e: Error) => addToast("error", `Failed to attach photos: ${e.message}`),
  });

  // The Tauri listener is subscribed once; refs carry the latest route and
  // callbacks into it.
  const refs = useRef({
    activityId,
    import: importMutation.mutate,
    attach: attachMutation.mutate,
    addToast,
  });
  refs.current = {
    activityId,
    import: importMutation.mutate,
    attach: attachMutation.mutate,
    addToast,
  };

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;

    async function setup() {
      try {
        const { getCurrentWebview } = await import("@tauri-apps/api/webview");
        if (cancelled) return;
        const webview = getCurrentWebview();
        // Whether the current drag carries anything this page accepts. Known
        // from the `enter` payload, which (unlike `over`) includes the paths.
        let relevant = false;
        unlisten = await webview.onDragDropEvent((event) => {
          const p = event.payload;
          if (p.type === "enter") {
            relevant = refs.current.activityId
              ? p.paths.some(isImagePath)
              : // An all-images drag is the activity page's business — don't
                // raise the workout overlay for it elsewhere either.
                !(p.paths.length > 0 && p.paths.every(isImagePath));
            setDragging(relevant);
          } else if (p.type === "over") {
            setDragging(relevant);
          } else if (p.type === "drop") {
            setDragging(false);
            const { activityId } = refs.current;
            if (activityId) {
              const images = p.paths.filter(isImagePath);
              if (images.length > 0) {
                refs.current.attach({ activityId, paths: images });
              } else if (p.paths.some(isWorkoutPath)) {
                refs.current.addToast(
                  "warning",
                  "To import workouts, drop them outside the activity page"
                );
              } else if (p.paths.length > 0) {
                refs.current.addToast(
                  "warning",
                  "Only JPG, PNG, WebP or HEIC images can be added to an activity"
                );
              }
              return;
            }
            const valid = p.paths.filter(isWorkoutPath);
            if (valid.length === 0) {
              refs.current.addToast(
                "warning",
                p.paths.length > 0 && p.paths.every(isImagePath)
                  ? "To add photos, drop them on an activity page"
                  : "No workout files found (GPX, FIT, TCX)"
              );
              return;
            }
            refs.current.import(valid);
          } else {
            setDragging(false);
          }
        });
      } catch {
        // Not running inside Tauri
      }
    }

    setup();

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []); // subscribe once, use refs for latest route/callbacks

  return {
    dragging,
    kind,
    importing: importMutation.isPending || attachMutation.isPending,
  };
}