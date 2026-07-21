import { describe, it, expect } from "vitest";
import { SPORT_COLORS, getSportColor } from "./sportColors";
import { SPORT_TYPES } from "./types";

describe("sport colors", () => {
  it("every sport has its own color — no two sports share a hex", () => {
    // Regression: all five water sports used one teal, so Paddling and Open
    // Water merged on the dashboard donut.
    const entries = Object.entries(SPORT_COLORS);
    const seen = new Map<string, string>();
    for (const [sport, color] of entries) {
      const clash = seen.get(color.toLowerCase());
      expect(clash, `${sport} and ${clash} share ${color}`).toBeUndefined();
      seen.set(color.toLowerCase(), sport);
    }
  });

  it("covers every SportType (nothing silently falls back to gray)", () => {
    for (const sport of SPORT_TYPES) {
      expect(SPORT_COLORS[sport], `missing color for ${sport}`).toBeDefined();
    }
  });

  it("unknown sports fall back to the 'other' gray", () => {
    expect(getSportColor("zorbing")).toBe(SPORT_COLORS.other);
  });
});
