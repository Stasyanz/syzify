import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { FolderOpen, RefreshCw, Eye, Trash2, X } from "lucide-react";
import { api } from "../../lib/tauri";
import type { ScanPreview } from "../../lib/types";
import { useToastStore } from "../../stores/toastStore";
import { invalidateActivityData } from "../../lib/activityInvalidation";
import { formatImportSummary } from "../../lib/importSummary";
import { Toggle } from "../ui/Toggle";

/**
 * Settings → Vault → Watch folders: the folders scanned for new workout and
 * monitoring files, a Preview of what is new, Import Now, and the
 * auto-import switch. Every change restarts the background watcher so it
 * follows the list.
 */
export function WatchFolders() {
  const queryClient = useQueryClient();
  const addToast = useToastStore((s) => s.addToast);

  const { data: folders = [] } = useQuery({
    queryKey: ["watchFolders"],
    queryFn: () => api.getWatchFolders(),
  });
  const { data: autoImport, isPending: autoImportPending } = useQuery({
    queryKey: ["setting", "watch_auto_import"],
    queryFn: () => api.getSetting("watch_auto_import"),
  });

  const [scanning, setScanning] = useState(false);
  const [previewing, setPreviewing] = useState(false);
  const [preview, setPreview] = useState<ScanPreview | null>(null);

  async function foldersChanged() {
    queryClient.invalidateQueries({ queryKey: ["watchFolders"] });
    setPreview(null);
    // The watcher follows the list. A restart that fails leaves the app
    // WITHOUT a watcher until the next launch (the old one is stopped
    // first), so the user hears about it.
    try {
      await api.restartWatcher();
    } catch (e) {
      addToast("warning", `Folder list saved, but watching could not restart: ${e}`);
    }
  }

  async function handleAddFolder() {
    try {
      const selected = await open({ directory: true, multiple: false });
      if (!selected || typeof selected !== "string") return;
      await api.addWatchFolder(selected);
      await foldersChanged();
    } catch (e) {
      addToast("error", `Could not add folder: ${e}`);
    }
  }

  async function handleRemove(id: number) {
    try {
      await api.removeWatchFolder(id);
      await foldersChanged();
    } catch (e) {
      addToast("error", `Could not remove folder: ${e}`);
    }
  }

  async function handleToggleAutoImport() {
    // Until the stored value is known a click would flip from "unknown",
    // writing "auto" over a saved "auto" instead of turning it off.
    if (autoImportPending) return;
    const next = autoImport === "auto" ? "ask" : "auto";
    try {
      await api.setSetting("watch_auto_import", next);
      queryClient.invalidateQueries({ queryKey: ["setting", "watch_auto_import"] });
    } catch (e) {
      addToast("error", `Could not change auto-import: ${e}`);
    }
  }

  async function handlePreview() {
    setPreviewing(true);
    setPreview(null);
    try {
      setPreview(await api.previewWatchFolders());
    } catch (e) {
      addToast("error", `Preview failed: ${e}`);
    } finally {
      setPreviewing(false);
    }
  }

  async function handleScan() {
    setScanning(true);
    setPreview(null);
    try {
      const result = await api.scanWatchFolders();
      if (result.import_result) {
        const summary = formatImportSummary(result.import_result);
        addToast(summary.level, summary.text);
        invalidateActivityData(queryClient);
      } else {
        addToast("info", "No new files in the watch folders.");
      }
    } catch (e) {
      addToast("error", `Import failed: ${e}`);
    } finally {
      setScanning(false);
    }
  }

  const busy = scanning || previewing;
  const nothingToScan = folders.length === 0;

  return (
    <>
      <div className="set-row">
        <div>
          <div className="sl">Watch folders</div>
          <div className="sd">
            Folders scanned for new workout and monitoring files (GPX, FIT, TCX)
          </div>
        </div>
        <div className="flex gap-2 shrink-0">
          <button onClick={handleAddFolder} className="btn ghost">
            <FolderOpen size={15} />
            Add Folder
          </button>
          <button
            onClick={handlePreview}
            disabled={busy || nothingToScan}
            className="btn ghost"
          >
            <Eye size={15} className={previewing ? "animate-pulse" : ""} />
            {previewing ? "Scanning…" : "Preview"}
          </button>
          <button
            onClick={handleScan}
            disabled={busy || nothingToScan}
            className="btn primary"
          >
            <RefreshCw size={15} className={scanning ? "animate-spin" : ""} />
            {scanning ? "Importing…" : "Import Now"}
          </button>
        </div>
      </div>

      {(folders.length > 0 || preview) && (
        <div className="pb-4 space-y-3">
          {folders.length > 0 && (
            <div className="set-paths" data-testid="watch-folders">
              {folders.map((wf) => (
                <div key={wf.id} className="set-path-row">
                  <FolderOpen size={16} className="shrink-0 text-faint" />
                  <span className="set-path">{wf.path}</span>
                  <button
                    onClick={() => handleRemove(wf.id)}
                    className="set-path-add danger"
                    data-tip="Remove"
                    aria-label={`Remove ${wf.path}`}
                  >
                    <Trash2 size={15} />
                  </button>
                </div>
              ))}
            </div>
          )}

          {preview && (
            <div className="bg-card-2 rounded-[9px] p-3 space-y-2" data-testid="scan-preview">
              <div className="flex items-center justify-between gap-2 text-xs text-muted">
                <span>
                  {preview.total_files} file{preview.total_files !== 1 ? "s" : ""} found,{" "}
                  <strong style={{ color: "var(--good)" }}>{preview.new_files} new</strong>
                </span>
                <div className="flex items-center gap-2">
                  {preview.new_files > 0 && (
                    <button
                      onClick={handleScan}
                      disabled={busy}
                      className="btn primary !px-2.5 !py-1 !text-xs"
                    >
                      {scanning ? "Importing…" : "Import All New"}
                    </button>
                  )}
                  <button
                    onClick={() => setPreview(null)}
                    className="iconbtn"
                    data-tip="Close preview"
                    aria-label="Close preview"
                  >
                    <X size={14} />
                  </button>
                </div>
              </div>
              {/* A device folder holds hundreds of files: the list scrolls
                  inside a bounded box instead of stretching the page. */}
              <div className="max-h-56 overflow-y-auto scroll-themed space-y-2 pr-1">
                {preview.folders.map((fp) => (
                  <div key={fp.folder} className="space-y-1">
                    <p className="set-path !flex-none">{fp.folder}</p>
                    {fp.files.map((f) => (
                      <div key={f.path} className="flex items-center gap-2 pl-3 text-xs">
                        <span
                          className={`inline-block px-1.5 py-0.5 rounded text-[10px] font-semibold ${
                            f.is_new ? "bg-accent-soft text-accent-2" : "bg-card text-muted"
                          }`}
                        >
                          {f.is_new ? "NEW" : "IMPORTED"}
                        </span>
                        <span className="text-muted truncate">{f.filename}</span>
                      </div>
                    ))}
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>
      )}

      <div className="set-row">
        <div>
          <div className="sl">Auto-import new files</div>
          <div className="sd">
            {autoImportPending
              ? "…"
              : autoImport === "auto"
                ? "Files the watch folders receive are imported right away"
                : "You are asked before files the watch folders receive are imported"}
          </div>
        </div>
        <Toggle
          on={autoImport === "auto"}
          onToggle={handleToggleAutoImport}
          ariaLabel="Auto-import new files"
        />
      </div>
    </>
  );
}
