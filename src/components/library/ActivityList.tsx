import { useState, useEffect } from "react";
import { keepPreviousData, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "react-router";
import { Check } from "lucide-react";
import { api } from "../../lib/tauri";
import { useActivityStore } from "../../stores/activityStore";
import { useToastStore } from "../../stores/toastStore";
import { ActivityListItem } from "./ActivityListItem";
import { ImportDialog } from "../import/ImportDialog";
import { SPORT_LABELS, triathlonDiscipline, type ActivitySummary, type SportType } from "../../lib/types";

const DEFAULT_PAGE_SIZE = 20;

/** Why the current selection can't merge into a du/triathlon, or null when
 * it can. Mirrors the backend gate (db/multisport_legs.rs) so the button
 * greys out with the reason instead of the request round-tripping to fail. */
export function mergeBlockReason(
  selected: Pick<ActivitySummary, "sport_type" | "start_time">[],
): string | null {
  if (selected.length < 2) return "Select at least two activities";
  if (selected.length > 3) return "A multisport event has at most three legs";
  const bad = selected.find((a) => triathlonDiscipline(a.sport_type) == null);
  if (bad) {
    const label = SPORT_LABELS[bad.sport_type as SportType] ?? bad.sport_type;
    return `${label} can't be a leg — only run, bike, swim and ski combine`;
  }
  if (new Set(selected.map((a) => a.start_time.slice(0, 10))).size > 1) {
    return "Legs of one event must share a day";
  }
  const disciplines = new Set(selected.map((a) => triathlonDiscipline(a.sport_type)));
  if (disciplines.size < 2) return "Legs must span at least two disciplines";
  return null;
}

export function ActivityList() {
  const filters = useActivityStore((s) => s.filters);
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const addToast = useToastStore((s) => s.addToast);

  // Multi-select mode for merging same-day activities into a triathlon.
  const [selecting, setSelecting] = useState(false);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [merging, setMerging] = useState(false);

  const toggle = (id: string) =>
    setSelected((prev) => {
      const next = new Set(prev);
      next.has(id) ? next.delete(id) : next.add(id);
      return next;
    });

  const exitSelect = () => {
    setSelecting(false);
    setSelected(new Set());
  };

  // Merge mode is hidden UI: Ctrl+M toggles it (no visible Select button).
  // Leaving the mode always drops the selection, same as the Cancel button.
  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      const tag = (e.target as HTMLElement).tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;
      if (e.key.toLowerCase() === "m" && e.ctrlKey && !e.metaKey && !e.altKey) {
        e.preventDefault();
        setSelecting((was) => {
          if (was) setSelected(new Set());
          return !was;
        });
      }
    }
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, []);

  const doMerge = async () => {
    setMerging(true);
    try {
      const id = await api.mergeIntoTriathlon([...selected]);
      await queryClient.invalidateQueries({ queryKey: ["activities"] });
      exitSelect();
      navigate(`/activity/${id}`);
    } catch (e) {
      addToast("error", String(e));
    } finally {
      setMerging(false);
    }
  };

  const { data: pageSizeSetting } = useQuery({
    queryKey: ["setting", "page_size"],
    queryFn: () => api.getSetting("page_size"),
  });

  const pageSize = pageSizeSetting ? Number(pageSizeSetting) : DEFAULT_PAGE_SIZE;
  const [loadedCount, setLoadedCount] = useState(pageSize);

  // Reset loaded count when filters or pageSize change
  useEffect(() => {
    setLoadedCount(pageSize);
  }, [filters, pageSize]);

  const { data: activities, isLoading, isFetching } = useQuery({
    queryKey: ["activities", { ...filters, limit: loadedCount }],
    queryFn: () => api.getActivities({ ...filters, limit: loadedCount }),
    // Load more bumps `limit`, which is a NEW query key: without carrying
    // the previous page's data across, the list unmounts into the loading
    // placeholder for a beat and the scroll position collapses to the top —
    // exactly one wall of already-read activities lost per click.
    placeholderData: keepPreviousData,
  });

  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-12 text-muted">
        Loading activities...
      </div>
    );
  }

  if (!activities || activities.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center py-16 text-faint">
        <p className="text-lg mb-2">No activities yet</p>
        <p className="text-sm mb-5">Import GPX or FIT files to get started</p>
        <ImportDialog />
      </div>
    );
  }

  // While the next page fetches, `activities` is still the previous page
  // (placeholderData), shorter than the new loadedCount — keep the button
  // in place (disabled) instead of blinking it away and back.
  const hasMore = activities.length === loadedCount || isFetching;

  const mergeBlock = mergeBlockReason(activities.filter((a) => selected.has(a.id)));

  return (
    <div>
      {/* Selection toolbar — only visible in merge mode (toggled by Ctrl+M). */}
      {selecting && (
        <div className="flex items-center justify-end gap-2 px-4 py-2 text-sm">
          <span className="mr-auto text-muted">{selected.size} selected</span>
          <button
            onClick={exitSelect}
            className="px-3 py-1.5 rounded-lg border border-border text-muted hover:bg-card-2"
          >
            Cancel
          </button>
          <button
            onClick={doMerge}
            disabled={mergeBlock != null || merging}
            className="px-3 py-1.5 rounded-lg bg-accent text-white font-medium hover:bg-accent-2 disabled:opacity-50"
            data-tip={mergeBlock ?? "Combine the selected activities into one du/triathlon"}
          >
            {merging ? "Merging…" : "Merge into triathlon"}
          </button>
        </div>
      )}

      <div className="divide-y divide-border">
        {activities.map((activity) =>
          selecting ? (
            <button
              key={activity.id}
              onClick={() => toggle(activity.id)}
              className="w-full flex items-center gap-3 pl-4 text-left hover:bg-card-2"
            >
              <span
                className={`grid place-items-center w-5 h-5 rounded-md border shrink-0 ${
                  selected.has(activity.id)
                    ? "bg-accent border-accent text-white"
                    : "border-border-2"
                }`}
              >
                {selected.has(activity.id) && <Check size={13} />}
              </span>
              <span className="flex-1 min-w-0 pointer-events-none">
                <ActivityListItem activity={activity} onClick={() => {}} />
              </span>
            </button>
          ) : (
            <ActivityListItem
              key={activity.id}
              activity={activity}
              onClick={() => navigate(`/activity/${activity.id}`)}
            />
          ),
        )}
      </div>
      {hasMore && (
        <div className="flex justify-center py-4">
          <button
            onClick={() => setLoadedCount((c) => c + pageSize)}
            disabled={isFetching}
            className="text-sm font-medium px-4 py-2 rounded-lg border border-border bg-card text-muted hover:bg-card-2 hover:text-ink transition-colors disabled:opacity-60"
          >
            {isFetching ? "Loading…" : "Load more"}
          </button>
        </div>
      )}
    </div>
  );
}
