import { open } from "@tauri-apps/plugin-dialog";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../../lib/tauri";
import { errorText } from "../../lib/errors";
import { formatDuration } from "../../lib/format";
import { useT, type Translator } from "../../lib/i18n";
import type { WorkoutFile, WorkoutNote, WorkoutProblem, WorkoutSegment } from "../../lib/types";

function problemText(problem: WorkoutProblem, t: Translator): string {
  switch (problem.kind) {
    case "unreadable":
      return t("training.workouts.problem.unreadable", { detail: problem.detail });
    case "not_a_workout":
      return t("training.workouts.problem.notAWorkout");
    case "run":
      return t("training.workouts.problem.run");
    case "distance":
      return t("training.workouts.problem.distance");
    case "empty":
      return t("training.workouts.problem.empty");
    case "too_long":
      return t("training.workouts.problem.tooLong");
    case "too_big":
      return t("training.workouts.problem.tooBig");
  }
}

function noteText(note: WorkoutNote, t: Translator): string {
  switch (note.kind) {
    case "unknown":
      return t("training.workouts.note.unknown", { element: note.element });
    case "ridden_free":
      return t("training.workouts.note.riddenFree", { element: note.element });
    case "bad_value":
      return t("training.workouts.note.badValue", { element: note.element });
    case "distance_cue":
      return t("training.workouts.note.distanceCue");
    case "cue_outside":
      return t("training.workouts.note.cueOutside");
  }
}

/** Shares of FTP above this are drawn at the top. */
const PROFILE_TOP = 1.5;

/** The workout's power over time, as a small filled outline. */
export function WorkoutProfile({ segments }: { segments: WorkoutSegment[] }) {
  const total = segments.reduce((sum, s) => sum + s.secs, 0);
  if (total <= 0) return null;
  const width = 120;
  const height = 24;
  const y = (share: number) => height - (Math.min(share, PROFILE_TOP) / PROFILE_TOP) * height;
  let x = 0;
  const shapes = segments.map((s, i) => {
    const w = (s.secs / total) * width;
    const from = s.free ? 0.5 : s.from;
    const to = s.free ? 0.5 : s.to;
    const points = `${x},${height} ${x},${y(from)} ${x + w},${y(to)} ${x + w},${height}`;
    x += w;
    return <polygon key={i} points={points} className={s.free ? "fill-faint" : "fill-accent"} />;
  });
  return (
    <svg width={width} height={height} viewBox={`0 0 ${width} ${height}`} aria-hidden="true" className="shrink-0">
      {shapes}
    </svg>
  );
}

function Row({ workout, canStart, onStart }: { workout: WorkoutFile; canStart: boolean; onStart: () => void }) {
  const t = useT();
  return (
    <li className="flex flex-wrap items-center gap-3 py-2 border-b border-border last:border-0">
      <WorkoutProfile segments={workout.profile} />
      <div className="flex-1 min-w-40 flex flex-col">
        <span className="text-sm font-medium truncate" translate="no">
          {workout.name}
        </span>
        <span className="text-xs text-muted">
          {workout.problem ? problemText(workout.problem, t) : formatDuration(workout.duration_s)}
          {workout.notes.map((n) => ` · ${noteText(n, t)}`).join("")}
        </span>
      </div>
      {!workout.problem && (
        <button
          className="px-3 py-1.5 rounded-field border border-border-2 text-sm font-medium hover:bg-card-2 disabled:opacity-50"
          disabled={!canStart}
          onClick={onStart}
        >
          {t("training.workouts.start")}
        </button>
      )}
    </li>
  );
}

/** The workouts folder's `.zwo` files: pick one and ERG follows it. */
export function WorkoutPicker({ ftp }: { ftp: number | null }) {
  const t = useT();
  const qc = useQueryClient();
  const { data: list } = useQuery({ queryKey: ["trainingWorkouts"], queryFn: api.trainingWorkouts });
  const pick = useMutation({
    mutationFn: async () => {
      const selected = await open({ directory: true, multiple: false, title: t("training.workouts.pickTitle") });
      if (typeof selected !== "string") return;
      await api.trainingSetWorkoutsDir(selected);
      await qc.invalidateQueries({ queryKey: ["trainingWorkouts"] });
    },
  });
  const start = useMutation({ mutationFn: (path: string) => api.trainingStartWorkout(path) });
  const error = pick.error ?? start.error;
  if (!list) return null;
  const button = "px-3 py-1.5 rounded-field border border-border-2 text-sm font-medium hover:bg-card-2 disabled:opacity-50";
  return (
    <section className="dash-card flex flex-col gap-2" aria-label={t("training.workouts.title")}>
      <div className="flex flex-wrap items-center gap-3">
        <span className="text-xs uppercase tracking-wide text-faint">{t("training.workouts.title")}</span>
        {list.dir && (
          <span className="text-xs text-muted truncate" translate="no">
            {list.dir}
          </span>
        )}
        <button className={button} disabled={pick.isPending} onClick={() => pick.mutate()}>
          {list.dir ? t("training.workouts.changeFolder") : t("training.workouts.pickFolder")}
        </button>
      </div>
      {!list.dir && <p className="text-sm text-muted">{t("training.workouts.hint")}</p>}
      {list.missing && <p className="text-sm">{t("training.workouts.missing")}</p>}
      {list.dir && !list.missing && list.workouts.length === 0 && (
        <p className="text-sm text-muted">{t("training.workouts.empty")}</p>
      )}
      {list.dir && list.workouts.length > 0 && ftp == null && <p className="text-sm text-muted">{t("training.workouts.noFtp")}</p>}
      {list.workouts.length > 0 && (
        <ul className="flex flex-col max-h-80 overflow-y-auto scroll-themed">
          {list.workouts.map((w) => (
            <Row key={w.path} workout={w} canStart={ftp != null && !start.isPending} onStart={() => start.mutate(w.path)} />
          ))}
        </ul>
      )}
      {error != null && <p className="text-sm text-danger">{errorText(error)}</p>}
    </section>
  );
}
