/**
 * Stretches of a track without a GPS fix (#256): jamming, a tunnel, or the
 * fixes the import's sanity filter dropped. The rows are still there (they
 * carry heart rate, power, time) — only the position is gone. The map draws
 * a break across such a stretch instead of a straight line, and the
 * activity tells how long it lasted. The same rule breaks an exported GPX
 * (`fixes_after_gaps` in `import/gps_clean.rs`).
 */

/** A stretch is a gap once it lasts this long; shorter ones are noise. A
 * step between two rows this long is a pause, and no time without GPS. */
export const GPS_GAP_MIN_S = 30;

export interface GpsGap {
  /** Index of the last fix before the gap, -1 when the track starts with it. */
  before: number;
  /** Index of the first fix after it, the track's length when it ends with it. */
  after: number;
  seconds: number;
}

interface Track {
  t: (number | null)[];
  lat: (number | null)[];
  lon: (number | null)[];
}

const hasFix = (tr: Track, i: number) => tr.lat[i] != null && tr.lon[i] != null;

/** The gaps of a track with at least one fix, in order. A pause is not a
 * gap, with or without a row beside it: nothing was recorded, nothing was
 * lost. */
export function gpsGaps(tr: Track): GpsGap[] {
  const n = tr.lat.length;
  const gaps: GpsGap[] = [];
  let i = 0;
  while (i < n) {
    if (hasFix(tr, i)) {
      i++;
      continue;
    }
    let j = i;
    while (j < n && !hasFix(tr, j)) j++;
    const before = i - 1;
    const after = j;
    // From the fix before (or the first row) to the fix after (or the last row).
    const from = before >= 0 ? before : i;
    const to = after < n ? after : n - 1;
    if (tr.t[from] != null && tr.t[to] != null && (before >= 0 || after < n)) {
      let seconds = 0;
      for (let k = from; k < to; k++) {
        const a = tr.t[k];
        const b = tr.t[k + 1];
        if (a != null && b != null && b - a < GPS_GAP_MIN_S) seconds += b - a;
      }
      if (seconds >= GPS_GAP_MIN_S) gaps.push({ before, after, seconds });
    }
    i = j;
  }
  return gaps;
}

/** Seconds the track lost its GPS between two fixes — the stretches its map
 * leaves out. The minutes before the first fix (waiting for satellites) and
 * after the last (finishing indoors) leave no hole in the route, so they
 * don't count; nor does a track with no fix at all (an indoor ride). */
export function secondsWithoutGps(tr: Track): number {
  return gpsGaps(tr)
    .filter((g) => g.before >= 0 && g.after < tr.lat.length)
    .reduce((s, g) => s + g.seconds, 0);
}

/** The fixes as runs of [lat, lon], a new run after every gap inside the
 * track — what the map draws as separate lines. */
export function fixRuns(tr: Track): [number, number][][] {
  const breaks = new Set(gpsGaps(tr).filter((g) => g.before >= 0).map((g) => g.after));
  const runs: [number, number][][] = [];
  let cur: [number, number][] = [];
  for (let i = 0; i < tr.lat.length; i++) {
    const lat = tr.lat[i];
    const lon = tr.lon[i];
    if (lat == null || lon == null) continue;
    if (breaks.has(i) && cur.length > 0) {
      runs.push(cur);
      cur = [];
    }
    cur.push([lat, lon]);
  }
  if (cur.length > 0) runs.push(cur);
  return runs;
}
