// TanStack Query's error type defaults to `Error`, which lets `onError:
// (e) => e.message` compile — and a rejection may be a bare string, where
// that read is "undefined" at runtime (#174), or a command's coded error,
// whose code it skips (#236). `unknown` makes every handler go through
// `errorText` (src/lib/errors.ts).
import "@tanstack/react-query";

declare module "@tanstack/react-query" {
  interface Register {
    defaultError: unknown;
  }
}
