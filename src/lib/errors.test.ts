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

  it("reads the message off a structured rejection, and stringifies the rest", () => {
    expect(errorText({ message: "quota exceeded" })).toBe("quota exceeded");
    expect(errorText({ message: "" })).toBe("[object Object]");
    expect(errorText({ code: 7 })).toBe("[object Object]");
    expect(errorText(42)).toBe("42");
  });

  it("never says undefined", () => {
    expect(errorText(undefined)).toBe("Unknown error");
    expect(errorText(null)).toBe("Unknown error");
    expect(errorText(42)).toBe("42");
  });
});
