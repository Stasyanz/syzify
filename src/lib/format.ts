import { isImperial, M_PER_MILE, M_PER_100YD, FT_PER_M, MPH_PER_MPS } from "./units";
import { isPaceSport, isSwimSport } from "./types";

export function formatDistance(meters: number | null): string {
  if (meters == null) return "--";
  if (isImperial()) {
    const miles = meters / M_PER_MILE;
    // Below a tenth of a mile feet read better (mirrors the metric m cutoff).
    return miles < 0.1
      ? `${Math.round(meters * FT_PER_M)} ft`
      : `${miles.toFixed(2)} mi`;
  }
  return meters < 1000
    ? `${Math.round(meters)} m`
    : `${(meters / 1000).toFixed(2)} km`;
}

export function formatDuration(seconds: number | null): string {
  if (seconds == null) return "--";
  const s = Math.round(seconds);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  if (h > 0) {
    return `${h}:${m.toString().padStart(2, "0")}:${sec.toString().padStart(2, "0")}`;
  }
  return `${m}:${sec.toString().padStart(2, "0")}`;
}

export function formatPace(speedMps: number | null): string {
  if (speedMps == null || speedMps <= 0) return "--";
  const imperial = isImperial();
  const secsPerUnit = Math.round((imperial ? M_PER_MILE : 1000) / speedMps);
  const mins = Math.floor(secsPerUnit / 60);
  const secs = secsPerUnit % 60;
  return `${mins}:${secs.toString().padStart(2, "0")} /${imperial ? "mi" : "km"}`;
}

/** Swim pace: time per 100 m (per 100 yd for imperial, Garmin-style). */
export function formatSwimPace(speedMps: number | null): string {
  if (speedMps == null || speedMps <= 0) return "--";
  const imperial = isImperial();
  const secsPer100 = Math.round((imperial ? M_PER_100YD : 100) / speedMps);
  const mins = Math.floor(secsPer100 / 60);
  const secs = secsPer100 % 60;
  return `${mins}:${secs.toString().padStart(2, "0")} /100${imperial ? "yd" : "m"}`;
}

/** Sport-aware speed metric: running pace for foot sports, swim pace for
 * swim sports, plain speed for everything else. */
export function formatPaceOrSpeed(sport: string, speedMps: number | null): string {
  if (isPaceSport(sport)) return formatPace(speedMps);
  if (isSwimSport(sport)) return formatSwimPace(speedMps);
  return formatSpeed(speedMps);
}

/** Metric name matching formatPaceOrSpeed's choice for the sport. */
export function paceOrSpeedLabel(sport: string): "Pace" | "Speed" {
  return isPaceSport(sport) || isSwimSport(sport) ? "Pace" : "Speed";
}

export function formatSpeed(speedMps: number | null): string {
  if (speedMps == null) return "--";
  return isImperial()
    ? `${(speedMps * MPH_PER_MPS).toFixed(1)} mph`
    : `${(speedMps * 3.6).toFixed(1)} km/h`;
}

export function formatElevation(meters: number | null): string {
  if (meters == null) return "--";
  return isImperial()
    ? `${Math.round(meters * FT_PER_M)} ft`
    : `${Math.round(meters)} m`;
}

export function formatHR(hr: number | null): string {
  if (hr == null) return "--";
  return `${Math.round(hr)} bpm`;
}

/** Signed grade percent, one decimal — "+8.3%", "-2.0%", "0.0%". One
 * decimal is all the smoothing window supports. */
export function formatGrade(pct: number): string {
  return `${pct > 0 ? "+" : ""}${pct.toFixed(1)}%`;
}

/** The elevation chart's selection badge: span distance, signed net climb,
 * signed average grade — "2.41 km · +183 m · +7.6%" (display units). */
export function formatSelectionStats(s: {
  distanceM: number;
  deltaM: number;
  gradePct: number;
}): string {
  const delta = `${s.deltaM > 0 ? "+" : ""}${formatElevation(s.deltaM)}`;
  return `${formatDistance(s.distanceM)} · ${delta} · ${formatGrade(s.gradePct)}`;
}

export function formatDurationHM(seconds: number | null): string {
  if (seconds == null) return "--";
  const s = Math.round(seconds);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  if (h > 0) return `${h}h ${m.toString().padStart(2, "0")}m`;
  return `${m}m`;
}

export function formatDate(isoString: string): string {
  const d = new Date(isoString);
  return d.toLocaleDateString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}
