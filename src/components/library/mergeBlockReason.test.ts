import { describe, expect, it } from "vitest";
import { mergeBlockReason } from "./ActivityList";
import { translate, useLanguageStore, type Translator } from "../../lib/i18n";

const act = (sport_type: string, start = "2021-07-17T08:00:00+03:00") => ({
  sport_type,
  start_time: start,
});

/** What `useT()` hands the component, in one language. */
const tIn =
  (lang: "en" | "ru" | "it"): Translator =>
  (key, ...args) =>
    translate(lang, key, ...args);
const en = tIn("en");

describe("mergeBlockReason", () => {
  it("allows 2-3 legs of distinct disciplines on one day", () => {
    expect(mergeBlockReason([act("swim"), act("ride"), act("run")], en)).toBeNull();
    // Run-bike-run duathlon: 3 legs, 2 disciplines.
    expect(mergeBlockReason([act("run"), act("ride"), act("trail_run")], en)).toBeNull();
    expect(mergeBlockReason([act("ski_xc"), act("run")], en)).toBeNull();
  });

  it("blocks wrong counts", () => {
    expect(mergeBlockReason([act("run")], en)).toContain("at least two");
    expect(
      mergeBlockReason([act("swim"), act("ride"), act("run"), act("run")], en),
    ).toContain("at most three");
  });

  it("blocks non-event sports, naming the offender", () => {
    expect(mergeBlockReason([act("strength"), act("paddle")], en)).toContain("Strength");
    // A sport the app does not know is named as it came.
    expect(mergeBlockReason([act("kitesurf"), act("run")], en)).toContain("kitesurf can't be a leg");
  });

  it("blocks cross-day and single-discipline selections", () => {
    expect(
      mergeBlockReason([act("swim"), act("run", "2021-07-18T08:00:00+03:00")], en),
    ).toContain("share a day");
    expect(mergeBlockReason([act("run"), act("trail_run")], en)).toContain("two disciplines");
  });

  it("says why in the interface language (#233)", () => {
    expect(mergeBlockReason([act("run")], tIn("ru"))).toBe("Выберите хотя бы две тренировки");
    // The sport is named by the same t as the sentence, whatever the store says.
    expect(useLanguageStore.getState().language).toBe("en");
    expect(mergeBlockReason([act("strength"), act("run")], tIn("it"))).toBe(
      "Forza non può essere una frazione: si combinano solo corsa, bici, nuoto e sci",
    );
  });
});
