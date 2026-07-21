import { describe, expect, it } from "vitest";
import { legRows } from "./MultisportLegs";
import type { MultisportLeg } from "../../lib/types";

const leg = (
  leg_number: number,
  sport_type: string,
  overrides: Partial<MultisportLeg> = {},
): MultisportLeg => ({
  id: null,
  activity_id: "tri-1",
  leg_number,
  sport_type,
  is_transition: sport_type === "transition",
  start_time: null,
  total_distance_m: 10000,
  total_timer_time_s: 2400,
  total_elapsed_time_s: 2460,
  avg_speed_mps: 4.0,
  avg_hr: 155,
  max_hr: 175,
  total_ascent_m: 40,
  total_calories: null,
  source_activity_id: null,
  ...overrides,
});

describe("legRows", () => {
  it("labels transitions T1/T2 by order and keeps sport legs labeled", () => {
    const rows = legRows([
      leg(1, "swim", { total_distance_m: 1500, avg_speed_mps: 1.3 }),
      leg(2, "transition", { total_timer_time_s: 95 }),
      leg(3, "ride", { total_distance_m: 40000, avg_speed_mps: 9.5 }),
      leg(4, "transition", { total_timer_time_s: 60 }),
      leg(5, "run"),
    ]);
    expect(rows.map((r) => r.label)).toEqual(["Swim", "T1", "Ride", "T2", "Run"]);
    // Transitions show only their time.
    expect(rows[1].time).not.toBe("");
    expect(rows[1].distance).toBe("");
    expect(rows[1].effort).toBe("");
  });

  it("marks FIT-native sport legs focusable; merged and transitions not", () => {
    const rows = legRows([
      leg(1, "swim", { start_time: "2021-07-17T08:30:00+03:00" }),
      leg(2, "transition", { total_timer_time_s: 90 }),
      leg(3, "run", { start_time: "2021-07-17T09:00:00+03:00", source_activity_id: "act-run" }),
      leg(4, "run"), // no start_time — nothing to window by
    ]);
    expect(rows.map((r) => r.focusable)).toEqual([true, false, false, false]);
  });

  it("links sport legs to their source activity, not transitions", () => {
    const rows = legRows([
      leg(1, "swim", { source_activity_id: "act-swim" }),
      leg(2, "transition", { total_timer_time_s: 90, source_activity_id: null }),
      leg(3, "run", { source_activity_id: "act-run" }),
    ]);
    expect(rows[0].link).toBe("act-swim");
    expect(rows[1].link).toBeNull();
    expect(rows[2].link).toBe("act-run");
  });

  it("shows swim pace for swims, running pace for runs, speed for rides", () => {
    const rows = legRows([
      leg(1, "swim", { avg_speed_mps: 1.3 }),
      leg(2, "ride", { avg_speed_mps: 9.5 }),
      leg(3, "run", { avg_speed_mps: 4.0 }),
    ]);
    // Swim pace is per 100 m; running pace per km; speed a rate with unit.
    expect(rows[0].effort).toBe("1:17 /100m");
    expect(rows[2].effort).toContain(" /km");
    expect(rows[1].effort).not.toContain(":");
  });

  it("prefers timer time and falls back to elapsed, dashes the unknown", () => {
    const rows = legRows([
      leg(1, "run", { total_timer_time_s: null, total_elapsed_time_s: 2460 }),
      leg(2, "run", {
        total_timer_time_s: null,
        total_elapsed_time_s: null,
        total_distance_m: null,
        avg_speed_mps: null,
        avg_hr: null,
        total_ascent_m: null,
      }),
    ]);
    expect(rows[0].time).not.toBe("—");
    expect(rows[1].time).toBe("—");
    expect(rows[1].distance).toBe("—");
    expect(rows[1].effort).toBe("—");
    expect(rows[1].hr).toBe("—");
  });
});
