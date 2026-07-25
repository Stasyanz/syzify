import { describe, it, expect } from "vitest";
import { QueryClient } from "@tanstack/react-query";
import { invalidateActivityData } from "./activityInvalidation";

function seed(qc: QueryClient, queryKey: unknown[]) {
  qc.setQueryData(queryKey, { seeded: true });
}

function isInvalidated(qc: QueryClient, queryKey: unknown[]): boolean {
  const state = qc.getQueryState(queryKey);
  if (!state) throw new Error(`query not found: ${JSON.stringify(queryKey)}`);
  return state.isInvalidated;
}

describe("invalidateActivityData", () => {
  it("invalidates every activity-derived query, including parameterized ones", () => {
    const qc = new QueryClient();
    const affected: unknown[][] = [
      ["activities", { sport: "run", limit: 50 }],
      ["activities", "year-range"],
      ["activity-locations", { sport: null }],
      ["calendar", 2026, 7],
      ["dashboard"],
      ["usedSportTypes"],
      ["recordBadges", "a-1"],
      ["adjacent", "a-1"],
    ];
    affected.forEach((k) => seed(qc, k));

    invalidateActivityData(qc);

    affected.forEach((k) => expect(isInvalidated(qc, k), JSON.stringify(k)).toBe(true));
  });

  it("leaves unrelated queries alone", () => {
    const qc = new QueryClient();
    const unrelated: unknown[][] = [["plugins"], ["encryptionStatus"], ["setting", "map_layer"]];
    unrelated.forEach((k) => seed(qc, k));

    invalidateActivityData(qc);

    unrelated.forEach((k) => expect(isInvalidated(qc, k), JSON.stringify(k)).toBe(false));
  });
});