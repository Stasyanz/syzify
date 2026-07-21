import { describe, it, expect } from "vitest";
import { countActiveFilters } from "./FilterDrawer";

describe("countActiveFilters", () => {
  it("is 0 for empty filters", () => {
    expect(countActiveFilters({})).toBe(0);
  });

  it("ignores blank/whitespace search", () => {
    expect(countActiveFilters({ search: "   " })).toBe(0);
    expect(countActiveFilters({ search: "loop" })).toBe(1);
  });

  it("counts a zero range bound as active (0 is a real bound)", () => {
    expect(countActiveFilters({ distance_min: 0 })).toBe(1);
    expect(countActiveFilters({ duration_max: 0 })).toBe(1);
  });

  it("counts each facet once and sums them", () => {
    expect(
      countActiveFilters({
        search: "x",
        sport_types: ["run", "ride"],
        tag_ids: [1, 2],
        date_from: "2026-01-01",
        distance_min: 1000,
        duration_max: 3600,
        elev_gain_min: 100,
        has_gps: false,
      })
    ).toBe(8);
  });

  it("counts the GPS facet for BOTH explicit states, not just true", () => {
    expect(countActiveFilters({ has_gps: true })).toBe(1);
    expect(countActiveFilters({ has_gps: false })).toBe(1);
    expect(countActiveFilters({ has_gps: undefined })).toBe(0);
  });

  it("treats an empty tag list and empty date range as inactive", () => {
    expect(countActiveFilters({ tag_ids: [] })).toBe(0);
    expect(countActiveFilters({ sport_types: [] })).toBe(0);
    expect(countActiveFilters({ date_from: undefined, date_to: undefined })).toBe(0);
  });
});
