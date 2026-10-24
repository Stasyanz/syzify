import { useQuery, useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { FolderOpen, RefreshCw, Eye, Trash2, X } from "lucide-react";
import { api } from "../../lib/tauri";
import { useToastStore } from "../../stores/toastStore";
import { useBusy, useBusyStore } from "../../stores/busyStore";
import { useWatchPreviewStore } from "../../stores/watchPreviewStore";
import { invalidateActivityData } from "../../lib/activityInvalidation";
import { formatImportSummary } from "../../lib/importSummary";
import { Toggle } from "../ui/Toggle";
import { errorText } from "../../lib/errors";
import { useRichT, useT } from "../../lib/i18n";

/**
 * Settings → Vault → Watch folders: the folders scanned for new workout and
 * monitoring files, a Preview of what is new, Import Now, and the
 * auto-import switch. Every change restarts the background watcher so it
 * follows the list. Scanning and previewing are flags of the busy store, so
 * a remount mid-scan keeps the buttons locked.
 */
export function WatchFolders() {
  const t = useT();
  const rt = useRichT();
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

  const scanning = useBusy("watchScan");
  const previewing = useBusy("watchPreview");
  // In a store: a preview that ends while the section is unmounted is still
  // shown when it comes back.
  const preview = useWatchPreviewStore((s) => s.preview);
  const setPreview = useWatchPreviewStore((s) => s.setPreview);
  // Not cleared on unmount: under StrictMode a dev double-mount runs a fake
  // cleanup that would wipe the very preview the return should show. A
  // stale one goes by what makes it stale — a new Preview, a scan, a change
  // of the folder list — or by its close button.

  async function foldersChanged() {
    queryClient.invalidateQueries({ queryKey: ["watchFolders"] });
    setPreview(null);
    // The watcher follows the list. A restart that fails leaves the app
    // WITHOUT a watcher until the next launch (the old one is stopped
    // first), so the user hears about it.
    try {
      await api.restartWatcher();
    } catch (e) {
      addToast("warning", t("settings.watch.restartFailed", { error: errorText(e) }));
    }
  }

  async function handleAddFolder() {
    try {
      const selected = await open({ directory: true, multiple: false });
      if (!selected || typeof selected !== "string") return;
      await api.addWatchFolder(selected);
      await foldersChanged();
    } catch (e) {
      addToast("error", t("settings.watch.addFailed", { error: errorText(e) }));
    }
  }

  async function handleRemove(id: number) {
    try {
      await api.removeWatchFolder(id);
      await foldersChanged();
    } catch (e) {
      addToast("error", t("settings.watch.removeFailed", { error: errorText(e) }));
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
      addToast("error", t("settings.watch.autoImportFailed", { error: errorText(e) }));
    }
  }

  async function handlePreview() {
    const ops = useBusyStore.getState();
    ops.start("watchPreview");
    setPreview(null);
    try {
      setPreview(await api.previewWatchFolders());
    } catch (e) {
      addToast("error", t("settings.watch.previewFailed", { error: errorText(e) }));
    } finally {
      ops.finish("watchPreview");
    }
  }

  async function handleScan() {
    const ops = useBusyStore.getState();
    ops.start("watchScan");
    setPreview(null);
    try {
      const result = await api.scanWatchFolders();
      if (result.import_result) {
        const summary = formatImportSummary(result.import_result, {}, t);
        addToast(summary.level, summary.text);
        invalidateActivityData(queryClient);
      } else {
        addToast("info", t("settings.watch.noNewFiles"));
      }
    } catch (e) {
      addToast("error", t("common.importFailed", { error: errorText(e) }));
    } finally {
      ops.finish("watchScan");
    }
  }

  const busy = scanning || previewing;
  const nothingToScan = folders.length === 0;

  return (
    <>
      <div className="set-row">
        <div>
          <div className="sl">{t("settings.watch.title")}</div>
          <div className="sd">{t("settings.watch.hint")}</div>
        </div>
        <div className="flex gap-2 shrink-0">
          <button onClick={handleAddFolder} className="btn ghost">
            <FolderOpen size={15} />
            {t("settings.watch.add")}
          </button>
          <button
            onClick={handlePreview}
            disabled={busy || nothingToScan}
            className="btn ghost"
          >
            <Eye size={15} className={previewing ? "animate-pulse" : ""} />
            {previewing ? t("settings.watch.scanning") : t("settings.watch.preview")}
          </button>
          <button
            onClick={handleScan}
            disabled={busy || nothingToScan}
            className="btn primary"
          >
            <RefreshCw size={15} className={scanning ? "animate-spin" : ""} />
            {scanning ? t("settings.watch.importing") : t("settings.watch.importNow")}
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
                    data-tip={t("settings.watch.remove")}
                    aria-label={t("settings.watch.removePath", { path: wf.path })}
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
                  {rt("settings.watch.found", {
                    count: preview.total_files,
                    fresh: (
                      <strong style={{ color: "var(--good)" }}>
                        {t("settings.watch.new", { count: preview.new_files })}
                      </strong>
                    ),
                  })}
                </span>
                <div className="flex items-center gap-2">
                  {preview.new_files > 0 && (
                    <button
                      onClick={handleScan}
                      disabled={busy}
                      className="btn primary !px-2.5 !py-1 !text-xs"
                    >
                      {/* A scan clears the preview as it starts, and none
                          opens while one runs: this button never says
                          Importing… — the one above does. */}
                      {t("settings.watch.importAllNew")}
                    </button>
                  )}
                  <button
                    onClick={() => setPreview(null)}
                    className="iconbtn"
                    data-tip={t("settings.watch.closePreview")}
                    aria-label={t("settings.watch.closePreview")}
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
                          {f.is_new ? t("settings.watch.badgeNew") : t("settings.watch.badgeImported")}
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
          <div className="sl">{t("settings.watch.autoImport")}</div>
          <div className="sd">
            {autoImportPending
              ? "…"
              : autoImport === "auto"
                ? t("settings.watch.autoOn")
                : t("settings.watch.autoOff")}
          </div>
        </div>
        <Toggle
          on={autoImport === "auto"}
          onToggle={handleToggleAutoImport}
          ariaLabel={t("settings.watch.autoImport")}
        />
      </div>
    </>
  );
}
