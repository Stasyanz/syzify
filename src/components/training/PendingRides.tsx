import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../../lib/tauri";
import { formatDuration } from "../../lib/format";
import { formatDateTime, useT } from "../../lib/i18n";
import { errorText } from "../../lib/errors";
import { confirmDialog } from "../../stores/confirmStore";
import { useToastStore } from "../../stores/toastStore";
import type { PendingRide } from "../../lib/types";
import { useFinished } from "./useFinished";

/** Rides a crash or a closed app cut short: finish one into the library or
 * throw it away. Shown only when there are any. */
export function PendingRides() {
  const t = useT();
  const qc = useQueryClient();
  const finishedRide = useFinished();
  const addToast = useToastStore((s) => s.addToast);
  const { data: pending } = useQuery({ queryKey: ["trainingPending"], queryFn: api.trainingPending });

  const refresh = () => qc.invalidateQueries({ queryKey: ["trainingPending"] });
  const finish = useMutation({
    mutationFn: (start: number) => api.trainingFinishPending(start),
    onSuccess: finishedRide,
    onError: (e) => {
      addToast("error", errorText(e));
      void refresh();
    },
  });
  const discard = useMutation({
    mutationFn: (start: number) => api.trainingDiscardPending(start),
    onSettled: refresh,
    onError: (e) => addToast("error", errorText(e)),
  });

  if (!pending || pending.length === 0) return null;

  const askDiscard = async (ride: PendingRide) => {
    const ok = await confirmDialog({
      title: t("training.pending.discardTitle"),
      message: t("training.pending.discardMessage"),
      confirmLabel: t("training.discard"),
      danger: true,
    });
    if (ok) discard.mutate(ride.start_unix);
  };

  const busy = finish.isPending || discard.isPending;
  return (
    <section className="dash-card" aria-label={t("training.pending.title")}>
      <h3 className="!mt-0 text-sm font-bold">{t("training.pending.title")}</h3>
      <p className="text-sm text-muted">{t("training.pending.intro")}</p>
      <ul className="mt-2 flex flex-col gap-2">
        {pending.map((ride) => {
          const when = formatDateTime(new Date(ride.start_unix * 1000), { dateStyle: "medium", timeStyle: "short" });
          return (
            <li key={ride.start_unix} className="flex items-center justify-between gap-3 text-sm">
              <div>
                <div className="text-ink">
                  {when}
                  {ride.ride_s != null && <> · {formatDuration(ride.ride_s)}</>}
                </div>
                {ride.locked && <div className="text-xs text-faint">{t("training.pending.locked")}</div>}
                {ride.unreadable && (
                  <div className="text-xs text-faint">{t("training.pending.unreadable", { why: ride.unreadable })}</div>
                )}
              </div>
              <div className="flex items-center gap-2 shrink-0">
                {!ride.unreadable && !ride.locked && (
                  <button
                    className="px-3 py-1 rounded-field bg-accent text-accent-ink text-xs font-medium hover:bg-accent-2 disabled:opacity-50"
                    disabled={busy}
                    onClick={() => finish.mutate(ride.start_unix)}
                  >
                    {t("training.finish")}
                  </button>
                )}
                <button
                  className="px-3 py-1 rounded-field border border-border-2 text-muted text-xs font-medium hover:text-ink hover:bg-card-2 disabled:opacity-50"
                  disabled={busy}
                  onClick={() => void askDiscard(ride)}
                >
                  {t("training.discard")}
                </button>
              </div>
            </li>
          );
        })}
      </ul>
    </section>
  );
}
