import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import {
  displayTitle,
  generatedTitle,
  parseGeneratedTitle,
  TITLE_EVENTS,
  TITLE_GENDERS,
  TITLE_PARTS,
} from "./activityTitle";
import { translate, type Language, type Translator } from "./i18n";
import { MAX_TITLE_LENGTH, SPORT_TYPES } from "./types";
import { en } from "../locales/en";
import { ru } from "../locales/ru";
import { it as itMessages } from "../locales/it";

const DICTIONARIES = { en, ru, it: itMessages };

/** A `t` of one language, as a component's `useT()` would be. */
const tr =
  (lang: Language): Translator =>
  (key, ...args) =>
    translate(lang, key, ...args);

/** The English the backend writes for a part and a sport. */
const english = (part: (typeof TITLE_PARTS)[number], sport: (typeof SPORT_TYPES)[number]) =>
  en[`title.sport.${sport}`].replace("{part}", en[`title.part.${part}.m`]);

describe("generated titles in the interface language (#240)", () => {
  it("say a generated title in Russian, the part agreeing with the noun's gender", () => {
    const t = tr("ru");
    // Masculine, feminine, neuter.
    expect(displayTitle("Morning Run", t)).toBe("Утренний забег");
    expect(displayTitle("Night Mountain Bike", t)).toBe("Ночной заезд на МТБ");
    expect(displayTitle("Evening Walk", t)).toBe("Вечерняя прогулка");
    expect(displayTitle("Evening Sailing", t)).toBe("Вечерняя прогулка под парусом");
    expect(displayTitle("Morning Activity", t)).toBe("Утренняя тренировка");
    expect(displayTitle("Morning Mountaineering", t)).toBe("Утреннее восхождение");
    expect(displayTitle("Afternoon Ski", t)).toBe("Дневное катание на горных лыжах");
  });

  it("say it in Italian with the part after the noun, before a complement", () => {
    const t = tr("it");
    expect(displayTitle("Morning Run", t)).toBe("Corsa mattutina");
    expect(displayTitle("Morning Treadmill", t)).toBe("Corsa mattutina sul tapis roulant");
    expect(displayTitle("Evening Open Water", t)).toBe("Nuotata serale in acque libere");
    expect(displayTitle("Morning Indoor Ride", t)).toBe("Allenamento mattutino sui rulli");
    expect(displayTitle("Afternoon Ride", t)).toBe("Giro in bici pomeridiano");
    expect(displayTitle("Night Virtual Ride", t)).toBe("Giro virtuale notturno");
    expect(displayTitle("Evening Walk", t)).toBe("Camminata serale");
    expect(displayTitle("Night Swim", t)).toBe("Nuotata notturna");
  });

  it("say a merged event's title in the language", () => {
    expect(displayTitle("Triathlon", tr("ru"))).toBe("Триатлон");
    expect(displayTitle("Duathlon", tr("ru"))).toBe("Дуатлон");
    expect(displayTitle("Duathlon", tr("it"))).toBe("Duathlon");
    // A FIT triathlon is titled by its part of the day.
    expect(displayTitle("Morning Triathlon", tr("ru"))).toBe("Утренний триатлон");
  });

  it("keep the backend's English exactly, for every part, sport and event", () => {
    const t = tr("en");
    for (const part of TITLE_PARTS) {
      for (const sport of SPORT_TYPES) {
        const text = english(part, sport);
        expect(parseGeneratedTitle(text)).toEqual({ kind: "part", part, sport });
        expect(displayTitle(text, t)).toBe(text);
        expect(generatedTitle(part, sport, t)).toBe(text);
      }
    }
    for (const event of TITLE_EVENTS) {
      expect(displayTitle(en[`title.event.${event}`], t)).toBe(en[`title.event.${event}`]);
    }
  });

  it("name every part and sport apart in each language, with a capital and within the title's cap", () => {
    for (const lang of ["en", "ru", "it"] as const) {
      const titles = [
        ...TITLE_PARTS.flatMap((part) => SPORT_TYPES.map((sport) => generatedTitle(part, sport, tr(lang)))),
        ...TITLE_EVENTS.map((event) => displayTitle(en[`title.event.${event}`], tr(lang))),
      ];
      expect(new Set(titles).size, lang).toBe(titles.length);
      for (const title of titles) {
        expect(title).not.toMatch(/[{}]/);
        expect(title.charAt(0)).toBe(title.charAt(0).toLocaleUpperCase(lang));
        expect(title.length).toBeLessThanOrEqual(MAX_TITLE_LENGTH);
      }
    }
  });

  it("leave any other title as it is, a lookalike too", () => {
    const t = tr("ru");
    for (const title of [
      "Alanya Gravel",
      "Morning Run 2",
      "morning run",
      "MORNING RUN",
      "Morning  Run",
      " Morning Run",
      "Morning Run ",
      "Morning Other",
      "Lunch Run",
      "Morning",
      "Run",
      "triathlon",
      "{part} Run",
      "",
    ]) {
      expect(parseGeneratedTitle(title)).toBeNull();
      expect(displayTitle(title, t)).toBe(title);
    }
    expect(displayTitle(null, t)).toBeNull();
  });

  it("take a title of any sport, not only the activity's: a sport edited after import keeps its title", () => {
    // The helper is told the title alone — a ride renamed to a walk still says its ride.
    expect(displayTitle("Evening Ride", tr("ru"))).toBe("Вечерний заезд");
  });

  it("give every noun a gender its language has; English has none", () => {
    const allowed: Record<Language, readonly string[]> = { en: ["-"], ru: TITLE_GENDERS, it: ["m", "f"] };
    for (const lang of ["en", "ru", "it"] as const) {
      const odd = SPORT_TYPES.filter((s) => !allowed[lang].includes(DICTIONARIES[lang][`title.gender.${s}`]));
      expect(odd, lang).toEqual([]);
    }
  });

  it("place the part once in every sport's title", () => {
    for (const lang of ["en", "ru", "it"] as const) {
      const odd = SPORT_TYPES.filter((s) => DICTIONARIES[lang][`title.sport.${s}`].split("{part}").length !== 2);
      expect(odd, lang).toEqual([]);
    }
  });
});

/** The backend writes the title (models/activity.rs `default_activity_title`,
 * db/multisport_legs.rs for a merged event): a part, a `SportType::label()`
 * or an event the helper does not know would leave that title in English.
 * The hours of each part do not matter here. Read from the source, not run:
 * a special case for one sport inside the Rust function is not detected. */
describe("generated title parity with the backend", () => {
  const source = (path: string) => readFileSync(new URL(`../../src-tauri/src/${path}`, import.meta.url), "utf8");
  const rust = source("models/activity.rs");
  const section = (from: string, to: string) => rust.slice(rust.indexOf(from), rust.indexOf(to, rust.indexOf(from)));

  it("knows every part, in the backend's words and its format", () => {
    const body = section("pub fn default_activity_title", "\n}\n");
    const parts = [...body.matchAll(/=> "([^"]+)"/g)].map((m) => m[1]);
    expect(parts).toEqual(TITLE_PARTS.map((p) => en[`title.part.${p}.m`]));
    expect(body).toContain('format!("{} {}", part, sport.label())');
  });

  it("knows every SportType::label(), by the sport's id", () => {
    const variant = (text: string) =>
      new Map([...text.matchAll(/SportType::(\w+) => "([^"]+)"/g)].map((m) => [m[1], m[2]] as const));
    const slugs = variant(section("pub fn as_str", "pub fn label"));
    const labels = variant(section("pub fn label", "\n    }\n}"));
    expect(labels.size).toBeGreaterThan(20);
    expect([...labels.keys()].sort()).toEqual([...slugs.keys()].sort());
    const fromRust = [...labels].map(([v, label]) => [slugs.get(v), `{part} ${label}`]).sort();
    const known = SPORT_TYPES.map((s) => [s, en[`title.sport.${s}`]]).sort();
    expect(known).toEqual(fromRust);
  });

  it("knows the title of every merged event", () => {
    const legs = source("db/multisport_legs.rs");
    const m = /let event_title = if [^{]*\{ "([^"]+)" \} else \{ "([^"]+)" \};/.exec(legs);
    expect(m).not.toBeNull();
    expect([m![1], m![2]].sort()).toEqual(TITLE_EVENTS.map((e) => en[`title.event.${e}`]).sort());
    expect(legs).toContain("title: Some(event_title.to_string())");
  });
});
