import { describe, it, expect, beforeEach, afterEach } from "vitest";
import {
  formatDistance,
  formatDuration,
  formatPace,
  formatSwimPace,
  formatPaceOrSpeed,
  paceOrSpeedLabel,
  avgPaceOrSpeedLabel,
  showsPace,
  formatSpeed,
  formatElevation,
  formatGrade,
  formatHR,
  formatSelectionStats,
  formatPower,
  formatWhole,
  formatWeight,
  formatDurationHM,
  formatMinutes,
  formatDate,
} from "./format";
import { useUnitsStore } from "./units";
import { useLanguageStore } from "./i18n";

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

describe("showsPace / avgPaceOrSpeedLabel (#234)", () => {
  afterEach(() => useLanguageStore.getState().setPref("system"));

  it("shows a pace for foot and swim sports, a speed for the rest", () => {
    expect(["run", "trail_run", "swim", "open_water"].map(showsPace)).toEqual([true, true, true, true]);
    expect(["ride", "paddle"].map(showsPace)).toEqual([false, false]);
  });

  it("names the average by the same choice, in the language", () => {
    expect(avgPaceOrSpeedLabel("run")).toBe("Avg Pace");
    expect(avgPaceOrSpeedLabel("ride")).toBe("Avg Speed");
    useLanguageStore.getState().setPref("ru");
    expect(avgPaceOrSpeedLabel("swim")).toBe("Ср. темп");
    expect(avgPaceOrSpeedLabel("ride")).toBe("Ср. скорость");
    useLanguageStore.getState().setPref("it");
    expect(avgPaceOrSpeedLabel("run")).toBe("Passo medio");
    expect(avgPaceOrSpeedLabel("ride")).toBe("Velocità media");
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

  it("slots elapsed time after the distance when present", () => {
    expect(
      formatSelectionStats({
        distanceM: 3070,
        deltaM: 146,
        gradePct: 4.8,
        durationS: 754,
      }),
    ).toBe("3.07 km · 12:34 · +146 m · +4.8%");
    // Timeless tracks (GPX without timestamps) keep the short form.
    expect(
      formatSelectionStats({ distanceM: 3070, deltaM: 146, gradePct: 4.8, durationS: null }),
    ).toBe("3.07 km · +146 m · +4.8%");
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
    expect(formatElevation(1000)).toBe("3,281 ft");
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

describe("formatPower / formatWhole / formatWeight", () => {
  afterEach(() => {
    useUnitsStore.setState({ mode: "metric" });
  });

  it("rounds to a whole number with its unit", () => {
    expect(formatPower(null)).toBe("--");
    expect(formatPower(212.6)).toBe("213 W");
    expect(formatWhole(1240.4, "kcal")).toBe("1,240 kcal");
    expect(formatWhole(86.2, "spm")).toBe("86 spm");
    // A rounded negative zero reads as zero, as the old templates wrote it.
    expect(formatElevation(-0.3)).toBe("0 m");
  });

  it("writes a weight whole, or with its one decimal, in the display unit", () => {
    expect(formatWeight(100)).toBe("100 kg");
    expect(formatWeight(22.5)).toBe("22.5 kg");
    useUnitsStore.setState({ mode: "imperial" });
    expect(formatWeight(22.5)).toBe("49.6 lb");
  });
});

describe("formatDurationHM / formatMinutes", () => {
  it("writes hours and minutes, or minutes alone", () => {
    expect(formatDurationHM(null)).toBe("--");
    expect(formatDurationHM(5000)).toBe("1h 23m");
    expect(formatDurationHM(3900)).toBe("1h 05m");
    expect(formatDurationHM(1500)).toBe("25m");
  });

  it("writes whole minutes under an hour, hours and minutes above", () => {
    expect(formatMinutes(24.6)).toBe("25 min");
    expect(formatMinutes(85)).toBe("1h 25m");
    expect(formatMinutes(120)).toBe("2h");
  });
});

describe("formatDate", () => {
  it("writes an activity's start with date and time", () => {
    // ICU writes the space before PM as a plain or a narrow no-break one.
    // The hour is the language's own, unpadded on the 12-hour clock.
    expect(formatDate("2026-06-02T18:05:00")).toMatch(/^Jun 2, 2026, 6:05\sPM$/u);
    expect(formatDate("2026-06-02T09:05:00")).toMatch(/^Jun 2, 2026, 9:05\sAM$/u);
  });
});

describe("in Russian and Italian (#232)", () => {
  afterEach(() => {
    useLanguageStore.getState().setPref("system");
    useUnitsStore.setState({ mode: "metric" });
  });

  it("writes Russian numbers, symbols and words", () => {
    useLanguageStore.getState().setPref("ru");
    expect(formatDistance(1500)).toBe("1,50 км");
    expect(formatDistance(500)).toBe("500 м");
    expect(formatSpeed(10)).toBe("36,0 км/ч");
    expect(formatPace(1000 / 300)).toBe("5:00 /км");
    expect(formatSwimPace(1)).toBe("1:40 /100 м");
    expect(formatHR(150)).toBe("150 уд/мин");
    expect(formatPower(250)).toBe("250 Вт");
    expect(formatWeight(22.5)).toBe("22,5 кг");
    expect(formatGrade(8.34)).toBe("+8,3\xa0%");
    // A grade that rounds to zero carries no sign.
    expect(formatGrade(-0.04)).toBe("0,0\xa0%");
    expect(paceOrSpeedLabel("run")).toBe("Темп");
    expect(paceOrSpeedLabel("ride")).toBe("Скорость");
    expect(formatDurationHM(5000)).toBe("1 ч 23 мин");
    expect(formatDurationHM(1500)).toBe("25 мин");
    expect(formatMinutes(25)).toBe("25 мин");
    expect(formatMinutes(120)).toBe("2 ч");
    expect(formatDate("2026-06-02T18:05:00")).toBe("2 июн. 2026 г., 18:05");
    // A 24-hour clock, whatever the system's region says.
    expect(formatDate("2026-06-02T09:05:00")).toBe("2 июн. 2026 г., 9:05");
    expect(formatSelectionStats({ distanceM: 2410, deltaM: 183.4, gradePct: 7.61 })).toBe(
      "2,41 км · +183 м · +7,6\xa0%",
    );
    useUnitsStore.setState({ mode: "imperial" });
    expect(formatDistance(5000)).toBe("3,11 ми");
    expect(formatDistance(100)).toBe("328 фт");
    expect(formatSpeed(10)).toBe("22,4 ми/ч");
    expect(formatPace(1000 / 300)).toBe("8:03 /ми");
    expect(formatSwimPace(1)).toBe("1:31 /100 ярд");
    expect(formatWeight(22.5)).toBe("49,6 фнт");
  });

  it("writes Italian numbers with the international symbols", () => {
    useLanguageStore.getState().setPref("it");
    expect(formatDistance(42195)).toBe("42,20 km");
    expect(formatSpeed(10)).toBe("36,0 km/h");
    expect(formatHR(150)).toBe("150 bpm");
    expect(paceOrSpeedLabel("swim")).toBe("Passo");
    expect(paceOrSpeedLabel("ride")).toBe("Velocità");
    expect(formatDurationHM(5000)).toBe("1 h 23 min");
    expect(formatMinutes(85)).toBe("1 h 25 min");
    expect(formatSwimPace(1)).toBe("1:40 /100 m");
    expect(formatDate("2026-06-02T18:05:00")).toBe("2 giu 2026, 18:05");
    expect(formatElevation(1214)).toBe("1214 m");
  });
});

describe("rounding (#232)", () => {
  it("rounds a tie on the decimal value as written, not on its binary", () => {
    // 1005 m is 1.005 km: toFixed(2) read the binary 1.00499… as "1.00".
    expect(formatDistance(1005)).toBe("1.01 km");
    expect(formatSpeed(1.25 / 3.6)).toBe("1.3 km/h");
  });
});
