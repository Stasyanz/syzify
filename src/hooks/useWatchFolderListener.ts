import { useState, useEffect, useRef, useCallback } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { api } from "../lib/tauri";
import { useToastStore } from "../stores/toastStore";
import { invalidateActivityData } from "../lib/activityInvalidation";
import { formatImportSummary } from "../lib/importSummary";

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
                invalidateActivityData(queryClient);
                const summary = formatImportSummary(result, { auto: true });
                addToastRef.current(
                  summary.level,
                  summary.text
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
      invalidateActivityData(queryClient);
      const summary = formatImportSummary(result);
      addToast(summary.level, summary.text);
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
