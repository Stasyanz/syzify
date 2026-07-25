import { useState, useEffect, useRef } from "react";
import { useQueryClient, useMutation } from "@tanstack/react-query";
import { api } from "../lib/tauri";
import { useToastStore } from "../stores/toastStore";
import { isWorkoutPath } from "../lib/fileTypes";
import { invalidateActivityData } from "../lib/activityInvalidation";

/// Whether a physical drop position lands on a component that handles its own
/// file drops (e.g. the photo gallery). Such drops must not be treated as
/// workout-file imports.
function isOverOwnDropZone(physicalX: number, physicalY: number): boolean {
  const dpr = window.devicePixelRatio || 1;
  const el = document.elementFromPoint(physicalX / dpr, physicalY / dpr);
  return !!el?.closest("[data-photo-dropzone]");
}

export function useDropImport() {
  const [dragging, setDragging] = useState(false);
  const queryClient = useQueryClient();
  const addToast = useToastStore((s) => s.addToast);

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

  const mutateRef = useRef(importMutation.mutate);
  mutateRef.current = importMutation.mutate;
  const addToastRef = useRef(addToast);
  addToastRef.current = addToast;

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;

    async function setup() {
      try {
        const { getCurrentWebview } = await import("@tauri-apps/api/webview");
        if (cancelled) return;
        const webview = getCurrentWebview();
        unlisten = await webview.onDragDropEvent((event) => {
          if (event.payload.type === "enter" || event.payload.type === "over") {
            setDragging(true);
          } else if (event.payload.type === "drop") {
            setDragging(false);
            // A photo gallery (or other zone) handles its own drops — don't
            // also try to import those files as workouts.
            const { x, y } = event.payload.position;
            if (isOverOwnDropZone(x, y)) return;
            const valid = event.payload.paths.filter(isWorkoutPath);
            if (valid.length === 0) {
              addToastRef.current("warning", "No workout files found (GPX, FIT, TCX)");
              return;
            }
            mutateRef.current(valid);
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
  }, []); // subscribe once, use refs for latest callbacks

  return { dragging, importing: importMutation.isPending };
}
