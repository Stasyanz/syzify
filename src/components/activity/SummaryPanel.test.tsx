// @vitest-environment happy-dom
import { describe, it, expect, afterEach } from "vitest";
import { render, cleanup } from "@testing-library/react";
import type { Activity } from "../../lib/types";
import { SummaryPanel } from "./SummaryPanel";

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
