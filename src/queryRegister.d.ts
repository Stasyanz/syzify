// TanStack Query's error type defaults to `Error`, which lets `onError:
// (e) => e.message` compile — and a Tauri command rejects with a bare
// string, so that read is "undefined" at runtime (#174). `unknown` makes
// every handler go through `errorText` (src/lib/errors.ts).
import "@tanstack/react-query";

declare module "@tanstack/react-query" {
  interface Register {
    defaultError: unknown;
  }
}
