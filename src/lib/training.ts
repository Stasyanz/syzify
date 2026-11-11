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

/** The roles in the order the screen lists them. */
export const ROLES: readonly SensorRole[] = ["trainer", "pedals", "strap"];

/** The devices found for one role. */
export function devicesFor(found: readonly FoundDevice[], role: SensorRole): FoundDevice[] {
  return found.filter((d) => d.role === role);
}

/** Whether the ride can start: the trainer is heard. */
export function canBegin(live: TrainingLive): boolean {
  return live.phase === "pairing" && live.links.some((l) => l.role === "trainer" && l.state === "connected");
}
