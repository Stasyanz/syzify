/// <reference types="vite/client" />

declare const __APP_VERSION__: string;

interface Window {
  // Tauri 2's runtime global (present in the app, absent in a plain browser).
  // Prefer the isTauri() helper in lib/tauri.ts over touching this directly.
  __TAURI_INTERNALS__?: unknown;
}
