/**
 * The text of whatever a promise rejected with. A Tauri command's
 * `Result<_, String>` rejects with the bare string — not an `Error` — so
 * reading `.message` off it says "undefined" in the toast (#174); a thrown
 * `Error` carries its text in `.message`; anything else is stringified.
 */
export function errorText(e: unknown): string {
  if (e instanceof Error) return e.message || e.name;
  if (typeof e === "string") return e;
  if (e == null) return "Unknown error";
  return String(e);
}
