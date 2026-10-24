import { t } from "./i18n";

/**
 * The text of whatever a promise rejected with. A Tauri command's
 * `Result<_, String>` rejects with the bare string — not an `Error` — so
 * reading `.message` off it says "undefined" in the toast (#174); a thrown
 * `Error` carries its text in `.message`; anything else is stringified.
 */
export function errorText(e: unknown): string {
  if (e instanceof Error) return e.message || e.name;
  if (typeof e === "string") return e;
  // The app's own words for "nothing said": in the interface language. What
  // the backend says stays as it said it (#236).
  if (e == null) return t("common.unknownError");
  // A structured rejection (none today: every command errs with a String)
  // that carries a message says the message, not "[object Object]".
  if (typeof e === "object" && "message" in e && typeof e.message === "string" && e.message) return e.message;
  return String(e);
}
