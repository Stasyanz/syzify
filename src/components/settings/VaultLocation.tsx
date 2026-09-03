import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import { FolderInput, FolderOpen } from "lucide-react";
import { api } from "../../lib/tauri";
import { confirmDialog } from "../../stores/confirmStore";
import { useToastStore } from "../../stores/toastStore";

/** macOS protects Documents/Desktop/Downloads (TCC): a vault there won't
 * open on the next launch until the app has Full Disk Access. Warn up front. */
export function protectedFolderNote(path: string): string {
  return /\/(Documents|Desktop|Downloads)(\/|$)/.test(path)
    ? `\n\nNote: "${path}" is a protected macOS folder — you'll need to grant Syzify Full Disk Access for it to open on the next launch.`
    : "";
}

/** Switching is a pointer change, not a move — say so, and name the vault
 * being left behind so an accidental switch is obvious before it happens. */
export function switchVaultMessage(selected: string, current: string | undefined): string {
  const stays = current
    ? `The current vault at "${current}" stays where it is — nothing is moved.`
    : "The current vault stays where it is — nothing is moved.";
  return `Open the vault in "${selected}"?\n\n${stays} The app will restart.${protectedFolderNote(selected)}`;
}

export function moveVaultMessage(selected: string): string {
  return `Move the vault to "${selected}"?\n\nAll files will be moved there and the app will restart.${protectedFolderNote(selected)}`;
}

type Busy = "switching" | "moving" | null;

/**
 * Settings → Vault → Location: where the vault lives, with two ways to change
 * it. "Open another…" repoints the app at a different existing vault (the
 * current one is left on disk untouched); "Move…" relocates the current
 * vault's files. Both restart the app: `AppState.vault_path` is immutable and
 * every service keeps reading the old root until then.
 */
export function VaultLocation() {
  const addToast = useToastStore((s) => s.addToast);
  const updateToast = useToastStore((s) => s.updateToast);
  const removeToast = useToastStore((s) => s.removeToast);
  const [busy, setBusy] = useState<Busy>(null);

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
        addToast("error", `Restart failed: ${e}. Quit and reopen Syzify to finish.`);
      });
    }, 1500);
  }

  async function handleOpenAnother() {
    const selected = await open({
      directory: true,
      multiple: false,
      title: "Open an existing vault folder",
    });
    if (!selected || typeof selected !== "string") return;

    const confirmed = await confirmDialog({
      title: "Open another vault",
      message: switchVaultMessage(selected, vaultPath),
      confirmLabel: "Open",
    });
    if (!confirmed) return;

    setBusy("switching");
    try {
      const root = await api.switchVault(selected, true);
      addToast("success", `Opening ${root} — restarting…`);
      restartSoon();
    } catch (e) {
      addToast("error", `Couldn't open vault: ${e}`);
      setBusy(null);
    }
  }

  async function handleMove() {
    const selected = await open({
      directory: true,
      multiple: false,
      title: "Choose a new vault location",
    });
    if (!selected || typeof selected !== "string") return;

    const confirmed = await confirmDialog({
      title: "Move vault",
      message: moveVaultMessage(selected),
      confirmLabel: "Move",
    });
    if (!confirmed) return;

    setBusy("moving");
    const toastId = addToast("info", "Moving vault… 0%", undefined, true);
    const unlisten = await listen<{ processed: number; total: number }>(
      "vault:relocate:progress",
      (e) => {
        const { processed, total } = e.payload;
        const pct = total > 0 ? Math.floor((processed / total) * 100) : 100;
        updateToast(toastId, { message: `Moving vault… ${pct}%` });
      }
    );
    try {
      const newPath = await api.relocateVault(selected);
      removeToast(toastId);
      addToast("success", `Vault moved to ${newPath} — restarting…`);
      restartSoon();
    } catch (e) {
      removeToast(toastId);
      addToast("error", `Move failed: ${e}`);
      setBusy(null);
    } finally {
      unlisten();
    }
  }

  return (
    <div className="set-row">
      <div className="min-w-0">
        <div className="sl">Location</div>
        <div className="sd code">{vaultPath ?? "…"}</div>
      </div>
      <div className="flex items-center gap-2 shrink-0">
        <button
          onClick={handleOpenAnother}
          disabled={busy !== null}
          className="btn ghost"
        >
          <FolderOpen size={15} />
          {busy === "switching" ? "Opening…" : "Open another…"}
        </button>
        <button onClick={handleMove} disabled={busy !== null} className="btn ghost">
          <FolderInput size={15} />
          {busy === "moving" ? "Moving…" : "Move…"}
        </button>
      </div>
    </div>
  );
}
