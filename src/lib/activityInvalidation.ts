import type { QueryClient } from "@tanstack/react-query";

/**
 * Every query-key prefix whose data is derived from the set of activities.
 * Prefix invalidation covers the parameterized variants (filters, year/month,
 * per-activity ids).
 */
const ACTIVITY_DATA_PREFIXES: readonly string[] = [
  "activities", // library list + year-range
  "activity-locations", // library map markers
  "calendar",
  "dashboard",
  "usedSportTypes", // sport filter options
  "recordBadges", // a new best effort can displace another activity's badge
  "adjacent", // prev/next navigation between activities
  "segment-efforts", // sport change / delete rematches segment passes
];

/**
 * Invalidate everything derived from the activity set. Call after any
 * mutation that changes it: import, delete, merge/unmerge, or an edit that
 * can reshape aggregates (sport change). Per-activity keys like
 * ["activity", id] stay the caller's job.
 */
export function invalidateActivityData(queryClient: QueryClient): Promise<void> {
  return Promise.all(
    ACTIVITY_DATA_PREFIXES.map((prefix) =>
      queryClient.invalidateQueries({ queryKey: [prefix] }),
    ),
  ).then(() => undefined);
}
