import type { ImportResult } from "./types";

/** Toast level for an import outcome. */
export type ImportSummaryLevel = "success" | "warning" | "info";

/** "Sep 5" / "Aug 20 – Sep 5" for the monitoring day range. */
function dayLabel(iso: string): string {
  const [y, m, d] = iso.split("-").map(Number);
  return new Date(y, m - 1, d).toLocaleDateString("en-US", { month: "short", day: "numeric" });
}

function rangeLabel(range: [string, string]): string {
  const [from, to] = range;
  return from === to ? dayLabel(from) : `${dayLabel(from)} – ${dayLabel(to)}`;
}

function plural(n: number, one: string, many: string): string {
  return `${n} ${n === 1 ? one : many}`;
}

/**
 * One sentence for an import result — the same wording on every surface
 * (drop, the Import button, watch folders). Activities and Garmin
 * monitoring are reported side by side; "skipped" means the file's hash
 * was already in the vault (or, for a Monitor file, it carried no data).
 * `auto` prefixes the watch-folder wording.
 */
export function formatImportSummary(
  result: ImportResult,
  opts: { auto?: boolean } = {},
): { level: ImportSummaryLevel; text: string } {
  const verb = opts.auto ? "Auto-imported" : "Imported";
  const activities = result.imported;
  const monitoringDays = result.monitoring_files > 0 ? result.monitoring_days : 0;
  const range = result.monitoring_range;

  const what: string[] = [];
  if (activities > 0) what.push(plural(activities, "activity", "activities"));
  if (monitoringDays > 0 && range) {
    what.push(`monitoring for ${plural(monitoringDays, "day", "days")} (${rangeLabel(range)})`);
  } else if (result.monitoring_files > 0) {
    what.push(plural(result.monitoring_files, "monitoring file", "monitoring files"));
  }

  const parts: string[] = [];
  if (what.length > 0) {
    parts.push(`${verb} ${what.join(" and ")}`);
  } else if (result.skipped > 0 && result.failed.length === 0) {
    return {
      level: "info",
      text: `Nothing new: ${plural(result.skipped, "file", "files")} already imported`,
    };
  } else if (result.skipped > 0 || result.failed.length > 0) {
    parts.push("Nothing imported");
  }
  if (result.skipped > 0) {
    parts.push(
      what.length > 0 && activities === 0
        ? `skipped ${plural(result.skipped, "file", "files")} (already imported)`
        : `skipped ${result.skipped} (duplicates)`,
    );
  }
  if (result.failed.length > 0) parts.push(`${plural(result.failed.length, "file", "files")} failed`);

  if (parts.length === 0) return { level: "info", text: "Nothing imported" };
  return { level: result.failed.length > 0 ? "warning" : "success", text: parts.join(", ") };
}
