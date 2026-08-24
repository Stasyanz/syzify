import { useEffect, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { X } from "lucide-react";
import { api } from "../../lib/tauri";

/** Setting row marking the first-run onboarding as passed ("1"). Delete the
 * row (or the whole vault) to see the overlay again — the testing hook. */
export const ONBOARDING_KEY = "onboarding_done";

/**
 * First-run gate, exported pure for tests: the overlay shows only while the
 * done-flag is absent AND the library is empty — an existing install
 * (activities present) must never see it, and either closing the overlay or
 * landing the first import writes the flag. `undefined` inputs mean the
 * queries haven't resolved yet — don't show anything on a loading flicker.
 *
 * An explicit "0" is the tester's override: it forces the overlay even over
 * a lived-in library (nothing in the app ever writes "0" — set it by hand
 * in the setting table to re-run onboarding).
 */
export function shouldShowOnboarding(
  done: string | null | undefined,
  activityCount: number | undefined,
): boolean {
  if (done === "0") return true;
  return done !== undefined && done !== "1" && activityCount === 0;
}

/**
 * Curved guide arrow (quadratic bezier) from the onboarding card toward the
 * import button, bowing sideways so it reads as hand-drawn rather than a
 * straight connector. Exported pure for tests.
 */
export function arrowPath(x1: number, y1: number, x2: number, y2: number): string {
  // Control point: midpoint pushed perpendicular to the line by ~25% of its
  // length — a consistent bow whatever the card/button geometry.
  const mx = (x1 + x2) / 2;
  const my = (y1 + y2) / 2;
  const dx = x2 - x1;
  const dy = y2 - y1;
  const cx = mx - dy * 0.25;
  const cy = my + dx * 0.25;
  return `M ${x1} ${y1} Q ${cx} ${cy} ${x2} ${y2}`;
}

/**
 * First-run onboarding: a spotlight overlay pointing the user at workout
 * import. Deliberately NON-blocking — the dim is a huge box-shadow around
 * the import button's cutout and every control underneath stays clickable,
 * so the user can follow the arrow and click Import right through it. The
 * first successful import (count flips >0) completes onboarding silently;
 * so does the X / "Got it".
 */
export function OnboardingOverlay() {
  const { data: done } = useQuery({
    queryKey: ["setting", ONBOARDING_KEY],
    queryFn: () => api.getSetting(ONBOARDING_KEY),
  });
  // Probe query: one row is enough to tell an empty library from a lived-in
  // one, and the shared "activities" key prefix means any import invalidates
  // it (invalidateActivityData), flipping the overlay off automatically.
  const { data: probe } = useQuery({
    queryKey: ["activities", { limit: 1, probe: "onboarding" }],
    queryFn: () => api.getActivities({ limit: 1 }),
  });
  const [dismissed, setDismissed] = useState(false);

  const count = probe?.length;
  const show = !dismissed && shouldShowOnboarding(done, count);

  // Whichever way onboarding ends — Got it, X, or the first import landing
  // while the overlay is up — persist the flag exactly once.
  const wrote = useRef(false);
  const finish = () => {
    setDismissed(true);
    if (!wrote.current) {
      wrote.current = true;
      api.setSetting(ONBOARDING_KEY, "1").catch(() => {});
    }
  };
  // `show` itself flips false the moment count goes >0, so completion-by-
  // import needs its own trace of "the overlay was up over an EMPTY
  // library" — over an empty one only, or the forced-"0" test mode would
  // self-complete instantly from the activities already present.
  const wasShownOnEmpty = useRef(false);
  if (show && count === 0) wasShownOnEmpty.current = true;
  const importLanded =
    wasShownOnEmpty.current && !dismissed && count != null && count > 0;
  useEffect(() => {
    if (importLanded) finish();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [importLanded]);

  // The import button's live position anchors the spotlight and the arrow;
  // re-measured on resize (the navbar is sticky, so scroll can't move it).
  const [target, setTarget] = useState<DOMRect | null>(null);
  useEffect(() => {
    if (!show) return;
    const measure = () =>
      setTarget(
        document.getElementById("import-workouts-btn")?.getBoundingClientRect() ?? null,
      );
    measure();
    window.addEventListener("resize", measure);
    return () => window.removeEventListener("resize", measure);
  }, [show]);

  if (!show) return null;

  // Arrow from the card's top edge to just under the spotlight ring.
  const arrow =
    target &&
    arrowPath(
      window.innerWidth / 2 + 40,
      window.innerHeight / 2 - 120,
      target.left + target.width / 2,
      target.bottom + 14,
    );

  return (
    <div className="fixed inset-0 z-[2000] pointer-events-none">
      {/* The dim IS the shadow of the spotlight cutout — no backdrop layer,
          so everything underneath (navbar, import button) stays clickable. */}
      {target && (
        <div
          className="absolute rounded-full"
          style={{
            left: target.left - 7,
            top: target.top - 7,
            width: target.width + 14,
            height: target.height + 14,
            boxShadow: "0 0 0 9999px rgba(0, 0, 0, 0.5)",
          }}
        />
      )}
      {!target && <div className="absolute inset-0 bg-black/50" />}

      {arrow && (
        <svg className="absolute inset-0 h-full w-full">
          <defs>
            <marker
              id="onboarding-arrowhead"
              markerWidth="8"
              markerHeight="8"
              refX="6"
              refY="4"
              orient="auto"
            >
              <path d="M 1 1 L 7 4 L 1 7" fill="none" stroke="#fff" strokeWidth="1.5" />
            </marker>
          </defs>
          <path
            d={arrow}
            fill="none"
            stroke="#fff"
            strokeWidth="2.5"
            strokeLinecap="round"
            strokeDasharray="2 9"
            markerEnd="url(#onboarding-arrowhead)"
          />
        </svg>
      )}

      <div
        className="dash-card pointer-events-auto absolute left-1/2 top-1/2 w-[380px] max-w-[90vw] -translate-x-1/2 -translate-y-1/2 shadow-2xl"
        role="dialog"
        aria-label="Getting started"
      >
        <button
          onClick={finish}
          aria-label="Close"
          className="absolute right-3 top-3 text-faint hover:text-ink"
        >
          <X size={18} />
        </button>
        <h2 className="!mb-2 text-lg font-bold">Import your first workout</h2>
        <p className="mb-5 text-sm leading-relaxed text-muted">
          Drag &amp; drop GPX, FIT or TCX files anywhere in this window — or click
          the Import button in the top-right corner.
        </p>
        <button
          onClick={finish}
          className="rounded-lg bg-accent px-4 py-2 font-medium text-white transition-colors hover:bg-accent-2"
        >
          Got it
        </button>
      </div>
    </div>
  );
}
