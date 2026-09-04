// @vitest-environment happy-dom
import { afterEach, describe, expect, it } from "vitest";
import {
  chartGridColor,
  chartInkColor,
  chartSurfaceColor,
  chartTextColor,
} from "./chartTheme";

describe("chartTheme", () => {
  afterEach(() => {
    document.documentElement.removeAttribute("style");
  });

  it("falls back to the light palette when no token is set", () => {
    expect(chartTextColor()).toBe("#6f675a");
    expect(chartGridColor()).toBe("#e6dfd1");
    expect(chartInkColor()).toBe("#221f1a");
    expect(chartSurfaceColor()).toBe("#faf7f1");
  });

  it("reads the live CSS tokens, trimmed", () => {
    const root = document.documentElement;
    root.style.setProperty("--muted", " #b4a892 ");
    root.style.setProperty("--border", "#352f24");
    root.style.setProperty("--ink", "#f2ece0");
    root.style.setProperty("--surface", "#1c1813");
    expect(chartTextColor()).toBe("#b4a892");
    expect(chartGridColor()).toBe("#352f24");
    expect(chartInkColor()).toBe("#f2ece0");
    expect(chartSurfaceColor()).toBe("#1c1813");
  });
});
