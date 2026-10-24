import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Trash2 } from "lucide-react";
import { api } from "../../lib/tauri";
import type { MonitoringDeleted, MonitoringSummary } from "../../lib/types";
import { confirmDialog } from "../../stores/confirmStore";
import { useToastStore } from "../../stores/toastStore";
import { useBusy, useBusyStore } from "../../stores/busyStore";
import { dateOfDayKey } from "../../lib/calendar";
import { errorText } from "../../lib/errors";
import { formatDateTime, useT, type Translator } from "../../lib/i18n";

const INPUT =
  "text-sm bg-card border border-border-2 rounded-[9px] px-2.5 py-1.5 outline-none focus:border-accent";

/** "Sep 5, 2026" from a "YYYY-MM-DD" key. */
export function longDate(key: string): string {
  return formatDateTime(dateOfDayKey(key), { year: "numeric", month: "short", day: "numeric" });
}

/** The summary line under "Monitoring data". */
export function describeSummary(s: MonitoringSummary, translate: Translator): string {
  if (s.days === 0 || !s.first_date || !s.last_date) {
    return translate("settings.monitoring.none");
  }
  const span =
    s.first_date === s.last_date
      ? longDate(s.first_date)
      : `${longDate(s.first_date)} – ${longDate(s.last_date)}`;
  return translate("settings.monitoring.summary", {
    days: translate("common.days", { count: s.days }),
    span,
    files: translate("common.files", { count: s.files }),
  });
}

/** Why a range cannot be deleted, or null when it can. */
export function rangeProblem(from: string, to: string, translate: Translator): string | null {
  const iso = /^\d{4}-\d{2}-\d{2}$/;
  if (!iso.test(from) || !iso.test(to)) return translate("settings.monitoring.pickBoth");
  if (to < from) return translate("settings.monitoring.endBeforeStart");
  return null;
}

export function deleteMessage(from: string, to: string, translate: Translator): string {
  const question =
    from === to
      ? translate("settings.monitoring.deleteOne", { date: longDate(from) })
      : translate("settings.monitoring.deleteRange", { from: longDate(from), to: longDate(to) });
  return `${question}\n\n${translate("settings.monitoring.deleteBody")}`;
}

/** Level and text of the toast after a delete. Files count what was really
 * removed; a refusal by the OS is said out loud, never folded into success. */
export function deleteToast(r: MonitoringDeleted, translate: Translator): ["success" | "info" | "warning", string] {
  const done =
    r.days === 0 && r.files === 0
      ? translate("settings.monitoring.nothingInRange")
      : translate("settings.monitoring.deleted", {
          days: translate("common.days", { count: r.days }),
          files: translate("common.files", { count: r.files }),
        });
  if (r.failed > 0) {
    return [
      "warning",
      translate("settings.monitoring.notRemoved", {
        done,
        count: r.failed,
        // Inside parentheses mid-sentence: the lower-case form.
        error: r.error ?? translate("settings.monitoring.unknownError"),
      }),
    ];
  }
  return [r.days === 0 && r.files === 0 ? "info" : "success", done];
}

/**
 * Settings → Vault → Monitoring data: what the vault holds from the watch's
 * Monitor files, and a date-range delete (ADR 0002, privacy). The delete's
 * busy flag lives in the busy store, so a remount keeps the button locked.
 */
export function MonitoringData() {
  const t = useT();
  const queryClient = useQueryClient();
  const addToast = useToastStore((s) => s.addToast);
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const busy = useBusy("monitoringDelete");

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
  const problem = rangeProblem(from, to, t);

  // Reachable only through the enabled button: no span or a bad range
  // keeps it disabled, and the backend re-checks the order anyway.
  async function handleDelete() {
    const confirmed = await confirmDialog({
      title: t("settings.monitoring.deleteTitle"),
      message: deleteMessage(from, to, t),
      confirmLabel: t("common.delete"),
      danger: true,
    });
    if (!confirmed) return;
    const ops = useBusyStore.getState();
    ops.start("monitoringDelete");
    try {
      const { days, files, failed, error } = await api.deleteMonitoringRange(from, to);
      addToast(...deleteToast({ days, files, failed, error }, t));
      queryClient.invalidateQueries({ queryKey: ["monitoring"] });
      queryClient.invalidateQueries({ queryKey: ["recovery"] });
    } catch (e) {
      addToast("error", t("settings.monitoring.deleteFailed", { error: errorText(e) }));
    } finally {
      ops.finish("monitoringDelete");
    }
  }

  return (
    <div className="set-row">
      <div className="flex-1 min-w-0">
        <div className="sl">{t("settings.monitoring.title")}</div>
        <div className="sd">{summary ? describeSummary(summary, t) : "…"}</div>
        {span && (
          <div className="flex flex-wrap items-center gap-2 mt-2.5 text-xs text-muted">
            <label className="flex items-center gap-1.5">
              {t("settings.monitoring.from")}
              <input
                type="date"
                aria-label={t("settings.monitoring.deleteFrom")}
                className={INPUT}
                value={from}
                min={span.min}
                max={span.max}
                onChange={(e) => setFrom(e.target.value)}
              />
            </label>
            <label className="flex items-center gap-1.5">
              {t("settings.monitoring.to")}
              <input
                type="date"
                aria-label={t("settings.monitoring.deleteTo")}
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
        {busy ? t("settings.monitoring.deleting") : t("settings.monitoring.delete")}
      </button>
    </div>
  );
}
