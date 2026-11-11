import { useEffect, useState } from "react";
import { api } from "../lib/tauri";
import type { TrainingLive } from "../lib/types";

/** The ride's live values, a few times a second; null when it is over. */
export const TRAINING_LIVE_EVENT = "training:live";

/**
 * The training session as it is now: `undefined` until known, `null` when
 * no session runs. Listens to `training:live` first, then asks what the
 * backend last showed (the screen may come back to a ride under way): a
 * session that ends in between is still heard. An event wins over the
 * slower answer.
 */
export function useTrainingLive(): TrainingLive | null | undefined {
  const [live, setLive] = useState<TrainingLive | null | undefined>(undefined);
  useEffect(() => {
    let cancelled = false;
    let heard = false;
    let unlisten: (() => void) | undefined;
    const ask = () => {
      if (cancelled) return;
      api
        .trainingLive()
        .then((first) => {
          if (!cancelled && !heard) setLive(first);
        })
        .catch(() => {
          if (!cancelled && !heard) setLive(null);
        });
    };
    import("@tauri-apps/api/event")
      .then(({ listen }) => {
        if (cancelled) return;
        return listen<TrainingLive | null>(TRAINING_LIVE_EVENT, (event) => {
          heard = true;
          if (!cancelled) setLive(event.payload);
        }).then((fn) => {
          // Cleaned up while subscribing: let go at once.
          if (cancelled) fn();
          else unlisten = fn;
        });
      })
      .catch(() => {
        // Outside Tauri (tests, a browser preview) there is no event bus.
      })
      .finally(ask);
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);
  return live;
}
