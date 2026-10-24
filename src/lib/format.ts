import {
  isImperial,
  toWeight,
  unitLabel,
  weightUnit,
  M_PER_MILE,
  M_PER_100YD,
  FT_PER_M,
  MPH_PER_MPS,
  type UnitSymbol,
} from "./units";
import { isPaceSport, isSwimSport } from "./types";
import { formatDateTime, formatNumber, formatPercent, t } from "./i18n";

// Numbers, units and dates here are written in the interface language
// (#232): "1,50 км" in Russian. The units mode is its own setting.

export function formatDistance(meters: number | null): string {
  if (meters == null) return "--";
  if (isImperial()) {
    const miles = meters / M_PER_MILE;
    // Below a tenth of a mile feet read better (mirrors the metric m cutoff).
    return miles < 0.1
      ? formatWhole(meters * FT_PER_M, "ft")
      : `${formatNumber(miles, 2)} ${unitLabel("mi")}`;
  }
  return meters < 1000 ? formatWhole(meters, "m") : `${formatNumber(meters / 1000, 2)} ${unitLabel("km")}`;
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
  return `${mins}:${secs.toString().padStart(2, "0")} /${unitLabel(imperial ? "mi" : "km")}`;
}

/** What a swim pace is per: "/100m" ("/100yd" imperial), Russian
 * "/100 м". */
export function swimPaceUnit(): string {
  return t("format.per100", { unit: unitLabel(isImperial() ? "yd" : "m") });
}

/** Swim pace: time per 100 m (per 100 yd for imperial, Garmin-style). */
export function formatSwimPace(speedMps: number | null): string {
  if (speedMps == null || speedMps <= 0) return "--";
  const secsPer100 = Math.round((isImperial() ? M_PER_100YD : 100) / speedMps);
  const mins = Math.floor(secsPer100 / 60);
  const secs = secsPer100 % 60;
  return `${mins}:${secs.toString().padStart(2, "0")} ${swimPaceUnit()}`;
}

/** Sport-aware speed metric: running pace for foot sports, swim pace for
 * swim sports, plain speed for everything else. */
export function formatPaceOrSpeed(sport: string, speedMps: number | null): string {
  if (isPaceSport(sport)) return formatPace(speedMps);
  if (isSwimSport(sport)) return formatSwimPace(speedMps);
  return formatSpeed(speedMps);
}

/** Metric name matching formatPaceOrSpeed's choice for the sport, in the
 * interface language. */
export function paceOrSpeedLabel(sport: string): string {
  return isPaceSport(sport) || isSwimSport(sport) ? t("format.pace") : t("format.speed");
}

export function formatSpeed(speedMps: number | null): string {
  if (speedMps == null) return "--";
  return isImperial()
    ? `${formatNumber(speedMps * MPH_PER_MPS, 1)} ${unitLabel("mph")}`
    : `${formatNumber(speedMps * 3.6, 1)} ${unitLabel("km/h")}`;
}

export function formatElevation(meters: number | null): string {
  if (meters == null) return "--";
  return isImperial() ? formatWhole(meters * FT_PER_M, "ft") : formatWhole(meters, "m");
}

/** A whole number and its unit: "213 W", "1,240 kcal" (Russian "213 Вт",
 * "1240 ккал"). */
export function formatWhole(value: number, unit: UnitSymbol): string {
  return `${formatNumber(Math.round(value))} ${unitLabel(unit)}`;
}

export function formatHR(hr: number | null): string {
  if (hr == null) return "--";
  return formatWhole(hr, "bpm");
}

export function formatPower(watts: number | null): string {
  if (watts == null) return "--";
  return formatWhole(watts, "W");
}

/** A weight in the display unit, whole or with its one decimal: "100 kg",
 * "22.5 kg", "49.6 lb". */
export function formatWeight(kg: number): string {
  const w = toWeight(kg);
  return `${formatNumber(w, Number.isInteger(w) ? 0 : 1)} ${unitLabel(weightUnit())}`;
}

/** Signed grade percent, one decimal — "+8.3%", "-2.0%", "0.0%". One
 * decimal is all the smoothing window supports. */
export function formatGrade(pct: number): string {
  return `${pct > 0 ? "+" : ""}${formatPercent(pct, 1)}`;
}

/** The elevation chart's selection badge: span distance, elapsed time (when
 * the track carries timestamps), signed net climb, signed average grade —
 * "2.41 km · 12:34 · +183 m · +7.6%" (display units). */
export function formatSelectionStats(s: {
  distanceM: number;
  deltaM: number;
  gradePct: number;
  durationS?: number | null;
}): string {
  const delta = `${s.deltaM > 0 ? "+" : ""}${formatElevation(s.deltaM)}`;
  const time = s.durationS != null ? ` · ${formatDuration(s.durationS)}` : "";
  return `${formatDistance(s.distanceM)}${time} · ${delta} · ${formatGrade(s.gradePct)}`;
}

/** "1h 23m" — Russian "1 ч 23 мин", Italian "1 h 23 min". */
export function formatDurationHM(seconds: number | null): string {
  if (seconds == null) return "--";
  const s = Math.round(seconds);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  if (h > 0) return t("format.durationHM", { h, m: m.toString().padStart(2, "0") });
  return t("format.durationM", { m });
}

/** Whole minutes in few words: "25 min" under an hour, "1h 25m" or "2h"
 * from there (Russian "1 ч 25 мин", "2 ч"). */
export function formatMinutes(minutes: number): string {
  const total = Math.round(minutes);
  if (total < 60) return `${formatNumber(total)} ${unitLabel("min")}`;
  const h = Math.floor(total / 60);
  const m = total % 60;
  return m ? t("format.durationHM", { h, m }) : t("format.durationH", { h });
}

/** An activity's start, date and time: "Jun 2, 2026, 6:05 PM" in
 * English, "2 июн. 2026 г., 18:05" in Russian. The clock is the
 * language's (12 h in English, 24 h in Russian and Italian), not the
 * system's — dates follow the interface language, not the region. */
export function formatDate(isoString: string): string {
  return formatDateTime(new Date(isoString), {
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
}
