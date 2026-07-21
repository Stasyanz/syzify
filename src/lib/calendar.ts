// Shared month-grid math for the calendar views (library CalendarView, dashboard
// MiniCalendar, FilterDrawer date picker). Monday-first, 1-based month.

/** Weekday headers, Monday-first. */
export const WEEKDAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/** Monday-based day-of-week index (0 = Mon … 6 = Sun) of the 1st of the month.
 * `month` is 1-based (1 = Jan … 12 = Dec). */
export function firstWeekday(year: number, month: number): number {
  const day = new Date(year, month - 1, 1).getDay(); // 0 = Sun … 6 = Sat
  return (day + 6) % 7;
}

/** Number of days in a 1-based month (handles leap Februaries). */
export function daysInMonth(year: number, month: number): number {
  return new Date(year, month, 0).getDate();
}

/** The month laid out as a Monday-first grid padded to whole weeks: leading and
 * trailing `null`s for the blank cells, real days as 1…daysInMonth. The length
 * is always a multiple of 7.
 *
 * `minWeeks` additionally pads to that many rows. Months span 4–6 weeks, so
 * fixed-height widgets (the date-picker popup, the dashboard mini calendar)
 * pass 6 — otherwise their height jumps when navigating between a 5-week and
 * a 6-week month. The full-page CalendarView deliberately doesn't. */
export function buildMonthGrid(
  year: number,
  month: number,
  minWeeks = 0,
): (number | null)[] {
  const lead = firstWeekday(year, month);
  const total = daysInMonth(year, month);
  const cells: (number | null)[] = [];
  for (let i = 0; i < lead; i++) cells.push(null);
  for (let d = 1; d <= total; d++) cells.push(d);
  while (cells.length % 7 !== 0 || cells.length < minWeeks * 7) cells.push(null);
  return cells;
}

/** Sports actually present in a month's day summaries, most frequent first
 * (ties keep first-seen order), capped at `limit`. Feeds the calendar legend —
 * a hardcoded list advertised sports the user never does and missed the ones
 * they do. */
export function monthSports(
  days: { activities: { sport_type: string }[] }[],
  limit = 6
): string[] {
  const counts = new Map<string, number>();
  for (const day of days) {
    for (const a of day.activities) {
      counts.set(a.sport_type, (counts.get(a.sport_type) ?? 0) + 1);
    }
  }
  return Array.from(counts.entries())
    .sort((a, b) => b[1] - a[1])
    .slice(0, limit)
    .map(([sport]) => sport);
}
