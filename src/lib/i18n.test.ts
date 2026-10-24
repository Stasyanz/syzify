// @vitest-environment happy-dom
import { afterEach, describe, expect, it, vi } from "vitest";
import { createElement } from "react";
import { act, render, renderHook } from "@testing-library/react";
import {
  capitalize,
  currentLanguage,
  formatDateTime,
  formatMessage,
  formatNumber,
  formatPercent,
  initLanguage,
  readStoredLanguage,
  resolveLanguage,
  richParts,
  t,
  tKnown,
  translate,
  useLanguageStore,
  useRichT,
  useT,
  type ArgsOf,
  type Language,
} from "./i18n";
import { en } from "../locales/en";
import { ru } from "../locales/ru";
import { it as itMessages } from "../locales/it";
import type { PluralMessage } from "../locales/types";
import { useBusyStore } from "../stores/busyStore";

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
    // The grouping policy of formatNumber, for parameters too (#232).
    expect(formatMessage("{v}", "en", { v: 1234.5 })).toBe("1,234.5");
    expect(formatMessage("{v}", "ru", { v: 1234.5 })).toBe("1234,5");
    expect(formatMessage("{v}", "ru", { v: 12345.5 })).toBe("12\xa0345,5");
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

describe("richParts and useRichT", () => {
  const bold = createElement("strong", null, "5");

  it("cuts the message at its placeholders before filling, an element taking its place as it is", () => {
    expect(richParts("Found {files} new {what}", "en", { files: bold, what: "files" })).toEqual([
      "Found ",
      bold,
      " new files",
    ]);
    // At either end: no empty text beside the element.
    expect(richParts("{files}", "en", { files: bold })).toEqual([bold]);
    expect(richParts("{files} found", "en", { files: bold })).toEqual([bold, " found"]);
  });

  it("picks the plural form first, so the element lands where the translation puts it", () => {
    const m: PluralMessage = {
      one: "Found {files} new file",
      few: "В папках найдено {files} новых файла",
      many: "В папках найдено {files} новых файлов",
      other: "Found {files} files",
    };
    expect(richParts(m, "ru", { count: 5, files: bold })).toEqual(["В папках найдено ", bold, " новых файлов"]);
    expect(richParts(m, "ru", { count: 3, files: bold })).toEqual(["В папках найдено ", bold, " новых файла"]);
    // Without a numeric count, the other form.
    expect(richParts(m, "ru", { files: bold })).toEqual(["Found ", bold, " files"]);
  });

  it("fills a number the language's way and leaves a missing placeholder visible", () => {
    expect(richParts("{v} km, {gone}", "ru", { v: 1234.5 })).toEqual(["1234,5 km, {gone}"]);
    expect(richParts("plain", "en")).toEqual(["plain"]);
  });

  it("renders one message in the language in use, and again when it changes", () => {
    const { result } = renderHook(() => useRichT());
    const view = () => {
      const { container, unmount } = render(
        createElement("p", null, result.current("app.watch.found", { count: 5, files: createElement("strong", null, "5") })),
      );
      const text = [container.textContent, container.querySelector("strong")!.textContent];
      unmount();
      return text;
    };
    expect(view()).toEqual(["Found 5 new workout files in watch folders", "5"]);
    act(() => useLanguageStore.getState().setPref("ru"));
    expect(view()).toEqual(["В папках наблюдения найдено 5 новых файлов тренировок", "5"]);
  });
});

describe("numbers and dates in the language (#232)", () => {
  it("write a number with exact or up to three fraction digits", () => {
    expect(formatNumber(1.5, 2, "en")).toBe("1.50");
    expect(formatNumber(1.5, 2, "ru")).toBe("1,50");
    expect(formatNumber(1.5, 2, "it")).toBe("1,50");
    expect(formatNumber(2.1234, undefined, "en")).toBe("2.123");
    expect(formatNumber(2.1234, undefined, "ru")).toBe("2,123");
    expect(formatNumber(42.4, 0, "en")).toBe("42");
  });

  it("group thousands by one policy per language", () => {
    // English groups every thousand; Italian (CLDR) and Russian (its
    // typography) leave four digits whole.
    expect(formatNumber(1214, 0, "en")).toBe("1,214");
    expect(formatNumber(1214, 0, "ru")).toBe("1214");
    expect(formatNumber(1214, 0, "it")).toBe("1214");
    expect(formatNumber(12345.6, 1, "en")).toBe("12,345.6");
    expect(formatNumber(12345.6, 1, "ru")).toBe("12\xa0345,6");
    expect(formatNumber(12345.6, 1, "it")).toBe("12.345,6");
  });

  it("round a tie away from zero on the decimal value, unlike toFixed", () => {
    // 1.005 is 1.00499… in binary: toFixed(2) says "1.00".
    expect((1.005).toFixed(2)).toBe("1.00");
    expect(formatNumber(1.005, 2, "en")).toBe("1.01");
    expect(formatNumber(-2.5, 0, "en")).toBe("-3");
  });

  it("write a value that rounds to zero without a sign", () => {
    expect(formatNumber(-0.4, 0, "en")).toBe("0");
    expect(formatNumber(-0, undefined, "ru")).toBe("0");
    expect(formatNumber(-0.04, 1, "ru")).toBe("0,0");
    expect(formatNumber(-0.6, 0, "en")).toBe("-1");
    expect(formatPercent(-0.04, 1, "en")).toBe("0.0%");
    expect(formatMessage("{n}", "it", { n: -0.0001 })).toBe("0");
    expect(formatMessage("{n}", "it", { n: -1.5 })).toBe("-1,5");
    expect(formatNumber(-Infinity, 0, "en")).toBe("-∞");
  });

  it("ask no formatter for an option an engine before NumberFormat v3 refuses", () => {
    // An older WebKit throws a RangeError on `signDisplay: "negative"` —
    // a blank window. Stand in for it, with options no other test caches.
    const Original = Intl.NumberFormat;
    const seen: (Intl.NumberFormatOptions | undefined)[] = [];
    Intl.NumberFormat = function (locales?: string | string[], options?: Intl.NumberFormatOptions) {
      seen.push(options);
      if (options?.signDisplay !== undefined) throw new RangeError("signDisplay");
      return new Original(locales, options);
    } as unknown as typeof Intl.NumberFormat;
    try {
      expect(formatNumber(-0.000004, 5, "it")).toBe("0,00000");
      expect(formatNumber(-1.23456, 5, "ru")).toBe("-1,23456");
      expect(formatPercent(-0.0004, 3, "en")).toBe("0.000%");
    } finally {
      Intl.NumberFormat = Original;
    }
    expect(seen).toHaveLength(3);
  });

  it("use the language in use when none is given", () => {
    expect(formatNumber(1.5, 1)).toBe("1.5");
    act(() => useLanguageStore.getState().setPref("ru"));
    expect(formatNumber(1.5, 1)).toBe("1,5");
    expect(formatPercent(52.34)).toBe("52,3\xa0%");
    expect(formatDateTime(new Date(2026, 8, 5), { month: "short", day: "numeric" })).toBe("5 сент.");
    expect(capitalize("пн")).toBe("Пн");
  });

  it("write a percentage", () => {
    expect(formatPercent(52.34, 1, "en")).toBe("52.3%");
    expect(formatPercent(52.34, 1, "it")).toBe("52,3%");
    expect(formatPercent(-2, 1, "en")).toBe("-2.0%");
    expect(formatPercent(45.4, 0, "en")).toBe("45%");
  });

  it("write a date with the language's names and order", () => {
    const d = new Date(2026, 7, 20, 18, 5);
    const short = { month: "short", day: "numeric" } as const;
    expect(formatDateTime(d, short, "en")).toBe("Aug 20");
    expect(formatDateTime(d, short, "ru")).toBe("20 авг.");
    expect(formatDateTime(d, short, "it")).toBe("20 ago");
    expect(formatDateTime(d, { hour: "numeric", minute: "2-digit" }, "ru")).toBe("18:05");
    // The same options twice reuse one formatter and say the same.
    expect(formatDateTime(d, short, "en")).toBe("Aug 20");
  });

  it("capitalize a name the language's way", () => {
    expect(capitalize("gennaio", "it")).toBe("Gennaio");
    expect(capitalize("", "en")).toBe("");
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

  /** The forms CLDR gives a language that a count written in digits may
   * pick and its messages can do without: Italian "many" is the "di" of
   * a spelled-out million ("un milione di file") — select(1000000) is
   * "many" — and with digits "1000000 file" is the `other` form, which
   * formatMessage falls back to. Russian has no such form: its "other" is
   * a fraction's (1,5 файла), and it, too, is required. */
  const FALLS_BACK_TO_OTHER: Record<Language, string[]> = { en: [], ru: [], it: ["many"] };

  it.each(["en", "ru", "it"] as const)("%s gives every plural message each form the language has", (lang) => {
    const forms = new Intl.PluralRules(lang).resolvedOptions().pluralCategories as string[];
    const needed = forms.filter((f) => !FALLS_BACK_TO_OTHER[lang].includes(f));
    const plurals = Object.entries(en).filter(([, m]) => typeof m !== "string");
    expect(plurals.length).toBeGreaterThan(0);
    const short = plurals
      .map(([k]) => [k, DICTIONARIES[lang][k]] as const)
      .filter(([, m]) => typeof m === "string" || needed.some((f) => !(f in m)))
      .map(([k]) => k);
    expect(short).toEqual([]);
  });

  it("falls back to the other form for Italian many, which digits never want", () => {
    // "many" since CLDR 38; an older ICU says "other", and the text is the same.
    expect(new Intl.PluralRules("it").select(1000000)).toMatch(/^(many|other)$/);
    expect(translate("it", "dashboard.sport.activities", { count: 1000000 })).toBe("attività");
    expect(translate("it", "library.filter.bulkDone", { count: 1000000, name: "Bici" })).toBe(
      "Bici assegnata a 1.000.000 attività",
    );
  });

  // The privacy disclosure (PRD §16) names the endpoint in every language.
  it.each(["en", "ru", "it"] as const)("%s names the geocoding service it discloses", (lang) => {
    expect(DICTIONARIES[lang]["settings.general.geocodingHint"]).toContain("nominatim.openstreetmap.org");
  });

  // A message identical to English is usually a forgotten one; these are
  // the same on purpose — Italian writes the international unit symbols,
  // and takes these words from English as its own.
  const SAME_ON_PURPOSE: Record<"ru" | "it", string[]> = {
    ru: [],
    it: [
      "units.km", "units.mi", "units.m", "units.ft", "units.yd", "units.kmh", "units.mph", "units.kg",
      "units.lb", "units.bpm", "units.w", "units.rpm", "units.kcal", "units.h", "units.min", "units.mm",
      "sport.triathlon", "sport.cardio", "sport.yoga", "sport.snowboard", "sport.golf",
      "unlock.password", "dashboard.records.sprint", "dashboard.volume.title",
      // "Multisport", Coggan's VO2 max as Italian coaching writes it, a segment's PR.
      "activity.header.multisport", "activity.zones.power.vo2max", "activity.segments.pr",
      // "Garage" and "Plugin" are Italian words too (the glossary of #237).
      "settings.tab.garage", "plugins.page.fallbackName", "settings.garage.title",
    ],
  };

  it.each(["ru", "it"] as const)("%s translates rather than copies", (lang) => {
    const copied = Object.entries(en).filter(([k, m]) => typeof m === "string" && DICTIONARIES[lang][k] === m);
    expect(copied.map(([k]) => k)).toEqual(SAME_ON_PURPOSE[lang]);
  });
});

describe("translate and t", () => {
  it("say a key in the given or the current language", () => {
    expect(translate("ru", "settings.general.theme")).toBe("Тема");
    expect(translate("it", "settings.general.tileCacheHint", { size: "2 KB" })).toBe("Tile della mappa su disco: 2 KB");
    act(() => useLanguageStore.getState().setPref("it"));
    expect(t("settings.general.theme")).toBe("Tema");
  });

  it("say a key known only at run time, or null when they cannot", () => {
    act(() => useLanguageStore.getState().setPref("ru"));
    expect(tKnown("error.vault.wrongPassword", {})).toBe("Неверный пароль");
    expect(tKnown("library.filter.bulkDone", { name: "Шоссе", count: 3 })).toBe("Снаряжение «Шоссе» назначено на 3 тренировки");
    expect(tKnown("error.nope", {})).toBeNull();
    expect(tKnown("toString", {})).toBeNull();
    expect(tKnown("library.filter.bulkDone", { count: 3 })).toBeNull();
    // A placeholder named like an Object member is still a parameter of its own.
    expect(tKnown("error.gear.notFound", Object.create({ id: "inherited" }) as Record<string, unknown>)).toBeNull();
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

  it("holds a change of the system's languages while a long operation runs, then follows it (#235)", () => {
    const stop = initLanguage();
    const busy = useBusyStore.getState();
    try {
      act(() => busy.start("backup"));
      act(() => busy.start("watchScan"));
      useSystemLanguages(["ru-RU"]);
      act(() => {
        window.dispatchEvent(new Event("languagechange"));
        window.dispatchEvent(new Event("languagechange"));
      });
      expect(currentLanguage()).toBe("en");
      // One of two ends: still waiting; progress is no reason to switch.
      act(() => busy.finish("backup"));
      act(() => busy.progress("watchScan", 50));
      expect(currentLanguage()).toBe("en");
      // The system changed again meanwhile: the latest one is taken.
      useSystemLanguages(["it-IT"]);
      act(() => busy.finish("watchScan"));
      expect(currentLanguage()).toBe("it");
      expect(document.documentElement.lang).toBe("it");
      // Waiting ended with it: a later operation does not switch anything.
      useSystemLanguages(["ru-RU"]);
      act(() => busy.start("backup"));
      act(() => busy.finish("backup"));
      expect(currentLanguage()).toBe("it");
    } finally {
      stop();
      useBusyStore.setState({ running: {}, lastError: {} });
    }
  });

  it("drops a held change when the listener is removed", () => {
    const stop = initLanguage();
    try {
      act(() => useBusyStore.getState().start("vaultMove"));
      useSystemLanguages(["ru-RU"]);
      act(() => {
        window.dispatchEvent(new Event("languagechange"));
      });
      stop();
      act(() => useBusyStore.getState().finish("vaultMove"));
      expect(currentLanguage()).toBe("en");
    } finally {
      useBusyStore.setState({ running: {}, lastError: {} });
    }
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
