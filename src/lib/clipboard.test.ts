// @vitest-environment happy-dom
import { describe, it, expect, vi, afterEach } from "vitest";
import { copyText } from "./clipboard";

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  // assigning document.execCommand directly isn't undone by restoreAllMocks
  delete (document as any).execCommand;
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
    // happy-dom has no execCommand — define it for the fallback path
    let captured: string | undefined;
    const execCommand = vi.fn().mockImplementation(() => {
      captured = document.querySelector("textarea")?.value;
      return true;
    });
    document.execCommand = execCommand;

    const button = document.createElement("button");
    document.body.appendChild(button);
    button.focus();
    expect(document.activeElement).toBe(button);

    expect(await copyText("fallback text")).toBe(true);
    expect(execCommand).toHaveBeenCalledWith("copy");
    // the fallback must operate on the text passed in, not stale state
    expect(captured).toBe("fallback text");
    // the temp textarea must not leak into the DOM
    expect(document.querySelector("textarea")).toBeNull();
    // focus must return to whatever had it before the copy attempt
    expect(document.activeElement).toBe(button);

    document.body.removeChild(button);
  });

  it("returns false when execCommand reports failure without throwing", async () => {
    const writeText = vi.fn().mockRejectedValue(new Error("denied"));
    vi.stubGlobal("navigator", { clipboard: { writeText } });
    document.execCommand = vi.fn().mockReturnValue(false);

    expect(await copyText("nope")).toBe(false);
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
