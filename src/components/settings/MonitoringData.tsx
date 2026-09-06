import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Trash2 } from "lucide-react";
import { api } from "../../lib/tauri";
import type { MonitoringDeleted, MonitoringSummary } from "../../lib/types";
import { confirmDialog } from "../../stores/confirmStore";
import { useToastStore } from "../../stores/toastStore";
import { dateOfDayKey } from "../../lib/calendar";

const INPUT =
  "text-sm bg-card border border-border-2 rounded-[9px] px-2.5 py-1.5 outline-none focus:border-accent";

/** "Sep 5, 2026" from a "YYYY-MM-DD" key. */
export function longDate(key: string): string {
  return dateOfDayKey(key).toLocaleDateString("en-US", {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
}

const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;

/** The summary line under "Monitoring data". */
export function describeSummary(s: MonitoringSummary): string {
  if (s.days === 0 || !s.first_date || !s.last_date) {
    return "No Garmin monitoring data stored";
  }
  const span =
    s.first_date === s.last_date
      ? longDate(s.first_date)
      : `${longDate(s.first_date)} – ${longDate(s.last_date)}`;
  return `${plural(s.days, "day")} of Garmin monitoring (${span}) · ${plural(s.files, "file")}`;
}

/** Why a range cannot be deleted, or null when it can. */
export function rangeProblem(from: string, to: string): string | null {
  const iso = /^\d{4}-\d{2}-\d{2}$/;
  if (!iso.test(from) || !iso.test(to)) return "Pick both dates";
  if (to < from) return "The end date is before the start date";
  return null;
}

export function deleteMessage(from: string, to: string): string {
  const span = from === to ? `on ${longDate(from)}` : `from ${longDate(from)} to ${longDate(to)}`;
  return (
    `Delete the monitoring data ${span}?\n\n` +
    "Night heart rate, stress, steps and the other readings of those days are removed. " +
    "Monitor files that cover only those days are removed too and can be imported again; " +
    "a file that also covers a kept day stays, and its deleted part will not come back on re-import.\n\n" +
    "Earlier backup archives are not touched. If a watch folder still holds these files, " +
    "the next scan imports them again."
  );
}

/** Level and text of the toast after a delete. Files count what was really
 * removed; a refusal by the OS is said out loud, never folded into success. */
export function deleteToast(r: MonitoringDeleted): ["success" | "info" | "warning", string] {
  const done =
    r.days === 0 && r.files === 0
      ? "No monitoring data in that range"
      : `Deleted ${plural(r.days, "day")} of monitoring and ${plural(r.files, "file")}`;
  if (r.failed > 0) {
    return [
      "warning",
      `${done} · ${plural(r.failed, "file")} could not be removed (${r.error ?? "unknown error"}) — try again later`,
    ];
  }
  return [r.days === 0 && r.files === 0 ? "info" : "success", done];
}

/**
 * Settings → Vault → Monitoring data: what the vault holds from the watch's
 * Monitor files, and a date-range delete (ADR 0002, privacy).
 */
export function MonitoringData() {
  const queryClient = useQueryClient();
  const addToast = useToastStore((s) => s.addToast);
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const [busy, setBusy] = useState(false);

  // Under the "monitoring" prefix so an import invalidates it too.
  const { data: summary } = useQuery({
    queryKey: ["monitoring", "summary"],
    queryFn: () => api.getMonitoringSummary(),
  });

  // The inputs follow the STORED span: seeded on load so "everything" is
  // one click away, re-seeded only when the span itself changes (a delete
  // that trimmed an edge). Keyed on the two strings, not the summary
  // object, so a refetch that changes nothing — or a delete that cut the
  // middle out — leaves the user's own range in place.
  const first = summary?.first_date ?? null;
  const last = summary?.last_date ?? null;
  useEffect(() => {
    if (!first || !last) return;
    setFrom(first);
    setTo(last);
  }, [first, last]);

  // The stored span, when there is one — what the inputs are bounded by.
  const span = first && last && summary && summary.days > 0 ? { min: first, max: last } : null;
  const problem = rangeProblem(from, to);

  // Reachable only through the enabled button: no span or a bad range
  // keeps it disabled, and the backend re-checks the order anyway.
  async function handleDelete() {
    const confirmed = await confirmDialog({
      title: "Delete monitoring data",
      message: deleteMessage(from, to),
      confirmLabel: "Delete",
      danger: true,
    });
    if (!confirmed) return;
    setBusy(true);
    try {
      const { days, files, failed, error } = await api.deleteMonitoringRange(from, to);
      addToast(...deleteToast({ days, files, failed, error }));
      queryClient.invalidateQueries({ queryKey: ["monitoring"] });
      queryClient.invalidateQueries({ queryKey: ["recovery"] });
    } catch (e) {
      addToast("error", `Delete failed: ${e}`);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="set-row">
      <div className="flex-1 min-w-0">
        <div className="sl">Monitoring data</div>
        <div className="sd">{summary ? describeSummary(summary) : "…"}</div>
        {span && (
          <div className="flex flex-wrap items-center gap-2 mt-2.5 text-xs text-muted">
            <label className="flex items-center gap-1.5">
              From
              <input
                type="date"
                aria-label="Delete from"
                className={INPUT}
                value={from}
                min={span.min}
                max={span.max}
                onChange={(e) => setFrom(e.target.value)}
              />
            </label>
            <label className="flex items-center gap-1.5">
              to
              <input
                type="date"
                aria-label="Delete to"
                className={INPUT}
                value={to}
                min={span.min}
                max={span.max}
                onChange={(e) => setTo(e.target.value)}
              />
            </label>
            {problem && <span className="text-faint">{problem}</span>}
          </div>
        )}
      </div>
      <button
        onClick={handleDelete}
        disabled={busy || !span || problem !== null}
        className="btn ghost shrink-0"
      >
        <Trash2 size={15} />
        {busy ? "Deleting…" : "Delete…"}
      </button>
    </div>
  );
}
