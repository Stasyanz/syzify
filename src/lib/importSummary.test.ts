import { describe, expect, it } from "vitest";
import { formatImportSummary } from "./importSummary";
import type { ImportResult } from "./types";

const base: ImportResult = {
  imported: 0,
  skipped: 0,
  failed: [],
  monitoring_files: 0,
  monitoring_days: 0,
  monitoring_range: null,
};

describe("formatImportSummary", () => {
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
    const r = { ...base, monitoring_files: 30, monitoring_days: 12, monitoring_range: ["2026-08-20", "2026-09-05"] as [string, string] };
    expect(formatImportSummary(r).text).toBe("Imported monitoring for 12 days (Aug 20 – Sep 5)");
    const one = { ...base, monitoring_files: 2, monitoring_days: 1, monitoring_range: ["2026-09-05", "2026-09-05"] as [string, string] };
    expect(formatImportSummary(one).text).toBe("Imported monitoring for 1 day (Sep 5)");
  });

  it("phrases monitoring duplicates as files already imported", () => {
    const r = {
      ...base,
      monitoring_files: 9,
      monitoring_days: 12,
      monitoring_range: ["2026-08-20", "2026-09-05"] as [string, string],
      skipped: 3,
    };
    expect(formatImportSummary(r).text).toBe(
      "Imported monitoring for 12 days (Aug 20 – Sep 5), skipped 3 files (already imported)",
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
      "Imported 2 activities and monitoring for 5 days (Sep 1 – Sep 5)",
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
});
