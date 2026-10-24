import { useState, useEffect, useRef } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { X, MapPin, Trash2, Loader2 } from "lucide-react";
import { api } from "../../lib/tauri";
import {
  sportLabel,
  SPORT_TYPES,
  MAX_TITLE_LENGTH,
  type Activity,
  type LocationHit,
} from "../../lib/types";
import { Select } from "../ui/Select";
import { SportIcon } from "../brand/SportIcon";
import { GearKindIcon } from "./GearChip";
import { gearChoicesFor } from "../../lib/gear";
import { useToastStore } from "../../stores/toastStore";
import { errorText } from "../../lib/errors";
import { useLanguageStore, useT } from "../../lib/i18n";
import { unitLabel } from "../../lib/units";

interface Props {
  activity: Activity;
  /** The gear item the activity is on (ADR 0003); null when unassigned. */
  gearId?: string | null;
  /** A multisport whole (merged container, FIT-native file) carries no
   * gear: its aggregate spans several sports. The field says so instead. */
  gearLocked?: boolean;
  onClose: () => void;
  onSaved: () => void;
  onDeleted: () => void;
  /** Open with the FTP field focused — the "Correct FTP" hint lands here. */
  focusFtp?: boolean;
}

/** The option that takes an activity off its gear. */
const NO_GEAR = "";

/** The Location field asks for suggestions this long after the last
 * keystroke — and never for fewer characters than this. Nominatim allows
 * one request a second, so the pause is a full second: the backend queues
 * anything faster, and a queue of searches whose answers the field would
 * throw away is only a wait. */
export const LOCATION_SEARCH_DEBOUNCE_MS = 1000;
export const LOCATION_SEARCH_MIN_CHARS = 3;

/** The next highlighted row after an arrow key: wraps, and -1 (nothing
 * highlighted) steps to the first or the last row. Exported for tests. */
export function stepHighlight(current: number, count: number, delta: 1 | -1): number {
  if (count === 0) return -1;
  if (current < 0) return delta > 0 ? 0 : count - 1;
  return (current + delta + count) % count;
}

export function EditActivityModal({
  activity,
  gearId = null,
  gearLocked = false,
  onClose,
  onSaved,
  onDeleted,
  focusFtp = false,
}: Props) {
  const addToast = useToastStore((s) => s.addToast);
  const t = useT();
  // Sport names sort in the language they are written in.
  const language = useLanguageStore((s) => s.language);
  const [confirmDelete, setConfirmDelete] = useState(false);

  const [title, setTitle] = useState(activity.title ?? "");
  const [notes, setNotes] = useState(activity.notes ?? "");
  const [sportType, setSportType] = useState(activity.sport_type);
  const [gear, setGear] = useState(gearId ?? NO_GEAR);
  const { data: gearItems = [] } = useQuery({
    queryKey: ["gear"],
    queryFn: () => api.listGear(),
    enabled: !gearLocked,
  });
  // The items that fit the sport being saved, plus the current one so a
  // save keeps it even if it is retired or of another kind.
  const gearChoices = gearChoicesFor(gearItems, sportType, gearId);
  // A sport change can take the picked item off the list; a pick that is
  // no longer offered falls back to what the activity is on, never to a
  // hidden value the save would still send.
  function changeSport(next: string) {
    setSportType(next);
    const offered = new Set(gearChoicesFor(gearItems, next, gearId).map((g) => g.id));
    setGear((g) => (g === NO_GEAR || offered.has(g) ? g : (gearId ?? NO_GEAR)));
  }
  const [locationText, setLocationText] = useState(activity.location_name ?? "");
  // The FTP the activity was recorded with — editable when the file carried
  // normalized power, since IF and TSS are recomputed from it.
  const hasPower = activity.normalized_power_w != null;
  // TSS needs a duration; a file with power but no timer time cannot be
  // corrected, and the field says so instead of failing on save.
  const canCorrectFtp = hasPower && activity.duration_s != null && activity.duration_s > 0;
  const [ftpText, setFtpText] = useState(activity.threshold_power_w != null ? String(Math.round(activity.threshold_power_w)) : "");
  // The suggestion picked from the list, if the text still is its name: the
  // save then writes its coordinates instead of geocoding the text again.
  const [picked, setPicked] = useState<LocationHit | null>(null);
  // Set by the first keystroke: opening the modal on a saved name must not
  // fire a request, but retyping that same name to pick its namesake must.
  const [locationDirty, setLocationDirty] = useState(false);
  const [hits, setHits] = useState<LocationHit[]>([]);
  // A request is in flight for the text as it stands now — not during the
  // debounce pause, and not once a newer keystroke has superseded it.
  const [searching, setSearching] = useState(false);
  // The last answer for the text as it stands was empty: say so under the
  // field, or silence reads as a broken feature.
  const [noMatches, setNoMatches] = useState(false);
  const [listOpen, setListOpen] = useState(false);
  const [highlight, setHighlight] = useState(-1);
  // Only the latest query may fill the list — a slow earlier answer is dropped.
  const searchSeq = useRef(0);
  // One warning per outage, not one per keystroke: set on a failed search,
  // cleared by the next answer that gets through.
  const searchWarned = useRef(false);
  const locationBoxRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const q = locationText.trim();
    // Nothing to ask for: the field is untouched, a pick just landed, or
    // the text is too short to mean anything.
    if (!locationDirty || picked || q.length < LOCATION_SEARCH_MIN_CHARS) {
      searchSeq.current += 1;
      setHits([]);
      setListOpen(false);
      setSearching(false);
      setNoMatches(false);
      return;
    }
    const seq = ++searchSeq.current;
    const timer = setTimeout(async () => {
      setSearching(true);
      let found: LocationHit[] | null = null;
      try {
        found = await api.searchLocations(q);
      } catch {
        found = null;
      }
      // A superseded query — or a modal already closed — says nothing:
      // no list, and no warning about a field that is not there.
      if (seq !== searchSeq.current) return;
      setSearching(false);
      if (found == null) {
        // No network or Nominatim down (the toggle off is an empty answer,
        // not an error): say so once, and keep the list away.
        if (!searchWarned.current) {
          searchWarned.current = true;
          addToast("warning", t("activity.edit.suggestionsDown"));
        }
        found = [];
      } else {
        searchWarned.current = false;
      }
      setHits(found);
      setHighlight(-1);
      setListOpen(found.length > 0);
      setNoMatches(found.length === 0 && !searchWarned.current);
    }, LOCATION_SEARCH_DEBOUNCE_MS);
    return () => {
      // Unmount or a newer keystroke: whatever this query answers is stale,
      // and nothing is in flight for the new text yet.
      clearTimeout(timer);
      searchSeq.current += 1;
      setSearching(false);
      setNoMatches(false);
    };
  }, [locationText, picked, locationDirty, addToast, t]);

  // Click outside the field and its list closes the list.
  useEffect(() => {
    if (!listOpen) return;
    const onDown = (e: MouseEvent) => {
      if (locationBoxRef.current?.contains(e.target as Node)) return;
      setListOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [listOpen]);

  const pickHit = (hit: LocationHit) => {
    setPicked(hit);
    setLocationText(hit.name);
    setListOpen(false);
    setHits([]);
  };

  const onLocationKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (!listOpen) return;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      setHighlight((h) => stepHighlight(h, hits.length, e.key === "ArrowDown" ? 1 : -1));
    } else if (e.key === "Enter" && highlight >= 0 && highlight < hits.length) {
      e.preventDefault();
      pickHit(hits[highlight]);
    } else if (e.key === "Escape") {
      e.stopPropagation();
      setListOpen(false);
    }
  };
  const updateMutation = useMutation({
    mutationFn: async (): Promise<{ ftpRefused: boolean; gearRefused: boolean }> => {
      let ftpRefused = false;
      let gearRefused = false;
      await api.updateActivity(activity.id, {
        title: title || undefined,
        notes: notes || undefined,
        sport_type: sportType,
      });
      // Gear is its own write; untouched, it is not re-sent (a retired
      // item already on the activity would be refused anew). Like the
      // FTP below, a refusal is reported by name and does not undo the
      // rest of the save.
      if (!gearLocked && gear !== (gearId ?? NO_GEAR)) {
        try {
          await api.setActivityGear(activity.id, gear === NO_GEAR ? null : gear);
        } catch (err) {
          gearRefused = true;
          addToast("error", t("activity.gear.notChanged", { error: errorText(err) }));
        }
      }

      // A changed FTP rewrites IF, TSS and the power zones on the backend.
      // Its own failure is reported by name and does not undo the rest of
      // the save — the title and notes above are already written.
      if (canCorrectFtp) {
        const ftp = Number(ftpText.trim());
        const before = activity.threshold_power_w != null ? Math.round(activity.threshold_power_w) : null;
        if (ftpText.trim() !== "" && Number.isFinite(ftp) && ftp !== before) {
          try {
            await api.setActivityFtp(activity.id, ftp);
          } catch (err) {
            ftpRefused = true;
            addToast("error", t("activity.edit.ftpNotChanged", { error: errorText(err) }));
          }
        }
      }

      // Handle location separately (forward geocoding)
      // A namesake picked from the list keeps the name and changes only the
      // coordinates — that is a change too.
      const locChanged =
        (locationText.trim() || "") !== (activity.location_name || "") ||
        (picked != null && (picked.lat !== activity.start_lat || picked.lon !== activity.start_lon));
      if (locChanged) {
        if (picked && picked.name === locationText.trim()) {
          // A picked suggestion carries its coordinates: no second lookup.
          await api.setActivityLocationNamed(activity.id, picked.name, picked.lat, picked.lon);
        } else {
          const result = await api.updateActivityLocation(activity.id, locationText);
          // Geocoding off is the user's choice and gets no warning; a
          // failed lookup does — and says that the map point, if the
          // activity had one, is still the old one.
          if (locationText.trim() && !result.geocoded && !result.geocoding_off) {
            addToast("warning", t("activity.edit.geocodeFailed"));
          }
        }
      }
      return { ftpRefused, gearRefused };
    },
    onSuccess: ({ ftpRefused, gearRefused }) => {
      // The last toast on screen must not read as "everything saved"
      // when the FTP or the gear was refused a moment earlier.
      addToast(
        "success",
        t(
          ftpRefused && gearRefused
            ? "activity.edit.updatedBothUnchanged"
            : ftpRefused
              ? "activity.edit.updatedFtpUnchanged"
              : gearRefused
                ? "activity.edit.updatedGearUnchanged"
                : "activity.edit.updated",
        ),
      );
      onSaved();
    },
    onError: (err: unknown) => {
      addToast("error", t("activity.edit.updateFailed", { error: errorText(err) }));
    },
  });

  const deleteMutation = useMutation({
    mutationFn: () => api.deleteActivity(activity.id),
    onSuccess: () => {
      addToast("success", t("activity.edit.deleted"));
      onDeleted();
    },
    onError: (err: unknown) => {
      addToast("error", t("activity.edit.deleteFailed", { error: errorText(err) }));
    },
  });

  return (
    // No backdrop-click close (app-wide modal policy): a stray click must
    // not discard half-edited fields. Closing is explicit — X or Cancel.
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/30">
      <div className="bg-card rounded-xl shadow-2xl w-full max-w-md mx-4 p-6 space-y-4">
        <div className="flex items-center justify-between">
          <h2 className="text-lg font-semibold">{t("activity.edit.title")}</h2>
          <button onClick={onClose} aria-label={t("common.close")} className="text-faint hover:text-muted">
            <X size={18} />
          </button>
        </div>

        {/* Title */}
        <div>
          <label className="text-xs text-muted block mb-1">{t("activity.edit.titleLabel")}</label>
          <input
            type="text"
            value={title}
            onChange={(e) => setTitle(e.target.value.slice(0, MAX_TITLE_LENGTH))}
            placeholder={t("activity.title.label")}
            className="w-full text-sm border border-border rounded px-3 py-2"
          />
        </div>

        {/* Location */}
        <div>
          <label className="text-xs text-muted block mb-1">{t("activity.edit.location")}</label>
          <div ref={locationBoxRef} className="relative">
            <MapPin size={14} className="absolute left-2.5 top-1/2 -translate-y-1/2 text-faint" />
            {searching && (
              // A live region, so assistive tech announces the search
              // starting; a bare SVG with a label would not be read.
              <span
                role="status"
                aria-label={t("activity.edit.searching")}
                className="absolute right-2.5 top-1/2 -translate-y-1/2 text-faint"
              >
                <Loader2 size={14} className="animate-spin" />
              </span>
            )}
            <input
              type="text"
              value={locationText}
              onChange={(e) => {
                setLocationText(e.target.value);
                setPicked(null);
                setLocationDirty(true);
              }}
              onKeyDown={onLocationKeyDown}
              placeholder={t("activity.edit.locationPlaceholder")}
              role="combobox"
              aria-expanded={listOpen}
              aria-controls="location-suggestions"
              aria-autocomplete="list"
              className="w-full text-sm border border-border rounded px-3 py-2 pl-8 pr-8"
            />
            {listOpen && hits.length > 0 && (
              <ul
                id="location-suggestions"
                role="listbox"
                aria-label={t("activity.edit.suggestions")}
                className="absolute left-0 right-0 top-full z-20 mt-1 max-h-56 overflow-auto rounded-lg border border-border bg-card py-1 shadow-lg"
              >
                {hits.map((hit, i) => (
                  <li
                    key={`${hit.name}|${hit.detail}`}
                    role="option"
                    aria-selected={i === highlight}
                    // mousedown, not click: the input keeps focus and the
                    // outside-click closer sees the list as inside.
                    onMouseDown={(e) => {
                      e.preventDefault();
                      pickHit(hit);
                    }}
                    onMouseEnter={() => setHighlight(i)}
                    className={`cursor-pointer px-3 py-1.5 text-sm ${
                      i === highlight ? "bg-accent-soft text-accent-2" : "text-ink hover:bg-card-2"
                    }`}
                  >
                    <div className="truncate">{hit.name}</div>
                    {hit.detail && <div className="truncate text-xs text-muted">{hit.detail}</div>}
                  </li>
                ))}
              </ul>
            )}
          </div>
          {noMatches && (
            <p role="status" className="mt-1 text-xs text-muted">
              {t("activity.edit.noMatches")}
            </p>
          )}
        </div>

        {/* FTP — only for activities with power */}
        {hasPower && (
          <div>
            <label className="text-xs text-muted block mb-1" htmlFor="activity-ftp">
              FTP ({unitLabel("W")})
            </label>
            <input
              id="activity-ftp"
              type="number"
              inputMode="numeric"
              min={1}
              max={2000}
              step={1}
              value={ftpText}
              onChange={(e) => setFtpText(e.target.value)}
              placeholder={t("activity.edit.ftpPlaceholder", { value: "238" })}
              autoFocus={focusFtp && canCorrectFtp}
              disabled={!canCorrectFtp}
              className="w-full text-sm border border-border rounded px-3 py-2 disabled:opacity-60"
            />
            <p className="mt-1 text-xs text-muted">
              {t(canCorrectFtp ? "activity.edit.ftpHint" : "activity.edit.ftpLocked")}
            </p>
          </div>
        )}

        {/* Sport type */}
        <div>
          <label className="text-xs text-muted block mb-1">{t("activity.edit.sport")}</label>
          <Select
            ariaLabel={t("activity.edit.sport")}
            className="w-full"
            value={sportType}
            onChange={changeSport}
            options={[...SPORT_TYPES]
              .sort((a, b) => sportLabel(a, a, t).localeCompare(sportLabel(b, b, t), language))
              .map((st) => ({
                value: st,
                label: sportLabel(st, st, t),
                icon: <SportIcon sport={st} size={18} />,
              }))}
          />
        </div>

        {/* Gear (ADR 0003) */}
        <div>
          <label className="text-xs text-muted block mb-1">{t("common.gear")}</label>
          {gearLocked ? (
            <p className="text-xs text-faint">{t("activity.edit.gearLocked")}</p>
          ) : (
            <Select
              ariaLabel={t("common.gear")}
              className="w-full"
              value={gear}
              onChange={setGear}
              options={[
                { value: NO_GEAR, label: t("activity.gear.none") },
                ...gearChoices.map((g) => ({
                  value: g.id,
                  label: g.retired_at ? t("library.filter.retired", { name: g.name }) : g.name,
                  icon: <GearKindIcon kind={g.kind} size={14} />,
                })),
              ]}
            />
          )}
        </div>

        {/* Notes */}
        <div>
          <label className="text-xs text-muted block mb-1">{t("common.notes")}</label>
          <textarea
            value={notes}
            onChange={(e) => setNotes(e.target.value)}
            rows={3}
            placeholder={t("activity.edit.notesPlaceholder")}
            className="w-full text-sm border border-border rounded px-3 py-2 resize-y"
          />
        </div>

        {/* Actions */}
        <div className="flex items-center justify-between pt-2">
          {!confirmDelete ? (
            <button
              type="button"
              onClick={() => setConfirmDelete(true)}
              className="text-sm px-3 py-2 text-red-500 hover:text-red-700 flex items-center gap-1"
            >
              <Trash2 size={14} />
              {t("common.delete")}
            </button>
          ) : (
            <button
              type="button"
              onClick={() => deleteMutation.mutate()}
              disabled={deleteMutation.isPending}
              className="text-sm px-3 py-2 bg-red-600 text-white rounded-lg hover:bg-red-700 disabled:opacity-50"
            >
              {t(deleteMutation.isPending ? "activity.edit.deleting" : "activity.edit.confirmDelete")}
            </button>
          )}
          <div className="flex gap-2">
            <button
              type="button"
              onClick={onClose}
              className="text-sm px-4 py-2 text-muted hover:text-ink"
            >
              {t("common.cancel")}
            </button>
            <button
              type="button"
              onClick={() => updateMutation.mutate()}
              disabled={updateMutation.isPending}
              className="text-sm px-4 py-2 bg-accent text-white rounded-lg hover:bg-accent-2 disabled:opacity-50"
            >
              {t(updateMutation.isPending ? "common.saving" : "common.save")}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
