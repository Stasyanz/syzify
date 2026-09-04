// Theme-aware colors for canvas charts (uPlot), read from the live CSS
// tokens so charts stay legible in both light and dark mode. Charts
// re-render on theme and data changes, picking up the current values.

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

/** Primary text color — canvas-drawn annotations (the avg label). */
export function chartInkColor(): string {
  return readVar("--ink", "#221f1a");
}

/** Card/page surface color — the halo behind canvas-drawn text so it stays
 * legible over bars and lines. */
export function chartSurfaceColor(): string {
  return readVar("--surface", "#faf7f1");
}
