import { useRef, useState, useEffect } from "react";
import { useQuery } from "@tanstack/react-query";
import { Calendar, ChevronLeft, ChevronRight } from "lucide-react";
import { api } from "../../lib/tauri";
import { buildMonthGrid } from "../../lib/calendar";
import { useToday } from "../../hooks/useToday";
import { Select } from "./Select";

const pad = (n: number) => String(n).padStart(2, "0");
const isoOf = (y: number, m: number, d: number) => `${y}-${pad(m + 1)}-${pad(d)}`;

/** Calendar-popover date field (no manual text entry), per the design.
 * The popover opens upward by default (the filter drawer sits at the
 * bottom of the screen); `drop="down"` for a field with room below it,
 * such as one in a modal. The year dropdown spans the activity years in
 * the vault, or `yearSpan` when the caller knows better (a purchase date
 * can predate the first import). */
export function DateField({
  label,
  value,
  onChange,
  min,
  max,
  align,
  drop,
  yearSpan,
  placeholder = "Any",
}: {
  label: string;
  value: string | undefined;
  onChange: (iso: string | undefined) => void;
  min?: string;
  max?: string;
  align?: "right";
  drop?: "down";
  yearSpan?: [number, number];
  placeholder?: string;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  // Live day, so the picker's "today" mark survives midnight.
  const today = useToday();
  const todayIso = isoOf(today.getFullYear(), today.getMonth(), today.getDate());
  const sel = value ? new Date(value + "T00:00:00") : null;
  const [view, setView] = useState(() => {
    const b = sel ?? today;
    return { y: b.getFullYear(), m: b.getMonth() };
  });

  useEffect(() => {
    if (!open) return;
    const onDoc = (e: MouseEvent) => {
      const t = e.target as Element;
      // The year Select portals its menu to <body> (outside our ref) — a
      // click on a year must not count as "outside" and close the popover.
      if (t.closest('[role="listbox"]')) return;
      if (ref.current && !ref.current.contains(t)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", onDoc);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDoc);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  const toggle = () => {
    const b = value ? new Date(value + "T00:00:00") : today;
    if (!open) setView({ y: b.getFullYear(), m: b.getMonth() });
    setOpen((o) => !o);
  };

  // 6 weeks always: a popup that changes height between 5- and 6-week months
  // jumps under the cursor while flipping months.
  const cells = buildMonthGrid(view.y, view.m + 1, 6); // view.m is 0-based
  const monthOptions = Array.from({ length: 12 }, (_, m) => ({
    value: String(m),
    label: new Date(2000, m, 1).toLocaleDateString("en-US", { month: "long" }),
  }));
  // Year dropdown: reaching an old import year via the month chevrons takes
  // 12 clicks per year. Only years that actually contain activities — keyed
  // under ["activities"] so imports/deletes refresh it like everything else.
  const { data: activityYears } = useQuery({
    queryKey: ["activities", "year-range"],
    queryFn: () => api.getActivityYearRange(),
    enabled: !yearSpan,
  });
  const yearRange = yearSpan ?? activityYears;
  const maxYear = Math.max(yearRange?.[1] ?? today.getFullYear(), view.y);
  const minYear = Math.min(yearRange?.[0] ?? today.getFullYear(), view.y);
  const yearOptions = Array.from({ length: maxYear - minYear + 1 }, (_, i) => {
    const y = String(maxYear - i);
    return { value: y, label: y };
  });
  const dow = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];
  const shift = (n: number) =>
    setView((p) => {
      let m = p.m + n;
      let y = p.y;
      if (m < 0) { m = 11; y--; }
      if (m > 11) { m = 0; y++; }
      return { y, m };
    });
  const display = sel
    ? sel.toLocaleDateString("en-GB", { day: "2-digit", month: "short", year: "numeric" })
    : "";
  const isDisabled = (iso: string) => (!!min && iso < min) || (!!max && iso > max);

  return (
    <div
      className={`dp${align === "right" ? " dp-right" : ""}${drop === "down" ? " dp-down" : ""}${open ? " open" : ""}`}
      ref={ref}
    >
      <button type="button" className={`dp-field${sel ? " has" : ""}`} onClick={toggle}>
        <span className="dp-ftop">
          <span className="dp-flabel">{label}</span>
          <Calendar size={13} />
        </span>
        <span className="dp-fval">{display || placeholder}</span>
      </button>
      {open && (
        <div className="dp-pop">
          <div className="dp-head">
            <div className="dp-month">
              <Select
                compact
                value={String(view.m)}
                options={monthOptions}
                onChange={(m) => setView((p) => ({ ...p, m: Number(m) }))}
                ariaLabel="Month"
              />
              <Select
                compact
                value={String(view.y)}
                options={yearOptions}
                onChange={(y) => setView((p) => ({ ...p, y: Number(y) }))}
                ariaLabel="Year"
              />
            </div>
            <div className="dp-nav">
              <button type="button" onClick={() => shift(-1)} aria-label="Previous month">
                <ChevronLeft size={16} />
              </button>
              <button type="button" onClick={() => shift(1)} aria-label="Next month">
                <ChevronRight size={16} />
              </button>
            </div>
          </div>
          <div className="dp-grid">
            {dow.map((w) => (
              <div className="dp-dow" key={w}>
                {w}
              </div>
            ))}
            {cells.map((d, i) => {
              // dp-empty keeps the day-cell aspect ratio — a bare div would
              // collapse an all-blank 6th row to zero height.
              if (d === null) return <div className="dp-empty" key={i} />;
              const iso = isoOf(view.y, view.m, d);
              return (
                <button
                  type="button"
                  key={i}
                  disabled={isDisabled(iso)}
                  className={`dp-day${iso === value ? " sel" : ""}${iso === todayIso ? " today" : ""}`}
                  onClick={() => {
                    onChange(iso);
                    setOpen(false);
                  }}
                >
                  {d}
                </button>
              );
            })}
          </div>
          <button
            type="button"
            className="dp-foot"
            disabled={!value}
            onClick={() => {
              onChange(undefined);
              setOpen(false);
            }}
          >
            Clear
          </button>
        </div>
      )}
    </div>
  );
}
