import { useEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";

export interface ActionMenuItem {
  label: string;
  /** Secondary line under the label (e.g. what the action does). */
  hint?: string;
  onSelect: () => void;
}

interface Props {
  items: ActionMenuItem[];
  ariaLabel: string;
  /** Tooltip for the trigger (WKWebView ignores title — see data-tip). */
  tip?: string;
  disabled?: boolean;
  /** Trigger button classes — callers style it like their sibling buttons. */
  className?: string;
  /** Trigger content (usually an icon). */
  children: ReactNode;
}

/** App-styled action dropdown: an icon trigger + portal menu of one-shot
 * actions. Same portal/positioning approach as Select, but menu items run a
 * callback instead of holding a value, and the menu right-aligns to the
 * trigger — these live at the window's right edge where a left-aligned
 * panel would overflow. */
export function ActionMenu({ items, ariaLabel, tip, disabled, className = "", children }: Props) {
  const [open, setOpen] = useState(false);
  const btnRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ right: number; top: number } | null>(null);

  useEffect(() => {
    if (!open) return;
    const place = () => {
      const r = btnRef.current?.getBoundingClientRect();
      if (r) setPos({ right: window.innerWidth - r.right, top: r.bottom + 4 });
    };
    place();
    const onDown = (e: MouseEvent) => {
      const t = e.target as Node;
      if (btnRef.current?.contains(t) || menuRef.current?.contains(t)) return;
      setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    window.addEventListener("resize", place);
    window.addEventListener("scroll", place, true);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
      window.removeEventListener("resize", place);
      window.removeEventListener("scroll", place, true);
    };
  }, [open]);

  return (
    <>
      <button
        ref={btnRef}
        type="button"
        aria-label={ariaLabel}
        aria-haspopup="menu"
        aria-expanded={open}
        disabled={disabled}
        data-tip={tip}
        onClick={() => setOpen((o) => !o)}
        className={className}
      >
        {children}
      </button>

      {open &&
        pos &&
        createPortal(
          <div
            ref={menuRef}
            role="menu"
            // Same stacking rule as Select: the portal must clear the
            // filters drawer (z-index: 2000).
            className="fixed z-[3000] w-max max-w-[360px] rounded-lg border border-border bg-card py-1 shadow-xl"
            style={{ right: pos.right, top: pos.top }}
          >
            {items.map((item) => (
              <button
                key={item.label}
                type="button"
                role="menuitem"
                onClick={() => {
                  setOpen(false);
                  item.onSelect();
                }}
                className="flex w-full flex-col items-start px-3 py-1.5 text-left text-sm text-ink hover:bg-card-2"
              >
                <span>{item.label}</span>
                {item.hint && <span className="text-xs text-faint">{item.hint}</span>}
              </button>
            ))}
          </div>,
          document.body,
        )}
    </>
  );
}
