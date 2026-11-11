import { useNavigate } from "react-router";
import { useQueryClient } from "@tanstack/react-query";
import { invalidateActivityData } from "../../lib/activityInvalidation";
import { useT } from "../../lib/i18n";
import { useToastStore } from "../../stores/toastStore";
import type { TrainingFinish } from "../../lib/types";

/** What follows a finished ride: the new activity opens (a duplicate opens
 * the one already in the library, and says so); an empty ride is said in a
 * toast. */
export function useFinished(): (finished: TrainingFinish) => void {
  const navigate = useNavigate();
  const qc = useQueryClient();
  const t = useT();
  const addToast = useToastStore((s) => s.addToast);
  return (finished) => {
    void invalidateActivityData(qc);
    void qc.invalidateQueries({ queryKey: ["trainingPending"] });
    if (finished.kind === "duplicate") addToast("info", t("training.finished.duplicate"));
    if (finished.kind === "empty") addToast("info", t("training.finished.empty"));
    if (finished.activity_id) navigate(`/activity/${finished.activity_id}`);
  };
}
