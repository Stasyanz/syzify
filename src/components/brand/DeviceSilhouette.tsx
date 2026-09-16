import type { ReactElement } from "react";
import type { DeviceForm } from "../../lib/devices";

// Device silhouettes, one per form factor — our own line drawings, not vendor
// renders (those are copyrighted and cannot ship in an AGPL app, and fetching
// them at view time would be a network call nobody opted into). The drawing
// says "rugged watch" / "bike computer"; the label next to it says which one.
// 32×32 viewBox, 1.6 px strokes in currentColor, buttons filled and short
// (they sit on the case stroke and stick out well under a pixel; longer ones
// read as spikes).

const STROKE = { fill: "none", stroke: "currentColor", strokeWidth: 1.6, strokeLinejoin: "round" as const };

/** Watch straps above and below a round or square case. */
function Straps() {
  return (
    <>
      <rect x="11" y="1.5" width="10" height="5.5" rx="1.6" {...STROKE} />
      <rect x="11" y="25" width="10" height="5.5" rx="1.6" {...STROKE} />
    </>
  );
}

/** The five-button layout of the rugged multisport line (three left, two right). */
function FiveButtons() {
  return (
    <g fill="currentColor">
      <rect x="5.6" y="9.6" width="1.2" height="2.4" rx="0.4" />
      <rect x="5.3" y="14.8" width="1.4" height="2.4" rx="0.4" />
      <rect x="5.6" y="20" width="1.2" height="2.4" rx="0.4" />
      <rect x="25.2" y="10.2" width="1.2" height="2.4" rx="0.4" />
      <rect x="25.2" y="19.4" width="1.2" height="2.4" rx="0.4" />
    </g>
  );
}

const SHAPES: Record<DeviceForm, () => ReactElement> = {
  // Chunky bezel + dial, five buttons: fenix, epix, Enduro, MARQ, Descent.
  watch_multi: () => (
    <>
      <Straps />
      <circle cx="16" cy="16" r="9.6" {...STROKE} />
      <circle cx="16" cy="16" r="6.4" {...STROKE} />
      <FiveButtons />
    </>
  ),
  // Thin bezel, a dial nearly the size of the case, two buttons right one left.
  watch_round: () => (
    <>
      <Straps />
      <circle cx="16" cy="16" r="9.6" {...STROKE} />
      <circle cx="16" cy="16" r="7.6" {...STROKE} />
      <g fill="currentColor">
        <rect x="25.3" y="10.8" width="1.1" height="2.6" rx="0.4" />
        <rect x="25.3" y="18.6" width="1.1" height="2.6" rx="0.4" />
        <rect x="5.6" y="14.7" width="1.1" height="2.6" rx="0.4" />
      </g>
    </>
  ),
  // The Instinct's sub-dial window in the top-right of the face.
  watch_instinct: () => (
    <>
      <Straps />
      <circle cx="16" cy="16" r="9.6" {...STROKE} />
      <circle cx="16" cy="16" r="7" {...STROKE} />
      <circle cx="19.4" cy="12.6" r="2.3" {...STROKE} />
      <FiveButtons />
    </>
  ),
  // Square case with a screen and a crown: Venu Sq, bands, Apple Watch.
  watch_rect: () => (
    <>
      <Straps />
      <rect x="8.5" y="7.8" width="15" height="16.4" rx="3.6" {...STROKE} />
      <rect x="11.2" y="10.6" width="9.6" height="10.8" rx="1.4" {...STROKE} />
      <rect x="23.6" y="13.4" width="1.1" height="4.2" rx="0.4" fill="currentColor" />
    </>
  ),
  // Portrait head unit, screen inset, two buttons along the bottom edge.
  bike_computer: () => (
    <>
      <rect x="9" y="3.5" width="14" height="22" rx="3" {...STROKE} />
      <rect x="11.6" y="6.2" width="8.8" height="13.2" rx="1.2" {...STROKE} />
      <g fill="currentColor">
        <rect x="10.8" y="25.8" width="4.2" height="1.4" rx="0.6" />
        <rect x="17" y="25.8" width="4.2" height="1.4" rx="0.6" />
      </g>
    </>
  ),
  // A gadget with a screen and a button: sensors, handhelds, apps, unknown.
  generic: () => (
    <>
      <rect x="4.5" y="9" width="23" height="14" rx="3.2" {...STROKE} />
      <rect x="7.6" y="12" width="12" height="8" rx="1.2" {...STROKE} />
      <circle cx="23.4" cy="16" r="1.7" fill="currentColor" />
    </>
  ),
};

/** Line silhouette of a device form factor; inherits its color from the parent. */
export function DeviceSilhouette({ form, size = 28 }: { form: DeviceForm; size?: number }) {
  const Shape = SHAPES[form] ?? SHAPES.generic;
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 32 32"
      style={{ display: "block" }}
      aria-hidden="true"
      data-form={form}
    >
      <Shape />
    </svg>
  );
}
