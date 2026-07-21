import { describe, it, expect } from "vitest";
import { toggleTagSelection } from "./EditActivityModal";

describe("toggleTagSelection", () => {
  it("adds a tag when under the limit", () => {
    expect(toggleTagSelection([1, 2], 3, 3)).toEqual([1, 2, 3]);
  });

  it("removes an already-selected tag (even at the limit)", () => {
    expect(toggleTagSelection([1, 2, 3], 2, 3)).toEqual([1, 3]);
  });

  it("ignores selecting a new tag once the cap is reached", () => {
    expect(toggleTagSelection([1, 2, 3], 4, 3)).toEqual([1, 2, 3]);
  });

  it("does not duplicate an already-selected tag passed as new", () => {
    // id 1 is present → treated as a deselect, never a duplicate add.
    expect(toggleTagSelection([1, 2], 1, 3)).toEqual([2]);
  });
});
