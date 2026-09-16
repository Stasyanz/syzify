import { describeDevice } from "../../lib/devices";
import { DeviceSilhouette } from "../brand/DeviceSilhouette";

/**
 * The recording device, as a chip for the activity header: a silhouette of
 * its form factor and the model label ("fenix 6X", "Edge 840"); hover gives
 * the full name. Nothing for an activity whose file named no device.
 */
export function DeviceChip({ source }: { source: string | null | undefined }) {
  const device = describeDevice(source);
  if (!device) return null;
  // The tooltip is a CSS pseudo-element (data-tip), invisible to assistive
  // technology; the aria-label carries the same sentence for it.
  const tip = `Recorded on ${device.full_name}`;
  return (
    <div className="device-chip" data-tip={tip} aria-label={tip} data-testid="device-chip">
      <DeviceSilhouette form={device.form} />
      <span>{device.label}</span>
    </div>
  );
}
