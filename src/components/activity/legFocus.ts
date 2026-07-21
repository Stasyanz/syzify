import type { MultisportLeg, TrackPointColumns } from "../../lib/types";

/** A FIT-native leg can be focused in place when it has a start and a
 * length to window the track by. Merged legs navigate to their standalone
 * activity instead, and transitions carry nothing worth focusing. */
export function isFocusableLeg(leg: MultisportLeg): boolean {
  return (
    !leg.is_transition &&
    leg.source_activity_id == null &&
    leg.start_time != null &&
    (leg.total_elapsed_time_s ?? leg.total_timer_time_s) != null
  );
}

/** The leg's [from, to] window in trackpoint time (epoch seconds — what the
 * columnar `t` holds). Elapsed time bounds the window (it includes pauses,
 * which the recorded points span); timer time is the fallback. */
export function legTimeWindow(leg: MultisportLeg): [number, number] | null {
  if (!isFocusableLeg(leg)) return null;
  const from = Date.parse(leg.start_time!) / 1000;
  if (!Number.isFinite(from)) return null;
  const len = leg.total_elapsed_time_s ?? leg.total_timer_time_s!;
  return [from, from + len];
}

/** Slice the columnar track to the points whose timestamp falls inside
 * [from, to]. Cumulative distance is rebased to start at 0 so the distance
 * axis, auto-laps and pace derivation read leg-relative; timestamps stay
 * absolute (the charts rebase time against the first point themselves).
 * Points without a timestamp are dropped — with no time there is nothing
 * to window them by. */
export function sliceTrackpoints(
  tp: TrackPointColumns,
  from: number,
  to: number,
): TrackPointColumns {
  const keep: number[] = [];
  for (let i = 0; i < tp.t.length; i++) {
    const t = tp.t[i];
    if (t != null && t >= from && t <= to) keep.push(i);
  }

  const out = {} as Record<keyof TrackPointColumns, (number | null)[]>;
  for (const key of Object.keys(tp) as (keyof TrackPointColumns)[]) {
    const col = tp[key];
    out[key] = keep.map((i) => col[i]);
  }

  const base = out.distance_m.find((d) => d != null) ?? null;
  if (base != null) {
    out.distance_m = out.distance_m.map((d) => (d != null ? d - base : null));
  }
  return out as TrackPointColumns;
}
