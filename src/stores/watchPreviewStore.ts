import { create } from "zustand";
import type { ScanPreview } from "../lib/types";

/**
 * The watch folders' preview (Settings → Vault): kept out of the component,
 * so a preview that finishes while the section is unmounted — another tab,
 * another page — is there when it comes back, not lost (#235). It stays
 * until something makes it stale: a new Preview, a scan, a change of the
 * folder list — or the user closes it.
 */
interface WatchPreviewState {
  preview: ScanPreview | null;
  setPreview: (preview: ScanPreview | null) => void;
}

export const useWatchPreviewStore = create<WatchPreviewState>((set) => ({
  preview: null,
  setPreview: (preview) => set({ preview }),
}));
