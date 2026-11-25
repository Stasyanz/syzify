import { useT } from "../lib/i18n";
import { trainingSupported } from "../lib/training";
import { useTrainingLive } from "../hooks/useTrainingLive";
import { AntKey } from "../components/training/AntKey";
import { DevicePicker } from "../components/training/DevicePicker";
import { LiveRide } from "../components/training/LiveRide";
import { PendingRides } from "../components/training/PendingRides";
import { useTrainingPageShown } from "../components/training/useFinished";

/** One centred column, the title's and the content's alike (#315), built
 * as Settings builds its own (#325): the same width, the padding inside. */
const COLUMN = "max-w-[760px] mx-auto px-6";

/** The /training page (#273, macOS only): pick devices and connect, then
 * ride; a ride a crash left behind is offered first. The ride itself runs
 * in the backend, so leaving the page does not stop it. */
export function Training() {
  const t = useT();
  const live = useTrainingLive();
  useTrainingPageShown();
  return (
    <div className="flex flex-col h-full">
      {/* overflow-y-scroll, as Settings: the scrollbar's gutter always
          kept, so the column neither shifts when the page starts to
          scroll nor sits off Settings' edges. */}
      <main className="flex-1 overflow-y-scroll scroll-themed">
        <div className="sticky top-0 z-10 bg-bg pt-3 pb-2">
          <div className={COLUMN}>
            <h2 className="!m-0 text-lg font-bold">{t("training.title")}</h2>
          </div>
        </div>
        <div className={`${COLUMN} pb-6 pt-1 flex flex-col gap-4`}>
          {live === undefined ? null : live === null ? (
            <>
              <PendingRides />
              {/* Elsewhere the page is reached from Settings, for rides a
                  Mac left in the vault. */}
              {trainingSupported() ? (
                <>
                  <DevicePicker />
                  <AntKey />
                </>
              ) : (
                <p className="text-sm text-muted">{t("training.unsupported")}</p>
              )}
            </>
          ) : (
            <LiveRide live={live} />
          )}
        </div>
      </main>
    </div>
  );
}
