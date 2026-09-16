import { describeDevice, type DeviceForm } from "./devices";
import type { DeviceStats } from "./types";

/** The `source_device` value the backend treats as "no device" in a filter. */
export const NO_DEVICE = "";

/** One option of the device filter: a display label with every raw
 * `source_device` string that resolves to it. */
export interface DeviceGroup {
  /** Stable option key (the label; "" for the no-device group). */
  key: string;
  label: string;
  form: DeviceForm;
  /** Activities across all of the group's raw strings. */
  count: number;
  /** Raw `source_device` values the filter must send for this option. */
  raws: string[];
}

/**
 * Group the library's distinct device strings by their registry label,
 * most used first; on equal counts the no-device group (empty name from
 * the backend) goes after the named ones, which sort by label. "Garmin
 * fenix6x" and "Garmin fenix6x_asia" are one "fenix 6X Pro" option whose
 * selection sends both raw strings.
 */
export function groupDevices(stats: DeviceStats[]): DeviceGroup[] {
  const groups = new Map<string, DeviceGroup>();
  for (const s of stats) {
    const info = s.device_name === NO_DEVICE ? null : describeDevice(s.device_name);
    const key = info ? info.label : NO_DEVICE;
    const group = groups.get(key) ?? {
      key,
      label: info ? info.label : "No device",
      form: info ? info.form : "generic",
      count: 0,
      raws: [],
    };
    group.count += s.activity_count;
    group.raws.push(s.device_name);
    groups.set(key, group);
  }
  return [...groups.values()].sort(
    (a, b) => b.count - a.count || (a.key === NO_DEVICE ? 1 : 0) - (b.key === NO_DEVICE ? 1 : 0) || a.label.localeCompare(b.label),
  );
}

/**
 * Option keys the filter's device list touches. ANY raw string of a group
 * counts, not all of them: a filter set before an import (or the #144
 * startup backfill) can hold a subset of a group's strings, and an option
 * that then showed unticked while the filter stayed active would be a
 * state the drawer cannot explain. Ticked, the next interaction resends
 * the group's full current set through `devicesForKeys`.
 */
export function selectedDeviceKeys(groups: DeviceGroup[], devices: string[]): string[] {
  const set = new Set(devices);
  return groups.filter((g) => g.raws.some((r) => set.has(r))).map((g) => g.key);
}

/** The raw strings to send for the chosen option keys, in group order. */
export function devicesForKeys(groups: DeviceGroup[], keys: string[]): string[] {
  const chosen = new Set(keys);
  return groups.filter((g) => chosen.has(g.key)).flatMap((g) => g.raws);
}
