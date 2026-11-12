import { useEffect, useRef } from "react";
import { useMutation } from "@tanstack/react-query";
import { api } from "../../lib/tauri";
import { errorText } from "../../lib/errors";
import { formatPower } from "../../lib/format";
import { useT, type Translator } from "../../lib/i18n";
import { firstTarget, stepTarget } from "../../lib/training";
import type { ErgState, TrainingLive } from "../../lib/types";

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
  }
}

/** ERG: hold a power whatever the cadence — on, the target up and down,
 * off. What the trainer confirmed is said under the target. */
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
  const erg = live.erg;
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
  return (
    <section className="dash-card flex flex-col gap-2" aria-label={t("training.erg.title")}>
      <div className="flex flex-wrap items-center gap-3">
        <span className="text-xs uppercase tracking-wide text-faint">{t("training.erg.title")}</span>
        {erg == null ? (
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
            <button
              className={button}
              onClick={() => {
                asked.current = null;
                set.mutate(null);
              }}
            >
              {t("training.erg.off")}
            </button>
          </>
        )}
      </div>
      {erg && (
        <p className={`text-sm ${erg.state === "holding" || erg.state === "ready" ? "text-muted" : ""}`} data-state={erg.state}>
          {stateText(erg.state, t)}
        </p>
      )}
      {set.error != null && <p className="text-sm text-danger">{errorText(set.error)}</p>}
    </section>
  );
}
