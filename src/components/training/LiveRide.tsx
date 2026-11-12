import type { ReactNode } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useToastStore } from "../../stores/toastStore";
import { api } from "../../lib/tauri";
import { errorText } from "../../lib/errors";
import { formatDistance, formatDuration, formatHR, formatPower, formatSpeed, formatWhole } from "../../lib/format";
import { formatPercent, useT, type Translator } from "../../lib/i18n";
import { canBegin, powerZone } from "../../lib/training";
import { confirmDialog } from "../../stores/confirmStore";
import type { LinkStatus, TrainingLive } from "../../lib/types";
import { useFinished } from "./useFinished";
import { ErgPanel } from "./ErgPanel";

function linkText(link: LinkStatus, t: Translator): string {
  switch (link.state) {
    case "searching":
      return t("training.link.searching");
    case "connected":
      return t("training.link.connected");
    case "lost":
      return t("training.link.lost");
    case "failed":
      switch (link.problem) {
        case "permission_denied":
          return t("training.problem.permissionDenied");
        case "radio_off":
          return t("training.problem.radioOff");
        case "adapter_gone":
          return t("training.problem.adapterGone");
        default:
          return t("training.problem.other", { detail: link.detail ?? "" });
      }
  }
}

/** One device's link: a quiet line, never an alarm (drops are routine for
 * some trainers and the ride keeps recording). */
function Links({ live }: { live: TrainingLive }) {
  const t = useT();
  const roleLabel = { trainer: t("training.role.trainer"), pedals: t("training.role.pedals"), strap: t("training.role.strap") };
  return (
    <ul className="flex flex-wrap gap-x-5 gap-y-1 text-xs text-muted" aria-label={t("training.links")}>
      {live.links.map((link) => (
        <li key={link.role} data-state={link.state}>
          <span className="text-faint">{roleLabel[link.role]}</span>{" "}
          {link.name && (
            <span translate="no" className="text-ink">
              {link.name}{" "}
            </span>
          )}
          <span className={link.state === "failed" ? "text-danger" : undefined}>{linkText(link, t)}</span>
        </li>
      ))}
    </ul>
  );
}

function Tile({ label, value, note }: { label: string; value: string; note?: ReactNode }) {
  return (
    <div className="dash-card flex flex-col gap-1" role="group" aria-label={label}>
      <span className="text-xs uppercase tracking-wide text-faint">{label}</span>
      <span className="text-3xl font-bold tabular-nums">{value}</span>
      {note && <span className="text-xs text-muted">{note}</span>}
    </div>
  );
}

/** The session under way: pairing (live values, Start), then the ride
 * (Lap, Pause/Resume, Finish, Discard). */
export function LiveRide({ live }: { live: TrainingLive }) {
  const t = useT();
  const finishedRide = useFinished();
  const { data: ftp } = useQuery({ queryKey: ["trainingFtp"], queryFn: api.trainingFtp });

  const qc = useQueryClient();
  const addToast = useToastStore((s) => s.addToast);
  // The session ends before these answer (the screen goes back to the
  // devices): an error is said in a toast, which outlives this component,
  // and the ride is offered again among the unfinished ones.
  const ended = (e: unknown) => {
    addToast("error", errorText(e));
    void qc.invalidateQueries({ queryKey: ["trainingPending"] });
  };
  const begin = useMutation({ mutationFn: api.trainingBegin });
  const leave = useMutation({ mutationFn: api.trainingLeave, onError: ended });
  const finish = useMutation({ mutationFn: api.trainingFinish, onSuccess: finishedRide, onError: ended });
  const discard = useMutation({ mutationFn: api.trainingDiscard, onError: ended });
  const quick = useMutation({ mutationFn: (order: () => Promise<void>) => order() });
  const error = begin.error ?? quick.error;

  const askDiscard = async () => {
    const ok = await confirmDialog({
      title: t("training.discardTitle"),
      message: t("training.discardMessage"),
      confirmLabel: t("training.discard"),
      danger: true,
    });
    if (ok) discard.mutate();
  };

  const zone = powerZone(live.watts, ftp ?? null);
  const powerNote =
    zone != null && live.watts != null && ftp
      ? t("training.zone", { zone, share: formatPercent((live.watts / ftp) * 100, 0) })
      : ftp == null
        ? t("training.noFtp")
        : undefined;
  const trainerNote =
    live.power_from === "pedals" && live.trainer_watts != null
      ? t("training.trainerWatts", { watts: formatPower(live.trainer_watts) })
      : undefined;
  const speed = live.speed_kmh == null ? null : live.speed_kmh / 3.6;
  const pairing = live.phase === "pairing";
  const ending = live.phase === "finishing" || finish.isPending || discard.isPending;

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <Links live={live} />
        {live.profile && (
          <span className="text-xs text-faint">
            {t("training.profile")} <span translate="no">{live.profile}</span>
          </span>
        )}
      </div>

      {pairing && <p className="text-sm text-muted">{t("training.pairing.hint")}</p>}
      {live.phase === "paused" && <p className="text-sm font-medium">{t("training.paused")}</p>}
      {ending && <p className="text-sm text-muted">{t("training.finishing")}</p>}
      {live.phase === "finishing" && live.release_unconfirmed && (
        <p className="text-sm px-3 py-2 rounded-field bg-warn-soft">{t("training.releaseUnconfirmed")}</p>
      )}
      {live.slept_s > 0 && (
        <p className="text-sm px-3 py-2 rounded-field bg-warn-soft">{t("training.slept", { duration: formatDuration(live.slept_s) })}</p>
      )}
      {live.warnings.map((why) => (
        <p key={why} className="text-sm px-3 py-2 rounded-field bg-warn-soft">
          {t("training.journalWarning", { why })}
        </p>
      ))}
      {error != null && <p className="text-sm text-danger">{errorText(error)}</p>}

      {!ending && <ErgPanel live={live} ftp={ftp ?? null} />}

      <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
        <Tile label={t("training.metric.power")} value={formatPower(live.watts)} note={powerNote ?? trainerNote} />
        <Tile
          label={t("training.metric.cadence")}
          value={live.cadence_rpm == null ? "--" : formatWhole(live.cadence_rpm, "rpm")}
        />
        <Tile label={t("training.metric.heartRate")} value={formatHR(live.heart_rate)} />
        <Tile label={t("training.metric.speed")} value={formatSpeed(speed)} />
        <Tile label={t("training.metric.time")} value={formatDuration(live.ride_s)} />
        <Tile label={t("training.metric.distance")} value={formatDistance(live.distance_m)} />
        <Tile label={t("training.metric.lap", { lap: live.lap })} value={formatDuration(live.lap_s)} />
        <Tile label={t("training.metric.avgPower")} value={formatPower(live.avg_watts)} />
      </div>

      <div className="flex flex-wrap gap-2">
        {pairing ? (
          <>
            <button
              className="px-4 py-1.5 rounded-field bg-accent text-accent-ink text-sm font-medium hover:bg-accent-2 disabled:opacity-50"
              disabled={!canBegin(live) || begin.isPending}
              onClick={() => begin.mutate()}
            >
              {t("training.start")}
            </button>
            <button
              className="px-4 py-1.5 rounded-field border border-border-2 text-sm font-medium hover:bg-card-2 disabled:opacity-50"
              disabled={leave.isPending}
              onClick={() => leave.mutate()}
            >
              {t("training.disconnect")}
            </button>
          </>
        ) : (
          <>
            <button
              className="px-4 py-1.5 rounded-field border border-border-2 text-sm font-medium hover:bg-card-2 disabled:opacity-50"
              disabled={ending || live.phase === "paused"}
              onClick={() => quick.mutate(api.trainingLap)}
            >
              {t("training.lap")}
            </button>
            <button
              className="px-4 py-1.5 rounded-field border border-border-2 text-sm font-medium hover:bg-card-2 disabled:opacity-50"
              disabled={ending}
              onClick={() => quick.mutate(live.phase === "paused" ? api.trainingResume : api.trainingPause)}
            >
              {live.phase === "paused" ? t("training.resume") : t("training.pause")}
            </button>
            <button
              className="px-4 py-1.5 rounded-field bg-accent text-accent-ink text-sm font-medium hover:bg-accent-2 disabled:opacity-50"
              disabled={ending}
              onClick={() => finish.mutate()}
            >
              {t("training.finish")}
            </button>
            <button
              className="px-4 py-1.5 rounded-field border border-border-2 text-muted text-sm font-medium hover:text-ink hover:bg-card-2 disabled:opacity-50"
              disabled={ending}
              onClick={() => void askDiscard()}
            >
              {t("training.discard")}
            </button>
          </>
        )}
      </div>
    </div>
  );
}
