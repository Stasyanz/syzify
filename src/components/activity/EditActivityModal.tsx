import { useState, useEffect, useRef } from "react";
import { useQuery, useMutation } from "@tanstack/react-query";
import { X, MapPin, Trash2, Loader2 } from "lucide-react";
import { api } from "../../lib/tauri";
import {
  SPORT_LABELS,
  SPORT_TYPES,
  MAX_TAGS_PER_ACTIVITY,
  MAX_TITLE_LENGTH,
  type Activity,
  type LocationHit,
} from "../../lib/types";
import { Select } from "../ui/Select";
import { SportIcon } from "../brand/SportIcon";
import { useToastStore } from "../../stores/toastStore";

interface Props {
  activity: Activity;
  currentTags: string[];
  onClose: () => void;
  onSaved: () => void;
  onDeleted: () => void;
}

/** Toggle `id` within `selected`, enforcing a maximum of `max` selections.
 * Deselecting is always allowed; selecting beyond the cap is a no-op. */
export function toggleTagSelection(selected: number[], id: number, max: number): number[] {
  if (selected.includes(id)) return selected.filter((t) => t !== id);
  if (selected.length >= max) return selected; // cap reached — ignore
  return [...selected, id];
}

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

export function EditActivityModal({ activity, currentTags, onClose, onSaved, onDeleted }: Props) {
  const addToast = useToastStore((s) => s.addToast);
  const [confirmDelete, setConfirmDelete] = useState(false);

  const [title, setTitle] = useState(activity.title ?? "");
  const [notes, setNotes] = useState(activity.notes ?? "");
  const [sportType, setSportType] = useState(activity.sport_type);
  const [locationText, setLocationText] = useState(activity.location_name ?? "");
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
          addToast("warning", "Location suggestions unavailable: no network or the geocoding service is down.");
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
  }, [locationText, picked, locationDirty, addToast]);

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
  const [selectedTagIds, setSelectedTagIds] = useState<number[]>([]);
  const [newTagName, setNewTagName] = useState("");

  const { data: allTags = [] } = useQuery({
    queryKey: ["tags"],
    queryFn: () => api.getTags(),
  });

  // Initialize selected tag IDs once tags are loaded
  useEffect(() => {
    if (allTags.length > 0) {
      const ids = allTags
        .filter((t) => currentTags.includes(t.name))
        .map((t) => t.id);
      setSelectedTagIds(ids);
    }
  }, [allTags, currentTags]);

  const updateMutation = useMutation({
    mutationFn: async () => {
      await api.updateActivity(activity.id, {
        title: title || undefined,
        notes: notes || undefined,
        sport_type: sportType,
      });
      await api.setActivityTags(activity.id, selectedTagIds);

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
            addToast(
              "warning",
              "Could not geocode location (network issue). Saved as text; the map point is unchanged.",
            );
          }
        }
      }
    },
    onSuccess: () => {
      addToast("success", "Activity updated");
      onSaved();
    },
    onError: (err: Error) => {
      addToast("error", `Failed to update: ${err.message}`);
    },
  });

  const deleteMutation = useMutation({
    mutationFn: () => api.deleteActivity(activity.id),
    onSuccess: () => {
      addToast("success", "Activity deleted");
      onDeleted();
    },
    onError: (err: Error) => {
      addToast("error", `Failed to delete: ${err.message}`);
    },
  });

  const atTagLimit = selectedTagIds.length >= MAX_TAGS_PER_ACTIVITY;

  const createTagMutation = useMutation({
    mutationFn: (name: string) => api.createTag(name),
    onSuccess: (tag) => {
      // Newly created tags are added to the library; only auto-select if there's
      // still room within the per-activity limit.
      setSelectedTagIds((prev) =>
        prev.length < MAX_TAGS_PER_ACTIVITY ? [...prev, tag.id] : prev,
      );
      setNewTagName("");
    },
  });

  function toggleTag(id: number) {
    setSelectedTagIds((prev) => toggleTagSelection(prev, id, MAX_TAGS_PER_ACTIVITY));
  }

  function handleAddTag() {
    const name = newTagName.trim();
    if (!name) return;
    const existing = allTags.find((t) => t.name.toLowerCase() === name.toLowerCase());
    if (existing) {
      if (!selectedTagIds.includes(existing.id)) {
        if (atTagLimit) {
          addToast("warning", `You can select up to ${MAX_TAGS_PER_ACTIVITY} tags`);
          return;
        }
        setSelectedTagIds((prev) => [...prev, existing.id]);
      }
      setNewTagName("");
    } else {
      if (atTagLimit) {
        addToast("warning", `You can select up to ${MAX_TAGS_PER_ACTIVITY} tags`);
        return;
      }
      createTagMutation.mutate(name);
    }
  }

  return (
    // No backdrop-click close (app-wide modal policy): a stray click must
    // not discard half-edited fields. Closing is explicit — X or Cancel.
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/30">
      <div className="bg-card rounded-xl shadow-2xl w-full max-w-md mx-4 p-6 space-y-4">
        <div className="flex items-center justify-between">
          <h2 className="text-lg font-semibold">Edit Activity</h2>
          <button onClick={onClose} className="text-faint hover:text-muted">
            <X size={18} />
          </button>
        </div>

        {/* Title */}
        <div>
          <label className="text-xs text-muted block mb-1">Title</label>
          <input
            type="text"
            value={title}
            onChange={(e) => setTitle(e.target.value.slice(0, MAX_TITLE_LENGTH))}
            placeholder="Activity title"
            className="w-full text-sm border border-border rounded px-3 py-2"
          />
        </div>

        {/* Location */}
        <div>
          <label className="text-xs text-muted block mb-1">Location</label>
          <div ref={locationBoxRef} className="relative">
            <MapPin size={14} className="absolute left-2.5 top-1/2 -translate-y-1/2 text-faint" />
            {searching && (
              // A live region, so assistive tech announces the search
              // starting; a bare SVG with a label would not be read.
              <span
                role="status"
                aria-label="Searching locations"
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
              placeholder="City, address..."
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
                aria-label="Location suggestions"
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
              No matches — try a town or a street name, without house numbers.
            </p>
          )}
        </div>

        {/* Sport type */}
        <div>
          <label className="text-xs text-muted block mb-1">Sport type</label>
          <Select
            ariaLabel="Sport type"
            className="w-full"
            value={sportType}
            onChange={setSportType}
            options={[...SPORT_TYPES]
              .sort((a, b) => SPORT_LABELS[a].localeCompare(SPORT_LABELS[b]))
              .map((st) => ({
                value: st,
                label: SPORT_LABELS[st],
                icon: <SportIcon sport={st} size={18} />,
              }))}
          />
        </div>

        {/* Tags */}
        <div>
          <label className="text-xs text-muted block mb-1">
            Tags <span className="text-faint">({selectedTagIds.length}/{MAX_TAGS_PER_ACTIVITY})</span>
          </label>
          {allTags.length > 0 && (
            <div className="flex flex-wrap gap-1 mb-2">
              {allTags.map((tag) => {
                const selected = selectedTagIds.includes(tag.id);
                const disabled = !selected && atTagLimit;
                return (
                  <button
                    key={tag.id}
                    type="button"
                    onClick={() => toggleTag(tag.id)}
                    disabled={disabled}
                    title={disabled ? `Up to ${MAX_TAGS_PER_ACTIVITY} tags` : undefined}
                    className={`text-xs px-2 py-1 rounded ${
                      selected
                        ? "bg-accent-soft text-accent-2 ring-1 ring-border-2"
                        : "bg-card-2 text-muted hover:bg-border"
                    } ${disabled ? "opacity-40 cursor-not-allowed hover:bg-card-2" : ""}`}
                  >
                    {tag.name}
                  </button>
                );
              })}
            </div>
          )}
          <div className="flex gap-1">
            <input
              type="text"
              value={newTagName}
              onChange={(e) => setNewTagName(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && (e.preventDefault(), handleAddTag())}
              placeholder={atTagLimit ? `Up to ${MAX_TAGS_PER_ACTIVITY} tags selected` : "New tag..."}
              disabled={atTagLimit}
              className="flex-1 text-sm border border-border rounded px-2 py-1 disabled:opacity-40"
            />
            <button
              type="button"
              onClick={handleAddTag}
              disabled={!newTagName.trim() || atTagLimit}
              className="text-xs px-3 py-1 bg-card-2 text-muted hover:bg-border rounded disabled:opacity-40"
            >
              Add
            </button>
          </div>
        </div>

        {/* Notes */}
        <div>
          <label className="text-xs text-muted block mb-1">Notes</label>
          <textarea
            value={notes}
            onChange={(e) => setNotes(e.target.value)}
            rows={3}
            placeholder="Notes..."
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
              Delete
            </button>
          ) : (
            <button
              type="button"
              onClick={() => deleteMutation.mutate()}
              disabled={deleteMutation.isPending}
              className="text-sm px-3 py-2 bg-red-600 text-white rounded-lg hover:bg-red-700 disabled:opacity-50"
            >
              {deleteMutation.isPending ? "Deleting..." : "Confirm delete"}
            </button>
          )}
          <div className="flex gap-2">
            <button
              type="button"
              onClick={onClose}
              className="text-sm px-4 py-2 text-muted hover:text-ink"
            >
              Cancel
            </button>
            <button
              type="button"
              onClick={() => updateMutation.mutate()}
              disabled={updateMutation.isPending}
              className="text-sm px-4 py-2 bg-accent text-white rounded-lg hover:bg-accent-2 disabled:opacity-50"
            >
              {updateMutation.isPending ? "Saving..." : "Save"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
