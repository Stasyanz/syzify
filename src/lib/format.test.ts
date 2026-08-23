import { describe, it, expect, beforeEach, afterEach } from "vitest";
import {
  formatDistance,
  formatDuration,
  formatPace,
  formatSwimPace,
  formatPaceOrSpeed,
  paceOrSpeedLabel,
  formatSpeed,
  formatElevation,
  formatGrade,
  formatHR,
  formatSelectionStats,
} from "./format";
import { useUnitsStore } from "./units";

describe("formatDistance", () => {
  it("returns -- for null", () => {
    expect(formatDistance(null)).toBe("--");
  });

  it("formats meters when < 1000", () => {
    expect(formatDistance(500)).toBe("500 m");
  });

  it("formats km when >= 1000", () => {
    expect(formatDistance(5000)).toBe("5.00 km");
    expect(formatDistance(1234)).toBe("1.23 km");
    expect(formatDistance(42195)).toBe("42.20 km");
  });
});

describe("formatDuration", () => {
  it("returns -- for null", () => {
    expect(formatDuration(null)).toBe("--");
  });

  it("formats minutes and seconds", () => {
    expect(formatDuration(90)).toBe("1:30");
    expect(formatDuration(65)).toBe("1:05");
  });

  it("formats hours", () => {
    expect(formatDuration(3661)).toBe("1:01:01");
    expect(formatDuration(7200)).toBe("2:00:00");
  });

  it("formats zero seconds", () => {
    expect(formatDuration(0)).toBe("0:00");
  });
});

describe("formatPace", () => {
  it("returns -- for null/zero", () => {
    expect(formatPace(null)).toBe("--");
    expect(formatPace(0)).toBe("--");
    expect(formatPace(-1)).toBe("--");
  });

  it("formats pace correctly", () => {
    // 1000m / 3.333 m/s = 300s/km = 5:00/km
    expect(formatPace(1000 / 300)).toBe("5:00 /km");
  });

  it("never renders 60 seconds (rounds into the minute)", () => {
    // 299.6 s/km would naively split into 4 min + round(59.6)=60 s.
    expect(formatPace(1000 / 299.6)).toBe("5:00 /km");
  });
});

describe("formatSwimPace", () => {
  it("returns -- for null/zero", () => {
    expect(formatSwimPace(null)).toBe("--");
    expect(formatSwimPace(0)).toBe("--");
    expect(formatSwimPace(-1)).toBe("--");
  });

  it("formats time per 100 m", () => {
    // 100 m / 1.0 m/s = 100 s = 1:40 /100m
    expect(formatSwimPace(1)).toBe("1:40 /100m");
    // 100 m / (100/90) m/s = 90 s = 1:30 /100m
    expect(formatSwimPace(100 / 90)).toBe("1:30 /100m");
  });
});

describe("formatPaceOrSpeed / paceOrSpeedLabel", () => {
  it("picks running pace for foot sports", () => {
    expect(formatPaceOrSpeed("run", 1000 / 300)).toBe("5:00 /km");
    expect(paceOrSpeedLabel("trail_run")).toBe("Pace");
  });

  it("picks swim pace for swim and open water", () => {
    expect(formatPaceOrSpeed("swim", 1)).toBe("1:40 /100m");
    expect(formatPaceOrSpeed("open_water", 1)).toBe("1:40 /100m");
    expect(paceOrSpeedLabel("swim")).toBe("Pace");
    expect(paceOrSpeedLabel("open_water")).toBe("Pace");
  });

  it("falls back to speed for everything else", () => {
    expect(formatPaceOrSpeed("ride", 10)).toBe("36.0 km/h");
    expect(paceOrSpeedLabel("ride")).toBe("Speed");
    // Non-swim water sports keep speed too.
    expect(paceOrSpeedLabel("paddle")).toBe("Speed");
  });
});

describe("formatSpeed", () => {
  it("returns -- for null", () => {
    expect(formatSpeed(null)).toBe("--");
  });

  it("converts m/s to km/h", () => {
    expect(formatSpeed(10)).toBe("36.0 km/h");
    expect(formatSpeed(1)).toBe("3.6 km/h");
  });
});

describe("formatElevation", () => {
  it("returns -- for null", () => {
    expect(formatElevation(null)).toBe("--");
  });

  it("rounds meters", () => {
    expect(formatElevation(123.7)).toBe("124 m");
    expect(formatElevation(0)).toBe("0 m");
  });
});

describe("formatGrade", () => {
  it("signs positive grades, keeps the minus, one decimal", () => {
    expect(formatGrade(8.34)).toBe("+8.3%");
    expect(formatGrade(-2.04)).toBe("-2.0%");
    expect(formatGrade(0)).toBe("0.0%");
  });
});

describe("formatSelectionStats", () => {
  it("joins distance, signed net climb and grade", () => {
    expect(
      formatSelectionStats({ distanceM: 2410, deltaM: 183.4, gradePct: 7.61 }),
    ).toBe("2.41 km · +183 m · +7.6%");
  });

  it("reads descents with negative climb and grade", () => {
    expect(
      formatSelectionStats({ distanceM: 500, deltaM: -50, gradePct: -10 }),
    ).toBe("500 m · -50 m · -10.0%");
  });
});

describe("imperial units", () => {
  beforeEach(() => {
    useUnitsStore.setState({ mode: "imperial" });
  });
  afterEach(() => {
    useUnitsStore.setState({ mode: "metric" });
  });

  it("formats distance in miles, short distances in feet", () => {
    expect(formatDistance(5000)).toBe("3.11 mi");
    expect(formatDistance(42195)).toBe("26.22 mi");
    expect(formatDistance(100)).toBe("328 ft");
    expect(formatDistance(null)).toBe("--");
  });

  it("formats pace per mile", () => {
    // 3.3333 m/s → 1609.344 / 3.3333 ≈ 483 s/mi = 8:03 /mi
    expect(formatPace(1000 / 300)).toBe("8:03 /mi");
  });

  it("formats swim pace per 100 yd", () => {
    // 91.44 m / 1.0 m/s ≈ 91 s = 1:31 /100yd
    expect(formatSwimPace(1)).toBe("1:31 /100yd");
  });

  it("formats speed in mph", () => {
    expect(formatSpeed(10)).toBe("22.4 mph");
  });

  it("formats elevation in feet", () => {
    expect(formatElevation(1000)).toBe("3281 ft");
    expect(formatElevation(0)).toBe("0 ft");
  });

  it("formats selection stats in mi/ft", () => {
    expect(
      formatSelectionStats({ distanceM: 2410, deltaM: 183.4, gradePct: 7.61 }),
    ).toBe("1.50 mi · +602 ft · +7.6%");
  });
});

describe("formatHR", () => {
  it("returns -- for null", () => {
    expect(formatHR(null)).toBe("--");
  });

  it("formats bpm", () => {
    expect(formatHR(150)).toBe("150 bpm");
    expect(formatHR(72.4)).toBe("72 bpm");
  });
});
