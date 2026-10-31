// @vitest-environment happy-dom
import { describe, it, expect, afterEach } from "vitest";
import { render, cleanup } from "@testing-library/react";
import type { Activity } from "../../lib/types";
import { SummaryPanel } from "./SummaryPanel";
import { useLanguageStore } from "../../lib/i18n";

afterEach(cleanup);

function makeActivity(sport: string): Activity {
  return {
    id: "a1",
    sport_type: sport,
    start_time: "2026-07-04T06:47:00+00:00",
    distance_m: 4000,
    duration_s: 4440,
    avg_speed_mps: 0.9,
    elev_gain_m: 15, // GPS noise for a swim, real gain for a ride
    avg_hr: null,
    calories: null,
  } as unknown as Activity;
}

describe("SummaryPanel in the interface language (#232)", () => {
  it("writes the values and units the language's way", () => {
    useLanguageStore.getState().setPref("ru");
    try {
      const a = { ...makeActivity("ride"), avg_speed_mps: 7.5, calories: 1240.4 } as Activity;
      const { container } = render(<SummaryPanel activity={a} />);
      const text = container.textContent!;
      // Each tile sets its unit apart from the number.
      expect(text).toContain("4,00км");
      expect(text).toContain("Ср. скорость27,0км/ч");
      expect(text).toContain("1240ккал");
    } finally {
      useLanguageStore.getState().setPref("system");
    }
  });

  it("names the tiles in the language (#234)", () => {
    useLanguageStore.getState().setPref("ru");
    try {
      const a = { ...makeActivity("run"), avg_hr: 150, calories: 500 } as Activity;
      const { container } = render(<SummaryPanel activity={a} />);
      const labels = Array.from(container.querySelectorAll(".uppercase"), (l) => l.textContent);
      expect(labels).toEqual(["Дистанция", "Время", "Ср. темп", "Набор высоты", "Ср. пульс", "Калории"]);
    } finally {
      useLanguageStore.getState().setPref("system");
    }
  });
});

describe("SummaryPanel elevation tile", () => {
  it("shown for land sports", () => {
    const { container } = render(<SummaryPanel activity={makeActivity("ride")} />);
    expect(container.textContent).toContain("Elev Gain");
  });

  it("hidden for water sports (swim elevation is GPS noise)", () => {
    for (const sport of ["swim", "open_water"]) {
      const { container } = render(<SummaryPanel activity={makeActivity(sport)} />);
      expect(container.textContent).not.toContain("Elev Gain");
      // The rest of the summary is intact.
      expect(container.textContent).toContain("Distance");
      expect(container.textContent).toContain("Duration");
      cleanup();
    }
  });
});

describe("SummaryPanel time without GPS (#256)", () => {
  it("says how long the GPS was gone and that the map leaves it out", () => {
    const { container } = render(<SummaryPanel activity={makeActivity("ride")} noGpsSeconds={23 * 60 + 10} />);
    expect(container.textContent).toContain("No GPS for 23 minutes — those stretches are missing from the map");
  });

  it("says nothing under a minute or without the figure", () => {
    const short = render(<SummaryPanel activity={makeActivity("ride")} noGpsSeconds={45} />);
    expect(short.container.textContent).not.toContain("No GPS");
    cleanup();
    const none = render(<SummaryPanel activity={makeActivity("ride")} />);
    expect(none.container.textContent).not.toContain("No GPS");
  });

  it("speaks the interface language", () => {
    useLanguageStore.getState().setPref("ru");
    try {
      const { container } = render(<SummaryPanel activity={makeActivity("ride")} noGpsSeconds={5 * 60} />);
      expect(container.textContent).toContain("5 минут без GPS — этого на карте нет");
    } finally {
      useLanguageStore.getState().setPref("system");
    }
  });
});
