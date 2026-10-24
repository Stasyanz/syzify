import { createElement, Fragment, useCallback, type ReactNode } from "react";
import { create } from "zustand";
import { en } from "../locales/en";
import { ru } from "../locales/ru";
import { it } from "../locales/it";
import type { MessageKey, Messages, PluralMessage } from "../locales/types";

export type Language = "en" | "ru" | "it";
/** What the user picked: a language, or whatever the system prefers. */
export type LanguagePref = "system" | Language;

export const LANGUAGES: readonly Language[] = ["en", "ru", "it"];

/** Each language in its own words, for the picker: whoever cannot read
 * the current language still finds theirs. */
export const LANGUAGE_NAMES: Record<Language, string> = {
  en: "English",
  ru: "Русский",
  it: "Italiano",
};

const DICTIONARIES: Record<Language, Messages> = { en, ru, it };

const STORAGE_KEY = "syzify-language";

/** Read the persisted preference, defaulting to "system".
 * localStorage is absent in node test environments — fall back quietly. */
export function readStoredLanguage(): LanguagePref {
  if (typeof localStorage === "undefined") return "system";
  const v = localStorage.getItem(STORAGE_KEY);
  return v === "system" || LANGUAGES.includes(v as Language) ? (v as LanguagePref) : "system";
}

/** The first of the system's preferred languages Syzify has, else English.
 * Matches on the primary subtag: "ru-RU" and "it-CH" count. Pure. */
export function resolveLanguage(pref: LanguagePref, preferred: readonly string[]): Language {
  if (pref !== "system") return pref;
  for (const tag of preferred) {
    const primary = tag.split("-")[0].toLowerCase();
    if (LANGUAGES.includes(primary as Language)) return primary as Language;
  }
  return "en";
}

function systemLanguages(): readonly string[] {
  if (typeof navigator === "undefined") return [];
  return navigator.languages?.length ? navigator.languages : [navigator.language];
}

// ── Messages ────────────────────────────────────────────────────────────

/** The `{name}` placeholders of a message, as a union of names. Names are
 * word characters, as `formatMessage` fills them (the dictionary tests
 * hold every message to that). */
type Placeholders<S> = S extends `${string}{${infer P}}${infer R}` ? P | Placeholders<R> : never;
/** The parameters an English message needs: none for a plain message
 * without placeholders, else every placeholder by name — and a numeric
 * `count` for a plural one, which picks the form. Not distributed over a
 * union of keys: `t(cond ? a : b)` needs whatever either needs. */
export type ArgsOf<M> = [M] extends [string]
  ? [Placeholders<M>] extends [never]
    ? []
    : [params: Record<Placeholders<M>, string | number>]
  : [params: Record<Exclude<Placeholders<M[keyof M]>, "count">, string | number> & { count: number }];
/** The parameters a key needs, checked by `tsc`. */
export type MessageArgs<K extends MessageKey> = ArgsOf<(typeof en)[K]>;
/** What `useRichT` takes for a message: every placeholder, each a value or
 * an element — and a numeric `count` for a plural one. */
export type RichArgsOf<M> = [M] extends [string]
  ? Record<Placeholders<M>, ReactNode>
  : Record<Exclude<Placeholders<M[keyof M]>, "count">, ReactNode> & { count: number };

// One formatter per language and options, not one per call: lists format
// hundreds.
const pluralRules = new Map<string, Intl.PluralRules>();
const numberFormats = new Map<string, Intl.NumberFormat>();
const dateFormats = new Map<string, Intl.DateTimeFormat>();

function cached<T>(cache: Map<string, T>, key: string, make: () => T): T {
  let v = cache.get(key);
  if (!v) cache.set(key, (v = make()));
  return v;
}

/** How each language groups thousands — one policy for every number the
 * app writes, message parameters included. English and Italian as CLDR
 * has them ("auto": "1,214", but "1214" in Italian, whose CLDR data starts
 * groups at five digits); Russian typography leaves a four-digit number
 * whole too ("1214", "12 345"), which CLDR's Russian data does not. An
 * engine before NumberFormat v3 reads either string as `true` and groups
 * every thousand instead — no error. */
const GROUPING: Record<Language, Intl.NumberFormatOptions["useGrouping"]> = {
  en: "auto",
  ru: "min2",
  it: "auto",
};

/** `value` by a cached formatter in `lang` with the language's grouping.
 * A value that rounds to zero ("-0.4" to no digits) is written "0", not
 * "-0": its minus is dropped from the parts, since `signDisplay:
 * "negative"` is NumberFormat v3 and an older WebKit throws a RangeError on
 * it. Parts, not the string: a language's minus may be U+2212. */
function formatIn(lang: Language, value: number, options: Intl.NumberFormatOptions = {}): string {
  const full: Intl.NumberFormatOptions = { useGrouping: GROUPING[lang], ...options };
  const parts = cached(numberFormats, lang + JSON.stringify(full), () => new Intl.NumberFormat(lang, full)).formatToParts(
    value,
  );
  const digits = parts.filter((p) => p.type === "integer" || p.type === "fraction");
  // No digits at all is infinity, not zero: "-∞" keeps its sign.
  const zero = digits.length > 0 && digits.every((p) => !/[1-9]/.test(p.value));
  return parts
    .filter((p) => !(zero && p.type === "minusSign"))
    .map((p) => p.value)
    .join("");
}

/** Fill a message in `lang`: pick the plural form by `count`, then
 * replace each `{name}`. Numbers are written the language's way
 * (`1 234,5` in Russian); pass a string for a number that must stay as
 * it is (a year, an id). A placeholder without a value is left visible,
 * and a plural message without a numeric `count` says its `other` form. */
export function formatMessage(
  message: string | PluralMessage,
  lang: Language,
  params: Record<string, string | number> = {},
): string {
  return pickForm(message, lang, params.count).replace(/\{(\w+)\}/g, (whole, name: string) => {
    const v = params[name];
    if (v === undefined) return whole;
    return typeof v === "number" ? formatIn(lang, v) : v;
  });
}

/** The text of a message: the plural form `count` picks, else `other`. */
function pickForm(message: string | PluralMessage, lang: Language, count: unknown): string {
  if (typeof message === "string") return message;
  if (typeof count !== "number") return message.other;
  return message[cached(pluralRules, lang, () => new Intl.PluralRules(lang)).select(count)] ?? message.other;
}

/** A message with some of its text rendered apart (bold, a link): the
 * form is picked and the message cut at its `{name}`s BEFORE anything is
 * filled, so a translation puts the part where its word order wants it.
 * A string or number value is filled as `formatMessage` fills it; any
 * other value (an element) takes its placeholder's place as it is. Pure —
 * `useRichT` keys the parts for React. */
export function richParts(
  message: string | PluralMessage,
  lang: Language,
  params: Record<string, ReactNode> = {},
): ReactNode[] {
  const parts: ReactNode[] = [];
  for (const piece of pickForm(message, lang, params.count).split(/(\{\w+\})/)) {
    const name = /^\{(\w+)\}$/.exec(piece)?.[1];
    const v = name === undefined ? piece : params[name];
    const part = v === undefined ? piece : typeof v === "number" ? formatIn(lang, v) : v;
    const last = parts.length - 1;
    // Text next to text is one string: what a test or a screen reader reads whole.
    if (typeof part === "string" && typeof parts[last] === "string") parts[last] += part;
    else if (part !== "") parts.push(part);
  }
  return parts;
}

// ── Numbers and dates ───────────────────────────────────────────────────
// Written the interface language's way (#232), not the OS region's: a
// decimal comma and its grouping in Russian and Italian, month and weekday
// names in the language. `lang` defaults to the language in use; tests
// and callers that hold one pass it.

/** A number as the language writes it: `digits` fraction digits exactly
 * (`1,50` in Russian for 2), or up to three when left out — what a chart
 * axis wants — grouped by the language's policy above. Ties round away
 * from zero on the decimal value as written (ICU's half-expand): 1.005 to
 * two digits is "1.01", where `toFixed` reads the binary 1.00499… and
 * writes "1.00". */
export function formatNumber(value: number, digits?: number, lang: Language = currentLanguage()): string {
  const options = digits === undefined ? {} : { minimumFractionDigits: digits, maximumFractionDigits: digits };
  return formatIn(lang, value, options);
}

/** A percentage from a value in percent (52.3 → "52.3%", Russian
 * "52,3 %"), `digits` fraction digits exactly. */
export function formatPercent(value: number, digits = 1, lang: Language = currentLanguage()): string {
  return formatIn(lang, value / 100, {
    style: "percent",
    minimumFractionDigits: digits,
    maximumFractionDigits: digits,
  });
}

/** A date (or its time) in the language: `options` say which parts and
 * how long, as `toLocaleDateString` takes them. */
export function formatDateTime(
  date: Date,
  options: Intl.DateTimeFormatOptions,
  lang: Language = currentLanguage(),
): string {
  return cached(dateFormats, lang + JSON.stringify(options), () => new Intl.DateTimeFormat(lang, options)).format(date);
}

/** The first letter upper-cased the language's way: Russian and Italian
 * write month and weekday names in lower case, a header or a column of
 * them starts with a capital. */
export function capitalize(text: string, lang: Language = currentLanguage()): string {
  return text.charAt(0).toLocaleUpperCase(lang) + text.slice(1);
}

/** A key's message in `lang`. Pure — what `t` and `useT` both call. */
export function translate<K extends MessageKey>(lang: Language, key: K, ...args: MessageArgs<K>): string {
  return formatMessage(DICTIONARIES[lang][key], lang, args[0]);
}

// ── Store ───────────────────────────────────────────────────────────────

interface LanguageState {
  pref: LanguagePref;
  /** The language in use: `pref` resolved against the system's. */
  language: Language;
  setPref: (pref: LanguagePref) => void;
}

export const useLanguageStore = create<LanguageState>((set) => {
  const pref = readStoredLanguage();
  return {
    pref,
    language: resolveLanguage(pref, systemLanguages()),
    setPref: (next) => {
      if (typeof localStorage !== "undefined") localStorage.setItem(STORAGE_KEY, next);
      const language = resolveLanguage(next, systemLanguages());
      applyLanguage(language);
      set({ pref: next, language });
    },
  };
});

/** The language in use, for code outside React (formatters, chart configs). */
export function currentLanguage(): Language {
  return useLanguageStore.getState().language;
}

/** A key's message in the language in use, for code outside React. A
 * component calls `useT()` instead: this one does not re-render it when the
 * language changes. */
export function t<K extends MessageKey>(key: K, ...args: MessageArgs<K>): string {
  return translate(currentLanguage(), key, ...args);
}

/** A `t`: the module's, a component's `useT()`, or one a test holds. */
export type Translator = <K extends MessageKey>(key: K, ...args: MessageArgs<K>) => string;

/** Subscribe the calling component to the language and return its `t`. */
export function useT(): Translator {
  const language = useLanguageStore((s) => s.language);
  return useCallback(
    <K extends MessageKey>(key: K, ...args: MessageArgs<K>) => translate(language, key, ...args),
    [language],
  );
}

/** Subscribe the calling component to the language and return a `t` for a
 * message with elements inside: `rt("app.watch.found", { count, files:
 * <strong>5</strong> })`. One message per phrase, never one cut in two
 * around the bold part — the translation keeps its own word order. */
export function useRichT(): <K extends MessageKey>(key: K, params: RichArgsOf<(typeof en)[K]>) => ReactNode {
  const language = useLanguageStore((s) => s.language);
  return useCallback(
    <K extends MessageKey>(key: K, params: RichArgsOf<(typeof en)[K]>) =>
      richParts(DICTIONARIES[language][key], language, params as Record<string, ReactNode>).map((part, i) =>
        createElement(Fragment, { key: i }, part),
      ),
    [language],
  );
}

function applyLanguage(language: Language): void {
  if (typeof document !== "undefined") document.documentElement.lang = language;
}

/**
 * Mark <html lang> before first paint and keep "system" in step with the
 * OS: a change of the preferred languages while the app runs re-resolves
 * it. Call from main.tsx; returns the listener's removal.
 */
export function initLanguage(): () => void {
  applyLanguage(currentLanguage());
  const onChange = () => {
    const { pref, language } = useLanguageStore.getState();
    if (pref !== "system") return;
    const next = resolveLanguage("system", systemLanguages());
    if (next === language) return;
    applyLanguage(next);
    useLanguageStore.setState({ language: next });
  };
  window.addEventListener("languagechange", onChange);
  return () => window.removeEventListener("languagechange", onChange);
}
