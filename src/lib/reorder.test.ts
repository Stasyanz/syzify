import { describe, it, expect } from "vitest";
import { moveItem } from "./reorder";

describe("moveItem", () => {
  it("moves an item forward", () => {
    expect(moveItem(["a", "b", "c", "d"], 0, 2)).toEqual(["b", "c", "a", "d"]);
  });

  it("moves an item backward", () => {
    expect(moveItem(["a", "b", "c", "d"], 3, 1)).toEqual(["a", "d", "b", "c"]);
  });

  it("is a no-op when from === to", () => {
    expect(moveItem(["a", "b", "c"], 1, 1)).toEqual(["a", "b", "c"]);
  });

  it("does not mutate the input", () => {
    const input = ["a", "b", "c"];
    moveItem(input, 0, 2);
    expect(input).toEqual(["a", "b", "c"]);
  });

  it("returns an unchanged copy for out-of-range indices", () => {
    expect(moveItem(["a", "b"], 5, 0)).toEqual(["a", "b"]);
    expect(moveItem(["a", "b"], 0, -1)).toEqual(["a", "b"]);
  });
});
