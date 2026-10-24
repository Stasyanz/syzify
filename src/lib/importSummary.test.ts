import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { formatAttachSummary, formatImportSummary } from "./importSummary";
import type { AttachPhotosResult, ImportResult } from "./types";
import { translate, useLanguageStore, type Translator } from "./i18n";

const base: ImportResult = {
  imported: 0,
  skipped: 0,
  failed: [],
  monitoring_files: 0,
  monitoring_days: 0,
  monitoring_range: null,
  monitoring_night: false,
};

describe("formatImportSummary", () => {
  it("dates the monitoring range in the interface language (#232)", () => {
    useLanguageStore.getState().setPref("ru");
    try {
      const r = { ...base, monitoring_files: 30, monitoring_days: 12, monitoring_range: ["2026-08-20", "2026-09-05"] as [string, string], monitoring_night: true };
      expect(formatImportSummary(r).text).toContain("(20 авг. – 5 сент.,");
    } finally {
      useLanguageStore.getState().setPref("system");
    }
  });

  it("keeps the activity wording", () => {
    expect(formatImportSummary({ ...base, imported: 3 })).toEqual({
      level: "success",
      text: "Imported 3 activities",
    });
    expect(formatImportSummary({ ...base, imported: 1, skipped: 2 }).text).toBe(
      "Imported 1 activity, skipped 2 (duplicates)",
    );
  });

  it("reports monitoring by the days it covered", () => {
    const r = { ...base, monitoring_files: 30, monitoring_days: 12, monitoring_range: ["2026-08-20", "2026-09-05"] as [string, string], monitoring_night: true };
    expect(formatImportSummary(r).text).toBe("Imported monitoring for 12 days (Aug 20 – Sep 5, night of Sep 5 included)");
    // A file closed at midnight: the evening before only, the card cannot change.
    const one = { ...base, monitoring_files: 2, monitoring_days: 1, monitoring_range: ["2026-09-05", "2026-09-05"] as [string, string] };
    expect(formatImportSummary(one).text).toBe(
      "Imported monitoring for 1 day (Sep 5, no night for Sep 5 yet — sync the watch after waking)",
    );
    // A watch folder: the watch is already connected, no advice.
    expect(formatImportSummary(one, { auto: true }).text).toBe(
      "Auto-imported monitoring for 1 day (Sep 5, no night for Sep 5 yet)",
    );
  });

  it("phrases monitoring duplicates as files already imported", () => {
    const r = {
      ...base,
      monitoring_files: 9,
      monitoring_days: 12,
      monitoring_range: ["2026-08-20", "2026-09-05"] as [string, string],
      monitoring_night: true,
      skipped: 3,
    };
    expect(formatImportSummary(r).text).toBe(
      "Imported monitoring for 12 days (Aug 20 – Sep 5, night of Sep 5 included), skipped 3 files (already imported)",
    );
  });

  it("reports a mixed drop side by side", () => {
    const r = {
      ...base,
      imported: 2,
      monitoring_files: 5,
      monitoring_days: 5,
      monitoring_range: ["2026-09-01", "2026-09-05"] as [string, string],
    };
    expect(formatImportSummary(r).text).toBe(
      "Imported 2 activities and monitoring for 5 days (Sep 1 – Sep 5, no night for Sep 5 yet — sync the watch after waking)",
    );
  });

  it("says nothing new when everything was already imported", () => {
    expect(formatImportSummary({ ...base, skipped: 185 })).toEqual({
      level: "info",
      text: "Nothing new: 185 files already imported",
    });
    expect(formatImportSummary({ ...base, skipped: 1 }).text).toBe(
      "Nothing new: 1 file already imported",
    );
  });

  it("warns on failures and counts them", () => {
    const r = { ...base, imported: 1, failed: [{ path: "a", reason: "x" }, { path: "b", reason: "y" }] };
    expect(formatImportSummary(r)).toEqual({
      level: "warning",
      text: "Imported 1 activity, 2 files failed",
    });
    expect(formatImportSummary({ ...base, failed: [{ path: "a", reason: "x" }] }).text).toBe(
      "Nothing imported, 1 file failed",
    );
    expect(
      formatImportSummary({ ...base, skipped: 2, failed: [{ path: "a", reason: "x" }] }).text,
    ).toBe("Nothing imported, skipped 2 (duplicates), 1 file failed");
  });

  it("uses the watch-folder verb", () => {
    expect(formatImportSummary({ ...base, imported: 1 }, { auto: true }).text).toBe(
      "Auto-imported 1 activity",
    );
  });

  it("falls back to file counts when the recompute reported no days", () => {
    expect(formatImportSummary({ ...base, monitoring_files: 2 }).text).toBe(
      "Imported 2 monitoring files",
    );
  });

  it("handles an empty result", () => {
    expect(formatImportSummary(base)).toEqual({ level: "info", text: "Nothing imported" });
  });

  describe("in Russian (#235)", () => {
    beforeEach(() => useLanguageStore.getState().setPref("ru"));
    afterEach(() => useLanguageStore.getState().setPref("system"));

    it("counts activities, days and files in the forms each number takes", () => {
      expect(formatImportSummary({ ...base, imported: 1 }).text).toBe("Импортировано: 1 тренировка");
      expect(formatImportSummary({ ...base, imported: 3 }).text).toBe("Импортировано: 3 тренировки");
      expect(formatImportSummary({ ...base, imported: 5, skipped: 2 }).text).toBe(
        "Импортировано: 5 тренировок, пропущено 2 (дубликаты)",
      );
      expect(formatImportSummary({ ...base, imported: 21 }, { auto: true }).text).toBe("Автоимпорт: 21 тренировка");
      expect(formatImportSummary({ ...base, skipped: 1 }).text).toBe("Ничего нового: 1 файл уже импортирован");
      expect(formatImportSummary({ ...base, skipped: 185 }).text).toBe("Ничего нового: 185 файлов уже импортированы");
      expect(formatImportSummary({ ...base, monitoring_files: 2 }).text).toBe("Импортировано: 2 файла мониторинга");
      expect(formatImportSummary({ ...base, skipped: 1, failed: [{ path: "a", reason: "x" }] }).text).toBe(
        "Ничего не импортировано, пропущен 1 (дубликат), 1 файл с ошибкой",
      );
      expect(formatImportSummary(base).text).toBe("Ничего не импортировано");
    });

    it("says the monitoring days, the night and the advice in one language", () => {
      const r = {
        ...base,
        imported: 2,
        monitoring_files: 5,
        monitoring_days: 5,
        monitoring_range: ["2026-09-01", "2026-09-05"] as [string, string],
        skipped: 3,
      };
      expect(formatImportSummary(r).text).toBe(
        "Импортировано: 2 тренировки и мониторинг за 5 дней (1 сент. – 5 сент., ночи на 5 сент. пока нет — синхронизируйте часы после пробуждения), пропущено 3 (дубликаты)",
      );
      const night = { ...r, imported: 0, monitoring_days: 1, monitoring_range: ["2026-09-05", "2026-09-05"] as [string, string], monitoring_night: true };
      expect(formatImportSummary(night, { auto: true }).text).toBe(
        "Автоимпорт: мониторинг за 1 день (5 сент., ночь на 5 сент. учтена), пропущено 3 файла (уже импортированы)",
      );
      const noNight = { ...night, monitoring_night: false, skipped: 0 };
      expect(formatImportSummary(noNight, { auto: true }).text).toBe(
        "Автоимпорт: мониторинг за 1 день (5 сент., ночи на 5 сент. пока нет)",
      );
    });

    it("takes the caller's language over the one in use", () => {
      const english: Translator = (key, ...args) => translate("en", key, ...args);
      expect(formatImportSummary({ ...base, imported: 2 }, {}, english).text).toBe("Imported 2 activities");
    });
  });
});

describe("formatAttachSummary", () => {
  const photo = { id: "p" } as AttachPhotosResult["attached"][number];

  it("is nothing when nothing happened", () => {
    expect(formatAttachSummary({ attached: [], skipped: [], failed: [] })).toBeNull();
  });

  it("counts what was added, skipped and failed, the first part opening the sentence", () => {
    expect(formatAttachSummary({ attached: [photo, photo], skipped: ["a"], failed: [] })).toEqual({
      level: "success",
      text: "Added 2 photos, skipped 1 duplicate",
    });
    expect(formatAttachSummary({ attached: [], skipped: ["a", "b"], failed: [{ path: "c", reason: "x" }] })).toEqual({
      level: "warning",
      text: "Skipped 2 duplicates, 1 failed",
    });
  });

  it("speaks the interface language (#235)", () => {
    useLanguageStore.getState().setPref("ru");
    try {
      expect(formatAttachSummary({ attached: [photo], skipped: ["a", "b", "c", "d", "e"], failed: [] })?.text).toBe(
        "Добавлено 1 фото, пропущено 5 дубликатов",
      );
    } finally {
      useLanguageStore.getState().setPref("system");
    }
  });
});
