import { useEffect, useState } from "react";
import { create } from "zustand";

export type ThemeMode = "light" | "dark" | "system";

const STORAGE_KEY = "syzify-theme";

/** True when the OS currently reports a dark color scheme. */
export function systemPrefersDark(): boolean {
  return window.matchMedia?.("(prefers-color-scheme: dark)").matches ?? false;
}

/** Resolve a mode to a concrete light/dark decision. Pure — easy to test. */
export function resolveDark(mode: ThemeMode, prefersDark: boolean): boolean {
  return mode === "dark" || (mode === "system" && prefersDark);
}

/** Read the persisted preference, defaulting to "system". */
export function readStoredMode(): ThemeMode {
  const v = localStorage.getItem(STORAGE_KEY);
  return v === "light" || v === "dark" || v === "system" ? v : "system";
}

/** Apply the resolved theme by toggling the `.dark` class on <html>. */
export function applyTheme(mode: ThemeMode): void {
  const dark = resolveDark(mode, systemPrefersDark());
  document.documentElement.classList.toggle("dark", dark);
  syncWindowBackground();
}

/**
 * Keep the native window background in step with the theme. On macOS the
 * transparent titlebar strip (see tauri.conf.json) shows the NSWindow
 * background — painting it with --surface makes it seamless with the navbar.
 * No-op outside Tauri (tests, plain browser).
 */
function syncWindowBackground(): void {
  if (!("__TAURI_INTERNALS__" in window)) return;
  const surface = getComputedStyle(document.documentElement)
    .getPropertyValue("--surface")
    .trim();
  const m = /^#([0-9a-f]{6})$/i.exec(surface);
  if (!m) return;
  const rgb: [number, number, number] = [
    parseInt(m[1].slice(0, 2), 16),
    parseInt(m[1].slice(2, 4), 16),
    parseInt(m[1].slice(4, 6), 16),
  ];
  import("@tauri-apps/api/window")
    .then(({ getCurrentWindow }) => getCurrentWindow().setBackgroundColor(rgb))
    .catch(() => {});
}

interface ThemeState {
  mode: ThemeMode;
  setMode: (mode: ThemeMode) => void;
}

export const useThemeStore = create<ThemeState>((set) => ({
  mode: readStoredMode(),
  setMode: (mode) => {
    localStorage.setItem(STORAGE_KEY, mode);
    applyTheme(mode);
    set({ mode });
  },
}));

/**
 * The currently rendered theme (the `.dark` class on <html>), kept live via
 * a MutationObserver. Unlike subscribing to the store mode, this also reacts
 * to OS-level scheme changes while in "system" mode — use it for imperative
 * consumers that bake colors in at creation time (uPlot, canvases).
 */
export function useResolvedDark(): boolean {
  const [dark, setDark] = useState(() =>
    document.documentElement.classList.contains("dark"),
  );
  useEffect(() => {
    const observer = new MutationObserver(() =>
      setDark(document.documentElement.classList.contains("dark")),
    );
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["class"],
    });
    return () => observer.disconnect();
  }, []);
  return dark;
}

/**
 * Apply the persisted theme once at startup (before first paint) and keep
 * "system" mode in sync with later OS-level changes. Call from main.tsx.
 */
export function initTheme(): void {
  applyTheme(readStoredMode());
  window
    .matchMedia?.("(prefers-color-scheme: dark)")
    .addEventListener?.("change", () => {
      if (useThemeStore.getState().mode === "system") applyTheme("system");
    });
}
