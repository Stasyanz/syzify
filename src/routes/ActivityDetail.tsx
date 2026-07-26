import { useState, useRef, useEffect, useCallback, useMemo } from "react";
import { useParams, useNavigate } from "react-router";
import { useQuery, useQueryClient, useMutation } from "@tanstack/react-query";
import { save } from "@tauri-apps/plugin-dialog";
import { Download, Pencil, ChevronLeft, ChevronRight, Share2, Trophy, Check, Split } from "lucide-react";
import { api } from "../lib/tauri";
import { invalidateActivityData } from "../lib/activityInvalidation";
import {
  formatDistance,
  formatElevation,
  formatDurationHM,
  formatPaceOrSpeed,
  paceOrSpeedLabel,
} from "../lib/format";
import { useUnits } from "../lib/units";
import { SportIcon, SportGlyph } from "../components/brand/SportIcon";
import { getSportColor } from "../lib/sportColors";
import { SummaryPanel } from "../components/activity/SummaryPanel";
import { RouteMap } from "../components/activity/RouteMap";
import { ChartPanel } from "../components/activity/ChartPanel";
import { CyclingDynamicsPanel } from "../components/activity/CyclingDynamicsPanel";
import { MultisportLegs } from "../components/activity/MultisportLegs";
import { isFocusableLeg, legTimeWindow, sliceTrackpoints } from "../components/activity/legFocus";
import { LapsTable } from "../components/activity/LapsTable";
import { EditActivityModal } from "../components/activity/EditActivityModal";
import { PhotoGallery } from "../components/activity/PhotoGallery";
import { ShareModal } from "../components/activity/ShareModal";
import { PluginContributions } from "../components/plugins/PluginContributions";
import { ActionMenu } from "../components/ui/ActionMenu";
import type { Photo } from "../lib/types";
import { useActivityStore } from "../stores/activityStore";
import { useToastStore } from "../stores/toastStore";
import { SPORT_LABELS, SPORT_TYPES, MAX_TAGS_PER_ACTIVITY, type SportType } from "../lib/types";

/** Strava's smallest privacy-zone radius — hides this much track around the
 * start and finish in the privacy GPX export. */
const PRIVACY_RADIUS_M = 200;

function SportBadge({ activityId, sportType, onChanged }: { activityId: string; sportType: string; onChanged: () => void }) {
  const addToast = useToastStore((s) => s.addToast);
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const label = SPORT_LABELS[sportType as SportType] ?? sportType;

  useEffect(() => {
    if (!open) return;
    function handleClick(e: MouseEvent) {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    }
    document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, [open]);

  const mutation = useMutation({
    mutationFn: (newSport: string) =>
      api.updateActivity(activityId, { sport_type: newSport }),
    onSuccess: () => {
      addToast("success", "Sport type updated");
      onChanged();
      setOpen(false);
    },
    onError: (err: Error) => addToast("error", err.message),
  });

  return (
    <div className="relative shrink-0" ref={ref}>
      <button
        onClick={() => setOpen(!open)}
        title={`${label} — change sport`}
        className="block rounded-xl hover:opacity-90 transition-opacity"
      >
        <SportIcon sport={sportType} size={40} />
      </button>
      {open && (
        <div className="absolute top-full left-0 mt-1.5 bg-card border border-border rounded-card shadow-xl py-1 z-20 w-52 max-h-80 overflow-y-auto scroll-themed">
          {SPORT_TYPES.map((st) => {
            const isSel = st === sportType;
            return (
              <button
                key={st}
                onClick={() => { if (!isSel) mutation.mutate(st); else setOpen(false); }}
                className={`flex w-full items-center gap-2.5 px-3 py-1.5 text-left text-[13px] ${
                  isSel ? "bg-accent-soft text-accent-2 font-medium" : "text-ink hover:bg-card-2"
                }`}
              >
                <span
                  className="grid h-6 w-6 shrink-0 place-items-center rounded-md text-white"
                  style={{ background: getSportColor(st) }}
                >
                  <SportGlyph sport={st} size={13} />
                </span>
                <span className="truncate">{SPORT_LABELS[st]}</span>
                {isSel && <Check size={14} className="ml-auto shrink-0" />}
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}

export function ActivityDetailPage() {
  useUnits();
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const addToast = useToastStore((s) => s.addToast);
  const setFilters = useActivityStore((s) => s.setFilters);
  const setHoveredPointIndex = useActivityStore((s) => s.setHoveredPointIndex);
  const scrollRef = useRef<HTMLDivElement>(null);
  const [exporting, setExporting] = useState(false);
  const [editing, setEditing] = useState(false);
  const [sharing, setSharing] = useState<Photo | "pick" | null>(null);
  // FIT-native leg focused in place (leg_number) — windows the map/charts/laps
  // to that leg's time range. Merged legs navigate away instead.
  const [focusedLegNo, setFocusedLegNo] = useState<number | null>(null);

  // Reset state and scroll when navigating between activities
  useEffect(() => {
    setEditing(false);
    setExporting(false);
    setSharing(null);
    setFocusedLegNo(null);
    scrollRef.current?.scrollTo(0, 0);
  }, [id]);

  const { data: allTags = [] } = useQuery({
    queryKey: ["tags"],
    queryFn: () => api.getTags(),
  });

  const { data, isLoading, error } = useQuery({
    queryKey: ["activity", id],
    queryFn: () => api.getActivityDetail(id!),
    enabled: !!id,
  });

  const { data: adjacent } = useQuery({
    queryKey: ["adjacent", id],
    queryFn: () => api.getAdjacentActivities(id!),
    enabled: !!id,
  });

  // A merged leg links back to its container. The full-detail fetch is cheap
  // here — a container owns no track — and is usually already cached from
  // navigating in through the container's Legs card.
  const parentId = data?.activity.parent_id ?? null;
  const { data: parent } = useQuery({
    queryKey: ["activity", parentId],
    queryFn: () => api.getActivityDetail(parentId!),
    enabled: !!parentId,
  });

  const { data: recordBadges = [] } = useQuery({
    queryKey: ["recordBadges", id],
    queryFn: () => api.getActivityRecordBadges(id!),
    enabled: !!id,
  });

  const goToPrev = useCallback(() => {
    if (adjacent?.prev_id) navigate(`/activity/${adjacent.prev_id}`);
  }, [adjacent, navigate]);

  const goToNext = useCallback(() => {
    if (adjacent?.next_id) navigate(`/activity/${adjacent.next_id}`);
  }, [adjacent, navigate]);

  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      // Don't navigate when typing in inputs
      const tag = (e.target as HTMLElement).tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;
      if (e.key === "ArrowLeft") goToPrev();
      if (e.key === "ArrowRight") goToNext();
      if (e.key === "Escape") navigate("/library");
    }
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [goToPrev, goToNext]);

  // The focused leg's view of the track. Memoized: an unmemoized slice would
  // be a fresh object every render, and the map/charts key their expensive
  // rebuilds off trackpoints identity.
  const focusedLeg = useMemo(() => {
    const leg = data?.legs.find((l) => l.leg_number === focusedLegNo);
    return leg && isFocusableLeg(leg) ? leg : undefined;
  }, [data, focusedLegNo]);
  const viewTrackpoints = useMemo(() => {
    if (!data) return null;
    const win = focusedLeg ? legTimeWindow(focusedLeg) : null;
    return win ? sliceTrackpoints(data.trackpoints, win[0], win[1]) : data.trackpoints;
  }, [data, focusedLeg]);

  const toggleLegFocus = useCallback(
    (legNumber: number) => {
      setFocusedLegNo((cur) => (cur === legNumber ? null : legNumber));
      // Hover indices refer to the current (sliced) arrays — a stale index
      // from the previous view would pin the map marker to a wrong point.
      setHoveredPointIndex(null);
    },
    [setHoveredPointIndex],
  );

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-full text-muted">
        Loading activity...
      </div>
    );
  }

  if (error || !data) {
    return (
      <div className="flex items-center justify-center h-full text-red-500">
        Failed to load activity
      </div>
    );
  }

  const { activity, trackpoints, tags, laps } = data;
  // A merged triathlon container: its legs link back to standalone activities
  // and it owns no track. Its map/charts/laps live on the legs' pages.
  const isMergedContainer = data.legs.some((l) => l.source_activity_id);
  const sportLabel =
    SPORT_LABELS[activity.sport_type as SportType] ?? "Activity";

  // Meta line "Tue, Jun 2, 2026 · 18:05 · Amsterdam" — weekday/date · time · place.
  const start = new Date(activity.start_time);
  const metaParts = [
    start.toLocaleDateString(undefined, {
      weekday: "short",
      month: "short",
      day: "numeric",
      year: "numeric",
    }),
    start.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" }),
  ];
  if (activity.location_name) metaParts.push(activity.location_name);
  const metaLine = metaParts.join(" · ");
  // Long-form date for the meta tooltip (full weekday/month names).
  const metaTitle = start.toLocaleString(undefined, {
    weekday: "long",
    day: "numeric",
    month: "long",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });

  // Trophy chips: format each record badge from the activity's own metric.
  const badgeView = (kind: string): { value: string; label: string } => {
    switch (kind) {
      case "distance":
        return { value: formatDistance(activity.distance_m), label: "Longest distance" };
      case "elevation":
        return { value: formatElevation(activity.elev_gain_m), label: "Highest climb" };
      case "duration":
        return { value: formatDurationHM(activity.duration_s), label: "Longest duration" };
      case "pace":
        return {
          value: formatPaceOrSpeed(activity.sport_type, activity.avg_speed_mps),
          label: `Fastest ${paceOrSpeedLabel(activity.sport_type).toLowerCase()}`,
        };
      default:
        return { value: "", label: "" };
    }
  };

  async function handleExportGpx(privacyRadiusM?: number) {
    const suffix = privacyRadiusM ? "_private" : "";
    const defaultName = `${activity.start_time.slice(0, 10)}_${activity.sport_type}${suffix}.gpx`;
    const dest = await save({
      defaultPath: defaultName,
      filters: [{ name: "GPX", extensions: ["gpx"] }],
    });
    if (!dest) return;
    setExporting(true);
    try {
      await api.exportActivityGpx(activity.id, dest, privacyRadiusM);
    } finally {
      setExporting(false);
    }
  }

  async function handleUnmerge() {
    try {
      await api.unmergeTriathlon(activity.id);
      await invalidateActivityData(queryClient);
      navigate("/library");
    } catch (e) {
      addToast("error", String(e));
    }
  }

  return (
    <div className="flex flex-col h-full min-h-0">
      {/* Header — fixed top bar with a bottom divider (matches the design) */}
      <div className="flex items-center justify-between gap-3 px-4 py-3 border-b border-border shrink-0">
        <div className="flex items-center gap-2 min-w-0">
          <button
            onClick={() => navigate("/library")}
            className="grid place-items-center w-9 h-9 rounded-xl border border-border-2 bg-card text-muted hover:text-ink hover:bg-card-2 hover:border-faint transition-colors shrink-0"
            title="Back to library (Esc)"
          >
            <ChevronLeft size={18} />
          </button>
          <SportBadge
            activityId={activity.id}
            sportType={activity.sport_type}
            onChanged={() => {
              queryClient.invalidateQueries({ queryKey: ["activity", id] });
              // A sport change reshapes aggregates everywhere (dashboard,
              // calendar colors, filters) and recomputes record eligibility.
              invalidateActivityData(queryClient);
            }}
          />
          <div className="min-w-0">
            <h1
              className="text-[21px] font-extrabold tracking-tight text-ink truncate leading-tight"
              style={{ fontFamily: "var(--font-head)" }}
            >
              {activity.title ?? sportLabel}
            </h1>
            <div className="flex items-center gap-2 mt-0.5 min-w-0 overflow-hidden">
              <span className="text-sm text-muted truncate" title={metaTitle}>{metaLine}</span>
              {activity.parent_id && (
                <button
                  onClick={() => navigate(`/activity/${activity.parent_id}`)}
                  className="flex items-center gap-1 text-xs bg-accent-soft text-accent-2 px-2 py-0.5 rounded hover:opacity-80 cursor-pointer whitespace-nowrap shrink-0"
                  data-tip="Open the multisport event this leg belongs to"
                >
                  <Trophy size={11} />
                  {parent?.activity.title ?? "Multisport"}
                </button>
              )}
              {tags.slice(0, MAX_TAGS_PER_ACTIVITY).map((tagName) => {
                const tagObj = allTags.find((t) => t.name === tagName);
                return (
                  <button
                    key={tagName}
                    onClick={() => {
                      if (tagObj) {
                        setFilters({ tag_ids: [tagObj.id] });
                        navigate("/library");
                      }
                    }}
                    className="text-xs bg-accent-soft text-accent-2 px-2 py-0.5 rounded hover:opacity-80 cursor-pointer whitespace-nowrap shrink-0"
                  >
                    {tagName}
                  </button>
                );
              })}
              {tags.length > MAX_TAGS_PER_ACTIVITY && (
                <span
                  className="text-xs text-faint whitespace-nowrap shrink-0"
                  title={tags.slice(MAX_TAGS_PER_ACTIVITY).join(", ")}
                >
                  +{tags.length - MAX_TAGS_PER_ACTIVITY}
                </span>
              )}
            </div>
          </div>
        </div>

        <div className="flex items-center gap-2.5 shrink-0">
          {recordBadges.length > 0 && (
            <div className="det-badges-inline">
              {recordBadges.map((b) => {
                const { value, label } = badgeView(b.kind);
                return (
                  <div
                    key={b.kind}
                    className="badge-pill"
                    data-tip={`${label} · ${b.all_time ? `all-time ${sportLabel} record` : `${sportLabel} personal best`}`}
                  >
                    <Trophy size={13} />
                    <span>{value}</span>
                  </div>
                );
              })}
            </div>
          )}

          <div className="flex items-center gap-1">
            <button
              onClick={goToPrev}
              disabled={!adjacent?.prev_id}
              className="grid place-items-center w-8 h-8 rounded-md text-faint hover:text-ink hover:bg-card-2 transition-colors disabled:opacity-40 disabled:hover:bg-transparent disabled:hover:text-faint disabled:cursor-not-allowed"
              data-tip="Previous activity (←)"
              aria-label="Previous activity"
            >
              <ChevronLeft size={18} />
            </button>
            <button
              onClick={goToNext}
              disabled={!adjacent?.next_id}
              className="grid place-items-center w-8 h-8 rounded-md text-faint hover:text-ink hover:bg-card-2 transition-colors disabled:opacity-40 disabled:hover:bg-transparent disabled:hover:text-faint disabled:cursor-not-allowed"
              data-tip="Next activity (→)"
              aria-label="Next activity"
            >
              <ChevronRight size={18} />
            </button>
            <span className="w-px h-[22px] bg-border mx-1" />
            <button
              onClick={() => setEditing(true)}
              className="grid place-items-center w-9 h-9 rounded-xl border border-border-2 bg-card text-muted hover:text-ink hover:bg-card-2 hover:border-faint transition-colors"
              data-tip="Edit"
              aria-label="Edit"
            >
              <Pencil size={16} />
            </button>
            <button
              onClick={() => setSharing("pick")}
              className="grid place-items-center w-9 h-9 rounded-xl border border-border-2 bg-card text-muted hover:text-ink hover:bg-card-2 hover:border-faint transition-colors"
              data-tip="Share"
              aria-label="Share"
            >
              <Share2 size={16} />
            </button>
            <ActionMenu
              ariaLabel="Export GPX"
              tip="Export GPX"
              disabled={exporting}
              className="grid place-items-center w-9 h-9 rounded-xl border border-border-2 bg-card text-muted hover:text-ink hover:bg-card-2 hover:border-faint transition-colors disabled:opacity-50"
              items={[
                {
                  label: "Original GPX",
                  hint: "Full track as recorded",
                  onSelect: () => handleExportGpx(),
                },
                {
                  label: "With privacy zone",
                  hint: `Hides ${PRIVACY_RADIUS_M} m around start & finish`,
                  onSelect: () => handleExportGpx(PRIVACY_RADIUS_M),
                },
              ]}
            >
              <Download size={16} />
            </ActionMenu>
            {/* Unmerge — only on a merged container (legs link back to
                standalone activities). Frees the legs and deletes the
                container, then returns to the library. */}
            {data.legs.some((l) => l.source_activity_id) && (
              <button
                onClick={handleUnmerge}
                className="grid place-items-center w-9 h-9 rounded-xl border border-border-2 bg-card text-muted hover:text-ink hover:bg-card-2 hover:border-faint transition-colors"
                data-tip="Unmerge"
                aria-label="Unmerge"
              >
                <Split size={16} />
              </button>
            )}
          </div>
        </div>
      </div>

      {/* Scroll body */}
      <div className="flex-1 overflow-y-auto" ref={scrollRef}>
        <div className="p-6 space-y-6">
          {/* Summary */}
          <SummaryPanel activity={activity} />

          {/* Multisport legs (triathlon breakdown) — absent for single-sport */}
          <MultisportLegs
            legs={data.legs}
            focusedLeg={focusedLegNo}
            onFocusLeg={toggleLegFocus}
          />

          {/* A merged triathlon container carries no track of its own — the
              map, charts and laps live on each leg's page. Show them only for
              activities that actually have trackpoints. */}
          {!isMergedContainer && (
            <>
              {/* Map/charts/laps window to the focused leg's slice: its sport
                  picks the right pace metric, keys force clean remounts on
                  focus change, and stored laps are dropped in favor of
                  auto-splitting the slice (they span the whole activity). */}
              {/* Map — key forces full remount so Leaflet reinitializes cleanly */}
              <RouteMap
                key={`${id}-${focusedLeg?.leg_number ?? "all"}`}
                trackpoints={viewTrackpoints!}
                sport={focusedLeg?.sport_type ?? activity.sport_type}
                activityId={activity.id}
              />

              {/* Charts — key forces uPlot remount on navigation */}
              <ChartPanel
                key={`chart-${id}-${focusedLeg?.leg_number ?? "all"}`}
                trackpoints={viewTrackpoints!}
                sport={focusedLeg?.sport_type ?? activity.sport_type}
                timeInZones={data.time_in_zones}
                ftpW={activity.threshold_power_w}
              />

              <CyclingDynamicsPanel activity={activity} />

              {/* Laps */}
              <LapsTable
                laps={focusedLeg ? [] : laps}
                trackpoints={viewTrackpoints!}
                sport={focusedLeg?.sport_type ?? activity.sport_type}
              />
            </>
          )}

          {/* Photos */}
          <PhotoGallery activityId={activity.id} onShare={(p) => setSharing(p)} />

          {/* Plugin panels */}
          <PluginContributions
            point="activity.detail.panel"
            context={JSON.stringify({ activity_id: activity.id })}
            className="space-y-4"
          />

          {/* Notes */}
          {activity.notes && (
            <div>
              <h3 className="text-sm font-medium text-muted mb-1">Notes</h3>
              <p className="text-ink whitespace-pre-wrap">{activity.notes}</p>
            </div>
          )}
        </div>
      </div>

      {sharing && (
        <ShareModal
          activity={activity}
          trackpoints={trackpoints}
          initialPhoto={sharing === "pick" ? null : sharing}
          onClose={() => setSharing(null)}
        />
      )}

      {editing && (
        <EditActivityModal
          activity={activity}
          currentTags={tags}
          onClose={() => setEditing(false)}
          onSaved={() => {
            setEditing(false);
            queryClient.invalidateQueries({ queryKey: ["activity", id] });
            // The edit modal can change sport_type → aggregates and badges.
            invalidateActivityData(queryClient);
          }}
          onDeleted={() => {
            invalidateActivityData(queryClient);
            navigate("/library");
          }}
        />
      )}
    </div>
  );
}
