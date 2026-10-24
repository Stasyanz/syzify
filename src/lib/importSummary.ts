import type { AttachPhotosResult, ImportResult } from "./types";
import { dateOfDayKey } from "./calendar";
import { capitalize, formatDateTime, t, type Translator } from "./i18n";

/** Toast level for an import outcome. */
export type ImportSummaryLevel = "success" | "warning" | "info";

/** "Sep 5" / "Aug 20 – Sep 5" for the monitoring day range, in the
 * interface language ("5 сент."). */
function dayLabel(iso: string): string {
  return formatDateTime(dateOfDayKey(iso), { month: "short", day: "numeric" });
}

function rangeLabel(range: [string, string]): string {
  const [from, to] = range;
  return from === to ? dayLabel(from) : `${dayLabel(from)} – ${dayLabel(to)}`;
}

/**
 * One sentence for an import result — the same wording on every surface
 * (drop, the Import button, watch folders). Activities and Garmin
 * monitoring are reported side by side; "skipped" means the file's hash
 * was already in the vault (or, for a Monitor file, it carried no data).
 * `auto` prefixes the watch-folder wording. In the interface language at
 * the moment it is called — a toast is written once, when it is shown.
 */
export function formatImportSummary(
  result: ImportResult,
  opts: { auto?: boolean } = {},
  translate: Translator = t,
): { level: ImportSummaryLevel; text: string } {
  const activities = result.imported;
  const monitoringDays = result.monitoring_files > 0 ? result.monitoring_days : 0;
  const range = result.monitoring_range;

  const what: string[] = [];
  if (activities > 0) what.push(translate("common.activities", { count: activities }));
  if (monitoringDays > 0 && range) {
    // Monitor files closed at local midnight carry only the evening before,
    // so a successful import can leave the Recovery card exactly as it was:
    // say whether the newest day's night is in, and what to do if not.
    const date = dayLabel(range[1]);
    const night = result.monitoring_night
      ? translate("import.summary.night", { date })
      : translate(opts.auto ? "import.summary.noNight" : "import.summary.noNightSync", { date });
    what.push(
      translate("import.summary.monitoring", {
        days: translate("common.days", { count: monitoringDays }),
        range: rangeLabel(range),
        night,
      }),
    );
  } else if (result.monitoring_files > 0) {
    what.push(translate("import.summary.monitoringFiles", { count: result.monitoring_files }));
  }

  const parts: string[] = [];
  if (what.length > 0) {
    const list = what.length > 1 ? translate("import.summary.and", { first: what[0], second: what[1] }) : what[0];
    parts.push(translate(opts.auto ? "import.summary.autoImported" : "import.summary.imported", { what: list }));
  } else if (result.skipped > 0 && result.failed.length === 0) {
    return { level: "info", text: translate("import.summary.nothingNew", { count: result.skipped }) };
  } else if (result.skipped > 0 || result.failed.length > 0) {
    parts.push(translate("import.summary.nothing"));
  }
  if (result.skipped > 0) {
    parts.push(
      what.length > 0 && activities === 0
        ? translate("import.summary.skippedFiles", { count: result.skipped })
        : translate("import.summary.skippedDuplicates", { count: result.skipped }),
    );
  }
  if (result.failed.length > 0) parts.push(translate("import.summary.failedFiles", { count: result.failed.length }));

  if (parts.length === 0) return { level: "info", text: translate("import.summary.nothing") };
  return { level: result.failed.length > 0 ? "warning" : "success", text: parts.join(", ") };
}

/** The toast after photos were added to an activity — the gallery's button
 * and a drop on the page say the same — or null when nothing happened. */
export function formatAttachSummary(
  res: AttachPhotosResult,
  translate: Translator = t,
): { level: "success" | "warning"; text: string } | null {
  const parts: string[] = [];
  if (res.attached.length) parts.push(translate("activity.photos.added", { count: res.attached.length }));
  if (res.skipped.length) parts.push(translate("activity.photos.skipped", { count: res.skipped.length }));
  if (res.failed.length) parts.push(translate("activity.photos.failed", { count: res.failed.length }));
  if (parts.length === 0) return null;
  // The parts after the first are written in lower case; whichever comes
  // first opens the sentence.
  return { level: res.failed.length > 0 ? "warning" : "success", text: capitalize(parts.join(", ")) };
}
