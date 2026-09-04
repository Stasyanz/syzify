// The "avg" reference line drawn on every metric chart (ChartPanel). Pure
// helpers only — the canvas drawing itself lives in SingleChart's draw hook.
//
// Where the value comes from, in order: the device's own summary average
// (the same number the summary tiles show, so the two never disagree), and
// only without one — GPX-only imports, a focused leg's power/cadence — the
// time-weighted mean of the recorded samples below.

/** Longest gap one sample is allowed to stand for, in seconds. Smart
 * recording spaces samples 1–8 s apart; an auto-pause or a dropped sensor
 * leaves gaps of minutes that must not let one sample dominate the mean
 * (the lesson from the power-curve review — see the memory on it). */
const MAX_SAMPLE_WEIGHT_S = 10;

/** Time-weighted mean of the recorded samples; null when there are none.
 * Each sample stands for the interval up to the next one (capped at
 * MAX_SAMPLE_WEIGHT_S), which is what the head unit's own average does with
 * 1 Hz data; without timestamps every sample weighs the same. Gaps (null =
 * no reading: HR before the strap catches, a dropped power meter) are NOT
 * samples and are skipped; recorded ZEROS count — coasting is part of the
 * ride's average power just as on the head unit's default "include zeros".
 * `keep` filters samples (pace excludes near-stops). Mind that ChartPanel
 * draws those gaps as 0 for uPlot — the line is the mean of the RECORDED
 * data, not of the zero-filled pixels. */
export function seriesMean(
  values: (number | null)[],
  t?: (number | null)[],
  keep: (v: number) => boolean = () => true,
): number | null {
  let sum = 0;
  let wsum = 0;
  const n = values.length;
  for (let i = 0; i < n; i++) {
    const v = values[i];
    if (v == null || !isFinite(v) || !keep(v)) continue;
    let w = 1;
    if (t) {
      const t0 = t[i];
      const t1 = i + 1 < n ? t[i + 1] : null;
      if (t0 != null && t1 != null && t1 > t0) w = Math.min(t1 - t0, MAX_SAMPLE_WEIGHT_S);
    }
    sum += v * w;
    wsum += w;
  }
  return wsum > 0 ? sum / wsum : null;
}

/** Average pace (min per `perUnit` meters) from per-point speeds (m/s): the
 * mean of the SPEEDS, converted once. Averaging per-point pace values would
 * be biased toward the slow end (a 10 min/km crawl weighs twice a 5 min/km
 * stride over the same time). Samples at or below `cutoff` are near-stops
 * that the pace series itself hides — they are skipped here the same way. */
export function meanPace(
  speedsMps: (number | null)[],
  cutoff: number,
  perUnit: number,
  t?: (number | null)[],
): number | null {
  const mean = seriesMean(speedsMps, t, (v) => v > cutoff);
  return mean != null && mean > 0 ? perUnit / mean / 60 : null;
}

/** Pace (min per `perUnit` meters) of a summary speed (m/s). */
export function paceOfSpeed(speedMps: number | null | undefined, perUnit: number): number | null {
  return speedMps != null && isFinite(speedMps) && speedMps > 0
    ? perUnit / speedMps / 60
    : null;
}

/** Widen a data range so it spans `v` too. Bar charts range from bucket
 * MAXES, which can sit entirely above the true average — without this the
 * avg line would fall below the plot. Applied BEFORE the metric's padding
 * (hrVisRange & co.) so the line keeps the same breathing room as data. */
export function rangeSpanning(
  min: number,
  max: number,
  v: number | null | undefined,
): [number, number] {
  if (v == null || !isFinite(v)) return [min, max];
  return [Math.min(min, v), Math.max(max, v)];
}

/** Which side of the line the label goes: above by default, below when the
 * line sits within a label's height of the plot top (the label would clip). */
export function avgLabelSide(
  lineY: number,
  plotTop: number,
  labelHeight: number,
): "above" | "below" {
  return lineY - plotTop < labelHeight ? "below" : "above";
}

/** Snap a horizontal line's y so it renders crisp: an odd-width stroke
 * needs its center on a half pixel, an even one on a whole pixel (the
 * classic +0.5 alone blurs a 2-device-px line on Retina). */
export function snapLineY(y: number, lineWidth: number): number {
  return Math.round(y) + (Math.round(lineWidth) % 2 === 1 ? 0.5 : 0);
}

/** Canvas placement of the avg line inside the plot area: the snapped y
 * and the label's side; null when the line falls outside the plot. The
 * y-range is widened to span the average beforehand (rangeSpanning), so
 * "outside" only happens where an axis clamp bites — hrVisRange floors at
 * 40 bpm / ceils at 220 — i.e. beyond any real average. */
export function avgLineLayout(
  linePos: number,
  plotTop: number,
  plotHeight: number,
  lineWidth: number,
  labelHeight: number,
): { y: number; side: "above" | "below" } | null {
  if (!isFinite(linePos)) return null;
  const y = snapLineY(linePos, lineWidth);
  if (y < plotTop || y > plotTop + plotHeight) return null;
  return { y, side: avgLabelSide(y, plotTop, labelHeight) };
}
