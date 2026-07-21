import { create } from "zustand";

export type ToastType = "success" | "error" | "info" | "warning";

export interface Toast {
  id: string;
  type: ToastType;
  message: string;
  duration?: number;
  // Sticky toasts never auto-dismiss; they must be removed/updated explicitly
  // (e.g. live progress indicators).
  sticky?: boolean;
}

interface ToastStore {
  toasts: Toast[];
  addToast: (
    type: ToastType,
    message: string,
    duration?: number,
    sticky?: boolean
  ) => string;
  updateToast: (id: string, patch: Partial<Omit<Toast, "id">>) => void;
  removeToast: (id: string) => void;
}

export const useToastStore = create<ToastStore>((set) => ({
  toasts: [],

  addToast: (type, message, duration, sticky) => {
    const id = crypto.randomUUID();
    set((state) => ({
      toasts: [...state.toasts, { id, type, message, duration, sticky }],
    }));
    return id;
  },

  updateToast: (id, patch) =>
    set((state) => ({
      toasts: state.toasts.map((t) => (t.id === id ? { ...t, ...patch } : t)),
    })),

  removeToast: (id) =>
    set((state) => ({
      toasts: state.toasts.filter((t) => t.id !== id),
    })),
}));
