// @vitest-environment happy-dom
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render } from "@testing-library/react";
import type { DashboardData } from "../../lib/types";
import { SummaryCards } from "./SummaryCards";
import { useLanguageStore } from "../../lib/i18n";

afterEach(() => {
  cleanup();
  useLanguageStore.getState().setPref("system");
});

const data = {
  week: { activities: 4, distance_m: 51234, duration_s: 9000, elev_gain_m: 640, avg_hr: 142.4 },
} as unknown as DashboardData;

/** Each tile's value and unit, as "value|unit". */
const tiles = (container: HTMLElement) =>
  Array.from(container.querySelectorAll(".font-num"), (el) => {
    const unit = el.querySelector("span")?.textContent;
    return unit ? `${el.firstChild!.textContent}|${unit}` : el.textContent;
  });

describe("SummaryCards (#232)", () => {
  it("splits each value from its unit", () => {
    expect(tiles(render(<SummaryCards data={data} />).container)).toEqual([
      "4",
      "51.23|km",
      "2.5|h",
      "640|m",
      "142|bpm",
    ]);
  });

  it("writes the numbers and units in Russian", () => {
    useLanguageStore.getState().setPref("ru");
    expect(tiles(render(<SummaryCards data={data} />).container)).toEqual([
      "4",
      "51,23|км",
      "2,5|ч",
      "640|м",
      "142|уд/мин",
    ]);
  });
});
