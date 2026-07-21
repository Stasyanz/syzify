import { describe, expect, it } from "vitest";
import { mergeBlockReason } from "./ActivityList";

const act = (sport_type: string, start = "2021-07-17T08:00:00+03:00") => ({
  sport_type,
  start_time: start,
});

describe("mergeBlockReason", () => {
  it("allows 2-3 legs of distinct disciplines on one day", () => {
    expect(mergeBlockReason([act("swim"), act("ride"), act("run")])).toBeNull();
    // Run-bike-run duathlon: 3 legs, 2 disciplines.
    expect(mergeBlockReason([act("run"), act("ride"), act("trail_run")])).toBeNull();
    expect(mergeBlockReason([act("ski_xc"), act("run")])).toBeNull();
  });

  it("blocks wrong counts", () => {
    expect(mergeBlockReason([act("run")])).toContain("at least two");
    expect(
      mergeBlockReason([act("swim"), act("ride"), act("run"), act("run")]),
    ).toContain("at most three");
  });

  it("blocks non-event sports, naming the offender", () => {
    expect(mergeBlockReason([act("strength"), act("paddle")])).toContain("Strength");
  });

  it("blocks cross-day and single-discipline selections", () => {
    expect(
      mergeBlockReason([act("swim"), act("run", "2021-07-18T08:00:00+03:00")]),
    ).toContain("share a day");
    expect(mergeBlockReason([act("run"), act("trail_run")])).toContain("two disciplines");
  });
});
