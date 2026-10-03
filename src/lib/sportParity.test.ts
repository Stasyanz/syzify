import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { SPORT_TYPES } from "./types";

/** The Rust enum and the TS vocabulary must name the same sports: a kind
 * added on one side only would import as a slug the other side cannot
 * label, color or draw. The backend's `as_str` table is the source. */
describe("sport vocabulary parity with the backend", () => {
  it("SPORT_TYPES equals the slugs of SportType::as_str", () => {
    const rust = readFileSync(new URL("../../src-tauri/src/models/activity.rs", import.meta.url), "utf8");
    const asStr = rust.slice(rust.indexOf("pub fn as_str"), rust.indexOf("pub fn label"));
    const slugs = [...asStr.matchAll(/SportType::\w+ => "(\w+)"/g)].map((m) => m[1]);
    expect(slugs.length).toBeGreaterThan(20);
    expect([...SPORT_TYPES].sort()).toEqual([...slugs].sort());
  });
});
