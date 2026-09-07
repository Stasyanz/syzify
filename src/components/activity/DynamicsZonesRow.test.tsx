// @vitest-environment happy-dom
import { describe, it, expect, afterEach } from "vitest";
import { render, cleanup, fireEvent, screen } from "@testing-library/react";
import type { Activity, TimeInZone } from "../../lib/types";
import type { ReactElement } from "react";
import { DynamicsZonesRow, zonesFiller } from "./DynamicsZonesRow";
import { TimeInZonesPanel } from "./TimeInZonesPanel";

afterEach(cleanup);

function row(zone_type: string, zone_index: number, time_s: number, hi: number | null): TimeInZone {
  return { id: null, activity_id: "a", zone_type, zone_index, time_s, zone_high_boundary: hi };
}

const HR: TimeInZone[] = [
  row("hr", 0, 19, 93),
  row("hr", 1, 2674, 112),
  row("hr", 2, 4602, 130),
  row("hr", 3, 1357, 149),
  row("hr", 4, 0, 167),
  row("hr", 5, 0, 186),
  row("hr", 6, 0, null),
];

const run = { id: "a", sport_type: "run", duration_s: 8652 } as unknown as Activity;
const pedalRide = {
  id: "b",
  sport_type: "ride",
  duration_s: 8652,
  avg_left_right_balance: 51.1,
} as unknown as Activity;

function cards() {
  return [...document.querySelectorAll(".dash-card")].map((c) => ({
    title: c.querySelector("h3")?.textContent,
    span: c.className.includes("col-span-2"),
  }));
}

describe("DynamicsZonesRow", () => {
  it("puts dynamics left and zones right when both exist", () => {
    render(<DynamicsZonesRow activity={pedalRide} timeInZones={HR} />);
    expect(cards()).toEqual([
      { title: "Cycling Dynamics", span: false },
      { title: "Time in Zones", span: false },
    ]);
  });

  it("stretches the zones card across the row without dynamics", () => {
    render(<DynamicsZonesRow activity={run} timeInZones={HR} />);
    expect(cards()).toEqual([{ title: "Time in Zones", span: true }]);
  });

  it("stretches the dynamics card without zones, and in leg focus", () => {
    render(<DynamicsZonesRow activity={pedalRide} timeInZones={[]} />);
    expect(cards()).toEqual([{ title: "Cycling Dynamics", span: true }]);
    cleanup();
    render(<DynamicsZonesRow activity={pedalRide} timeInZones={HR} hideZones />);
    expect(cards()).toEqual([{ title: "Cycling Dynamics", span: true }]);
  });

  it("opens a new activity on power again after heart rate was picked", () => {
    const POWER: TimeInZone[] = [
      row("power", 0, 0, 0),
      row("power", 1, 2661, 129),
      row("power", 2, 2898, 176),
      row("power", 3, 1799, 212),
      row("power", 4, 825, 247),
      row("power", 5, 326, 282),
      row("power", 6, 135, 353),
      row("power", 7, 8, 3393),
    ];
    const both = [...HR, ...POWER];
    const { rerender } = render(<DynamicsZonesRow activity={pedalRide} timeInZones={both} />);
    const pressed = () =>
      screen.getAllByRole("button").find((b) => b.getAttribute("aria-pressed") === "true")!
        .textContent;
    fireEvent.click(screen.getByText("Heart rate"));
    expect(pressed()).toBe("Heart rate");
    // Same activity refreshed: the pick stays.
    rerender(<DynamicsZonesRow activity={{ ...pedalRide }} timeInZones={both} />);
    expect(pressed()).toBe("Heart rate");
    // Another activity: power again.
    rerender(<DynamicsZonesRow activity={{ ...pedalRide, id: "c" }} timeInZones={both} />);
    expect(pressed()).toBe("Power");
  });

  it("hands the chart grid a keyed zones card, or nothing", () => {
    expect(zonesFiller(run, [], false)).toBeUndefined();
    expect(zonesFiller(run, HR, true)).toBeUndefined();
    const card = zonesFiller(run, HR, false) as ReactElement;
    expect(card.type).toBe(TimeInZonesPanel);
    expect(card.key).toBe("a");
    render(card);
    expect(screen.getByText("Time in Zones")).toBeTruthy();
  });

  it("renders no row with neither", () => {
    render(<DynamicsZonesRow activity={run} timeInZones={[]} />);
    expect(screen.queryByTestId("dynamics-zones-row")).toBeNull();
    cleanup();
    render(<DynamicsZonesRow activity={run} timeInZones={HR} hideZones />);
    expect(screen.queryByTestId("dynamics-zones-row")).toBeNull();
  });
});
