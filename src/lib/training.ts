import { COGGAN_FTP_FACTORS } from "../components/activity/chartZones";
import type { FoundDevice, SensorRole, TrainingLive } from "./types";

/** The Coggan power zone (1–7) `watts` fall in at this FTP; null without
 * either. A ceiling belongs to its zone: 55 % of FTP is still Z1. */
export function powerZone(watts: number | null, ftp: number | null): number | null {
  if (watts == null || ftp == null || ftp <= 0) return null;
  const share = watts / ftp;
  const i = COGGAN_FTP_FACTORS.findIndex((ceiling) => share <= ceiling);
  return i < 0 ? 7 : i + 1;
}

/** Riding a trainer needs macOS (its Bluetooth link is macOS only). */
export function trainingSupported(userAgent: string = navigator.userAgent): boolean {
  return userAgent.includes("Mac");
}

/** The highest ERG target (the backend's `MAX_TARGET`). */
export const MAX_TARGET = 2000;

/** The ERG target `step` watts away from `target`, kept within 10–MAX. */
export function stepTarget(target: number, step: number): number {
  return Math.min(MAX_TARGET, Math.max(10, target + step));
}

/** Where ERG starts when switched on: an easy endurance effort (65 % of
 * FTP, Z2), rounded to 5 W; 150 W without an FTP. */
export function firstTarget(ftp: number | null): number {
  return ftp && ftp > 0 ? Math.round((ftp * 0.65) / 5) * 5 : 150;
}

/** The roles in the order the screen lists them. */
export const ROLES: readonly SensorRole[] = ["trainer", "pedals", "strap"];

/** The addresses to link for the picked pedals: a pair's two, else one. */
export function pedalAddresses(found: readonly FoundDevice[], picked: string | null): string[] {
  const device = found.find((d) => d.role === "pedals" && d.address === picked);
  if (!device) return [];
  return device.other_side ? [device.address, device.other_side] : [device.address];
}

/** The devices found for one role. */
export function devicesFor(found: readonly FoundDevice[], role: SensorRole): FoundDevice[] {
  return found.filter((d) => d.role === role);
}

/** Whether the ride can start: the trainer is heard. */
export function canBegin(live: TrainingLive): boolean {
  return live.phase === "pairing" && live.links.some((l) => l.role === "trainer" && l.state === "connected");
}
