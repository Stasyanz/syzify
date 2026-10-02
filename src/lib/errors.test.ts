import { describe, it, expect } from "vitest";
import { errorText } from "./errors";

describe("errorText", () => {
  it("reads the bare string a Tauri command rejects with", () => {
    expect(errorText("Gear not found: g-1")).toBe("Gear not found: g-1");
  });

  it("reads an Error's message, or its name when the message is empty", () => {
    expect(errorText(new Error("boom"))).toBe("boom");
    expect(errorText(new TypeError("typed"))).toBe("typed");
    expect(errorText(new RangeError())).toBe("RangeError");
  });

  it("never says undefined", () => {
    expect(errorText(undefined)).toBe("Unknown error");
    expect(errorText(null)).toBe("Unknown error");
    expect(errorText(42)).toBe("42");
  });
});
