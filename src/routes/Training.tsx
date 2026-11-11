import { useT } from "../lib/i18n";
import { trainingSupported } from "../lib/training";
import { useTrainingLive } from "../hooks/useTrainingLive";
import { DevicePicker } from "../components/training/DevicePicker";
import { LiveRide } from "../components/training/LiveRide";
import { PendingRides } from "../components/training/PendingRides";

/** The /training page (#273, macOS only): pick devices and connect, then
 * ride; a ride a crash left behind is offered first. The ride itself runs
 * in the backend, so leaving the page does not stop it. */
export function Training() {
  const t = useT();
  const live = useTrainingLive();
  return (
    <div className="flex flex-col h-full">
      <main className="flex-1 overflow-y-auto scroll-themed">
        <div className="sticky top-0 z-10 bg-bg px-6 pt-3 pb-2">
          <h2 className="!m-0 text-lg font-bold">{t("training.title")}</h2>
        </div>
        <div className="px-6 pb-6 pt-1 flex flex-col gap-4">
          {live === undefined ? null : live === null ? (
            <>
              <PendingRides />
              {/* Elsewhere the page is reached from Settings, for rides a
                  Mac left in the vault. */}
              {trainingSupported() ? <DevicePicker /> : <p className="text-sm text-muted">{t("training.unsupported")}</p>}
            </>
          ) : (
            <LiveRide live={live} />
          )}
        </div>
      </main>
    </div>
  );
}
