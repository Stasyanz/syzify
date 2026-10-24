import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { History, Pencil, Plus, Trash2, Wand2, X } from "lucide-react";
import { api } from "../../lib/tauri";
import {
  sportLabel,
  MAX_GEAR_NAME_LENGTH,
  type GearInput,
  type GearItem,
  type GearKind,
  type GearRule,
  type RuleCandidates,
  type SportType,
} from "../../lib/types";
import { kindSports, odometerM, rulesSummary, sensorLabel, wearFraction, wearState, wearTip, WEAR_COLORS } from "../../lib/gear";
import { invalidateActivityData } from "../../lib/activityInvalidation";
import { formatDistance, formatDurationHM, formatElevation } from "../../lib/format";
import { useUnits, isImperial, distanceUnit, unitLabel, M_PER_MILE } from "../../lib/units";
import { formatDateTime, formatNumber, useRichT, useT, type Translator } from "../../lib/i18n";
import type { MessageKey } from "../../locales/types";
import { useToastStore } from "../../stores/toastStore";
import { confirmDialog } from "../../stores/confirmStore";
import { Select } from "../ui/Select";
import { DateField } from "../ui/DateField";
import { GearKindIcon } from "../activity/GearChip";
import { useToday } from "../../hooks/useToday";
import { errorText } from "../../lib/errors";

/** How far back the purchase-date picker's year list reaches. */
export const PURCHASE_YEARS_BACK = 25;

/** The kinds, each with the key of its name. */
export const GEAR_KINDS = [
  { id: "bike", labelKey: "activity.gear.kind.bike" },
  { id: "shoes", labelKey: "activity.gear.kind.shoes" },
  { id: "other", labelKey: "settings.garage.kind.other" },
] as const satisfies readonly { id: GearKind; labelKey: MessageKey }[];

/** Sport names for a sentence of `translate`'s language: "Run, Ride". */
function sportList(sports: SportType[], translate: Translator): string {
  return sports.map((s) => sportLabel(s, s, translate)).join(", ");
}

export { kindSports, odometerM, wearFraction, wearTip };

/** What a distance field holds. `untouched` is the text the field opened
 * with and `original` the meters behind it: text left as it was gives
 * the original meters back, so a save that changed something else does
 * not re-round the mileage through the display unit. Text that is not a
 * number is `invalid` — never 0 or "no limit" by accident. */
export interface DistanceField {
  meters: number | null;
  invalid: boolean;
}
export function readDistanceField(text: string, untouched: string, original: number | null): DistanceField {
  if (text === untouched) return { meters: original, invalid: false };
  const t = text.trim();
  if (t === "") return { meters: null, invalid: false };
  const v = Number(t.replace(",", "."));
  if (!Number.isFinite(v) || v < 0) return { meters: null, invalid: true };
  return { meters: v * (isImperial() ? M_PER_MILE : 1000), invalid: false };
}

/** Meters as the number the distance input shows (display unit, two
 * decimals at most, no trailing zeros). */
export function distanceInputValue(meters: number | null): string {
  if (meters == null) return "";
  const v = meters / (isImperial() ? M_PER_MILE : 1000);
  return String(Math.round(v * 100) / 100);
}

function formatDay(iso: string): string {
  return formatDateTime(new Date(iso), { year: "numeric", month: "short", day: "numeric" });
}

/**
 * Settings → Garage: the bikes, shoes and other items activities are done
 * on, each with the mileage its activities add up to. Items in use come
 * first; retired ones keep their history at the bottom.
 */
export function Garage() {
  useUnits();
  const t = useT();
  const rt = useRichT();
  const queryClient = useQueryClient();
  const addToast = useToastStore((s) => s.addToast);
  const { data: items = [], isPending, error } = useQuery({ queryKey: ["gear"], queryFn: () => api.listGear() });
  // What the files carried, for the rules: named on the cards and offered
  // in the modal. Under the activity-derived prefixes so an import refreshes it.
  const { data: candidates } = useQuery({ queryKey: ["gear-rule-candidates"], queryFn: () => api.gearRuleCandidates() });
  const [editing, setEditing] = useState<GearItem | "new" | null>(null);
  const anyRules = items.some((g) => g.retired_at == null && g.rules.length > 0);

  // Put the rules over the history: every activity without gear that a
  // rule matches, multisport events left out.
  const applyRules = useMutation({
    mutationFn: async () => {
      const ok = await confirmDialog({
        title: t("settings.garage.applyTitle"),
        message: t("settings.garage.applyMessage"),
        confirmLabel: t("settings.garage.apply"),
      });
      if (!ok) return null;
      return api.applyGearRules();
    },
    onSuccess: (n) => {
      if (n == null) return;
      addToast(
        n > 0 ? "success" : "info",
        n > 0 ? t("settings.garage.applied", { count: n }) : t("settings.garage.applyNothing"),
      );
      if (n > 0) invalidateActivityData(queryClient);
    },
    onError: (e: unknown) => addToast("error", t("settings.garage.applyFailed", { error: errorText(e) })),
  });

  const changed = () => queryClient.invalidateQueries({ queryKey: ["gear"] });

  const retire = useMutation({
    mutationFn: ({ id, retired }: { id: string; retired: boolean }) => api.setGearRetired(id, retired),
    onSuccess: changed,
    onError: (e: unknown) => addToast("error", t("settings.garage.updateFailed", { error: errorText(e) })),
  });

  /** The sentence the quick action offers: the item's default sports,
   * from its purchase date on (all time without one) — the sport names in
   * the sentence's own language. */
  function historyOffer(item: GearItem): string {
    const sports = sportList(item.default_for, t);
    return item.purchased_at
      ? t("settings.garage.assignAllSince", { sports, date: formatDay(item.purchased_at + "T00:00:00") })
      : t("settings.garage.assignAll", { sports });
  }

  // A mutation, so a second press while one runs is inert (the backend
  // is idempotent anyway: only activities without gear are touched).
  const assignHistory = useMutation({
    mutationFn: async (item: GearItem) => {
      const ok = await confirmDialog({
        title: `${historyOffer(item)}?`,
        message: t("settings.garage.assignMessage"),
        confirmLabel: t("settings.garage.assign"),
      });
      if (!ok) return null;
      return { item, n: await api.assignGearHistory(item.id) };
    },
    onSuccess: (done) => {
      if (!done) return;
      const { item, n } = done;
      addToast(
        n > 0 ? "success" : "info",
        n > 0 ? t("library.filter.bulkDone", { name: item.name, count: n }) : t("settings.garage.assignNothing"),
      );
      // The activity pages and the library show the gear too; the Garage's
      // own totals are among the activity-derived queries this refreshes.
      invalidateActivityData(queryClient);
    },
    onError: (e: unknown) => addToast("error", t("library.filter.bulkFailed", { error: errorText(e) })),
  });

  async function remove(item: GearItem) {
    const n = item.stats.activities;
    const ok = await confirmDialog({
      title: t("settings.garage.deleteTitle", { name: item.name }),
      message: n > 0 ? t("settings.garage.deleteMessage", { count: n }) : t("common.cannotUndo"),
      confirmLabel: t("common.delete"),
      danger: true,
    });
    if (!ok) return;
    try {
      await api.deleteGear(item.id);
      changed();
    } catch (e) {
      addToast("error", t("settings.garage.deleteFailed", { error: errorText(e) }));
    }
  }

  return (
    <section className="card">
      <div className="set-row">
        <div>
          <div className="sl">{t("settings.garage.title")}</div>
          <div className="sd">{t("settings.garage.hint")}</div>
        </div>
        <div className="flex shrink-0 gap-2">
          {anyRules && (
            <button onClick={() => applyRules.mutate()} disabled={applyRules.isPending} className="btn ghost" data-testid="apply-rules">
              <Wand2 size={15} />
              {applyRules.isPending ? t("settings.garage.applying") : t("settings.garage.applyRules")}
            </button>
          )}
          <button onClick={() => setEditing("new")} className="btn primary">
            <Plus size={15} />
            {t("settings.garage.add")}
          </button>
        </div>
      </div>

      {error != null && (
        <p className="sd pb-4" style={{ color: "var(--danger)" }} role="alert">
          {t("settings.garage.loadFailed", { error: errorText(error) })}
        </p>
      )}

      {!isPending && !error && items.length === 0 && (
        <p className="sd pb-4">{t("settings.garage.empty")}</p>
      )}

      {items.length > 0 && (
        <div className="pb-4 space-y-2" data-testid="gear-list">
          {items.map((item) => {
            const retired = item.retired_at != null;
            const wear = wearFraction(item);
            const detail = [item.brand, item.model].filter(Boolean).join(" ");
            return (
              <div
                key={item.id}
                className={`rounded-[9px] border border-border-2 bg-card-2 px-3.5 py-3 ${retired ? "opacity-60" : ""}`}
                data-testid="gear-card"
              >
                <div className="flex items-start gap-3">
                  <span className="mt-0.5 shrink-0 text-accent-2" aria-hidden="true">
                    <GearKindIcon kind={item.kind} size={18} />
                  </span>
                  <div className="min-w-0 flex-1">
                    <div className="flex flex-wrap items-baseline gap-x-2">
                      <span className="text-sm font-semibold text-ink">{item.name}</span>
                      {detail && <span className="text-xs text-muted">{detail}</span>}
                      {retired && (
                        <span className="rounded-md bg-card px-1.5 py-0.5 text-[11px] font-semibold text-faint">
                          {t("settings.garage.retired")}
                        </span>
                      )}
                    </div>
                    {item.stats.activities === 0 ? (
                      // A row of zeros reads like a broken counter; say
                      // what is true instead, with the pre-Syzify mileage
                      // when there is one.
                      <div className="mt-1 text-xs text-muted">
                        {item.initial_distance_m > 0 && (
                          <span>
                            {rt("settings.garage.beforeSyzify", {
                              distance: <span className="font-num text-ink">{formatDistance(item.initial_distance_m)}</span>,
                            })}
                            {" · "}
                          </span>
                        )}
                        {t("settings.garage.noActivities")}
                      </div>
                    ) : (
                      <div className="mt-1 text-xs text-muted">
                        <span className="font-num text-ink">{formatDistance(odometerM(item))}</span>
                        {item.initial_distance_m > 0 && (
                          <span> {t("settings.garage.inSyzify", { distance: formatDistance(item.stats.distance_m) })}</span>
                        )}
                        <span> · {formatDurationHM(item.stats.duration_s)}</span>
                        <span> · {formatElevation(item.stats.elev_gain_m)}</span>
                        <span> · {t("common.activities", { count: item.stats.activities })}</span>
                        {item.stats.last_used && (
                          <span> · {t("settings.garage.lastUsed", { date: formatDay(item.stats.last_used) })}</span>
                        )}
                      </div>
                    )}
                    {item.default_for.length > 0 && (
                      <div className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-faint">
                        <span>{t("settings.garage.defaultFor", { sports: sportList(item.default_for, t) })}</span>
                        {!retired && (
                          // The way the history gets its gear: one click
                          // instead of a library filter (#165).
                          <button
                            type="button"
                            onClick={() => assignHistory.mutate(item)}
                            disabled={assignHistory.isPending}
                            className="inline-flex items-center gap-1 text-accent-2 hover:underline disabled:opacity-50 disabled:no-underline"
                          >
                            <History size={12} />
                            {historyOffer(item)}
                          </button>
                        )}
                      </div>
                    )}
                    {item.rules.length > 0 && (
                      <div className="mt-1 text-xs text-faint" data-testid="gear-rules">
                        {t("settings.garage.autoAssignBy", { rules: rulesSummary(item.rules, candidates, t) })}
                      </div>
                    )}
                    {wear != null && (
                      // The tooltip hangs off a wrapper with a bit of hover
                      // room: the track itself clips its overflow, which
                      // would cut the tooltip off, and is too thin to hit.
                      <div
                        className="mt-1 w-full max-w-xs py-1"
                        role="progressbar"
                        aria-valuemin={0}
                        aria-valuemax={100}
                        aria-valuenow={Math.round(wear * 100)}
                        aria-label={wearTip(item, wear)}
                        data-tip={wearTip(item, wear)}
                      >
                        <div className="h-1.5 w-full overflow-hidden rounded-full bg-border">
                          <div
                            className="h-full rounded-full"
                            data-testid="wear-fill"
                            style={{
                              width: `${wear * 100}%`,
                              background: WEAR_COLORS[wearState(wear)],
                            }}
                          />
                        </div>
                      </div>
                    )}
                  </div>
                  <div className="flex shrink-0 items-center gap-1">
                    <button
                      onClick={() => setEditing(item)}
                      className="set-path-add"
                      data-tip={t("common.edit")}
                      aria-label={t("settings.garage.editItem", { name: item.name })}
                    >
                      <Pencil size={15} />
                    </button>
                    <button
                      onClick={() => retire.mutate({ id: item.id, retired: !retired })}
                      className="btn ghost !px-2.5 !py-1 !text-xs"
                      aria-label={t(retired ? "settings.garage.bringBackItem" : "settings.garage.retireItem", {
                        name: item.name,
                      })}
                    >
                      {retired ? t("settings.garage.bringBack") : t("settings.garage.retire")}
                    </button>
                    <button
                      onClick={() => remove(item)}
                      className="set-path-add danger"
                      data-tip={t("common.delete")}
                      aria-label={t("settings.garage.deleteItem", { name: item.name })}
                    >
                      <Trash2 size={15} />
                    </button>
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      )}

      {editing && (
        <GearModal
          item={editing === "new" ? null : editing}
          candidates={candidates}
          onClose={() => setEditing(null)}
          onSaved={() => {
            setEditing(null);
            changed();
          }}
        />
      )}
    </section>
  );
}

/** The form a new or an existing item is edited in. The distance fields
 * take the display unit and are stored in meters. */
export function GearModal({
  item,
  candidates,
  onClose,
  onSaved,
}: {
  item: GearItem | null;
  /** What the vault's files carried, for the rules; absent = still loading. */
  candidates?: RuleCandidates;
  onClose: () => void;
  onSaved: () => void;
}) {
  const t = useT();
  const addToast = useToastStore((s) => s.addToast);
  const today = useToday();
  const todayIso = `${today.getFullYear()}-${String(today.getMonth() + 1).padStart(2, "0")}-${String(today.getDate()).padStart(2, "0")}`;
  const [kind, setKind] = useState<GearKind>(item?.kind ?? "bike");
  const [name, setName] = useState(item?.name ?? "");
  const [brand, setBrand] = useState(item?.brand ?? "");
  const [model, setModel] = useState(item?.model ?? "");
  const [purchasedAt, setPurchasedAt] = useState(item?.purchased_at ?? "");
  // The distance fields open with the stored meters rendered once; what
  // they send back depends on whether that text was touched (see
  // readDistanceField).
  const [initialText0] = useState(() => distanceInputValue(item?.initial_distance_m ?? null));
  const [limitText0] = useState(() => distanceInputValue(item?.distance_limit_m ?? null));
  const [initial, setInitial] = useState(initialText0);
  const [limit, setLimit] = useState(limitText0);
  const [notes, setNotes] = useState(item?.notes ?? "");
  const [defaultFor, setDefaultFor] = useState<SportType[]>(item?.default_for ?? []);
  const [ruleProfiles, setRuleProfiles] = useState<string[]>(
    (item?.rules ?? []).filter((r) => r.kind === "profile_name").map((r) => r.value),
  );
  const [ruleSensors, setRuleSensors] = useState<string[]>(
    (item?.rules ?? []).filter((r) => r.kind === "sensor_serial").map((r) => r.value),
  );
  const unit = unitLabel(distanceUnit());
  const retired = item?.retired_at != null;
  const initialField = readDistanceField(initial, initialText0, item?.initial_distance_m ?? null);
  const limitField = readDistanceField(limit, limitText0, item?.distance_limit_m ?? null);
  const invalid = initialField.invalid || limitField.invalid;

  // The kind decides which sports are offered; a default for a sport the
  // new kind does not offer is dropped with the kind.
  function changeKind(next: GearKind) {
    setKind(next);
    const offered = new Set<string>(kindSports(next));
    setDefaultFor((prev) => prev.filter((s) => offered.has(s)));
  }

  const save = useMutation({
    mutationFn: async () => {
      const input: GearInput = {
        kind,
        name,
        brand: brand || null,
        model: model || null,
        purchased_at: purchasedAt || null,
        initial_distance_m: initialField.meters ?? 0,
        distance_limit_m: limitField.meters,
        notes: notes || null,
        // A retired item is no default for anything (the field is hidden).
        default_for: retired ? [] : defaultFor,
        rules: [
          ...ruleProfiles.map((value): GearRule => ({ kind: "profile_name", value })),
          ...ruleSensors.map((value): GearRule => ({ kind: "sensor_serial", value })),
        ],
      };
      if (item) await api.updateGear(item.id, input);
      else await api.createGear(input);
    },
    onSuccess: () => {
      addToast("success", item ? t("settings.garage.updated") : t("settings.garage.added"));
      onSaved();
    },
    onError: (e: unknown) => addToast("error", t("settings.garage.saveFailed", { error: errorText(e) })),
  });

  const sportOptions = kindSports(kind).map((s) => ({ value: s, label: sportLabel(s, s, t) }));
  // A rule's value the vault has not seen (set elsewhere, files gone) stays
  // offered so the save keeps it.
  const profileOptions = [
    ...(candidates?.profiles ?? []).map((p) => ({ value: p.value, label: `${p.value} (${formatNumber(p.count)})` })),
    ...ruleProfiles.filter((v) => !candidates?.profiles.some((p) => p.value === v)).map((v) => ({ value: v, label: v })),
  ];
  const sensorOptions = [
    ...(candidates?.sensors ?? []).map((s) => ({
      value: s.serial,
      label: `${sensorLabel(s, t)} · ${s.serial} (${formatNumber(s.count)})`,
    })),
    ...ruleSensors.filter((v) => !candidates?.sensors.some((s) => s.serial === v)).map((v) => ({ value: v, label: v })),
  ];
  const rulesOffered = profileOptions.length > 0 || sensorOptions.length > 0;

  return (
    // No backdrop-click close (app-wide modal policy): closing is explicit.
    // The overlay scrolls: in a window at the minimum height the calendar
    // popover reaches below the fold, and a fixed overlay would hide it.
    <div className="fixed inset-0 z-50 flex items-center justify-center overflow-y-auto bg-black/30 py-6" role="dialog" aria-modal="true" aria-label={item ? t("settings.garage.editTitle") : t("settings.garage.add")}>
      <div className="bg-card rounded-xl shadow-2xl w-full max-w-md mx-4 my-auto p-6 space-y-4">
        <div className="flex items-center justify-between">
          <h2 className="text-lg font-semibold">{item ? t("settings.garage.editTitle") : t("settings.garage.add")}</h2>
          <button onClick={onClose} className="text-faint hover:text-muted" aria-label={t("common.close")}>
            <X size={18} />
          </button>
        </div>

        <div className="grid grid-cols-2 gap-3">
          <div>
            <label className="text-xs text-muted block mb-1">{t("settings.garage.kind")}</label>
            <Select
              ariaLabel={t("settings.garage.kind")}
              className="w-full"
              value={kind}
              onChange={(v) => changeKind(v as GearKind)}
              options={GEAR_KINDS.map((k) => ({ value: k.id, label: t(k.labelKey), icon: <GearKindIcon kind={k.id} size={14} /> }))}
            />
          </div>
          <div>
            <label className="text-xs text-muted block mb-1">{t("settings.garage.name")}</label>
            <input
              type="text"
              value={name}
              // By code point, not UTF-16 unit: a cut surrogate pair would
              // reach the backend as a lone surrogate it cannot decode.
              onChange={(e) => setName(Array.from(e.target.value).slice(0, MAX_GEAR_NAME_LENGTH).join(""))}
              // A shoe's example is a model name, the same in every language.
              placeholder={kind === "shoes" ? "Pegasus 41" : t("settings.garage.namePlaceholder")} // i18n-ignore
              aria-label={t("settings.garage.name")}
              className="w-full text-sm border border-border rounded px-3 py-2"
            />
          </div>
          <div>
            <label className="text-xs text-muted block mb-1">{t("settings.garage.brand")}</label>
            <input type="text" value={brand} onChange={(e) => setBrand(e.target.value)} aria-label={t("settings.garage.brand")} className="w-full text-sm border border-border rounded px-3 py-2" />
          </div>
          <div>
            <label className="text-xs text-muted block mb-1">{t("settings.garage.model")}</label>
            <input type="text" value={model} onChange={(e) => setModel(e.target.value)} aria-label={t("settings.garage.model")} className="w-full text-sm border border-border rounded px-3 py-2" />
          </div>
          {/* The app's own calendar, as in the library filters; the field
              carries its label itself. A purchase is never in the future. */}
          <div className="flex items-end">
            <DateField
              label={t("settings.garage.purchased")}
              value={purchasedAt || undefined}
              onChange={(iso) => setPurchasedAt(iso ?? "")}
              max={todayIso}
              drop="down"
              yearSpan={[today.getFullYear() - PURCHASE_YEARS_BACK, today.getFullYear()]}
              placeholder={t("settings.garage.notSet")}
            />
          </div>
          <div>
            <label className="text-xs text-muted block mb-1">{t("settings.garage.initial", { unit })}</label>
            <input
              type="text"
              inputMode="decimal"
              value={initial}
              onChange={(e) => setInitial(e.target.value)}
              placeholder="0"
              aria-label={t("settings.garage.initial", { unit })}
              aria-invalid={initialField.invalid || undefined}
              className="w-full text-sm border border-border rounded px-3 py-2"
            />
            {initialField.invalid && <p className="mt-1 text-xs" style={{ color: "var(--danger)" }}>{t("settings.garage.enterNumber")}</p>}
          </div>
          <div>
            <label className="text-xs text-muted block mb-1">{t("settings.garage.limitOptional", { unit })}</label>
            <input
              type="text"
              inputMode="decimal"
              value={limit}
              onChange={(e) => setLimit(e.target.value)}
              placeholder={kind === "shoes" ? "800" : ""}
              aria-label={t("settings.garage.limit", { unit })}
              aria-invalid={limitField.invalid || undefined}
              className="w-full text-sm border border-border rounded px-3 py-2"
            />
            {limitField.invalid && <p className="mt-1 text-xs" style={{ color: "var(--danger)" }}>{t("settings.garage.enterNumber")}</p>}
          </div>
          {retired ? (
            <p className="self-end text-xs text-faint">{t("settings.garage.retiredNoDefault")}</p>
          ) : (
            <div>
              <label className="text-xs text-muted block mb-1">{t("settings.garage.defaultForLabel")}</label>
              <Select
                multiple
                ariaLabel={t("settings.garage.defaultForLabel")}
                className="w-full"
                values={defaultFor}
                onChange={(v) => setDefaultFor(v as SportType[])}
                options={sportOptions}
                placeholder={t("settings.garage.noSport")}
              />
            </div>
          )}
        </div>

        {rulesOffered && (
          // Auto-assignment (ADR 0003): what a file carries that names the
          // bike — the profile it was recorded under, the paired sensors.
          <div className="grid grid-cols-2 gap-3" data-testid="gear-rules-form">
            <div className="col-span-2 text-xs text-muted">
              {t("settings.garage.rulesIntro")}
              {retired && <span className="text-faint"> {t("settings.garage.rulesAsleep")}</span>}
            </div>
            <div>
              <label className="text-xs text-muted block mb-1">{t("settings.garage.profileIs")}</label>
              <Select
                multiple
                ariaLabel={t("settings.garage.profileIsLabel")}
                className="w-full"
                values={ruleProfiles}
                onChange={setRuleProfiles}
                options={profileOptions}
                placeholder={t("settings.garage.anyProfile")}
              />
            </div>
            <div>
              <label className="text-xs text-muted block mb-1">{t("settings.garage.sensorPaired")}</label>
              <Select
                multiple
                ariaLabel={t("settings.garage.sensorPairedLabel")}
                className="w-full"
                values={ruleSensors}
                onChange={setRuleSensors}
                options={sensorOptions}
                placeholder={t("settings.garage.anySensor")}
              />
            </div>
          </div>
        )}

        <div>
          <label className="text-xs text-muted block mb-1">{t("common.notes")}</label>
          <textarea value={notes} onChange={(e) => setNotes(e.target.value)} rows={2} aria-label={t("common.notes")} className="w-full text-sm border border-border rounded px-3 py-2" />
        </div>

        <div className="flex justify-end gap-2">
          <button type="button" onClick={onClose} className="text-sm px-4 py-2 text-muted hover:text-ink">
            {t("common.cancel")}
          </button>
          <button
            type="button"
            onClick={() => save.mutate()}
            disabled={save.isPending || !name.trim() || invalid}
            className="text-sm px-4 py-2 bg-accent text-white rounded-lg hover:bg-accent-2 disabled:opacity-50"
          >
            {save.isPending ? t("common.saving") : t("common.save")}
          </button>
        </div>
      </div>
    </div>
  );
}
