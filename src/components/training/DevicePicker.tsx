import { useState } from "react";
import { useMutation } from "@tanstack/react-query";
import { Bluetooth } from "lucide-react";
import { api } from "../../lib/tauri";
import { errorText } from "../../lib/errors";
import { useT } from "../../lib/i18n";
import { ROLES, devicesFor } from "../../lib/training";
import type { FoundDevice, SensorRole } from "../../lib/types";

type Picks = Record<SensorRole, string | null>;

const NONE: Picks = { trainer: null, pedals: null, strap: null };

/** Search for devices, pick a trainer (and pedals and a strap, if there are
 * any), connect. */
export function DevicePicker() {
  const t = useT();
  const [found, setFound] = useState<FoundDevice[] | null>(null);
  const [picks, setPicks] = useState<Picks>(NONE);

  const scan = useMutation({
    mutationFn: () => api.trainingScan(),
    onSuccess: (devices) => {
      setFound(devices);
      // The first of each role is the likely one; pedals and a strap stay
      // a choice (a watch's strap may be someone else's).
      setPicks({ ...NONE, trainer: devicesFor(devices, "trainer")[0]?.address ?? null });
    },
  });
  const connect = useMutation({
    mutationFn: (trainer: string) => api.trainingConnect(trainer, picks.pedals, picks.strap),
  });
  const trainer = picks.trainer;

  const roleLabel = { trainer: t("training.role.trainer"), pedals: t("training.role.pedals"), strap: t("training.role.strap") };
  const error = scan.error ?? connect.error;

  return (
    <section className="dash-card" aria-label={t("training.setup.title")}>
      <h3 className="!mt-0 text-sm font-bold">{t("training.setup.title")}</h3>
      <p className="text-sm text-muted">{t("training.setup.intro")}</p>
      <button
        className="mt-3 inline-flex items-center gap-2 px-3 py-1.5 rounded-field border border-border-2 text-sm font-medium hover:bg-card-2 disabled:opacity-50"
        disabled={scan.isPending || connect.isPending}
        onClick={() => scan.mutate()}
      >
        <Bluetooth size={15} />
        {scan.isPending ? t("training.setup.searching") : found ? t("training.setup.searchAgain") : t("training.setup.search")}
      </button>
      {error != null && <p className="mt-2 text-sm text-danger">{errorText(error)}</p>}

      {found && found.length === 0 && <p className="mt-3 text-sm text-muted">{t("training.setup.nothing")}</p>}

      {found && found.length > 0 && (
        <div className="mt-3 flex flex-col gap-3">
          {ROLES.map((role) => {
            const devices = devicesFor(found, role);
            return (
              <fieldset key={role} className="flex flex-col gap-1">
                <legend className="text-xs uppercase tracking-wide text-faint mb-1">{roleLabel[role]}</legend>
                {role !== "trainer" && (
                  <label className="flex items-center gap-2 text-sm">
                    <input
                      type="radio"
                      name={role}
                      checked={picks[role] == null}
                      onChange={() => setPicks({ ...picks, [role]: null })}
                    />
                    {t("training.setup.none")}
                  </label>
                )}
                {devices.length === 0 && role === "trainer" && (
                  <p className="text-sm text-muted">{t("training.setup.noTrainer")}</p>
                )}
                {devices.map((d) => (
                  <label key={d.address} className="flex items-center gap-2 text-sm">
                    <input
                      type="radio"
                      name={role}
                      checked={picks[role] === d.address}
                      onChange={() => setPicks({ ...picks, [role]: d.address })}
                    />
                    {d.name ? <span translate="no">{d.name}</span> : t("training.setup.unnamed")}
                  </label>
                ))}
              </fieldset>
            );
          })}
          <div>
            <button
              className="px-4 py-1.5 rounded-field bg-accent text-accent-ink text-sm font-medium hover:bg-accent-2 disabled:opacity-50"
              disabled={trainer == null || connect.isPending}
              // No trainer picked, nothing to connect: the button is off.
              onClick={trainer == null ? undefined : () => connect.mutate(trainer)}
            >
              {t("training.setup.connect")}
            </button>
          </div>
        </div>
      )}
    </section>
  );
}
