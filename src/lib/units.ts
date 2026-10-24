import { create } from "zustand";
import { t, translate, type Language } from "./i18n";
import type { MessageKey } from "../locales/types";

export type UnitsMode = "metric" | "imperial";

const STORAGE_KEY = "syzify-units";

export const M_PER_MILE = 1609.344;
export const M_PER_100YD = 91.44;
export const FT_PER_M = 3.280839895;
export const MPH_PER_MPS = 2.2369362921;
export const LB_PER_KG = 2.2046226218;

/** Read the persisted preference, defaulting to "metric".
 * localStorage is absent in node test environments — fall back quietly. */
export function readStoredUnits(): UnitsMode {
  if (typeof localStorage === "undefined") return "metric";
  const v = localStorage.getItem(STORAGE_KEY);
  return v === "imperial" ? "imperial" : "metric";
}

interface UnitsState {
  mode: UnitsMode;
  setMode: (mode: UnitsMode) => void;
}

export const useUnitsStore = create<UnitsState>((set) => ({
  mode: readStoredUnits(),
  setMode: (mode) => {
    if (typeof localStorage !== "undefined") localStorage.setItem(STORAGE_KEY, mode);
    set({ mode });
  },
}));

/** Subscribe the calling component to the units mode and return it.
 *
 * The formatters in this module and in format.ts read the store imperatively
 * (`getState()`), so a component whose render output goes through them will
 * NOT re-render when the user flips metric/imperial — unless it calls this
 * hook. Call it at the top of every component that formats distance, pace,
 * speed or elevation. */
export function useUnits(): UnitsMode {
  return useUnitsStore((s) => s.mode);
}

/** Current mode for non-React code (formatters, canvases, chart configs). */
export function isImperial(): boolean {
  return useUnitsStore.getState().mode === "imperial";
}

export function distanceUnit(): "km" | "mi" {
  return isImperial() ? "mi" : "km";
}

export function elevationUnit(): "m" | "ft" {
  return isImperial() ? "ft" : "m";
}

export function speedUnit(): "km/h" | "mph" {
  return isImperial() ? "mph" : "km/h";
}

/** Meters → the display distance number (km or miles). */
export function toDistance(meters: number): number {
  return isImperial() ? meters / M_PER_MILE : meters / 1000;
}

/** Meters → the display elevation number (m or ft). */
export function toElevation(meters: number): number {
  return isImperial() ? meters * FT_PER_M : meters;
}

export function weightUnit(): "kg" | "lb" {
  return isImperial() ? "lb" : "kg";
}

/** Kilograms → the display weight number (kg or lb). */
export function toWeight(kg: number): number {
  return isImperial() ? kg * LB_PER_KG : kg;
}

/** The symbols the units above and the formatters speak in. A symbol is
 * what logic compares; `unitLabel` is what the screen shows. Each is the
 * English text but "strokes/min", which English, like steps, writes "spm". */
export type UnitSymbol =
  | ReturnType<typeof distanceUnit>
  | ReturnType<typeof elevationUnit>
  | ReturnType<typeof speedUnit>
  | ReturnType<typeof weightUnit>
  | "yd" | "bpm" | "W" | "rpm" | "spm" | "strokes/min" | "kcal" | "h" | "min" | "mm";

const UNIT_KEYS: Record<UnitSymbol, Extract<MessageKey, `units.${string}`>> = {
  km: "units.km",
  mi: "units.mi",
  m: "units.m",
  ft: "units.ft",
  yd: "units.yd",
  "km/h": "units.kmh",
  mph: "units.mph",
  kg: "units.kg",
  lb: "units.lb",
  bpm: "units.bpm",
  W: "units.w",
  rpm: "units.rpm",
  spm: "units.spm",
  "strokes/min": "units.strokesPerMin",
  kcal: "units.kcal",
  h: "units.h",
  min: "units.min",
  mm: "units.mm",
};

/** A unit symbol as the interface language writes it ("км" in Russian). */
export function unitLabel(unit: UnitSymbol): string {
  return t(UNIT_KEYS[unit]);
}

/** A unit symbol as `lang` writes it — for code that holds its own
 * language, so its words, numbers and units come from one source. */
export function unitLabelIn(lang: Language, unit: UnitSymbol): string {
  return translate(lang, UNIT_KEYS[unit]);
}
