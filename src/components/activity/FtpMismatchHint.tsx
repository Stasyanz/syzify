import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { X } from "lucide-react";
import { api } from "../../lib/tauri";
import type { Activity, PreviousPower } from "../../lib/types";

/** The setting under which a dismissed hint is remembered, per activity —
 * in the vault, so it survives restarts and travels with the library. */
export const ftpHintDismissedKey = (activityId: string) => `ftp_hint_dismissed.${activityId}`;

/** The FTP the rider's recent rides agree on: the value most of the given
 * rides (newest first) carry, the newest winning a tie — and the newest
 * ride that carries it, for its device. */
export function recentFtp(recent: PreviousPower[]): { ftp: number; device: string | null } | null {
  if (recent.length === 0) return null;
  const counts = new Map<number, number>();
  for (const r of recent) {
    const f = Math.round(r.threshold_power_w);
    counts.set(f, (counts.get(f) ?? 0) + 1);
  }
  let best = -1;
  let ftp = Math.round(recent[0].threshold_power_w);
  // Iterate newest first so a tie keeps the first (newest) value.
  for (const r of recent) {
    const f = Math.round(r.threshold_power_w);
    const c = counts.get(f) ?? 0;
    if (c > best) {
      best = c;
      ftp = f;
    }
  }
  const carrier = recent.find((r) => Math.round(r.threshold_power_w) === ftp);
  return { ftp, device: carrier?.source_device ?? null };
}

/** The hint's two sides when an activity's FTP differs from what the
 * rider's recent rides used, or null when there is nothing to say: no FTP
 * on this activity, no earlier rides with one, or a difference under 1 W.
 * "Recent" is the majority of the last few rides, not the neighbor and not
 * the library's newest: a device left at an old FTP stands out against the
 * rides around it, old rides recorded under the FTP of their day are left
 * alone, and after a deliberate change the hint shows on the first ride or
 * two and then the new value is the majority. */
export function ftpMismatch(
  activity: Pick<Activity, "threshold_power_w" | "source_device">,
  recent: PreviousPower[] | null | undefined,
): { thisFtp: number; recentFtp: number; thisDevice: string | null; recentDevice: string | null } | null {
  if (activity.threshold_power_w == null || !recent || recent.length === 0) return null;
  const agreed = recentFtp(recent);
  if (!agreed) return null;
  const thisFtp = Math.round(activity.threshold_power_w);
  if (Math.abs(thisFtp - agreed.ftp) < 1) return null;
  return { thisFtp, recentFtp: agreed.ftp, thisDevice: activity.source_device, recentDevice: agreed.device };
}

/** "Recorded with FTP 200 W on Edge 840 — your recent rides used 238 W
 * (fenix 7)" with a way into the FTP correction (#137). A device that had
 * a stale FTP is otherwise noticed only by reading the file. */
export function FtpMismatchHint({
  activity,
  recent,
  onCorrect,
}: {
  activity: Pick<Activity, "id" | "threshold_power_w" | "source_device">;
  recent: PreviousPower[] | null | undefined;
  onCorrect: () => void;
}) {
  const m = ftpMismatch(activity, recent);
  const key = ftpHintDismissedKey(activity.id);
  const { data: saved, isPending } = useQuery({
    queryKey: ["setting", key],
    queryFn: () => api.getSetting(key),
    enabled: m != null,
  });
  // Hidden the moment the cross is clicked, whatever the write's timing.
  // Remembered by activity id, not as a bare flag: the activity page keeps
  // this component mounted while the user pages to the next ride, and a
  // hint dismissed on one ride must not stay hidden on another.
  const [dismissedFor, setDismissedFor] = useState<string | null>(null);
  if (!m || dismissedFor === activity.id || isPending || saved === "1") return null;
  const dismiss = () => {
    setDismissedFor(activity.id);
    api.setSetting(key, "1").catch(() => {});
  };
  const on = m.thisDevice ? ` on ${m.thisDevice}` : "";
  const with_ = m.recentDevice ? ` (${m.recentDevice})` : "";
  return (
    <div
      role="note"
      className="flex flex-wrap items-center gap-x-3 gap-y-1 rounded-lg border border-border bg-card px-3 py-2 text-sm text-muted"
    >
      <span>
        Recorded with FTP <strong className="text-ink">{m.thisFtp} W</strong>
        {on} — your recent rides used <strong className="text-ink">{m.recentFtp} W</strong>
        {with_}.
      </span>
      <button type="button" onClick={onCorrect} className="text-accent-2 underline hover:no-underline">
        Correct FTP
      </button>
      <button
        type="button"
        onClick={dismiss}
        aria-label="Dismiss"
        className="ml-auto grid h-6 w-6 place-items-center rounded text-faint hover:bg-card-2 hover:text-ink"
      >
        <X size={14} />
      </button>
    </div>
  );
}
