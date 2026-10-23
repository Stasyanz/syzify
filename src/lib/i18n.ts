import { useCallback } from "react";
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

// One formatter per language, not one per message: lists format hundreds.
const pluralRules = new Map<Language, Intl.PluralRules>();
const numberFormats = new Map<Language, Intl.NumberFormat>();

function cached<T>(cache: Map<Language, T>, lang: Language, make: () => T): T {
  let v = cache.get(lang);
  if (!v) cache.set(lang, (v = make()));
  return v;
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
  let text: string;
  if (typeof message === "string") {
    text = message;
  } else if (typeof params.count === "number") {
    const count = params.count;
    text = message[cached(pluralRules, lang, () => new Intl.PluralRules(lang)).select(count)] ?? message.other;
  } else {
    text = message.other;
  }
  return text.replace(/\{(\w+)\}/g, (whole, name: string) => {
    const v = params[name];
    if (v === undefined) return whole;
    return typeof v === "number" ? cached(numberFormats, lang, () => new Intl.NumberFormat(lang)).format(v) : v;
  });
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

/** Subscribe the calling component to the language and return its `t`. */
export function useT(): <K extends MessageKey>(key: K, ...args: MessageArgs<K>) => string {
  const language = useLanguageStore((s) => s.language);
  return useCallback(
    <K extends MessageKey>(key: K, ...args: MessageArgs<K>) => translate(language, key, ...args),
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
