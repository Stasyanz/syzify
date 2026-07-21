import { useEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { ChevronDown, Check } from "lucide-react";

export interface SelectOption {
  value: string;
  label: string;
  /** Optional leading visual (e.g. a sport glyph) shown before the label. */
  icon?: ReactNode;
}

interface BaseProps {
  options: SelectOption[];
  /** Extra classes for the trigger button (e.g. width). */
  className?: string;
  ariaLabel?: string;
  /** Small inline trigger (e.g. the date picker's year) instead of the
   * full form-field look. */
  compact?: boolean;
}

interface SingleProps extends BaseProps {
  multiple?: false;
  value: string;
  onChange: (value: string) => void;
}

interface MultiProps extends BaseProps {
  /** Checkbox-style selection: option clicks toggle and keep the menu open. */
  multiple: true;
  values: string[];
  onChange: (values: string[]) => void;
  /** Trigger text while nothing is selected (= no filtering). */
  placeholder: string;
  /** Optional first row that clears the selection ("All sports"); it shows
   * the check while nothing is selected and closes the menu. */
  clearLabel?: string;
}

type Props = SingleProps | MultiProps;

/** App-styled dropdown: a custom trigger + portal menu so the option list
 * matches the app's theme (a native <select> list is OS-rendered and can't be
 * styled). The menu renders to <body> so it can't be clipped by a scrolling
 * drawer or modal. */
export function Select(props: Props) {
  const { options, className = "", ariaLabel, compact = false } = props;
  const [open, setOpen] = useState(false);
  const btnRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ left: number; top: number; width: number } | null>(null);

  const triggerLabel = props.multiple
    ? options
        .filter((o) => props.values.includes(o.value))
        .map((o) => o.label)
        .join(", ") || props.placeholder
    : options.find((o) => o.value === props.value)?.label ?? "";

  const isSelected = (o: SelectOption) =>
    props.multiple ? props.values.includes(o.value) : o.value === props.value;

  const pick = (o: SelectOption) => {
    if (props.multiple) {
      // Toggle, keeping the options' own order; the menu stays open so
      // several sports can be ticked in one visit.
      const next = new Set(props.values);
      if (next.has(o.value)) next.delete(o.value);
      else next.add(o.value);
      props.onChange(options.filter((op) => next.has(op.value)).map((op) => op.value));
    } else {
      props.onChange(o.value);
      setOpen(false);
    }
  };

  useEffect(() => {
    if (!open) return;
    const place = () => {
      const r = btnRef.current?.getBoundingClientRect();
      if (r) setPos({ left: r.left, top: r.bottom + 4, width: r.width });
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
        aria-haspopup="listbox"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
        className={`flex items-center justify-between border border-border bg-card cursor-pointer ${
          compact
            ? "gap-1 rounded-[7px] px-1.5 py-0.5 text-[12.5px] font-bold"
            : "gap-2 rounded px-3 py-2 text-sm"
        } ${className}`}
      >
        <span className="truncate">{triggerLabel}</span>
        <ChevronDown size={compact ? 13 : 16} className="shrink-0 text-faint" />
      </button>

      {open &&
        pos &&
        createPortal(
          <div
            ref={menuRef}
            role="listbox"
            aria-multiselectable={props.multiple || undefined}
            // z-index must clear the filters drawer (z-index: 2000) since the
            // menu portals to <body>; otherwise it renders behind the panel.
            className="fixed z-[3000] max-h-64 w-max overflow-auto rounded-lg border border-border bg-card py-1 shadow-xl"
            // At least as wide as the trigger, then grow to fit the content:
            // a fixed trigger width truncated the selected row (label + check
            // icon) of narrow compact triggers like the year picker.
            style={{ left: pos.left, top: pos.top, minWidth: pos.width, maxWidth: 360 }}
          >
            {props.multiple && props.clearLabel && (
              <button
                type="button"
                role="option"
                aria-selected={props.values.length === 0}
                onClick={() => {
                  props.onChange([]);
                  setOpen(false);
                }}
                className={`flex w-full items-center justify-between gap-2 px-3 py-1.5 text-left text-sm ${
                  props.values.length === 0
                    ? "bg-accent-soft text-accent-2"
                    : "text-ink hover:bg-card-2"
                }`}
              >
                <span className="truncate">{props.clearLabel}</span>
                {props.values.length === 0 && <Check size={14} className="shrink-0" />}
              </button>
            )}
            {options.map((o) => {
              const isSel = isSelected(o);
              return (
                <button
                  key={o.value}
                  type="button"
                  role="option"
                  aria-selected={isSel}
                  onClick={() => pick(o)}
                  className={`flex w-full items-center justify-between gap-2 px-3 py-1.5 text-left text-sm ${
                    isSel ? "bg-accent-soft text-accent-2" : "text-ink hover:bg-card-2"
                  }`}
                >
                  <span className="flex min-w-0 items-center gap-2">
                    {o.icon}
                    <span className="truncate">{o.label}</span>
                  </span>
                  {isSel && <Check size={14} className="shrink-0" />}
                </button>
              );
            })}
          </div>,
          document.body,
        )}
    </>
  );
}
