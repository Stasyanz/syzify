import { describe, expect, it } from "vitest";
import { ALL_SCOPES, unencryptedParts } from "./encryptionScopes";

describe("encryption scopes", () => {
  it("turning encryption on covers raw files, the database and photos, and cannot be narrowed", () => {
    expect(ALL_SCOPES).toEqual({ activities: true, database: true, photos: true });
    expect(Object.isFrozen(ALL_SCOPES)).toBe(true);
  });

  it("a fully encrypted vault leaves nothing readable", () => {
    expect(unencryptedParts(ALL_SCOPES)).toEqual([]);
  });

  it.each([
    [{ activities: true, database: false, photos: false }, ["the database", "photos"]],
    [{ activities: false, database: true, photos: true }, ["raw files (activities and monitoring)"]],
    // A lock whose enable died before any scope was done.
    [
      { activities: false, database: false, photos: false },
      ["raw files (activities and monitoring)", "the database", "photos"],
    ],
    [{ activities: true, database: true, photos: false }, ["photos"]],
    [{ activities: true, database: false, photos: true }, ["the database"]],
  ])("names what a vault encrypted with %j leaves readable", (scopes, parts) => {
    expect(unencryptedParts(scopes)).toEqual(parts);
  });
});
