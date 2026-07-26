// @vitest-environment happy-dom
import { describe, it, expect, afterEach } from "vitest";
import { render, cleanup } from "@testing-library/react";
import type { Activity } from "../../lib/types";
import { CyclingDynamicsPanel, hasCyclingDynamics } from "./CyclingDynamicsPanel";

afterEach(cleanup);

// Values from the reference Edge 1030 + dual-sided pedals ride.
function dynamicsActivity(over: Partial<Activity> = {}): Activity {
  return {
    id: "a1",
    sport_type: "ride",
    start_time: "2026-07-26T07:58:20+03:00",
    duration_s: 10723,
    avg_left_right_balance: 42.58, // right %
    avg_left_pco_mm: 0,
    avg_right_pco_mm: 9,
    avg_left_power_phase_start_deg: 324.8,
    avg_left_power_phase_end_deg: 230.6,
    avg_left_power_phase_peak_start_deg: 70.3,
    avg_left_power_phase_peak_end_deg: 125.2,
    avg_right_power_phase_start_deg: 355.8,
    avg_right_power_phase_end_deg: 208.1,
    avg_right_power_phase_peak_start_deg: 68.9,
    avg_right_power_phase_peak_end_deg: 113.9,
    avg_power_seated_w: 231,
    avg_power_standing_w: 161,
    max_power_seated_w: 1013,
    max_power_standing_w: 956,
    avg_cadence_seated: 83,
    avg_cadence_standing: 55,
    max_cadence_seated: 111,
    max_cadence_standing: 105,
    time_standing_s: 1156.852,
    stand_count: 90,
    ...over,
  } as unknown as Activity;
}

describe("CyclingDynamicsPanel", () => {
  it("renders balance, PCO, phases and the seated/standing split", () => {
    const { container } = render(<CyclingDynamicsPanel activity={dynamicsActivity()} />);
    const text = container.textContent!;
    // Balance: stored right % → left = 100 - right.
    expect(text).toContain("57.4%");
    expect(text).toContain("42.6%");
    expect(text).toContain("+9 mm");
    expect(text).toContain("325° → 231°");
    expect(text).toContain("356° → 208°");
    // Seated time = timer − standing (10723 − 1157 = 9566 s → 2:39:26).
    expect(text).toContain("2:39:26");
    expect(text).toContain("19:17");
    expect(text).toContain("×90");
    expect(text).toContain("231 / 1013 W");
    expect(text).toContain("55 / 105 rpm");
    // Both phase gauges drew their arcs.
    expect(container.querySelectorAll("svg path").length).toBe(4);
  });

  it("renders nothing without any dynamics data", () => {
    const bare = dynamicsActivity({
      avg_left_right_balance: null,
      avg_left_pco_mm: null,
      avg_right_pco_mm: null,
      avg_left_power_phase_start_deg: null,
      avg_right_power_phase_start_deg: null,
      avg_power_seated_w: null,
      time_standing_s: null,
    });
    expect(hasCyclingDynamics(bare)).toBe(false);
    const { container } = render(<CyclingDynamicsPanel activity={bare} />);
    expect(container.firstChild).toBeNull();
  });

  it("balance-only meters still get the panel, without phase/position rows", () => {
    const balanceOnly = dynamicsActivity({
      avg_left_pco_mm: null,
      avg_right_pco_mm: null,
      avg_left_power_phase_start_deg: null,
      avg_left_power_phase_end_deg: null,
      avg_left_power_phase_peak_start_deg: null,
      avg_left_power_phase_peak_end_deg: null,
      avg_right_power_phase_start_deg: null,
      avg_right_power_phase_end_deg: null,
      avg_right_power_phase_peak_start_deg: null,
      avg_right_power_phase_peak_end_deg: null,
      avg_power_seated_w: null,
      avg_power_standing_w: null,
      max_power_seated_w: null,
      max_power_standing_w: null,
      avg_cadence_seated: null,
      avg_cadence_standing: null,
      max_cadence_seated: null,
      max_cadence_standing: null,
      time_standing_s: null,
      stand_count: null,
    });
    const { container } = render(<CyclingDynamicsPanel activity={balanceOnly} />);
    const text = container.textContent!;
    expect(text).toContain("Balance");
    expect(text).not.toContain("Power phase");
    expect(text).not.toContain("Seated");
    expect(container.querySelector("svg")).toBeNull();
  });
});