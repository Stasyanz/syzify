// @vitest-environment happy-dom
import { describe, it, expect, afterEach } from "vitest";
import { render, cleanup, screen } from "@testing-library/react";
import type { ActivitySummary } from "../../lib/types";
import { ActivityListItem, showDeviceFromSetting } from "./ActivityListItem";

afterEach(cleanup);

const summary = (source_device: string | null): ActivitySummary => ({
  id: "a-1",
  start_time: "2026-07-01T08:00:00+00:00",
  sport_type: "ride",
  title: "Morning Ride",
  distance_m: 65000,
  duration_s: 9000,
  elev_gain_m: 1400,
  avg_speed_mps: 7.2,
  avg_hr: 140,
  location_name: "Mahmutlar, Alanya",
  source_device: source_device,
  tags: [],
});

describe("ActivityListItem device line", () => {
  it("names the recording device with its silhouette and the full name on hover", () => {
    render(<ActivityListItem activity={summary("Garmin fenix6x")} onClick={() => {}} />);
    const dev = screen.getByTestId("row-device");
    expect(dev.textContent).toBe("fenix 6X Pro");
    expect(dev.getAttribute("data-tip")).toBe("Recorded on Garmin fenix 6X Pro");
    expect(dev.getAttribute("aria-label")).toBe("Recorded on Garmin fenix 6X Pro");
    expect(dev.querySelector("svg")!.getAttribute("data-form")).toBe("watch_multi");
    expect(dev.querySelector("svg")!.getAttribute("width")).toBe("16");
  });

  it("can be switched off per vault setting, unset meaning on", () => {
    render(<ActivityListItem activity={summary("Garmin fenix6x")} onClick={() => {}} showDevice={false} />);
    expect(screen.queryByTestId("row-device")).toBeNull();
    expect(showDeviceFromSetting(null)).toBe(true);
    expect(showDeviceFromSetting(undefined)).toBe(true);
    expect(showDeviceFromSetting("1")).toBe(true);
    expect(showDeviceFromSetting("0")).toBe(false);
  });

  it("shows nothing for an activity without a device, and keeps the location", () => {
    render(<ActivityListItem activity={summary(null)} onClick={() => {}} />);
    expect(screen.queryByTestId("row-device")).toBeNull();
    expect(screen.getByText("Mahmutlar, Alanya")).toBeTruthy();
  });

  it("lets a long location truncate rather than push the device onto the numbers", () => {
    const { container } = render(<ActivityListItem activity={summary("Garmin edge_840")} onClick={() => {}} />);
    const location = screen.getByText("Mahmutlar, Alanya");
    expect(location.className).toContain("truncate");
    const meta = location.closest("div")!;
    expect(meta.className).toContain("min-w-0");
    expect(meta.className).toContain("overflow-hidden");
    expect(screen.getByTestId("row-device").className).toContain("shrink-0");
    expect(container.querySelector("svg[data-form]")).toBeTruthy();
  });
});
