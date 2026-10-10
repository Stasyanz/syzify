import type { EncryptionScopes } from "./types";

/** What turning encryption on covers: all three scopes (#209). A route lives
 * both in its raw file and, point by point, in the database, so encrypting
 * either one alone leaves it readable in the other. Not the whole vault
 * folder: the map tile cache and installed plugins belong to no scope. */
export const ALL_SCOPES: Readonly<EncryptionScopes> = Object.freeze({
  activities: true,
  database: true,
  photos: true,
});

/** Each scope as it reads in a sentence. Raw files include Garmin monitoring
 * (night heart rate, stress, SpO2) — the same `raw/` machinery, so the same
 * scope, and Settings has to say so (ADR 0002). `satisfies` makes a scope
 * added to the type without a name here a compile error, not a vault that
 * reads as fully encrypted. */
const SCOPE_NAMES = {
  activities: "raw files (activities and monitoring)",
  database: "the database",
  photos: "photos",
} as const satisfies Record<keyof EncryptionScopes, string>;

/** The scopes of an encrypted vault that are still readable on disk. Empty
 * for a vault encrypted since #209; non-empty for one encrypted earlier, when
 * the scopes were ticked one by one. */
export function unencryptedParts(scopes: EncryptionScopes): string[] {
  return (Object.keys(SCOPE_NAMES) as (keyof typeof SCOPE_NAMES)[])
    .filter((scope) => !scopes[scope])
    .map((scope) => SCOPE_NAMES[scope]);
}
