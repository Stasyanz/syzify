import type { CommandError } from "../lib/types";

/**
 * What a Tauri command rejects with (`CommandError`, src-tauri/src/error.rs)
 * — for a test's mocked rejection, so a mock speaks the backend's contract:
 * `commandError("gear.notFound", "Gear not found: g-1", { id: "g-1" })`, or
 * `commandError(null, "database is locked")` for a library's own words.
 */
export function commandError(
  code: string | null,
  message: string,
  params: Record<string, string | number> = {},
): CommandError {
  return { code, params, message };
}
