import { describe, it, expect } from "vitest";
import { groupDevices, selectedDeviceKeys, devicesForKeys, NO_DEVICE } from "./deviceFilter";
import type { DeviceStats } from "./types";

const stat = (device_name: string, activity_count: number): DeviceStats => ({
  device_name,
  activity_count,
  last_activity: "2026-09-16T08:00:00+00:00",
});

describe("groupDevices", () => {
  it("merges raw strings that resolve to the same label and sums their counts", () => {
    const groups = groupDevices([
      stat("Garmin fenix6x", 900),
      stat("Garmin edge_840", 40),
      stat("Garmin fenix6x_asia", 300),
      stat("", 12),
    ]);
    expect(groups.map((g) => [g.key, g.label, g.count, g.form])).toEqual([
      ["fenix 6X Pro", "fenix 6X Pro", 1200, "watch_multi"],
      ["Edge 840", "Edge 840", 40, "bike_computer"],
      [NO_DEVICE, "No device", 12, "generic"],
    ]);
    expect(groups[0].raws).toEqual(["Garmin fenix6x", "Garmin fenix6x_asia"]);
  });

  it("orders by count, the no-device group after a tie, then by label", () => {
    const groups = groupDevices([stat("", 5), stat("Garmin edge_840", 5), stat("ELEMNT BOLT", 5)]);
    expect(groups.map((g) => g.label)).toEqual(["Edge 840", "ELEMNT BOLT", "No device"]);
  });

  it("is empty for an empty library", () => {
    expect(groupDevices([])).toEqual([]);
  });
});

describe("selection ↔ raw strings", () => {
  const groups = groupDevices([stat("Garmin fenix6x", 2), stat("Garmin fenix6x_asia", 1), stat("Garmin edge_840", 1), stat("", 1)]);

  it("maps chosen option keys to every raw string of those groups", () => {
    expect(devicesForKeys(groups, ["fenix 6X Pro", NO_DEVICE])).toEqual(["Garmin fenix6x", "Garmin fenix6x_asia", ""]);
    expect(devicesForKeys(groups, [])).toEqual([]);
    expect(devicesForKeys(groups, ["unknown"])).toEqual([]);
  });

  it("marks an option selected when any of its raw strings is in the filter", () => {
    expect(selectedDeviceKeys(groups, ["Garmin fenix6x", "Garmin fenix6x_asia", ""])).toEqual(["fenix 6X Pro", NO_DEVICE]);
    // A filter set before a second raw string of the model appeared (an
    // import, the #144 backfill) still shows the option ticked, and the
    // next pick resends the full current set.
    expect(selectedDeviceKeys(groups, ["Garmin fenix6x"])).toEqual(["fenix 6X Pro"]);
    expect(devicesForKeys(groups, selectedDeviceKeys(groups, ["Garmin fenix6x"]))).toEqual(["Garmin fenix6x", "Garmin fenix6x_asia"]);
    expect(selectedDeviceKeys(groups, [])).toEqual([]);
    expect(selectedDeviceKeys(groups, ["Garmin gone_1620"])).toEqual([]);
  });
});
