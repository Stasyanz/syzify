import { useRef, useState, useEffect } from "react";
import { useQuery } from "@tanstack/react-query";
import {
  X,
  Search,
  ArrowUpNarrowWide,
  ArrowDownNarrowWide,
  Calendar,
  ChevronLeft,
  ChevronRight,
} from "lucide-react";
import { api } from "../../lib/tauri";
import { useActivityStore } from "../../stores/activityStore";
import { SPORT_LABELS, SPORT_TYPES, type ActivityFilters } from "../../lib/types";
import { SportIcon } from "../brand/SportIcon";
import { buildMonthGrid } from "../../lib/calendar";
import { Select } from "../ui/Select";
import {
  useUnits,
  distanceUnit,
  elevationUnit,
  M_PER_MILE,
  FT_PER_M,
} from "../../lib/units";

/** Number of active filter facets — drives the navbar badge. */
export function countActiveFilters(f: ActivityFilters): number {
  let n = 0;
  if (f.search && f.search.trim()) n++;
  if (f.sport_types && f.sport_types.length) n++;
  if (f.tag_ids && f.tag_ids.length) n++;
  if (f.date_from || f.date_to) n++;
  if (f.distance_min != null || f.distance_max != null) n++;
  if (f.duration_min != null || f.duration_max != null) n++;
  if (f.elev_gain_min != null || f.elev_gain_max != null) n++;
  if (f.has_gps != null) n++;
  return n;
}

const pad = (n: number) => String(n).padStart(2, "0");
const isoOf = (y: number, m: number, d: number) => `${y}-${pad(m + 1)}-${pad(d)}`;

/** Calendar-popover date field (no manual text entry), per the design. */
function DateField({
  label,
  value,
  onChange,
  min,
  max,
  align,
}: {
  label: string;
  value: string | undefined;
  onChange: (iso: string | undefined) => void;
  min?: string;
  max?: string;
  align?: "right";
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const today = new Date();
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
  const { data: yearRange } = useQuery({
    queryKey: ["activities", "year-range"],
    queryFn: () => api.getActivityYearRange(),
  });
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
    <div className={`dp${align === "right" ? " dp-right" : ""}${open ? " open" : ""}`} ref={ref}>
      <button type="button" className={`dp-field${sel ? " has" : ""}`} onClick={toggle}>
        <span className="dp-ftop">
          <span className="dp-flabel">{label}</span>
          <Calendar size={13} />
        </span>
        <span className="dp-fval">{display || "Any"}</span>
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

function RangeField({
  minValue,
  maxValue,
  onMinChange,
  onMaxChange,
  placeholderMin = "0",
  placeholderMax = "∞",
}: {
  minValue: number | undefined;
  maxValue: number | undefined;
  onMinChange: (v: number | undefined) => void;
  onMaxChange: (v: number | undefined) => void;
  placeholderMin?: string;
  placeholderMax?: string;
}) {
  return (
    <div className="field">
      <input
        type="number"
        value={minValue ?? ""}
        onChange={(e) => onMinChange(e.target.value ? Number(e.target.value) : undefined)}
        placeholder={placeholderMin}
        className="[appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none"
      />
      <span className="text-faint self-center">—</span>
      <input
        type="number"
        value={maxValue ?? ""}
        onChange={(e) => onMaxChange(e.target.value ? Number(e.target.value) : undefined)}
        placeholder={placeholderMax}
        className="[appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none"
      />
    </div>
  );
}

export function FilterDrawer() {
  const filters = useActivityStore((s) => s.filters);
  const setFilters = useActivityStore((s) => s.setFilters);
  const resetFilters = useActivityStore((s) => s.resetFilters);
  const viewMode = useActivityStore((s) => s.viewMode);
  const setFiltersOpen = useActivityStore((s) => s.setFiltersOpen);
  // Subscribe so an always-mounted drawer relabels/reconverts on units change.
  const imperial = useUnits() === "imperial";
  const mPerDist = imperial ? M_PER_MILE : 1000;

  const [closing, setClosing] = useState(false);
  const close = () => {
    setClosing(true);
    setTimeout(() => {
      setClosing(false);
      setFiltersOpen(false);
    }, 95);
  };

  const { data: allTags = [] } = useQuery({ queryKey: ["tags"], queryFn: () => api.getTags() });
  const { data: usedSports = [] } = useQuery({
    queryKey: ["usedSportTypes"],
    queryFn: () => api.getUsedSportTypes(),
  });
  // Only show sports actually present in the library, alphabetically by label.
  const usedSet = new Set(usedSports);
  const shownSports = SPORT_TYPES.filter((st) => usedSet.has(st)).sort((a, b) =>
    SPORT_LABELS[a].localeCompare(SPORT_LABELS[b]),
  );
  const activeTagIds = filters.tag_ids ?? [];
  const activeCount = countActiveFilters(filters);

  function toggleTagFilter(id: number) {
    const next = activeTagIds.includes(id)
      ? activeTagIds.filter((t) => t !== id)
      : [...activeTagIds, id];
    setFilters({ tag_ids: next.length > 0 ? next : undefined });
  }

  return (
    <>
      <div className="filters-backdrop" onClick={close} />
      <div className={`filters${closing ? " closing" : ""}`}>
        <div className="filters-head">
          <span style={{ fontSize: 13, fontWeight: 700 }}>Filters</span>
          <span className="filters-x" onClick={close} title="Close">
            <X size={16} />
          </span>
        </div>

        <div className="filters-body scroll-themed">
          {/* Sort (list only) — first: at the drawer's bottom its dropdown
              had no room to open. */}
          {viewMode === "list" && (
            <div className="fgroup">
              <div className="fh">Sort by</div>
              <div className="field">
                <Select
                  ariaLabel="Sort by"
                  className="flex-1"
                  value={filters.sort_by ?? "date"}
                  onChange={(v) => setFilters({ sort_by: v })}
                  options={[
                    { value: "date", label: "Date" },
                    { value: "distance", label: "Distance" },
                    { value: "duration", label: "Duration" },
                    { value: "elevation", label: "Elevation" },
                  ]}
                />
                <button
                  onClick={() => setFilters({ sort_dir: filters.sort_dir === "asc" ? "desc" : "asc" })}
                  className="grid place-items-center w-8 h-8 border border-border-2 rounded-[7px] bg-card text-muted hover:text-ink"
                  title={filters.sort_dir === "asc" ? "Ascending" : "Descending"}
                >
                  {filters.sort_dir === "asc" ? (
                    <ArrowUpNarrowWide size={16} />
                  ) : (
                    <ArrowDownNarrowWide size={16} />
                  )}
                </button>
              </div>
            </div>
          )}

          {/* Search — free-text over title / notes / location */}
          <div className="fgroup">
            <div className="fsearch">
              <Search size={15} />
              <input
                type="text"
                value={filters.search ?? ""}
                onChange={(e) => setFilters({ search: e.target.value || undefined })}
                placeholder="Search workouts…"
              />
              {filters.search && (
                <span
                  className="fsearch-x"
                  title="Clear search"
                  onClick={() => setFilters({ search: undefined })}
                >
                  <X size={14} />
                </span>
              )}
            </div>
          </div>

          {/* Sport */}
          <div className="fgroup">
            <div className="fh">Sport</div>
            <Select
              multiple
              ariaLabel="Sport"
              className="w-full"
              values={filters.sport_types ?? []}
              onChange={(vs) => setFilters({ sport_types: vs.length ? vs : undefined })}
              placeholder="All sports"
              clearLabel="All sports"
              options={shownSports.map((st) => ({
                value: st,
                label: SPORT_LABELS[st],
                icon: <SportIcon sport={st} size={18} />,
              }))}
            />
          </div>

          {/* GPS track */}
          <div className="fgroup">
            <div className="fh">GPS track</div>
            <div className="seg">
              <button
                onClick={() => setFilters({ has_gps: undefined })}
                className={filters.has_gps == null ? "on" : ""}
              >
                Any
              </button>
              <button
                onClick={() => setFilters({ has_gps: true })}
                className={filters.has_gps === true ? "on" : ""}
              >
                With
              </button>
              <button
                onClick={() => setFilters({ has_gps: false })}
                className={filters.has_gps === false ? "on" : ""}
              >
                Without
              </button>
            </div>
          </div>

          {/* Tags */}
          {allTags.length > 0 && (
            <div className="fgroup">
              <div className="fh">Tags</div>
              <div className="chips">
                {allTags.map((tag) => (
                  <span
                    key={tag.id}
                    className={`chip tag${activeTagIds.includes(tag.id) ? " on" : ""}`}
                    onClick={() => toggleTagFilter(tag.id)}
                  >
                    {tag.name}
                  </span>
                ))}
              </div>
            </div>
          )}

          {/* Distance */}
          <div className="fgroup">
            <div className="fh">Distance · {distanceUnit()}</div>
            <RangeField
              // Round the derived display value (3.1068… → 3.1); the exact
              // stored metres are untouched unless the field is edited.
              minValue={
                filters.distance_min != null
                  ? Math.round((filters.distance_min / mPerDist) * 10) / 10
                  : undefined
              }
              maxValue={
                filters.distance_max != null
                  ? Math.round((filters.distance_max / mPerDist) * 10) / 10
                  : undefined
              }
              onMinChange={(v) => setFilters({ distance_min: v != null ? v * mPerDist : undefined })}
              onMaxChange={(v) => setFilters({ distance_max: v != null ? v * mPerDist : undefined })}
            />
          </div>

          {/* Duration */}
          <div className="fgroup">
            <div className="fh">Duration · min</div>
            <RangeField
              minValue={filters.duration_min != null ? filters.duration_min / 60 : undefined}
              maxValue={filters.duration_max != null ? filters.duration_max / 60 : undefined}
              onMinChange={(v) => setFilters({ duration_min: v != null ? v * 60 : undefined })}
              onMaxChange={(v) => setFilters({ duration_max: v != null ? v * 60 : undefined })}
            />
          </div>

          {/* Elevation */}
          <div className="fgroup">
            <div className="fh">Elevation · {elevationUnit()}</div>
            <RangeField
              minValue={
                filters.elev_gain_min != null
                  ? imperial
                    ? Math.round(filters.elev_gain_min * FT_PER_M)
                    : filters.elev_gain_min
                  : undefined
              }
              maxValue={
                filters.elev_gain_max != null
                  ? imperial
                    ? Math.round(filters.elev_gain_max * FT_PER_M)
                    : filters.elev_gain_max
                  : undefined
              }
              onMinChange={(v) =>
                setFilters({ elev_gain_min: v != null && imperial ? v / FT_PER_M : v })
              }
              onMaxChange={(v) =>
                setFilters({ elev_gain_max: v != null && imperial ? v / FT_PER_M : v })
              }
            />
          </div>

          {/* Date range */}
          <div className="fgroup">
            <div className="fh">Date range</div>
            <div className="fdates">
              <DateField
                label="From"
                value={filters.date_from}
                max={filters.date_to}
                onChange={(v) => setFilters({ date_from: v })}
              />
              <DateField
                label="To"
                align="right"
                value={filters.date_to}
                min={filters.date_from}
                onChange={(v) => setFilters({ date_to: v })}
              />
            </div>
          </div>
        </div>

        <div className="filters-foot">
          <button className="filters-reset" onClick={resetFilters} disabled={activeCount === 0}>
            Reset filters
          </button>
        </div>
      </div>
    </>
  );
}
