import { useNavigate } from "react-router";
import { useT } from "../../lib/i18n";

/** The way to the unfinished rides that hold up an encryption change
 * (`training.pendingRides`): the training page lists them on any system. */
export function PendingRidesLink() {
  const t = useT();
  const navigate = useNavigate();
  return (
    <button className="text-xs text-accent underline" onClick={() => navigate("/training")}>
      {t("training.pending.open")}
    </button>
  );
}
