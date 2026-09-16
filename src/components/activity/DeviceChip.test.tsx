// @vitest-environment happy-dom
import { describe, it, expect, afterEach } from "vitest";
import { render, cleanup, screen } from "@testing-library/react";
import { DeviceChip } from "./DeviceChip";

afterEach(cleanup);

describe("DeviceChip", () => {
  it("shows the model label with the family's silhouette and the full name on hover", () => {
    render(<DeviceChip source="Garmin fenix6x" />);
    const chip = screen.getByTestId("device-chip");
    expect(chip.textContent).toBe("fenix 6X Pro");
    expect(chip.getAttribute("data-tip")).toBe("Recorded on Garmin fenix 6X Pro");
    // The same sentence for screen readers: data-tip is CSS-only.
    expect(chip.getAttribute("aria-label")).toBe("Recorded on Garmin fenix 6X Pro");
    expect(chip.querySelector("svg")!.getAttribute("data-form")).toBe("watch_multi");
  });

  it.each([
    ["Garmin edge_840", "Edge 840", "bike_computer"],
    ["Garmin fr265_small", "Forerunner 265S", "watch_round"],
    ["Garmin instinct_2", "Instinct 2", "watch_instinct"],
    ["Garmin venusq2", "Venu Sq 2", "watch_rect"],
    ["Garmin 1620", "Garmin", "generic"],
    ["ELEMNT BOLT", "ELEMNT BOLT", "bike_computer"],
    ["StravaGPX", "Strava", "generic"],
    ["WorkOutDoors", "WorkOutDoors", "watch_rect"],
  ])("%s → chip %s with the %s silhouette", (source, label, form) => {
    render(<DeviceChip source={source} />);
    const chip = screen.getByTestId("device-chip");
    expect(chip.textContent).toBe(label);
    expect(chip.querySelector("svg")!.getAttribute("data-form")).toBe(form);
  });

  it("renders nothing for an activity without a device", () => {
    const { container } = render(<DeviceChip source={null} />);
    expect(container.innerHTML).toBe("");
    expect(render(<DeviceChip source="  " />).container.innerHTML).toBe("");
  });
});
