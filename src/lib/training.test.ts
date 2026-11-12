import { describe, it, expect } from "vitest";
import { canBegin, devicesFor, firstTarget, MAX_TARGET, pedalAddresses, powerZone, stepTarget, trainingSupported } from "./training";

describe("ERG targets", () => {
  it("start at an easy effort and step within the range", () => {
    expect(firstTarget(244)).toBe(160);
    expect(firstTarget(null)).toBe(150);
    expect(firstTarget(0)).toBe(150);
    expect(stepTarget(150, 10)).toBe(160);
    expect(stepTarget(15, -10)).toBe(10);
    expect(stepTarget(MAX_TARGET - 5, 10)).toBe(MAX_TARGET);
  });
});
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
    { role: "trainer", address: "T", name: "AVANTI COG", other_side: null, lone_side: false },
    { role: "pedals", address: "P", name: null, other_side: null, lone_side: false },
    { role: "strap", address: "H", name: "HRM-Pro", other_side: null, lone_side: false },
  ];

  it("rides on macOS only", () => {
    expect(trainingSupported("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)")).toBe(true);
    expect(trainingSupported("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")).toBe(false);
    expect(typeof trainingSupported()).toBe("boolean");
  });

  it("links both sides of a pedal pair", () => {
    const pair: FoundDevice[] = [{ role: "pedals", address: "PL", name: "ASSIOMA55430", other_side: "PR", lone_side: false }];
    expect(pedalAddresses(pair, "PL")).toEqual(["PL", "PR"]);
    expect(pedalAddresses(found, "P")).toEqual(["P"]);
    expect(pedalAddresses(found, null)).toEqual([]);
    expect(pedalAddresses(found, "T")).toEqual([]); // a trainer is no pedal
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
