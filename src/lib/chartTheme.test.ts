// @vitest-environment happy-dom
import { afterEach, describe, expect, it } from "vitest";
import {
  chartGradeSteps,
  chartGridColor,
  chartInkColor,
  chartSurfaceColor,
  chartTextColor,
  GRADE_STEPS_DARK,
  GRADE_STEPS_LIGHT,
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
    expect(chartGradeSteps()).toEqual(GRADE_STEPS_LIGHT);
  });

  it("reads the theme's grade ladder from its tokens, step by step", () => {
    const root = document.documentElement;
    GRADE_STEPS_DARK.forEach((c, i) => root.style.setProperty(`--grade-${i + 1}`, ` ${c} `));
    expect(chartGradeSteps()).toEqual(GRADE_STEPS_DARK);
    // A single missing token falls back on its own step, not the whole ladder.
    root.style.removeProperty("--grade-3");
    expect(chartGradeSteps()[2]).toBe(GRADE_STEPS_LIGHT[2]);
    expect(chartGradeSteps()[4]).toBe(GRADE_STEPS_DARK[4]);
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
