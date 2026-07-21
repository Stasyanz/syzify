import { create } from "zustand";

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
