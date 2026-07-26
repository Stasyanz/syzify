import { useEffect } from "react";
import { useConfirmStore } from "../../stores/confirmStore";

/**
 * The single host for `confirmDialog()` requests — a themed modal in place
 * of the native (unstylable) ask() panel. No backdrop-click close (app-wide
 * modal policy); Escape cancels, Enter confirms.
 */
export function ConfirmDialogHost() {
  const pending = useConfirmStore((s) => s.pending);
  const settle = useConfirmStore((s) => s.settle);

  useEffect(() => {
    if (!pending) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        // Capture + stop so page-level Escape handlers (detail-page back
        // navigation, fullscreen map exit) don't also fire underneath.
        e.stopPropagation();
        settle(false);
      } else if (e.key === "Enter") {
        e.stopPropagation();
        settle(true);
      }
    };
    document.addEventListener("keydown", onKey, true);
    return () => document.removeEventListener("keydown", onKey, true);
  }, [pending, settle]);

  if (!pending) return null;

  return (
    <div
      className="fixed inset-0 z-[60] flex items-center justify-center bg-black/30"
      role="alertdialog"
      aria-modal="true"
      aria-label={pending.title}
    >
      <div className="bg-card rounded-xl shadow-2xl w-full max-w-sm mx-4 p-5">
        <h2 className="text-base font-semibold text-ink">{pending.title}</h2>
        <p className="mt-2 text-sm text-muted whitespace-pre-line">{pending.message}</p>
        <div className="mt-5 flex justify-end gap-2">
          <button
            onClick={() => settle(false)}
            className="px-3.5 py-1.5 rounded-lg border border-border-2 text-sm font-medium text-muted hover:text-ink hover:bg-card-2"
          >
            {pending.cancelLabel ?? "Cancel"}
          </button>
          <button
            autoFocus
            onClick={() => settle(true)}
            className={
              pending.danger
                ? "px-3.5 py-1.5 rounded-lg text-sm font-medium text-white bg-red-700 hover:bg-red-800"
                : "px-3.5 py-1.5 rounded-lg text-sm font-medium bg-accent text-accent-ink hover:bg-accent-2"
            }
          >
            {pending.confirmLabel ?? "Confirm"}
          </button>
        </div>
      </div>
    </div>
  );
}