import type uPlot from "uplot";

/** Fit the y-axis to its actual tick labels instead of reserving a fixed
 * 48px: "80" needs half of what "20:00" does, and the difference is plot
 * width. measureText is transform-independent, so the plain 11px font gives
 * CSS-px widths — exactly at a whole pixel ratio; at 125 % or 150 % uPlot
 * rounds its own font size and a label can sit half a device pixel off. 15
 * covers uPlot's default tick marks (10) + label gap (5). A fixed width is
 * also how a label gets cut: "1,000" needs 43px and lost its left edge in
 * the 40px the Power Curve used to reserve (#219).
 *
 * The context's font is put back afterwards. uPlot remembers the font it
 * last set and skips setting it again, so a font left behind here would be
 * the one the labels are drawn in on the next redraw that keeps the canvas —
 * half size on a 2x screen. */
export function yAxisSize(u: uPlot, values: string[] | null): number {
  if (!values || values.length === 0) return 40;
  const font = u.ctx.font;
  u.ctx.font = "11px sans-serif";
  const widest = Math.max(...values.map((v) => u.ctx.measureText(v).width));
  u.ctx.font = font;
  return Math.max(28, Math.ceil(widest) + 15);
}
