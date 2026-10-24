import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { X, Search, ArrowUpNarrowWide, ArrowDownNarrowWide } from "lucide-react";
import { api } from "../../lib/tauri";
import { useActivityStore } from "../../stores/activityStore";
import { sportLabel, SPORT_TYPES, type ActivityFilters } from "../../lib/types";
import { formatNumber, useLanguageStore, useT } from "../../lib/i18n";
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
  unitLabel,
  M_PER_MILE,
  FT_PER_M,
} from "../../lib/units";
import { errorText } from "../../lib/errors";

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
  const t = useT();
  const language = useLanguageStore((s) => s.language);

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
  // Only show sports actually present in the library, alphabetically by
  // label in the interface language.
  const usedSet = new Set(usedSports);
  const shownSports = SPORT_TYPES.filter((st) => usedSet.has(st)).sort((a, b) =>
    sportLabel(a).localeCompare(sportLabel(b), language),
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
      if (!item) throw new Error(t("library.filter.bulkGone"));
      // Sort and paging are the list's; the backend ignores them, but the
      // confirmation counts the same set — what would change, not what
      // the list shows (the ones already on the item are not counted).
      const targets = await api.countGearTargets(filters, item.id);
      if (targets.eligible === 0) return { item, n: 0, none: true };
      const movedFrom = targets.moved_from.map((m) =>
        t("library.filter.bulkMovedFrom", { count: m.count, name: m.name }),
      );
      const moved = movedFrom.length ? ` ${t("library.filter.bulkMoved", { list: movedFrom.join(", ") })}` : "";
      const ok = await confirmDialog({
        title: t("library.filter.bulkTitle", { count: targets.eligible, name: item.name }),
        message: t("library.filter.bulkMessage", { name: item.name }) + moved,
        confirmLabel: t("library.filter.assign"),
      });
      if (!ok) return null;
      return { item, n: await api.assignGearToFiltered(filters, item.id), none: false };
    },
    onSuccess: (done) => {
      if (!done) return;
      if (done.none) {
        addToast("info", t("library.filter.bulkNone", { name: done.item.name }));
        return;
      }
      addToast("success", t("library.filter.bulkDone", { count: done.n, name: done.item.name }));
      invalidateActivityData(queryClient);
    },
    onError: (e: unknown) => {
      addToast("error", t("library.filter.bulkFailed", { error: errorText(e) }));
      // A pick the registry no longer has must not stay selected.
      if (!gearInUse.some((g) => g.id === bulkGear)) setBulkGear("");
    },
  });

  return (
    <>
      <div className="filters-backdrop" onClick={close} />
      <div className={`filters${closing ? " closing" : ""}`}>
        <div className="filters-head">
          <span style={{ fontSize: 13, fontWeight: 700 }}>{t("library.filter.title")}</span>
          <span className="filters-x" onClick={close} title={t("common.close")}>
            <X size={16} />
          </span>
        </div>

        <div className="filters-body scroll-themed">
          {/* Sort (list only) — first: at the drawer's bottom its dropdown
              had no room to open. */}
          {viewMode === "list" && (
            <div className="fgroup">
              <div className="fh">{t("library.filter.sortBy")}</div>
              <div className="field">
                <Select
                  ariaLabel={t("library.filter.sortBy")}
                  className="flex-1"
                  value={filters.sort_by ?? "date"}
                  onChange={(v) => setFilters({ sort_by: v })}
                  options={[
                    { value: "date", label: t("library.filter.sortDate") },
                    { value: "distance", label: t("common.distance") },
                    { value: "duration", label: t("common.duration") },
                    { value: "elevation", label: t("common.elevation") },
                  ]}
                />
                <button
                  onClick={() => setFilters({ sort_dir: filters.sort_dir === "asc" ? "desc" : "asc" })}
                  className="grid place-items-center w-8 h-8 border border-border-2 rounded-[7px] bg-card text-muted hover:text-ink"
                  title={filters.sort_dir === "asc" ? t("library.filter.ascending") : t("library.filter.descending")}
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
                placeholder={t("library.filter.search")}
              />
              {filters.search && (
                <span
                  className="fsearch-x"
                  title={t("library.filter.clearSearch")}
                  onClick={() => setFilters({ search: undefined })}
                >
                  <X size={14} />
                </span>
              )}
            </div>
          </div>

          {/* Sport */}
          <div className="fgroup">
            <div className="fh">{t("library.filter.sport")}</div>
            <Select
              multiple
              ariaLabel={t("library.filter.sport")}
              className="w-full"
              values={filters.sport_types ?? []}
              onChange={(vs) => setFilters({ sport_types: vs.length ? vs : undefined })}
              placeholder={t("library.filter.allSports")}
              clearLabel={t("library.filter.allSports")}
              options={shownSports.map((st) => ({
                value: st,
                label: sportLabel(st),
                icon: <SportIcon sport={st} size={18} />,
              }))}
            />
          </div>

          {/* Device */}
          {deviceGroups.length > 1 && (
            <div className="fgroup">
              <div className="fh">{t("library.filter.device")}</div>
              <Select
                multiple
                ariaLabel={t("library.filter.device")}
                className="w-full"
                values={selectedDeviceKeys(deviceGroups, filters.devices ?? [])}
                onChange={(keys) => {
                  const raws = devicesForKeys(deviceGroups, keys);
                  setFilters({ devices: raws.length ? raws : undefined });
                }}
                placeholder={t("library.filter.allDevices")}
                clearLabel={t("library.filter.allDevices")}
                options={deviceGroups.map((g) => ({
                  value: g.key,
                  label: `${g.label} (${formatNumber(g.count, 0)})`,
                  icon: <DeviceSilhouette form={g.form} size={18} />,
                }))}
              />
            </div>
          )}

          {/* Gear (ADR 0003) */}
          {gearItems.length > 0 && (
            <div className="fgroup">
              <div className="fh">{t("library.filter.gear")}</div>
              <Select
                multiple
                ariaLabel={t("library.filter.gear")}
                className="w-full"
                values={filters.gear_ids ?? []}
                onChange={(ids) => setFilters({ gear_ids: ids.length ? ids : undefined })}
                placeholder={t("library.filter.allGear")}
                clearLabel={t("library.filter.allGear")}
                options={[
                  ...gearItems.map((g) => ({
                    value: g.id,
                    label: `${g.retired_at ? t("library.filter.retired", { name: g.name }) : g.name} (${formatNumber(g.stats.activities, 0)})`,
                    icon: <GearKindIcon kind={g.kind} size={18} />,
                  })),
                  { value: "", label: t("library.filter.noGear") },
                ]}
              />
              {/* The bulk action, once a filter narrows the library: put
                  what the LIST shows on one item. List only: the calendar
                  adds its month and the map drops what has no track, so
                  there "what you see" and the filters part ways. */}
              {viewMode === "list" && activeCount > 0 && gearInUse.length > 0 && (
                <div className="mt-2 flex items-center gap-2" data-testid="bulk-gear">
                  <Select
                    ariaLabel={t("library.filter.putOn")}
                    className="min-w-0 flex-1"
                    value={bulkGear}
                    onChange={setBulkGear}
                    options={[
                      { value: "", label: t("library.filter.putOnPlaceholder") },
                      ...gearInUse.map((g) => ({ value: g.id, label: g.name, icon: <GearKindIcon kind={g.kind} size={16} /> })),
                    ]}
                  />
                  <button
                    type="button"
                    className="btn primary !px-2.5 !py-1 !text-xs shrink-0"
                    disabled={!bulkGear || bulkAssign.isPending}
                    onClick={() => bulkAssign.mutate()}
                  >
                    {bulkAssign.isPending ? t("library.filter.assigning") : t("library.filter.assign")}
                  </button>
                </div>
              )}
            </div>
          )}

          {/* GPS track */}
          <div className="fgroup">
            <div className="fh">{t("library.filter.gps")}</div>
            <div className="seg">
              <button
                onClick={() => setFilters({ has_gps: undefined })}
                className={filters.has_gps == null ? "on" : ""}
              >
                {t("library.filter.gpsAny")}
              </button>
              <button
                onClick={() => setFilters({ has_gps: true })}
                className={filters.has_gps === true ? "on" : ""}
              >
                {t("library.filter.gpsWith")}
              </button>
              <button
                onClick={() => setFilters({ has_gps: false })}
                className={filters.has_gps === false ? "on" : ""}
              >
                {t("library.filter.gpsWithout")}
              </button>
            </div>
          </div>

          {/* Distance */}
          <div className="fgroup">
            <div className="fh">{t("library.filter.distance", { unit: unitLabel(distanceUnit()) })}</div>
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
            <div className="fh">{t("library.filter.duration", { unit: unitLabel("min") })}</div>
            <RangeField
              minValue={filters.duration_min != null ? filters.duration_min / 60 : undefined}
              maxValue={filters.duration_max != null ? filters.duration_max / 60 : undefined}
              onMinChange={(v) => setFilters({ duration_min: v != null ? v * 60 : undefined })}
              onMaxChange={(v) => setFilters({ duration_max: v != null ? v * 60 : undefined })}
            />
          </div>

          {/* Elevation */}
          <div className="fgroup">
            <div className="fh">{t("library.filter.elevation", { unit: unitLabel(elevationUnit()) })}</div>
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
            <div className="fh">{t("library.filter.dateRange")}</div>
            <div className="fdates">
              <DateField
                label={t("library.filter.from")}
                value={filters.date_from}
                max={filters.date_to}
                onChange={(v) => setFilters({ date_from: v })}
              />
              <DateField
                label={t("library.filter.to")}
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
            {t("library.filter.reset")}
          </button>
        </div>
      </div>
    </>
  );
}
