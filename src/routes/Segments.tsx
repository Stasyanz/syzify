import { useState } from "react";
import { useNavigate } from "react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Check, ChevronDown, ChevronRight, Pencil, Search, Trash2, Trophy, X } from "lucide-react";
import { api } from "../lib/tauri";
import { MAX_SEGMENT_NAME_LENGTH, type SegmentSummaryRow } from "../lib/types";
import {
  formatDate,
  formatDistance,
  formatDuration,
  formatGrade,
  formatPaceOrSpeed,
  formatPower,
} from "../lib/format";
import { SportGlyph } from "../components/brand/SportIcon";
import { confirmDialog } from "../stores/confirmStore";

/** Case-insensitive substring filter over segment names. Client-side by
 * design: the list is user-curated and small, a backend query would only
 * add a round trip. */
export function filterSegments(
  segments: SegmentSummaryRow[],
  query: string,
): SegmentSummaryRow[] {
  const q = query.trim().toLowerCase();
  if (!q) return segments;
  return segments.filter((s) => s.name.toLowerCase().includes(q));
}

/** The /segments page: every saved segment with rename/delete and an
 * expandable per-segment leaderboard (best effort first, click → activity). */
export function Segments() {
  const qc = useQueryClient();
  const [expanded, setExpanded] = useState<string | null>(null);
  const [editing, setEditing] = useState<{ id: string; draft: string } | null>(null);
  const [query, setQuery] = useState("");

  const { data: segments, isLoading } = useQuery({
    queryKey: ["segments"],
    queryFn: () => api.listSegments(),
  });

  const invalidate = () => {
    qc.invalidateQueries({ queryKey: ["segments"] });
    // Activity pages show segment names in their efforts panel.
    qc.invalidateQueries({ queryKey: ["segment-efforts"] });
    // Expanded leaderboards cache under their own key prefix.
    qc.invalidateQueries({ queryKey: ["segment-leaderboard"] });
  };

  const rename = useMutation({
    mutationFn: ({ id, name }: { id: string; name: string }) =>
      api.renameSegment(id, name),
    onSuccess: () => {
      setEditing(null);
      invalidate();
    },
  });

  const remove = useMutation({
    mutationFn: (id: string) => api.deleteSegment(id),
    onSuccess: invalidate,
  });

  const askDelete = async (s: SegmentSummaryRow) => {
    const ok = await confirmDialog({
      title: "Delete segment",
      message: `Delete “${s.name}” and all its recorded efforts? This cannot be undone.`,
      confirmLabel: "Delete",
      danger: true,
    });
    if (ok) remove.mutate(s.id);
  };

  const submitRename = () => {
    if (!editing) return;
    const name = editing.draft.trim();
    if (!name) return;
    rename.mutate({ id: editing.id, name });
  };

  // One switch for the whole table (segment rows AND expanded leaderboards):
  // per-row power cells would shift the table-fixed grid (#41/#42 lesson).
  const showPower = (segments ?? []).some((s) => s.best_effort_power_w != null);

  return (
    <div className="flex flex-col h-full">
      <main className="flex-1 overflow-y-auto scroll-themed">
        {/* Sticky header bar (#59): title + search stay put while the list
            scrolls underneath. Needs the opaque page background — an
            unpainted sticky element would show rows through itself. Spacing
            keeps the #57 rhythm: 12px above the title, more room below. */}
        {/* 1fr|auto|1fr grid: the search sits on the bar's true center line
            regardless of the title's width (flex justify-between pinned it
            to the right edge). */}
        <div className="sticky top-0 z-10 bg-bg grid grid-cols-[1fr_auto_1fr] items-center gap-4 px-6 pt-3 pb-2">
          <h2 className="!m-0 text-lg font-bold">Segments</h2>
          <div className="fsearch w-64">
            <Search size={15} />
            <input
              type="text"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Search segments…"
              aria-label="Search segments"
            />
            {query && (
              <span
                className="fsearch-x"
                title="Clear search"
                onClick={() => setQuery("")}
              >
                <X size={14} />
              </span>
            )}
          </div>
        </div>
        <div className="px-6 pb-6 pt-1">
          {isLoading ? (
            <p className="text-sm text-faint">Loading segments…</p>
          ) : !segments || segments.length === 0 ? (
            <div className="dash-card">
              <p className="text-sm text-muted">
                No segments yet. Drag-select a section of the elevation chart on
                an activity, then right-click the selection and choose Save
                segment — every past and future workout over the same route
                will be timed against it.
              </p>
            </div>
          ) : filterSegments(segments, query).length === 0 ? (
            <div className="dash-card">
              <p className="text-sm text-muted">
                No segments match “{query.trim()}”.
              </p>
            </div>
          ) : (
            <div className="dash-card">
              {/* table-fixed: expanding a leaderboard must not re-measure
                  column widths — auto layout makes every column jump. */}
              <table className="w-full text-sm table-fixed">
                <thead>
                  <tr className="text-faint text-xs uppercase tracking-wide">
                    <th className="w-6 pb-2" aria-label="Expand" />
                    <th className="font-semibold pb-2 text-left">Segment</th>
                    <th className="font-semibold pb-2 text-right w-24">Distance</th>
                    <th className="font-semibold pb-2 text-right w-20">Grade</th>
                    <th className="font-semibold pb-2 text-right w-20">Efforts</th>
                    {showPower && (
                      <th className="font-semibold pb-2 text-right w-20">Power</th>
                    )}
                    <th className="font-semibold pb-2 text-right w-20">Best</th>
                    <th className="w-16 pb-2" aria-label="Actions" />
                  </tr>
                </thead>
                <tbody>
                  {filterSegments(segments, query).map((s) => (
                    <SegmentRows
                      key={s.id}
                      segment={s}
                      showPower={showPower}
                      expanded={expanded === s.id}
                      onToggle={() => setExpanded(expanded === s.id ? null : s.id)}
                      editing={editing?.id === s.id ? editing.draft : null}
                      onEditStart={() => setEditing({ id: s.id, draft: s.name })}
                      onEditChange={(draft) => setEditing({ id: s.id, draft })}
                      onEditSubmit={submitRename}
                      onEditCancel={() => setEditing(null)}
                      onDelete={() => void askDelete(s)}
                      renameError={
                        editing?.id === s.id && rename.isError ? String(rename.error) : null
                      }
                    />
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>
      </main>
    </div>
  );
}

function SegmentRows({
  segment: s,
  showPower,
  expanded,
  onToggle,
  editing,
  onEditStart,
  onEditChange,
  onEditSubmit,
  onEditCancel,
  onDelete,
  renameError,
}: {
  segment: SegmentSummaryRow;
  /** The whole table shows a Power column (any segment has a powered pass). */
  showPower: boolean;
  expanded: boolean;
  onToggle: () => void;
  /** Non-null while this row's name is being edited (the draft value). */
  editing: string | null;
  onEditStart: () => void;
  onEditChange: (draft: string) => void;
  onEditSubmit: () => void;
  onEditCancel: () => void;
  onDelete: () => void;
  renameError: string | null;
}) {
  const Chevron = expanded ? ChevronDown : ChevronRight;
  return (
    <>
      <tr
        className="cursor-pointer border-t border-border transition-colors hover:bg-card-2"
        onClick={() => {
          if (editing == null) onToggle();
        }}
        aria-expanded={expanded}
      >
        <td className="py-2 text-faint">
          <Chevron size={14} aria-hidden />
        </td>
        <td className="py-2 pr-2 font-medium truncate">
          {editing != null ? (
            <span className="flex items-center gap-1.5" onClick={(e) => e.stopPropagation()}>
              <span className="field flex">
                <input
                  autoFocus
                  value={editing}
                  onChange={(e) =>
                    onEditChange(e.target.value.slice(0, MAX_SEGMENT_NAME_LENGTH))
                  }
                  onKeyDown={(e) => {
                    if (e.key === "Enter") onEditSubmit();
                    if (e.key === "Escape") onEditCancel();
                  }}
                />
              </span>
              <button
                className="text-faint hover:text-ink"
                aria-label="Save name"
                onClick={onEditSubmit}
              >
                <Check size={15} />
              </button>
              <button
                className="text-faint hover:text-ink"
                aria-label="Cancel rename"
                onClick={onEditCancel}
              >
                <X size={15} />
              </button>
              {renameError && <span className="text-xs text-red-600">{renameError}</span>}
            </span>
          ) : (
            <span className="inline-flex items-center gap-2">
              <SportGlyph sport={s.sport} size={14} />
              {s.name}
            </span>
          )}
        </td>
        <td className="py-2 text-right tabular-nums">{formatDistance(s.distance_m)}</td>
        <td className="py-2 text-right tabular-nums">
          {s.avg_grade_pct != null ? formatGrade(s.avg_grade_pct) : "--"}
        </td>
        <td className="py-2 text-right tabular-nums">{s.effort_count}</td>
        {showPower && (
          <td className="py-2 text-right tabular-nums">
            {formatPower(s.best_effort_power_w)}
          </td>
        )}
        <td className="py-2 text-right tabular-nums">{formatDuration(s.best_elapsed_s)}</td>
        <td className="py-2 text-right">
          <span className="inline-flex gap-2" onClick={(e) => e.stopPropagation()}>
            <button
              className="text-faint hover:text-ink"
              aria-label="Rename segment"
              onClick={onEditStart}
            >
              <Pencil size={14} />
            </button>
            <button
              className="text-faint hover:text-red-600"
              aria-label="Delete segment"
              onClick={onDelete}
            >
              <Trash2 size={14} />
            </button>
          </span>
        </td>
      </tr>
      {expanded && <LeaderboardRows segment={s} showPower={showPower} />}
    </>
  );
}

function LeaderboardRows({
  segment,
  showPower,
}: {
  segment: SegmentSummaryRow;
  showPower: boolean;
}) {
  const navigate = useNavigate();
  const { data: efforts, isLoading } = useQuery({
    queryKey: ["segment-leaderboard", segment.id],
    queryFn: () => api.getSegmentEfforts(segment.id),
  });

  if (isLoading) {
    return (
      <tr>
        <td colSpan={showPower ? 8 : 7} className="py-2 pl-8 text-xs text-faint">
          Loading efforts…
        </td>
      </tr>
    );
  }
  if (!efforts || efforts.length === 0) {
    return (
      <tr>
        <td colSpan={showPower ? 8 : 7} className="py-2 pl-8 text-xs text-faint">
          No efforts yet — ride or run this route and import the workout.
        </td>
      </tr>
    );
  }
  return (
    <>
      {efforts.map((e) => (
        <tr
          key={e.id}
          className="cursor-pointer border-t border-border/50 bg-card-2/40 text-xs hover:bg-card-2"
          onClick={() => navigate(`/activity/${e.activity_id}`)}
          title="Open activity"
        >
          <td />
          <td className="py-1.5 pl-6 truncate">
            <span className="inline-flex items-center gap-1.5">
              {e.rank === 1 && efforts.length > 1 && (
                <Trophy size={11} className="text-accent-2" aria-label="Best effort" />
              )}
              <span className="tabular-nums text-faint">{e.rank != null ? `#${e.rank}` : "—"}</span>
              <span className="font-medium">{e.activity_title ?? "Untitled"}</span>
              <span className="text-faint">{formatDate(e.start_time)}</span>
            </span>
          </td>
          <td className="py-1.5 text-right tabular-nums">{formatDistance(e.distance_m)}</td>
          <td className="py-1.5 text-right tabular-nums text-faint">
            {e.elapsed_s != null
              ? formatPaceOrSpeed(segment.sport, e.distance_m / e.elapsed_s)
              : "--"}
          </td>
          {showPower && (
            <>
              {/* Empty slot under Efforts — rank already lives by the name. */}
              <td />
              <td className="py-1.5 text-right tabular-nums text-faint">
                {formatPower(e.avg_power_w)}
              </td>
            </>
          )}
          <td className="py-1.5 text-right tabular-nums" colSpan={showPower ? 1 : 2}>
            {formatDuration(e.elapsed_s)}
          </td>
          <td />
        </tr>
      ))}
    </>
  );
}
