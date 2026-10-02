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

/** The light theme's five climb-grade steps (gentle → wall), the fallback
 * when no token is set. Kept here, not in App.css only, so the pure chart
 * helpers and their tests see the same ladder. */
export const GRADE_STEPS_LIGHT = ["#b98e03", "#b66f02", "#b44a00", "#a82309", "#8e0014"];

/** The dark theme's five climb-grade steps, as App.css sets them —
 * exported for the palette tests, which measure both ladders. */
export const GRADE_STEPS_DARK = ["#feda92", "#ffba70", "#fe975b", "#fe6f4a", "#eb5049"];

/** The five climb-grade steps of the live theme (#126): the dark card
 * needs a ladder of its own — the light ladder's wall red disappears on it. */
export function chartGradeSteps(): string[] {
  return GRADE_STEPS_LIGHT.map((fallback, i) => readVar(`--grade-${i + 1}`, fallback));
}
