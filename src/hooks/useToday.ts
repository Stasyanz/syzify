import {
  useEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
  type Dispatch,
  type SetStateAction,
} from "react";
import { useQueryClient, type QueryKey } from "@tanstack/react-query";
import { dateOfDayKey, dayKey } from "../lib/calendar";

// The current LOCAL calendar day as an external store. `new Date()` read
// during render goes stale the moment the app outlives midnight — nothing
// re-rendered the calendars, so "today" stayed on yesterday until some
// unrelated action. ONE ticker serves every subscriber (the dashboard alone
// mounts three), so two widgets on one screen can never disagree about the
// day: there is a single value, not one interval per component each with
// its own phase.

/** How often the day is re-checked. A minute keeps the midnight rollover
 * unnoticeable without a wake-up per second — and this interval is the one
 * guaranteed signal: after the machine slept through midnight the window may
 * come back already visible and focused, so neither event below fires, and
 * the next tick is what catches up (within a minute of waking). */
const POLL_MS = 60_000;

let currentKey = dayKey(new Date());
const listeners = new Set<() => void>();
let timer: ReturnType<typeof setInterval> | null = null;

function check() {
  const now = dayKey(new Date());
  if (now === currentKey) return;
  currentKey = now;
  for (const notify of listeners) notify();
}

// Timers sleep while the app is in the background; when the window comes
// back (tab shown, app focused) the first paint should already be right
// rather than wait for the tick.
function onVisible() {
  if (document.visibilityState === "visible") check();
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  if (listeners.size === 1) {
    // Nobody was watching in between — catch up before the first read.
    check();
    timer = setInterval(check, POLL_MS);
    document.addEventListener("visibilitychange", onVisible);
    window.addEventListener("focus", check);
  }
  return () => {
    listeners.delete(listener);
    if (listeners.size === 0 && timer != null) {
      clearInterval(timer);
      timer = null;
      document.removeEventListener("visibilitychange", onVisible);
      window.removeEventListener("focus", check);
    }
  };
}

// While nobody is subscribed there is no ticker keeping `currentKey` fresh,
// so the first read after an idle stretch (or after module load) computes
// it on the spot — the initial render then already sees the right day, and
// the subscribe-time catch-up below never looks like a day change to it.
function getSnapshot(): string {
  if (timer == null) currentKey = dayKey(new Date());
  return currentKey;
}

/** The current local day as "YYYY-MM-DD" (the backend's day-bucket key),
 * re-rendering the subscriber when it changes. */
export function useTodayKey(): string {
  return useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
}

/** The current local day as a Date at the start of that day; a NEW instance
 * only when the day changes, so it can sit in dependency lists. */
export function useToday(): Date {
  const key = useTodayKey();
  return useMemo(() => dateOfDayKey(key), [key]);
}

/** Invalidate a React Query key when the day changes — for data the backend
 * cuts at ITS clock (`date('now','localtime')`: the dashboard's This Week
 * tiles and 7-day volume). Invalidation keeps the cached data on screen
 * while it refetches; putting the day INTO the key would instead start a
 * fresh pending query at midnight, flash the loading state and unmount the
 * page's children (with their month-view state). Not on mount: the query
 * fetches then anyway. */
export function useInvalidateOnNewDay(queryKey: QueryKey): void {
  const queryClient = useQueryClient();
  const key = useTodayKey();
  const seen = useRef(key);
  useEffect(() => {
    if (seen.current === key) return;
    seen.current = key;
    queryClient.invalidateQueries({ queryKey });
    // `queryKey` is a fresh array literal per render; the `seen` guard keeps
    // those re-runs no-ops — only a DAY change reaches invalidateQueries.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, queryClient]);
}

/** Month-view state (1-based month) seeded with `today`'s month. When the
 * day rolls into a new month it follows — but ONLY while the view still
 * shows the month today used to be in: a user browsing March in October is
 * left where they are, a user looking at "this month" keeps looking at this
 * month. Shared by the dashboard MiniCalendar and the Library CalendarView. */
export function useMonthView(today: Date): {
  year: number;
  month: number;
  // useState setters verbatim — the Dispatch type says they are stable, so
  // callers' dependency lists can leave them out as they always did.
  setYear: Dispatch<SetStateAction<number>>;
  setMonth: Dispatch<SetStateAction<number>>;
} {
  const [year, setYear] = useState(today.getFullYear());
  const [month, setMonth] = useState(today.getMonth() + 1);
  const prevToday = useRef(today);
  useEffect(() => {
    const prev = prevToday.current;
    prevToday.current = today;
    if (prev === today) return;
    const wasOnTodaysMonth =
      year === prev.getFullYear() && month === prev.getMonth() + 1;
    if (!wasOnTodaysMonth) return;
    setYear(today.getFullYear());
    setMonth(today.getMonth() + 1);
    // year/month are read, not tracked: the effect answers a DAY change
    // only; the user's own paging must not re-trigger it.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [today]);
  return { year, month, setYear, setMonth };
}
