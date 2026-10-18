import { useState, useEffect, useRef, useCallback } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { api } from "../lib/tauri";
import { useToastStore } from "../stores/toastStore";
import { invalidateActivityData } from "../lib/activityInvalidation";
import { formatImportSummary } from "../lib/importSummary";
import { errorText } from "../lib/errors";

/**
 * Offers new workout files from the watch folders and device volumes:
 * imported at once in auto mode, queued for the banner in ask mode.
 * `vaultOpen` — the vault is loaded and unlocked; the first time it is,
 * the hook asks for what arrived while the app was closed (#221), since
 * the watcher only reports files that appear while it runs.
 */
export function useWatchFolderListener(vaultOpen: boolean) {
  const [pendingFiles, setPendingFiles] = useState<string[]>([]);
  const [importing, setImporting] = useState(false);
  const queryClient = useQueryClient();
  const addToast = useToastStore((s) => s.addToast);
  const addToastRef = useRef(addToast);
  addToastRef.current = addToast;

  // One way in for files, whether the watcher saw them arrive or the
  // launch catch-up found them waiting.
  const offerFiles = useCallback(
    async (files: string[]) => {
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
            `Auto-import failed: ${errorText(e)}`
          );
        }
      } else {
        // Ask mode: accumulate pending files
        setPendingFiles((prev) => {
          const set = new Set([...prev, ...files]);
          return Array.from(set);
        });
      }
    },
    [queryClient]
  );

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;

    async function setup() {
      try {
        const { listen } = await import("@tauri-apps/api/event");
        if (cancelled) return;

        const u = await listen<{ files: string[] }>(
          "watch:files-detected",
          (event) => offerFiles(event.payload.files)
        );
        // The cleanup may have run while `listen` was still on its IPC
        // round trip: nobody is left to call `unlisten`, so let the
        // subscription go now instead of leaving it to fire forever.
        if (cancelled) {
          u();
          return;
        }
        unlisten = u;
      } catch {
        // Not running inside Tauri
      }
    }

    setup();

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [offerFiles]);

  // Once per app run: a ref, not state, so StrictMode's second effect pass
  // (and any re-render) does not ask — and auto-import — twice.
  const caughtUp = useRef(false);
  useEffect(() => {
    if (!vaultOpen || caughtUp.current) return;
    caughtUp.current = true;
    api
      .catchUpWatchFiles()
      .then(offerFiles)
      .catch((e) => {
        addToastRef.current(
          "error",
          `Could not check for new files: ${errorText(e)}`
        );
      });
  }, [vaultOpen, offerFiles]);

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
      addToast("error", `Import failed: ${errorText(e)}`);
    } finally {
      setImporting(false);
    }
  }, [pendingFiles, queryClient, addToast]);

  const handleDismiss = useCallback(() => {
    setPendingFiles([]);
  }, []);

  return { pendingFiles, importing, handleImport, handleDismiss };
}
