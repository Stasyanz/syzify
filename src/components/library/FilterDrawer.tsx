import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { X, Search, ArrowUpNarrowWide, ArrowDownNarrowWide } from "lucide-react";
import { api } from "../../lib/tauri";
import { useActivityStore } from "../../stores/activityStore";
import { SPORT_LABELS, SPORT_TYPES, type ActivityFilters } from "../../lib/types";
import { SportIcon } from "../brand/SportIcon";
import { Select } from "../ui/Select";
import { DateField } from "../ui/DateField";
import { DeviceSilhouette } from "../brand/DeviceSilhouette";
import { GearKindIcon } from "../activity/GearChip";
import { useToastStore } from "../../stores/toastStore";
import { confirmDialog } from "../../stores/confirmStore";
import { invalidateActivityData } from "../../lib/activityInvalidation";
import { groupDevices, devicesForKeys, selectedDeviceKeys } from "../../lib/deviceFilter";
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
  if (f.devices && f.devices.length) n++;
  if (f.gear_ids && f.gear_ids.length) n++;
  if (f.date_from || f.date_to) n++;
  if (f.distance_min != null || f.distance_max != null) n++;
  if (f.duration_min != null || f.duration_max != null) n++;
  if (f.elev_gain_min != null || f.elev_gain_max != null) n++;
  if (f.has_gps != null) n++;
  return n;
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

  const { data: usedSports = [] } = useQuery({
    queryKey: ["usedSportTypes"],
    queryFn: () => api.getUsedSportTypes(),
  });
  // Only show sports actually present in the library, alphabetically by label.
  const usedSet = new Set(usedSports);
  const shownSports = SPORT_TYPES.filter((st) => usedSet.has(st)).sort((a, b) =>
    SPORT_LABELS[a].localeCompare(SPORT_LABELS[b]),
  );
  const { data: detectedDevices = [] } = useQuery({
    queryKey: ["detectedDevices"],
    queryFn: () => api.getDetectedDevices(),
  });
  // One option per display label: several raw strings can name the same
  // model ("Garmin fenix6x" and "Garmin fenix6x_asia"), and the "no device"
  // group is offered like any other.
  const deviceGroups = groupDevices(detectedDevices);
  const activeCount = countActiveFilters(filters);

  // Gear (ADR 0003): a facet over the items, retired ones included (the
  // history sits on them), plus "No gear"; and the bulk action that puts
  // every activity the filters show on one item in use.
  const { data: gearItems = [] } = useQuery({ queryKey: ["gear"], queryFn: () => api.listGear() });
  const gearInUse = gearItems.filter((g) => g.retired_at == null);
  const [bulkGear, setBulkGear] = useState("");
  const queryClient = useQueryClient();
  const addToast = useToastStore((s) => s.addToast);
  const bulkAssign = useMutation({
    mutationFn: async () => {
      const item = gearInUse.find((g) => g.id === bulkGear);
      // Retired or deleted since it was picked (the registry refreshed).
      if (!item) throw new Error("That gear is no longer available — pick another");
      // Sort and paging are the list's; the backend ignores them, but the
      // confirmation counts the same set — what would change, not what
      // the list shows (the ones already on the item are not counted).
      const targets = await api.countGearTargets(filters, item.id);
      if (targets.eligible === 0) return { item, n: 0, none: true };
      const moved = targets.moved_from.length
        ? ` ${targets.moved_from.map((m) => `${m.count} from ${m.name}`).join(", ")} will be moved.`
        : "";
      const ok = await confirmDialog({
        title: `Put ${targets.eligible} activit${targets.eligible === 1 ? "y" : "ies"} on ${item.name}?`,
        message: `Every activity matching the filters — all dates unless filtered — that is not on ${item.name} yet; multisport events are left out.${moved}`,
        confirmLabel: "Assign",
      });
      if (!ok) return null;
      return { item, n: await api.assignGearToFiltered(filters, item.id), none: false };
    },
    onSuccess: (done) => {
      if (!done) return;
      if (done.none) {
        addToast("info", `Nothing to assign: every matching activity is on ${done.item.name} already, or cannot carry gear`);
        return;
      }
      addToast("success", `${done.item.name} assigned to ${done.n} activit${done.n === 1 ? "y" : "ies"}`);
      invalidateActivityData(queryClient);
    },
    onError: (e: unknown) => {
      addToast("error", `Could not assign gear: ${e instanceof Error ? e.message : String(e)}`);
      // A pick the registry no longer has must not stay selected.
      if (!gearInUse.some((g) => g.id === bulkGear)) setBulkGear("");
    },
  });

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

          {/* Device */}
          {deviceGroups.length > 1 && (
            <div className="fgroup">
              <div className="fh">Device</div>
              <Select
                multiple
                ariaLabel="Device"
                className="w-full"
                values={selectedDeviceKeys(deviceGroups, filters.devices ?? [])}
                onChange={(keys) => {
                  const raws = devicesForKeys(deviceGroups, keys);
                  setFilters({ devices: raws.length ? raws : undefined });
                }}
                placeholder="All devices"
                clearLabel="All devices"
                options={deviceGroups.map((g) => ({
                  value: g.key,
                  label: `${g.label} (${g.count})`,
                  icon: <DeviceSilhouette form={g.form} size={18} />,
                }))}
              />
            </div>
          )}

          {/* Gear (ADR 0003) */}
          {gearItems.length > 0 && (
            <div className="fgroup">
              <div className="fh">Gear</div>
              <Select
                multiple
                ariaLabel="Gear"
                className="w-full"
                values={filters.gear_ids ?? []}
                onChange={(ids) => setFilters({ gear_ids: ids.length ? ids : undefined })}
                placeholder="All gear"
                clearLabel="All gear"
                options={[
                  ...gearItems.map((g) => ({
                    value: g.id,
                    label: `${g.name}${g.retired_at ? " (retired)" : ""} (${g.stats.activities})`,
                    icon: <GearKindIcon kind={g.kind} size={18} />,
                  })),
                  { value: "", label: "No gear" },
                ]}
              />
              {/* The bulk action, once a filter narrows the library: put
                  what the LIST shows on one item. List only: the calendar
                  adds its month and the map drops what has no track, so
                  there "what you see" and the filters part ways. */}
              {viewMode === "list" && activeCount > 0 && gearInUse.length > 0 && (
                <div className="mt-2 flex items-center gap-2" data-testid="bulk-gear">
                  <Select
                    ariaLabel="Put the filtered activities on"
                    className="min-w-0 flex-1"
                    value={bulkGear}
                    onChange={setBulkGear}
                    options={[
                      { value: "", label: "Put them on…" },
                      ...gearInUse.map((g) => ({ value: g.id, label: g.name, icon: <GearKindIcon kind={g.kind} size={16} /> })),
                    ]}
                  />
                  <button
                    type="button"
                    className="btn primary !px-2.5 !py-1 !text-xs shrink-0"
                    disabled={!bulkGear || bulkAssign.isPending}
                    onClick={() => bulkAssign.mutate()}
                  >
                    {bulkAssign.isPending ? "Assigning…" : "Assign"}
                  </button>
                </div>
              )}
            </div>
          )}

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
