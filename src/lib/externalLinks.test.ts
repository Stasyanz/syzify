// @vitest-environment happy-dom
import { describe, it, expect, vi, beforeAll, afterEach } from "vitest";
import { isExternalHttpHref, initExternalLinks } from "./externalLinks";

vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: vi.fn().mockResolvedValue(undefined),
}));

import { openUrl } from "@tauri-apps/plugin-opener";

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

describe("initExternalLinks", () => {
  // the listener is delegated on document and stays registered for the
  // process lifetime, so install it once for this file rather than per test
  beforeAll(() => {
    initExternalLinks();
  });

  afterEach(() => {
    vi.clearAllMocks();
    document.body.innerHTML = "";
  });

  function clickAnchor(href: string): MouseEvent {
    const anchor = document.createElement("a");
    anchor.href = href;
    document.body.appendChild(anchor);
    const event = new MouseEvent("click", {
      bubbles: true,
      cancelable: true,
      button: 0,
    });
    anchor.dispatchEvent(event);
    return event;
  }

  it("intercepts clicks on external http(s) links and opens them via the system browser", () => {
    const event = clickAnchor("https://leafletjs.com/some/path");

    expect(event.defaultPrevented).toBe(true);
    expect(openUrl).toHaveBeenCalledWith("https://leafletjs.com/some/path");
  });

  it("does not intercept mailto: links", () => {
    const event = clickAnchor("mailto:a@b.com");

    expect(event.defaultPrevented).toBe(false);
    expect(openUrl).not.toHaveBeenCalled();
  });

  it("does not intercept relative/same-origin links", () => {
    const event = clickAnchor("/activity/123");

    expect(event.defaultPrevented).toBe(false);
    expect(openUrl).not.toHaveBeenCalled();
  });
});
