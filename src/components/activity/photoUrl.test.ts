import { describe, it, expect } from "vitest";
import { photoUrl } from "./photoUrl";

describe("photoUrl", () => {
  it("defaults to the full size", () => {
    expect(photoUrl("abc")).toBe("photo://localhost/abc?size=full");
  });

  it("builds a thumbnail URL", () => {
    expect(photoUrl("abc", "thumb")).toBe("photo://localhost/abc?size=thumb");
  });
});
