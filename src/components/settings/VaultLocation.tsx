import { useQuery } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import { FolderInput, FolderOpen } from "lucide-react";
import { api } from "../../lib/tauri";
import { confirmDialog } from "../../stores/confirmStore";
import { useToastStore } from "../../stores/toastStore";
import { useBusy, useBusyStore } from "../../stores/busyStore";
import { errorText } from "../../lib/errors";
import { formatPercent, useT, type Translator } from "../../lib/i18n";

/** macOS protects Documents/Desktop/Downloads (TCC): a vault there won't
 * open on the next launch until the app has Full Disk Access. Warn up front. */
export function protectedFolderNote(path: string, translate: Translator): string {
  return /\/(Documents|Desktop|Downloads)(\/|$)/.test(path)
    ? `\n\n${translate("settings.vault.protectedNote", { path })}`
    : "";
}

/** Switching is a pointer change, not a move — say so, and name the vault
 * being left behind so an accidental switch is obvious before it happens. */
export function switchVaultMessage(selected: string, current: string | undefined, translate: Translator): string {
  const stays = current
    ? translate("settings.vault.switchStays", { current })
    : translate("settings.vault.switchStaysUnknown");
  return `${translate("settings.vault.switchQuestion", { path: selected })}\n\n${stays} ${translate(
    "settings.vault.restart",
  )}${protectedFolderNote(selected, translate)}`;
}

export function moveVaultMessage(selected: string, translate: Translator): string {
  return `${translate("settings.vault.moveQuestion", { path: selected })}\n\n${translate(
    "settings.vault.moveBody",
  )}${protectedFolderNote(selected, translate)}`;
}

/**
 * Settings → Vault → Location: where the vault lives, with two ways to change
 * it. "Open another…" repoints the app at a different existing vault (the
 * current one is left on disk untouched); "Move…" relocates the current
 * vault's files. Both restart the app: `AppState.vault_path` is immutable and
 * every service keeps reading the old root until then. The busy flags live
 * in the busy store: a remount (a language change, another tab) must not
 * offer the buttons again while the backend still holds the vault.
 */
export function VaultLocation() {
  const t = useT();
  const addToast = useToastStore((s) => s.addToast);
  const updateToast = useToastStore((s) => s.updateToast);
  const removeToast = useToastStore((s) => s.removeToast);
  const switching = useBusy("vaultSwitch");
  const moving = useBusy("vaultMove");
  const busy = switching || moving;

  const { data: vaultPath } = useQuery({
    queryKey: ["vaultPath"],
    queryFn: () => api.getVaultPath(),
  });

  // Give the toast a beat to render; the restart reloads every service
  // against the new location. The timer is deliberately not cancelled on
  // unmount: the marker is already written and the backend keeps the
  // vault-operation slot taken until the process restarts, so a restart that
  // never comes would leave the app stuck. Note: in `tauri dev` the
  // relaunched window comes up blank (the CLI tears down vite when its child
  // exits) — a manual dev-server restart is needed there; production
  // restarts fine.
  function restartSoon() {
    setTimeout(() => {
      api.restartApp().catch((e) => {
        addToast("error", t("settings.vault.restartFailed", { error: errorText(e) }));
      });
    }, 1500);
  }

  async function handleOpenAnother() {
    const selected = await open({
      directory: true,
      multiple: false,
      title: t("app.vaultError.pickExisting"),
    });
    if (!selected || typeof selected !== "string") return;

    const confirmed = await confirmDialog({
      title: t("settings.vault.openAnotherTitle"),
      message: switchVaultMessage(selected, vaultPath, t),
      confirmLabel: t("common.open"),
    });
    if (!confirmed) return;

    const ops = useBusyStore.getState();
    ops.start("vaultSwitch");
    try {
      const root = await api.switchVault(selected, true);
      // Busy until the restart: the backend holds the vault until then.
      addToast("success", t("settings.vault.opening", { path: root }));
      restartSoon();
    } catch (e) {
      addToast("error", t("settings.vault.openFailed", { error: errorText(e) }));
      ops.finish("vaultSwitch");
    }
  }

  async function handleMove() {
    const selected = await open({
      directory: true,
      multiple: false,
      title: t("settings.vault.pickMoveTarget"),
    });
    if (!selected || typeof selected !== "string") return;

    const confirmed = await confirmDialog({
      title: t("settings.vault.moveTitle"),
      message: moveVaultMessage(selected, t),
      confirmLabel: t("settings.vault.moveConfirm"),
    });
    if (!confirmed) return;

    const ops = useBusyStore.getState();
    ops.start("vaultMove");
    // The progress lives in a sticky toast, which the toast store keeps
    // across a remount.
    const toastId = addToast("info", t("settings.vault.moving", { percent: formatPercent(0, 0) }), undefined, true);
    // Everything after `start` is inside the try: a listener that cannot
    // subscribe must not leave the flag (and the language picker) locked.
    let unlisten = () => {};
    try {
      unlisten = await listen<{ processed: number; total: number }>("vault:relocate:progress", (e) => {
        const { processed, total } = e.payload;
        const pct = total > 0 ? Math.floor((processed / total) * 100) : 100;
        updateToast(toastId, { message: t("settings.vault.moving", { percent: formatPercent(pct, 0) }) });
      });
      const newPath = await api.relocateVault(selected);
      removeToast(toastId);
      // Busy until the restart, as for a switch.
      addToast("success", t("settings.vault.moved", { path: newPath }));
      restartSoon();
    } catch (e) {
      removeToast(toastId);
      addToast("error", t("settings.vault.moveFailed", { error: errorText(e) }));
      ops.finish("vaultMove");
    } finally {
      unlisten();
    }
  }

  return (
    <div className="set-row">
      <div className="min-w-0">
        <div className="sl">{t("settings.vault.location")}</div>
        <div className="sd code">{vaultPath ?? "…"}</div>
      </div>
      <div className="flex items-center gap-2 shrink-0">
        <button
          onClick={handleOpenAnother}
          disabled={busy}
          className="btn ghost"
        >
          <FolderOpen size={15} />
          {switching ? t("settings.vault.openingButton") : t("settings.vault.openAnother")}
        </button>
        <button onClick={handleMove} disabled={busy} className="btn ghost">
          <FolderInput size={15} />
          {moving ? t("settings.vault.movingButton") : t("settings.vault.move")}
        </button>
      </div>
    </div>
  );
}
