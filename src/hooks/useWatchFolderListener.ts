import { useState, useEffect, useRef, useCallback } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { api } from "../lib/tauri";
import { useToastStore } from "../stores/toastStore";

export function useWatchFolderListener() {
  const [pendingFiles, setPendingFiles] = useState<string[]>([]);
  const [importing, setImporting] = useState(false);
  const queryClient = useQueryClient();
  const addToast = useToastStore((s) => s.addToast);
  const addToastRef = useRef(addToast);
  addToastRef.current = addToast;

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;

    async function setup() {
      try {
        const { listen } = await import("@tauri-apps/api/event");
        if (cancelled) return;

        unlisten = await listen<{ files: string[] }>(
          "watch:files-detected",
          async (event) => {
            const files = event.payload.files;
            if (!files || files.length === 0) return;

            // Check setting
            let mode: string | null = null;
            try {
              mode = await api.getSetting("watch_auto_import");
            } catch {
              // default to ask
            }

            if (mode === "auto") {
              // Auto-import immediately
              try {
                const result = await api.importFiles(files);
                queryClient.invalidateQueries({ queryKey: ["activities"] });
                queryClient.invalidateQueries({ queryKey: ["calendar"] });
                queryClient.invalidateQueries({ queryKey: ["dashboard"] });
                const parts = [
                  `Auto-imported ${result.imported} activit${result.imported === 1 ? "y" : "ies"}`,
                ];
                if (result.skipped > 0)
                  parts.push(`skipped ${result.skipped}`);
                if (result.failed.length > 0)
                  parts.push(`${result.failed.length} failed`);
                addToastRef.current(
                  result.failed.length > 0 ? "warning" : "success",
                  parts.join(", ")
                );
              } catch (e) {
                addToastRef.current(
                  "error",
                  `Auto-import failed: ${e}`
                );
              }
            } else {
              // Ask mode: accumulate pending files
              setPendingFiles((prev) => {
                const set = new Set([...prev, ...files]);
                return Array.from(set);
              });
            }
          }
        );
      } catch {
        // Not running inside Tauri
      }
    }

    setup();

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [queryClient]);

  const handleImport = useCallback(async () => {
    if (pendingFiles.length === 0) return;
    setImporting(true);
    try {
      const result = await api.importFiles(pendingFiles);
      queryClient.invalidateQueries({ queryKey: ["activities"] });
      queryClient.invalidateQueries({ queryKey: ["calendar"] });
      const parts = [
        `Imported ${result.imported} activit${result.imported === 1 ? "y" : "ies"}`,
      ];
      if (result.skipped > 0) parts.push(`skipped ${result.skipped}`);
      if (result.failed.length > 0)
        parts.push(`${result.failed.length} failed`);
      addToast(
        result.failed.length > 0 ? "warning" : "success",
        parts.join(", ")
      );
      setPendingFiles([]);
    } catch (e) {
      addToast("error", `Import failed: ${e}`);
    } finally {
      setImporting(false);
    }
  }, [pendingFiles, queryClient, addToast]);

  const handleDismiss = useCallback(() => {
    setPendingFiles([]);
  }, []);

  return { pendingFiles, importing, handleImport, handleDismiss };
}
