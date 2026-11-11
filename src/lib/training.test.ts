import { describe, it, expect } from "vitest";
import { canBegin, devicesFor, powerZone, trainingSupported } from "./training";
import type { FoundDevice, TrainingLive } from "./types";

describe("powerZone", () => {
  it("is the Coggan zone at the FTP, a ceiling in its own zone", () => {
    expect(powerZone(100, 250)).toBe(1);
    expect(powerZone(137.5, 250)).toBe(1); // 55 %
    expect(powerZone(138, 250)).toBe(2);
    expect(powerZone(210, 254)).toBe(3);
    expect(powerZone(250, 250)).toBe(4);
    expect(powerZone(290, 250)).toBe(5);
    expect(powerZone(360, 250)).toBe(6);
    expect(powerZone(800, 250)).toBe(7);
  });

  it("is nothing without watts or an FTP", () => {
    expect(powerZone(null, 250)).toBeNull();
    expect(powerZone(200, null)).toBeNull();
    expect(powerZone(200, 0)).toBeNull();
  });
});

describe("devices and the start", () => {
  const found: FoundDevice[] = [
    { role: "trainer", address: "T", name: "AVANTI COG" },
    { role: "pedals", address: "P", name: null },
    { role: "strap", address: "H", name: "HRM-Pro" },
  ];

  it("rides on macOS only", () => {
    expect(trainingSupported("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)")).toBe(true);
    expect(trainingSupported("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")).toBe(false);
    expect(typeof trainingSupported()).toBe("boolean");
  });

  it("lists the devices of a role", () => {
    expect(devicesFor(found, "pedals").map((d) => d.address)).toEqual(["P"]);
    expect(devicesFor([], "trainer")).toEqual([]);
  });

  it("lets the ride begin once the trainer is heard while pairing", () => {
    const at = (phase: TrainingLive["phase"], state: "connected" | "searching") =>
      ({ phase, links: [{ role: "trainer", state, name: null, problem: null, detail: null }] }) as unknown as TrainingLive;
    expect(canBegin(at("pairing", "connected"))).toBe(true);
    expect(canBegin(at("pairing", "searching"))).toBe(false);
    expect(canBegin(at("riding", "connected"))).toBe(false);
  });
});
