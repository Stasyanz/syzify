import { describe, it, expect } from "vitest";
import { isExternalHttpHref } from "./externalLinks";

const APP = "tauri://localhost";

describe("isExternalHttpHref", () => {
  it("flags http(s) links on another origin", () => {
    expect(isExternalHttpHref("https://leafletjs.com", APP)).toBe(true);
    expect(isExternalHttpHref("http://example.com/path", APP)).toBe(true);
  });

  it("ignores non-web schemes", () => {
    expect(isExternalHttpHref("mailto:a@b.com", APP)).toBe(false);
    expect(isExternalHttpHref("tile://localhost/osm/1/2/3.png", APP)).toBe(false);
    expect(isExternalHttpHref("photo://localhost/x", APP)).toBe(false);
    expect(isExternalHttpHref("javascript:void(0)", APP)).toBe(false);
  });

  it("ignores in-app links (relative or same origin)", () => {
    expect(isExternalHttpHref("/activity/123", APP)).toBe(false);
    expect(isExternalHttpHref("#section", APP)).toBe(false);
    expect(isExternalHttpHref("tauri://localhost/index.html", APP)).toBe(false);
  });

  it("treats same-origin http as internal in dev", () => {
    const dev = "http://localhost:1420";
    expect(isExternalHttpHref("http://localhost:1420/x", dev)).toBe(false);
    expect(isExternalHttpHref("https://leafletjs.com", dev)).toBe(true);
  });

  it("returns false for an unparseable href", () => {
    expect(isExternalHttpHref("http://[bad", APP)).toBe(false);
  });
});
