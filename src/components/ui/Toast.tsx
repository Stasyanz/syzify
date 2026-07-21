import { useEffect, useState } from "react";
import { CheckCircle2, XCircle, Info, AlertTriangle, X } from "lucide-react";
import { useToastStore, type Toast } from "../../stores/toastStore";

const toastTypeClass = {
  success: "t-success",
  error: "t-error",
  warning: "t-warning",
  info: "t-info",
} as const;

const toastIcons = {
  success: CheckCircle2,
  error: XCircle,
  warning: AlertTriangle,
  info: Info,
} as const;

const defaultDuration: Record<string, number> = {
  error: 6000,
};

function ToastItem({ toast }: { toast: Toast }) {
  const removeToast = useToastStore((s) => s.removeToast);
  const [visible, setVisible] = useState(false);

  useEffect(() => {
    // Trigger slide-in on next frame
    const frame = requestAnimationFrame(() => setVisible(true));
    return () => cancelAnimationFrame(frame);
  }, []);

  useEffect(() => {
    if (toast.sticky) return;
    const ms = toast.duration ?? defaultDuration[toast.type] ?? 4000;
    const timer = setTimeout(() => removeToast(toast.id), ms);
    return () => clearTimeout(timer);
  }, [toast.id, toast.type, toast.duration, toast.sticky, removeToast]);

  const Icon = toastIcons[toast.type];

  return (
    <div
      className={`toast ${toastTypeClass[toast.type]} transition-transform duration-300 ease-out ${
        visible ? "translate-x-0" : "translate-x-[calc(100%+1rem)]"
      }`}
    >
      <span className="toast-ic">
        <Icon size={17} />
      </span>
      <p className="toast-msg">{toast.message}</p>
      <button onClick={() => removeToast(toast.id)} className="toast-x" aria-label="Dismiss">
        <X size={14} />
      </button>
    </div>
  );
}

export function ToastContainer() {
  const toasts = useToastStore((s) => s.toasts);

  if (toasts.length === 0) return null;

  return (
    <div className="fixed bottom-4 right-4 z-50 space-y-2">
      {toasts.map((toast) => (
        <ToastItem key={toast.id} toast={toast} />
      ))}
    </div>
  );
}
