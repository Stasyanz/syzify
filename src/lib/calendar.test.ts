import { describe, it, expect } from "vitest";
import { firstWeekday, daysInMonth, buildMonthGrid, monthSports } from "./calendar";

describe("monthSports", () => {
  const day = (...sports: string[]) => ({
    activities: sports.map((sport_type) => ({ sport_type })),
  });

  it("lists only the sports present, most frequent first", () => {
    const days = [day("ride", "swim"), day("ride"), day("paddle", "ride"), day("swim")];
    expect(monthSports(days)).toEqual(["ride", "swim", "paddle"]);
  });

  it("keeps first-seen order on ties", () => {
    expect(monthSports([day("swim"), day("paddle")])).toEqual(["swim", "paddle"]);
  });

  it("caps at the limit", () => {
    const days = [day("a", "b", "c"), day("d", "e"), day("f", "g")];
    expect(monthSports(days, 6)).toHaveLength(6);
  });

  it("is empty for an empty month (legend renders nothing)", () => {
    expect(monthSports([])).toEqual([]);
  });
});

describe("daysInMonth", () => {
  it("knows month lengths and leap Februaries", () => {
    expect(daysInMonth(2024, 2)).toBe(29); // leap year
    expect(daysInMonth(2023, 2)).toBe(28);
    expect(daysInMonth(2026, 2)).toBe(28);
    expect(daysInMonth(2026, 4)).toBe(30);
    expect(daysInMonth(2025, 12)).toBe(31);
  });
});

describe("firstWeekday (Monday-first)", () => {
  it("maps the 1st to a Monday-based index", () => {
    expect(firstWeekday(2024, 1)).toBe(0); // Jan 1 2024 was a Monday
    expect(firstWeekday(2023, 1)).toBe(6); // Jan 1 2023 was a Sunday
  });
});

describe("buildMonthGrid", () => {
  it("pads to whole weeks with leading/trailing blanks", () => {
    const grid = buildMonthGrid(2024, 2); // lead 3 (Thu) + 29 days = 32 → 35
    expect(grid.length % 7).toBe(0);
    expect(grid.length).toBe(35);
    expect(grid.slice(0, 3)).toEqual([null, null, null]);
    expect(grid[3]).toBe(1);
  });

  it("contains exactly the real days in order", () => {
    const grid = buildMonthGrid(2025, 12);
    const days = grid.filter((d): d is number => d !== null);
    expect(days).toEqual(Array.from({ length: 31 }, (_, i) => i + 1));
  });

  it("places leading blanks equal to the first weekday", () => {
    const grid = buildMonthGrid(2023, 1); // Sunday start → 6 leading blanks
    expect(grid.slice(0, 6)).toEqual([null, null, null, null, null, null]);
    expect(grid[6]).toBe(1);
  });

  it("minWeeks pads short months to a fixed 6-week height", () => {
    // April 2026: Wed start, 30 days → 5 natural weeks; padded to 6.
    const grid = buildMonthGrid(2026, 4, 6);
    expect(grid.length).toBe(42);
    expect(grid.slice(35)).toEqual([null, null, null, null, null, null, null]);
    // The days themselves are untouched.
    expect(grid.filter((d) => d !== null)).toHaveLength(30);
    // February 2021: Monday start, exactly 4 weeks → two padding rows.
    expect(buildMonthGrid(2021, 2, 6).length).toBe(42);
    // A natural 6-week month gains nothing.
    expect(buildMonthGrid(2026, 3, 6).length).toBe(42);
    expect(buildMonthGrid(2026, 3).length).toBe(42);
  });
});
