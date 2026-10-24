import { create } from "zustand";

/**
 * The long operations of Settings whose indicator must outlive the
 * component that started them (#235): a language change remounts the whole
 * app (#232), and switching the Settings tab — or leaving Settings — unmounts
 * the section, while the operation itself runs on. The backend's
 * vault_flight keeps the data safe, but a button would come back enabled and
 * the progress gone. So the busy flags live here, the sections read them,
 * the Settings language picker stays locked while any of them runs, and a
 * change of the system's languages waits for them (lib/i18n.ts).
 */
export type BusyOp =
  | "clearTiles" // Settings → General: Clear Cache
  | "backup" // Settings → Vault: Backup
  | "restore" // Settings → Vault: Restore (busy until the restart)
  | "encrypt" // Settings → Vault: enable encryption
  | "decrypt" // Settings → Vault: disable encryption
  | "vaultSwitch" // VaultLocation: Open another… (busy until the restart)
  | "vaultMove" // VaultLocation: Move… (busy until the restart)
  | "watchPreview" // WatchFolders: Preview
  | "watchScan" // WatchFolders: Import now
  | "updateInstall" // UpdateCheck: download and install
  | "monitoringDelete"; // MonitoringData: Delete…

interface BusyState {
  /** The operations running, each with its progress in percent — null
   * until it reports one. */
  running: Partial<Record<BusyOp, number | null>>;
  /** What the last run of an operation failed with, for a section that
   * shows the error itself and may have been remounted since. Cleared by
   * the next start. */
  lastError: Partial<Record<BusyOp, unknown>>;
  start: (op: BusyOp) => void;
  /** Report progress; ignored once the operation finished, so a late event
   * does not bring a finished operation back. */
  progress: (op: BusyOp, percent: number) => void;
  /** End the operation, with the error it failed with if it did — any
   * value a promise rejects with, `undefined` included. */
  finish: (op: BusyOp, ...error: [] | [unknown]) => void;
}

export const useBusyStore = create<BusyState>((set) => ({
  running: {},
  lastError: {},
  start: (op) =>
    set((s) => {
      const lastError = { ...s.lastError };
      delete lastError[op];
      return { running: { ...s.running, [op]: null }, lastError };
    }),
  progress: (op, percent) => set((s) => (op in s.running ? { running: { ...s.running, [op]: percent } } : s)),
  finish: (op, ...error) =>
    set((s) => {
      const running = { ...s.running };
      delete running[op];
      return error.length === 0 ? { running } : { running, lastError: { ...s.lastError, [op]: error[0] } };
    }),
}));

/** Whether `op` runs. */
export function useBusy(op: BusyOp): boolean {
  return useBusyStore((s) => op in s.running);
}

/** The progress `op` reported, in percent, or null. */
export function useBusyProgress(op: BusyOp): number | null {
  return useBusyStore((s) => s.running[op] ?? null);
}

/** Whether the last run of `op` failed, and with what. */
export function useBusyError(op: BusyOp): { failed: boolean; error: unknown } {
  const failed = useBusyStore((s) => op in s.lastError);
  const error = useBusyStore((s) => s.lastError[op]);
  return { failed, error };
}

/** Whether any long operation runs — what the language picker waits for. */
export function useAnyBusy(): boolean {
  return useBusyStore((s) => anyBusy(s));
}

/** The same outside React: whether anything runs in `state`. */
export function anyBusy(state: Pick<BusyState, "running"> = useBusyStore.getState()): boolean {
  return Object.keys(state.running).length > 0;
}
