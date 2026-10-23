// @vitest-environment happy-dom
import { afterEach, describe, expect, it, vi } from "vitest";
import { act, renderHook } from "@testing-library/react";
import {
  currentLanguage,
  formatMessage,
  initLanguage,
  readStoredLanguage,
  resolveLanguage,
  t,
  translate,
  useLanguageStore,
  useT,
  type ArgsOf,
  type Language,
} from "./i18n";
import { en } from "../locales/en";
import { ru } from "../locales/ru";
import { it as itMessages } from "../locales/it";
import type { PluralMessage } from "../locales/types";

const DICTIONARIES: Record<Language, Record<string, string | PluralMessage>> = { en, ru, it: itMessages };

function useSystemLanguages(langs: string[]) {
  vi.spyOn(navigator, "languages", "get").mockReturnValue(langs);
}

afterEach(() => {
  vi.restoreAllMocks();
  act(() => useLanguageStore.getState().setPref("system"));
  localStorage.clear();
});

describe("resolveLanguage", () => {
  it("takes the first system language Syzify has, by its primary subtag", () => {
    expect(resolveLanguage("system", ["ru-RU", "en-US"])).toBe("ru");
    expect(resolveLanguage("system", ["de-DE", "it-CH", "ru"])).toBe("it");
    expect(resolveLanguage("system", ["RU"])).toBe("ru");
  });

  it("falls back to English when the system has none of them", () => {
    expect(resolveLanguage("system", ["de-DE", "fr"])).toBe("en");
    expect(resolveLanguage("system", [])).toBe("en");
  });

  it("keeps a language the user picked whatever the system says", () => {
    expect(resolveLanguage("it", ["ru-RU"])).toBe("it");
  });
});

describe("readStoredLanguage", () => {
  it("reads a stored choice and treats anything else as system", () => {
    expect(readStoredLanguage()).toBe("system");
    localStorage.setItem("syzify-language", "ru");
    expect(readStoredLanguage()).toBe("ru");
    localStorage.setItem("syzify-language", "de");
    expect(readStoredLanguage()).toBe("system");
  });
});

describe("formatMessage", () => {
  it("fills placeholders and leaves one without a value visible", () => {
    expect(formatMessage("{name} — {n}", "en", { name: "Edge", n: "x" })).toBe("Edge — x");
    expect(formatMessage("{name} — {n}", "en", { name: "Edge" })).toBe("Edge — {n}");
  });

  it("writes a number the language's way and a string as it is", () => {
    expect(formatMessage("{v}", "en", { v: 1234.5 })).toBe("1,234.5");
    expect(formatMessage("{v}", "ru", { v: 1234.5 })).toBe(new Intl.NumberFormat("ru").format(1234.5));
    expect(formatMessage("{v}", "ru", { v: 1234.5 })).toMatch(/^1\s234,5$/u);
    expect(formatMessage("{v}", "it", { v: 2.5 })).toBe("2,5");
    expect(formatMessage("{v}", "ru", { v: "2026" })).toBe("2026");
  });

  it("picks the Russian form for one, few and many", () => {
    const m: PluralMessage = { one: "{count} файл", few: "{count} файла", many: "{count} файлов", other: "{count} файла" };
    expect([1, 3, 5, 11, 21, 22, 25].map((count) => formatMessage(m, "ru", { count }))).toEqual([
      "1 файл", "3 файла", "5 файлов", "11 файлов", "21 файл", "22 файла", "25 файлов",
    ]);
    expect(formatMessage(m, "ru", { count: 1.5 })).toBe("1,5 файла");
  });

  it("falls back to other for a form a message does not have", () => {
    const m: PluralMessage = { one: "one file", other: "{count} files" };
    expect(formatMessage(m, "en", { count: 1 })).toBe("one file");
    expect(formatMessage(m, "en", { count: 0 })).toBe("0 files");
    expect(formatMessage(m, "ru", { count: 5 })).toBe("5 files");
  });

  it("says the other form when a plural message gets no numeric count", () => {
    const m: PluralMessage = { one: "{count} file", other: "{count} files" };
    expect(formatMessage(m, "en", { count: "1" })).toBe("1 files");
    expect(formatMessage(m, "en")).toBe("{count} files");
  });
});

describe("the dictionaries", () => {
  const placeholders = (m: string | PluralMessage) =>
    [...new Set(Object.values(typeof m === "string" ? { m } : m).flatMap((s) => [...s.matchAll(/\{(\w+)\}/g)].map((x) => x[1])))].sort();

  it.each(["ru", "it"] as const)("%s has every English key and no other", (lang) => {
    expect(Object.keys(DICTIONARIES[lang]).sort()).toEqual(Object.keys(en).sort());
  });

  it.each(["ru", "it"] as const)("%s keeps each message's placeholders", (lang) => {
    const drift = Object.entries(en).filter(([k, m]) => {
      return placeholders(m).join() !== placeholders(DICTIONARIES[lang][k]).join();
    });
    expect(drift.map(([k]) => k)).toEqual([]);
  });

  it("name every placeholder in word characters, the only ones filled", () => {
    const odd = Object.values(DICTIONARIES)
      .flatMap((d) => Object.values(d))
      .flatMap((m) => (typeof m === "string" ? [m] : Object.values(m)))
      .flatMap((s) => [...s.matchAll(/\{([^}]*)\}/g)].map((x) => x[1]))
      .filter((name) => !/^\w+$/.test(name));
    expect(odd).toEqual([]);
  });

  // The privacy disclosure (PRD §16) names the endpoint in every language.
  it.each(["en", "ru", "it"] as const)("%s names the geocoding service it discloses", (lang) => {
    expect(DICTIONARIES[lang]["settings.general.geocodingHint"]).toContain("nominatim.openstreetmap.org");
  });

  it.each(["ru", "it"] as const)("%s translates rather than copies", (lang) => {
    // A message identical to English is usually a forgotten one; the few
    // that are the same on purpose would be listed here.
    const copied = Object.entries(en).filter(([k, m]) => typeof m === "string" && DICTIONARIES[lang][k] === m);
    expect(copied.map(([k]) => k)).toEqual([]);
  });
});

describe("translate and t", () => {
  it("say a key in the given or the current language", () => {
    expect(translate("ru", "settings.general.theme")).toBe("Тема");
    expect(translate("it", "settings.general.tileCacheHint", { size: "2 KB" })).toBe("Tile della mappa su disco: 2 KB");
    act(() => useLanguageStore.getState().setPref("it"));
    expect(t("settings.general.theme")).toBe("Tema");
  });

  it("are checked by tsc for keys and parameters", () => {
    // Never called: these lines only have to fail to compile.
    const typeErrors = () => {
      // @ts-expect-error — no such key
      t("settings.general.nope");
      // @ts-expect-error — `size` is required
      t("settings.general.tileCacheHint");
      // @ts-expect-error — `size`, not `bytes`
      t("settings.general.tileCacheHint", { bytes: "1" });
      // @ts-expect-error — a plain message takes no parameters
      t("settings.general.theme", { x: 1 });
      // @ts-expect-error — one of the two keys needs `size`
      t(Math.random() > 0.5 ? "settings.general.theme" : "settings.general.tileCacheHint");
    };
    expect(typeof typeErrors).toBe("function");
  });

  it("want a numeric count and every other placeholder for a plural message", () => {
    type Files = { readonly one: "{count} file in {dir}"; readonly other: "{count} files in {dir}" };
    const ok: ArgsOf<Files> = [{ count: 3, dir: "inbox" }];
    // @ts-expect-error — a string count would pick the wrong form
    const stringCount: ArgsOf<Files> = [{ count: "3", dir: "inbox" }];
    // @ts-expect-error — `dir` is missing
    const noDir: ArgsOf<Files> = [{ count: 3 }];
    const none: ArgsOf<"Plain"> = [];
    const named: ArgsOf<"Hi {name}"> = [{ name: "Ann" }];
    expect([ok, stringCount, noDir, none, named]).toHaveLength(5);
  });
});

describe("the language store", () => {
  it("stores a choice, marks <html lang> and says it", () => {
    act(() => useLanguageStore.getState().setPref("ru"));
    expect(localStorage.getItem("syzify-language")).toBe("ru");
    expect(document.documentElement.lang).toBe("ru");
    expect(currentLanguage()).toBe("ru");
  });

  it("resolves System against the system's languages", () => {
    useSystemLanguages(["it-IT", "en"]);
    act(() => useLanguageStore.getState().setPref("system"));
    expect(useLanguageStore.getState()).toMatchObject({ pref: "system", language: "it" });
  });

  it("re-renders a component through useT when the language changes", () => {
    const { result } = renderHook(() => useT());
    expect(result.current("settings.general.language")).toBe("Language");
    act(() => useLanguageStore.getState().setPref("ru"));
    expect(result.current("settings.general.language")).toBe("Язык");
  });

  it("follows a change of the system's languages while on System, and only then", () => {
    const stop = initLanguage();
    try {
      useSystemLanguages(["ru-RU"]);
      act(() => {
        window.dispatchEvent(new Event("languagechange"));
      });
      expect(currentLanguage()).toBe("ru");
      expect(document.documentElement.lang).toBe("ru");

      // The same language again: no new state, no re-render.
      const before = useLanguageStore.getState();
      act(() => {
        window.dispatchEvent(new Event("languagechange"));
      });
      expect(useLanguageStore.getState()).toBe(before);

      act(() => useLanguageStore.getState().setPref("it"));
      useSystemLanguages(["en-US"]);
      act(() => {
        window.dispatchEvent(new Event("languagechange"));
      });
      expect(currentLanguage()).toBe("it");
    } finally {
      stop();
    }
    // Removed: a later change reaches no one.
    act(() => useLanguageStore.getState().setPref("system"));
    useSystemLanguages(["ru-RU"]);
    act(() => {
      window.dispatchEvent(new Event("languagechange"));
    });
    expect(currentLanguage()).toBe("en");
  });

  it("marks <html lang> at start with the language already chosen", () => {
    act(() => useLanguageStore.setState({ language: "it" }));
    document.documentElement.lang = "xx";
    initLanguage()();
    expect(document.documentElement.lang).toBe("it");
  });

  it("starts in the stored language, or the system's on System", async () => {
    localStorage.setItem("syzify-language", "ru");
    vi.resetModules();
    const stored = await import("./i18n");
    expect(stored.useLanguageStore.getState()).toMatchObject({ pref: "ru", language: "ru" });

    localStorage.setItem("syzify-language", "system");
    useSystemLanguages(["it-IT"]);
    vi.resetModules();
    const system = await import("./i18n");
    expect(system.useLanguageStore.getState()).toMatchObject({ pref: "system", language: "it" });
  });

  it("reads the single navigator.language when the list is empty or missing", () => {
    vi.spyOn(navigator, "language", "get").mockReturnValue("ru-RU");
    useSystemLanguages([]);
    act(() => useLanguageStore.getState().setPref("system"));
    expect(currentLanguage()).toBe("ru");
    vi.spyOn(navigator, "languages", "get").mockReturnValue(undefined as unknown as string[]);
    vi.spyOn(navigator, "language", "get").mockReturnValue("it-IT");
    act(() => useLanguageStore.getState().setPref("system"));
    expect(currentLanguage()).toBe("it");
  });

  it("works without the browser's globals, as in a node test", () => {
    vi.stubGlobal("localStorage", undefined);
    vi.stubGlobal("navigator", undefined);
    vi.stubGlobal("document", undefined);
    try {
      expect(readStoredLanguage()).toBe("system");
      useLanguageStore.getState().setPref("ru");
      expect(currentLanguage()).toBe("ru");
      useLanguageStore.getState().setPref("system");
      expect(currentLanguage()).toBe("en");
    } finally {
      vi.unstubAllGlobals();
    }
  });
});
