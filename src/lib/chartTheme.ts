// Theme-aware colors for chart.js, read from the live CSS tokens so charts
// stay legible in both light and dark mode. Charts re-render on the frequent
// period/metric/grouping changes, picking up the current theme's values.

function readVar(name: string, fallback: string): string {
  if (typeof document === "undefined") return fallback;
  const v = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  return v || fallback;
}

/** Axis tick / legend label color (secondary text). */
export function chartTextColor(): string {
  return readVar("--muted", "#6f675a");
}

/** Grid line color (default border). */
export function chartGridColor(): string {
  return readVar("--border", "#e6dfd1");
}
