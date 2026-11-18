import { useState, type ReactNode } from "react";
import { useMutation } from "@tanstack/react-query";
import { Ban, Bike, Bluetooth, Check, HeartPulse, Usb, Zap, type LucideIcon } from "lucide-react";
import { api } from "../../lib/tauri";
import { errorText } from "../../lib/errors";
import { useT } from "../../lib/i18n";
import { ROLES, devicesFor, pedalAddresses } from "../../lib/training";
import type { FoundDevice, SensorRole, StickProblem } from "../../lib/types";

type Picks = Record<SensorRole, string | null>;

const NONE: Picks = { trainer: null, pedals: null, strap: null };

const ROLE_ICONS: Record<SensorRole, LucideIcon> = { trainer: Bike, pedals: Zap, strap: HeartPulse };

/** One choice of a role as a tile of its own, the way Zwift lays devices
 * out (#315). A native radio covers the tile, unseen: the arrow keys,
 * VoiceOver (its cursor on the whole tile) and a click anywhere on it work
 * as on a plain radio. Keyboard focus draws an outline set off the tile, so
 * it is not taken for the picked tile's border, as Checkbox does. */
function Tile({
  name,
  checked,
  onPick,
  icon: Icon,
  none = false,
  children,
}: {
  name: SensorRole;
  checked: boolean;
  onPick: () => void;
  icon: LucideIcon;
  /** "None": drawn lighter than a device, its icon never the accent's (a
   * no-entry sign in the accent reads as "forbidden"). */
  none?: boolean;
  children: ReactNode;
}) {
  const look = checked
    ? "border-accent bg-accent-soft"
    : `${none ? "border-dashed text-muted" : ""} border-border-2 hover:bg-card-2`;
  return (
    <label
      className={`relative flex flex-col gap-1.5 min-h-24 p-3 rounded-card border cursor-pointer transition-colors has-[:focus-visible]:outline-2 has-[:focus-visible]:outline-offset-2 has-[:focus-visible]:outline-accent ${look}`}
    >
      <input
        type="radio"
        className="absolute inset-0 m-0 opacity-0 appearance-none cursor-pointer"
        name={name}
        checked={checked}
        onChange={onPick}
      />
      {checked && <Check size={16} className="absolute top-3 right-3 text-accent" aria-hidden />}
      <Icon size={20} className={checked && !none ? "text-accent" : "text-muted"} aria-hidden />
      {children}
    </label>
  );
}

/** Search for devices, pick a trainer (and pedals and a strap, if there are
 * any), connect. */
export function DevicePicker() {
  const t = useT();
  const [found, setFound] = useState<FoundDevice[] | null>(null);
  const [stick, setStick] = useState<StickProblem | null>(null);
  const [picks, setPicks] = useState<Picks>(NONE);

  const scan = useMutation({
    mutationFn: () => api.trainingScan(),
    onSuccess: ({ devices, stick }) => {
      setFound(devices);
      setStick(stick);
      // The first of each role is the likely one; pedals and a strap stay
      // a choice (a watch's strap may be someone else's).
      setPicks({ ...NONE, trainer: devicesFor(devices, "trainer")[0]?.address ?? null });
    },
  });
  const connect = useMutation({
    mutationFn: (trainer: string) => api.trainingConnect(trainer, pedalAddresses(found, picks.pedals), picks.strap),
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
      {stick && (
        <p className="mt-2 text-sm px-3 py-2 rounded-field bg-warn-soft">
          {stick.kind === "in_use" ? t("training.setup.stickInUse") : t("training.setup.stickOther", { detail: stick.detail })}
        </p>
      )}

      {found && found.length === 0 && <p className="mt-3 text-sm text-muted">{t("training.setup.nothing")}</p>}

      {found && found.length > 0 && (
        <div className="mt-3 flex flex-col gap-3">
          {ROLES.map((role) => {
            const devices = devicesFor(found, role);
            return (
              <fieldset key={role} className="flex flex-col">
                <legend className="text-xs uppercase tracking-wide text-faint mb-2">{roleLabel[role]}</legend>
                {devices.length === 0 && role === "trainer" && (
                  <p className="text-sm text-muted">{t("training.setup.noTrainer")}</p>
                )}
                <div className="grid grid-cols-[repeat(auto-fill,minmax(11rem,1fr))] gap-3">
                  {devices.map((d) => {
                    const ChannelIcon = d.radio === "ant" ? Usb : Bluetooth;
                    return (
                      <Tile
                        key={d.address}
                        name={role}
                        icon={ROLE_ICONS[role]}
                        checked={picks[role] === d.address}
                        onPick={() => setPicks({ ...picks, [role]: d.address })}
                      >
                        <span className="text-sm font-medium break-words">
                          {d.name ? <span translate="no">{d.name}</span> : t("training.setup.unnamed")}
                        </span>
                        {/* The same trainer may be found both ways: the user picks. */}
                        <span className="inline-flex items-center gap-1 text-xs text-muted">
                          <ChannelIcon size={12} aria-hidden />
                          {d.radio === "ant" ? t("training.setup.viaAnt") : t("training.setup.viaBle")}
                        </span>
                        {d.other_side && <span className="text-xs text-muted">{t("training.setup.bothPedals")}</span>}
                        {/* Picked alone it records one leg: say so plainly. (Not
                            searched for by name: a one-sided pedal may be named
                            the same way and would get no power at all.) */}
                        {d.lone_side && (
                          <span className="text-xs px-2 py-0.5 rounded-field bg-warn-soft">{t("training.setup.lonePedal")}</span>
                        )}
                      </Tile>
                    );
                  })}
                  {role !== "trainer" && (
                    <Tile
                      name={role}
                      icon={Ban}
                      none
                      checked={picks[role] == null}
                      onPick={() => setPicks({ ...picks, [role]: null })}
                    >
                      <span className="text-sm font-medium">{t("training.setup.none")}</span>
                    </Tile>
                  )}
                </div>
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
