import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../../lib/tauri";
import { errorText } from "../../lib/errors";
import { useT } from "../../lib/i18n";

/** ANT+ needs Garmin's network key, which is not in Syzify's sources: a
 * build given it says nothing here, nor one whose user typed it in; until
 * then the user may type it in (checked against its hash). */
export function AntKey() {
  const t = useT();
  const qc = useQueryClient();
  const [text, setText] = useState("");
  const { data: status, error } = useQuery({ queryKey: ["trainingAntKey"], queryFn: api.trainingAntKey });
  const set = useMutation({
    mutationFn: (key: string) => api.trainingSetAntKey(key),
    onSuccess: () => {
      setText("");
      void qc.invalidateQueries({ queryKey: ["trainingAntKey"] });
    },
  });
  const title = <h3 className="!m-0 text-sm font-bold">{t("training.antKey.title")}</h3>;
  // Not known whether there is a key: said, not hidden — this is the one
  // place to enter it. (A failed refresh of a key known keeps it known.)
  if (error != null && !status?.known) {
    return (
      <section className="dash-card flex flex-col gap-2" aria-label={t("training.antKey.title")}>
        {title}
        <p className="text-sm text-danger">{errorText(error)}</p>
      </section>
    );
  }
  // A key built in is known too.
  if (!status || status.known) return null;
  const button = "px-3 py-1.5 rounded-field border border-border-2 text-sm font-medium hover:bg-card-2 disabled:opacity-50";
  return (
    <section className="dash-card flex flex-col gap-2" aria-label={t("training.antKey.title")}>
      {title}
      <p className="text-sm text-muted">{t("training.antKey.why")}</p>
      <div className="flex flex-wrap gap-x-4 gap-y-1">
        <a className="text-sm text-accent underline" href={status.page}>
          {t("training.antKey.page")}
        </a>
        <a className="text-sm text-accent underline" href={status.agreement}>
          {t("training.antKey.agreement")}
        </a>
      </div>
      <form
        className="flex flex-wrap items-center gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          set.mutate(text);
        }}
      >
        <input
          className="flex-1 min-w-48 px-2 py-1.5 rounded-field border border-border-2 bg-card text-sm font-mono"
          aria-label={t("training.antKey.field")}
          placeholder="00 00 00 00 00 00 00 00"
          value={text}
          onChange={(e) => setText(e.target.value)}
          spellCheck={false}
          autoComplete="off"
        />
        <button className={button} type="submit" disabled={set.isPending || text.trim() === ""}>
          {t("training.antKey.save")}
        </button>
      </form>
      {set.error != null && <p className="text-sm text-danger">{errorText(set.error)}</p>}
    </section>
  );
}
