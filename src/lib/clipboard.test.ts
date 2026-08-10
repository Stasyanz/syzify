// @vitest-environment happy-dom
import { describe, it, expect, vi, afterEach } from "vitest";
import { copyText } from "./clipboard";

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("copyText", () => {
  it("uses the async Clipboard API when available", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    vi.stubGlobal("navigator", { clipboard: { writeText } });

    expect(await copyText("hello@example.com")).toBe(true);
    expect(writeText).toHaveBeenCalledWith("hello@example.com");
  });

  it("falls back to execCommand when the Clipboard API rejects", async () => {
    const writeText = vi.fn().mockRejectedValue(new Error("denied"));
    vi.stubGlobal("navigator", { clipboard: { writeText } });
    // jsdom has no execCommand — define it for the fallback path
    const execCommand = vi.fn().mockReturnValue(true);
    document.execCommand = execCommand;

    expect(await copyText("fallback text")).toBe(true);
    expect(execCommand).toHaveBeenCalledWith("copy");
    // the temp textarea must not leak into the DOM
    expect(document.querySelector("textarea")).toBeNull();
  });

  it("returns false when both paths fail", async () => {
    vi.stubGlobal("navigator", {}); // no clipboard at all
    document.execCommand = vi.fn(() => {
      throw new Error("not supported");
    });

    expect(await copyText("nope")).toBe(false);
    expect(document.querySelector("textarea")).toBeNull();
  });
});
