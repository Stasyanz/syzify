import { create } from "zustand";

export interface ConfirmOptions {
  title: string;
  message: string;
  /** Confirm button label (default "Confirm"). */
  confirmLabel?: string;
  cancelLabel?: string;
  /** Red confirm button — destructive actions (delete, uninstall). */
  danger?: boolean;
}

interface ConfirmState {
  pending: (ConfirmOptions & { resolve: (ok: boolean) => void }) | null;
  request: (opts: ConfirmOptions) => Promise<boolean>;
  settle: (ok: boolean) => void;
}

export const useConfirmStore = create<ConfirmState>((set, get) => ({
  pending: null,
  request: (opts) =>
    new Promise<boolean>((resolve) => {
      // A second request while one is open would orphan the first promise —
      // settle it as cancelled instead of hanging its caller forever.
      get().pending?.resolve(false);
      set({ pending: { ...opts, resolve } });
    }),
  settle: (ok) => {
    get().pending?.resolve(ok);
    set({ pending: null });
  },
}));

/**
 * Themed in-app replacement for the native `ask()` dialog: resolves to the
 * user's answer once they click a button (or Escape/Enter). Rendered by the
 * single `<ConfirmDialogHost />` in App.tsx.
 */
export function confirmDialog(opts: ConfirmOptions): Promise<boolean> {
  return useConfirmStore.getState().request(opts);
}