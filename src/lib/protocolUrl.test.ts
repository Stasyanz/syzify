// @vitest-environment happy-dom
import { describe, it, expect, afterEach } from "vitest";
import { protocolBase } from "./protocolUrl";

afterEach(() => {
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
});

describe("protocolBase", () => {
  it("falls back to the macOS scheme form outside Tauri", () => {
    expect(protocolBase("tile")).toBe("tile://localhost/");
  });

  it("uses the platform mapping from Tauri internals when present", () => {
    // Windows/WebView2 form, as produced by convertFileSrc there.
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {
      convertFileSrc: (path: string, protocol: string) =>
        `http://${protocol}.localhost/${path}`,
    };
    expect(protocolBase("tile")).toBe("http://tile.localhost/");
    expect(protocolBase("photo")).toBe("http://photo.localhost/");
  });
});
