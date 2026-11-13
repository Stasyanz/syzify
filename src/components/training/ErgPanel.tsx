import { useEffect, useRef } from "react";
import { useMutation } from "@tanstack/react-query";
import { api } from "../../lib/tauri";
import { errorText } from "../../lib/errors";
import { formatDuration, formatPower } from "../../lib/format";
import { useT, type Translator } from "../../lib/i18n";
import { firstTarget, stepTarget } from "../../lib/training";
import type { ErgState, NextStep, TrainingLive, WorkoutLive } from "../../lib/types";

function stateText(state: ErgState, t: Translator): string {
  switch (state) {
    case "ready":
      return t("training.erg.ready");
    case "waiting":
      return t("training.erg.waiting");
    case "holding":
      return t("training.erg.holding");
    case "off_target":
      return t("training.erg.offTarget");
    case "low_cadence":
      return t("training.erg.lowCadence");
    case "paused":
      return t("training.erg.paused");
    case "no_cadence":
      return t("training.erg.noCadence");
    case "releasing":
      return t("training.erg.releasing");
  }
}

function nextText(next: NextStep, t: Translator): string {
  const duration = formatDuration(next.secs);
  if (next.from_w === 0 && next.to_w === 0) return t("training.workout.nextFree", { duration });
  if (next.from_w === next.to_w) return t("training.workout.next", { power: formatPower(next.from_w), duration });
  return t("training.workout.nextRamp", { from: formatPower(next.from_w), to: formatPower(next.to_w), duration });
}

/** The workout under way: where it is, the countdown, what comes next. */
function WorkoutProgress({ workout }: { workout: WorkoutLive }) {
  const t = useT();
  if (workout.done) {
    return <p className="text-sm">{t("training.workout.done", { name: workout.name })}</p>;
  }
  return (
    <div className="flex flex-col gap-1" aria-label={t("training.workout.title")} role="group">
      <div className="flex flex-wrap items-baseline justify-between gap-2">
        <span className="text-sm font-medium" translate="no">
          {workout.name}
        </span>
        <span className="text-xs text-muted tabular-nums">
          {t("training.workout.elapsed", { elapsed: formatDuration(workout.elapsed_s), total: formatDuration(workout.total_s) })}
        </span>
      </div>
      <p className="text-sm tabular-nums">
        {t("training.workout.interval", { step: workout.step + 1, steps: workout.steps })}
        {" · "}
        <span className="font-bold">{t("training.workout.left", { duration: formatDuration(workout.step_left_s) })}</span>
        {workout.cadence != null && ` · ${t("training.workout.cadence", { rpm: workout.cadence })}`}
      </p>
      <p className="text-sm text-muted">{workout.next ? nextText(workout.next, t) : t("training.workout.last")}</p>
      {workout.cue && <p className="text-sm px-3 py-2 rounded-field bg-accent-soft">{workout.cue}</p>}
    </div>
  );
}

/** ERG: hold a power whatever the cadence — on, the target up and down,
 * off — or follow a workout. What the trainer confirmed is said under the
 * target. */
export function ErgPanel({ live, ftp }: { live: TrainingLive; ftp: number | null }) {
  const t = useT();
  // The target asked for and not yet shown back: two quick +10s are +20.
  // A refused one was never taken: forget it.
  const asked = useRef<number | null>(null);
  const set = useMutation({
    mutationFn: (watts: number | null) => api.trainingSetTarget(watts),
    onError: () => {
      asked.current = null;
    },
  });
  // Letting the brake go after riding free is no target: ERG is off.
  const erg = live.erg?.state === "releasing" ? null : live.erg;
  useEffect(() => {
    if (erg?.target_w === asked.current) asked.current = null;
  }, [erg?.target_w]);
  const step = (current: number, by: number) => {
    const next = stepTarget(asked.current ?? current, by);
    asked.current = next;
    set.mutate(next);
  };
  const button =
    "px-3 py-1.5 rounded-field border border-border-2 text-sm font-medium hover:bg-card-2 disabled:opacity-50";
  const off = () => {
    asked.current = null;
    set.mutate(null);
  };
  // A workout sets the target: no steps of one's own, a way out.
  const following = live.workout != null && !live.workout.done;
  // A trainer without ERG: nothing to set, said why — before the ride
  // with a way out, during it with the brake being let go if it was on. A
  // workout picked still guides, and can still be stopped.
  if (live.erg_unsupported) {
    return (
      <section className="dash-card flex flex-col gap-2" aria-label={t("training.erg.title")}>
        <div className="flex flex-wrap items-center gap-3">
          <span className="text-xs uppercase tracking-wide text-faint">{t("training.erg.title")}</span>
          {following && (
            <button className={button} disabled={set.isPending} onClick={off}>
              {t("training.workout.stop")}
            </button>
          )}
        </div>
        <p className="text-sm px-3 py-2 rounded-field bg-warn-soft" role="status">
          {t(live.phase === "pairing" ? "training.erg.unsupportedPairing" : "training.erg.unsupported")}
        </p>
        {live.erg && (
          <p className="text-sm" data-state={live.erg.state}>
            {stateText(live.erg.state, t)}
          </p>
        )}
        {live.workout && <WorkoutProgress workout={live.workout} />}
      </section>
    );
  }
  return (
    <section className="dash-card flex flex-col gap-2" aria-label={t("training.erg.title")}>
      <div className="flex flex-wrap items-center gap-3">
        <span className="text-xs uppercase tracking-wide text-faint">{t("training.erg.title")}</span>
        {following ? (
          <>
            <span className="text-2xl font-bold tabular-nums" aria-label={t("training.erg.target")}>
              {erg ? formatPower(erg.target_w) : t("training.workout.free")}
            </span>
            <button className={button} disabled={set.isPending} onClick={off}>
              {t("training.workout.stop")}
            </button>
          </>
        ) : erg == null ? (
          <button className={button} disabled={set.isPending} onClick={() => set.mutate(firstTarget(ftp))}>
            {t("training.erg.on")}
          </button>
        ) : (
          <>
            <button className={button} aria-label={t("training.erg.down")} onClick={() => step(erg.target_w, -10)}>
              −10
            </button>
            <span className="text-2xl font-bold tabular-nums" aria-label={t("training.erg.target")}>
              {formatPower(erg.target_w)}
            </span>
            <button className={button} aria-label={t("training.erg.up")} onClick={() => step(erg.target_w, 10)}>
              +10
            </button>
            <button className={button} onClick={off}>
              {t("training.erg.off")}
            </button>
          </>
        )}
      </div>
      {live.workout && <WorkoutProgress workout={live.workout} />}
      {live.erg && (
        <p
          className={`text-sm ${live.erg.state === "holding" || live.erg.state === "ready" ? "text-muted" : ""}`}
          data-state={live.erg.state}
        >
          {stateText(live.erg.state, t)}
        </p>
      )}
      {erg && live.power_match && (
        <p
          className={`text-sm ${live.power_match.state === "matching" ? "text-muted" : ""}`}
          data-match={live.power_match.state}
        >
          {live.power_match.state === "matching"
            ? t("training.erg.matching", { watts: formatPower(live.power_match.trainer_target_w) })
            : t("training.erg.noPedals")}
        </p>
      )}
      {set.error != null && <p className="text-sm text-danger">{errorText(set.error)}</p>}
    </section>
  );
}
