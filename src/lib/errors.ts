import { t, tKnown } from "./i18n";

/**
 * The text of whatever a promise rejected with, in the interface language
 * where the app has the words. A Tauri command errs with a `CommandError`
 * (src-tauri/src/error.rs), which rejects with `{ code, params, message }`:
 * a `code` the dictionaries know as `error.<code>` is said in the language,
 * filled with `params` (#236); an unknown code, or none — a library's or a
 * plugin's words — says `message`, the English the backend wrote. A bare
 * string (a plugin's, a rejection that is not a command's) is said as it
 * is — reading `.message` off it said "undefined" in the toast (#174); a
 * thrown `Error` carries its text in `.message`; anything else is
 * stringified.
 */
export function errorText(e: unknown): string {
  if (e instanceof Error) return e.message || e.name;
  if (typeof e === "string") return e;
  // The app's own words for "nothing said": in the interface language.
  if (e == null) return t("common.unknownError");
  if (typeof e === "object") {
    const coded = codedText(e);
    if (coded !== null) return coded;
    if ("message" in e && typeof e.message === "string" && e.message) return e.message;
    // An error that says nothing — an empty message, an unknown code
    // without one — is no "[object Object]".
    if ("message" in e || "code" in e) return t("common.unknownError");
  }
  return String(e);
}

/** The code a command refused with, for a screen that words one refusal
 * its own way; null for anything else. Compare codes, never texts. */
export function errorCode(e: unknown): string | null {
  return typeof e === "object" && e !== null && "code" in e && typeof e.code === "string" ? e.code : null;
}

/** A `CommandError`'s code said in the interface language, or null. */
function codedText(e: object): string | null {
  const code = errorCode(e);
  if (code === null) return null;
  const params = "params" in e && typeof e.params === "object" && e.params !== null ? e.params : {};
  return tKnown(`error.${code}`, params as Record<string, unknown>);
}
