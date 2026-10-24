import { en } from "../locales/en";
import { capitalize, type Translator } from "./i18n";
import { SPORT_TYPES, type SportType } from "./types";

/** The parts of the day `default_activity_title` names (#240). */
export const TITLE_PARTS = ["morning", "afternoon", "evening", "night"] as const;
export type TitlePart = (typeof TITLE_PARTS)[number];

/** A title noun's grammatical gender: the form of the part's adjective.
 * Russian has all three, Italian m and f; no noun is a plural. */
export const TITLE_GENDERS = ["m", "f", "n"] as const;
export type TitleGender = (typeof TITLE_GENDERS)[number];

/** The events a merge names (db/multisport_legs.rs): "Triathlon", "Duathlon". */
export const TITLE_EVENTS = ["triathlon", "duathlon"] as const;
export type TitleEvent = (typeof TITLE_EVENTS)[number];

type Generated = { kind: "part"; part: TitlePart; sport: SportType } | { kind: "event"; event: TitleEvent };

/** Every title the backend generates, as it writes it ("Morning Run",
 * "Triathlon"), to what it is made of. Built once from the English
 * dictionary, which holds the backend's words (`activityTitle.test.ts`
 * keeps them equal). */
let generated: Map<string, Generated> | null = null;

function generatedTitles(): Map<string, Generated> {
  if (generated) return generated;
  generated = new Map();
  for (const part of TITLE_PARTS) {
    for (const sport of SPORT_TYPES) {
      const text = en[`title.sport.${sport}`].replace("{part}", en[`title.part.${part}.m`]);
      generated.set(text, { kind: "part", part, sport });
    }
  }
  for (const event of TITLE_EVENTS) generated.set(en[`title.event.${event}`], { kind: "event", event });
  return generated;
}

/** What a title the backend generated is made of, or null for any other
 * title. Only its exact text counts — "Morning Run 2" and "morning run"
 * are the user's — and any sport's: a sport changed after the import
 * keeps the title it came with. The hour is not checked. */
export function parseGeneratedTitle(title: string): Generated | null {
  return generatedTitles().get(title) ?? null;
}

/** A gender as a dictionary names it; English, which has none ("-"),
 * takes the first form. */
function titleGender(value: string): TitleGender {
  return (TITLE_GENDERS as readonly string[]).includes(value) ? (value as TitleGender) : "m";
}

/** The generated title of a part and a sport in `translate`'s language:
 * «Утренний забег», «Corsa mattutina sul tapis roulant» — the English is
 * the backend's. */
export function generatedTitle(part: TitlePart, sport: SportType, translate: Translator): string {
  const gender = titleGender(translate(`title.gender.${sport}`));
  return capitalize(translate(`title.sport.${sport}`, { part: translate(`title.part.${part}.${gender}`) }));
}

/** An activity's title as it is shown: a generated one ("Morning Run",
 * "Triathlon", stored in English) in the interface language, any other as
 * it is. Only the display changes — the stored title, exports and search
 * keep the English. */
export function displayTitle(title: string, translate: Translator): string;
export function displayTitle(title: string | null, translate: Translator): string | null;
export function displayTitle(title: string | null, translate: Translator): string | null {
  if (title === null) return null;
  const parsed = parseGeneratedTitle(title);
  if (!parsed) return title;
  return parsed.kind === "event"
    ? translate(`title.event.${parsed.event}`)
    : generatedTitle(parsed.part, parsed.sport, translate);
}
