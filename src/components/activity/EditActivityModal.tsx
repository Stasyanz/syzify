import { useState, useEffect } from "react";
import { useQuery, useMutation } from "@tanstack/react-query";
import { X, MapPin, Trash2 } from "lucide-react";
import { api } from "../../lib/tauri";
import { SPORT_LABELS, SPORT_TYPES, MAX_TAGS_PER_ACTIVITY, MAX_TITLE_LENGTH, type Activity } from "../../lib/types";
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

export function EditActivityModal({ activity, currentTags, onClose, onSaved, onDeleted }: Props) {
  const addToast = useToastStore((s) => s.addToast);
  const [confirmDelete, setConfirmDelete] = useState(false);

  const [title, setTitle] = useState(activity.title ?? "");
  const [notes, setNotes] = useState(activity.notes ?? "");
  const [sportType, setSportType] = useState(activity.sport_type);
  const [locationText, setLocationText] = useState(activity.location_name ?? "");
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
      const locChanged = (locationText.trim() || "") !== (activity.location_name || "");
      if (locChanged) {
        const result = await api.updateActivityLocation(activity.id, locationText);
        if (locationText.trim() && !result.geocoded) {
          addToast("warning", "Could not geocode location (network issue). Saved as text only.");
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
          <div className="relative">
            <MapPin size={14} className="absolute left-2.5 top-1/2 -translate-y-1/2 text-faint" />
            <input
              type="text"
              value={locationText}
              onChange={(e) => setLocationText(e.target.value)}
              placeholder="City, address..."
              className="w-full text-sm border border-border rounded px-3 py-2 pl-8"
            />
          </div>
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
