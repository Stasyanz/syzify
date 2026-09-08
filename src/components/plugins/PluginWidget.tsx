import { useEffect, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Puzzle } from "lucide-react";
import type { ViewSpec } from "../../lib/types";
import { api } from "../../lib/tauri";
import { PluginViewRenderer } from "./PluginViewRenderer";

// Renders one plugin's contribution at `point`, isolated: a plugin that errors
// shows an inline error card and never takes down the surrounding page.
//
// Interactivity: input/select values are tracked here; pressing a button
// re-invokes the plugin with `{ action, values, ...context }` and swaps in the
// returned ViewSpec — no backend changes needed, the context is opaque.
//
// The continue loop (`followContinue`, the sync page only): a view that came
// back from a button and carries `continue` is shown, then the plugin is
// called again with that action — round after round until a view without it,
// an error, Stop, the round cap, or the page going away. The initial render's
// `continue` is ignored: no page opens into a running loop. One chain at a
// time: a button press while a loop runs starts a new generation, and the old
// chain's answers are dropped (two chains would read and write the plugin's
// cursor in turns — a duplicate import and a skipped step).

/** Rounds one button press may run before the host stops the loop: a plugin
 * must not turn one click into unbounded network time on a page left open.
 * The user presses the button again to go on. */
export const MAX_CONTINUE_ROUNDS = 200;
export function PluginWidget({
  pluginId,
  name,
  point,
  context = "{}",
  followContinue = false,
}: {
  pluginId: string;
  name: string;
  point: string;
  context?: string;
  followContinue?: boolean;
}) {
  const { data: initial, error, isLoading } = useQuery({
    queryKey: ["pluginView", pluginId, point, context],
    queryFn: () => api.renderPluginView(pluginId, point, context),
    retry: false,
  });

  const [liveSpec, setLiveSpec] = useState<ViewSpec | null>(null);
  const [values, setValues] = useState<Record<string, string>>({});
  const [actionError, setActionError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  // A loop is running: how many rounds so far, and the Stop button.
  const [rounds, setRounds] = useState(0);
  const [continuing, setContinuing] = useState(false);
  const [capped, setCapped] = useState(false);
  const stopRef = useRef(false);
  const aliveRef = useRef(true);
  // The generation of the running chain; an answer from an older one is dropped.
  const runRef = useRef(0);
  // The values a round sends, read at call time (an effect, not a render:
  // a discarded concurrent render must not write the ref).
  const valuesRef = useRef(values);
  useEffect(() => {
    valuesRef.current = values;
  }, [values]);

  useEffect(() => {
    aliveRef.current = true;
    return () => {
      aliveRef.current = false;
    };
  }, []);

  const spec = liveSpec ?? initial;

  // Seed input/select defaults from the current spec, preserving user edits.
  useEffect(() => {
    if (!spec) return;
    const defaults: Record<string, string> = {};
    for (const el of spec.elements) {
      if (el.type === "input" || el.type === "select") defaults[el.id] = el.value;
    }
    setValues((v) => ({ ...defaults, ...v }));
  }, [spec]);

  async function invoke(action: string, generation: number, round: number) {
    const current = () => aliveRef.current && runRef.current === generation;
    setActionError(null);
    setBusy(true);
    try {
      const base = JSON.parse(context || "{}");
      const next = await api.renderPluginView(
        pluginId,
        point,
        JSON.stringify({ ...base, action, values: valuesRef.current })
      );
      if (!current()) return;
      setLiveSpec(next);
      const again = followContinue && !stopRef.current ? (next.continue ?? null) : null;
      if (again && round >= MAX_CONTINUE_ROUNDS) {
        setCapped(true);
        setContinuing(false);
      } else if (again) {
        setContinuing(true);
        setRounds(round + 1);
        // Let the progress paint before the next round.
        setTimeout(() => {
          if (current() && !stopRef.current) void invoke(again, generation, round + 1);
        }, 0);
      } else {
        setContinuing(false);
      }
    } catch (e) {
      if (!current()) return;
      setActionError(String(e));
      setContinuing(false);
    } finally {
      if (current()) setBusy(false);
    }
  }

  function onAction(action: string) {
    runRef.current += 1;
    stopRef.current = false;
    setRounds(0);
    setCapped(false);
    setContinuing(false);
    void invoke(action, runRef.current, 0);
  }

  function onStop() {
    stopRef.current = true;
    setContinuing(false);
  }

  const onChange = (id: string, value: string) =>
    setValues((v) => ({ ...v, [id]: value }));

  return (
    <div className="bg-card-2 rounded-lg p-4">
      <div className="flex items-center gap-1.5 mb-2">
        <Puzzle size={12} className="text-faint" />
        <span className="text-[10px] uppercase tracking-wider text-faint">{name}</span>
      </div>
      {isLoading ? (
        <p className="text-xs text-faint">Loading…</p>
      ) : error ? (
        <p className="text-xs text-red-600">Plugin error: {String(error)}</p>
      ) : spec ? (
        <>
          <PluginViewRenderer
            spec={spec}
            values={values}
            onChange={onChange}
            onAction={onAction}
            disabled={busy || continuing}
          />
          {(busy || continuing) && (
            <div className="flex items-center gap-3 mt-2">
              <p className="text-xs text-faint">
                {continuing ? `Working… round ${rounds}` : "Working…"}
              </p>
              {/* On the sync page the first round may take the whole budget:
                  Stop is there from the start. */}
              {(continuing || (busy && followContinue)) && (
                <button
                  onClick={onStop}
                  className="px-2 py-0.5 rounded text-xs font-medium border border-border-2 text-muted hover:bg-card"
                >
                  Stop
                </button>
              )}
            </div>
          )}
          {capped && (
            <p className="text-xs text-faint mt-2">
              Stopped after {MAX_CONTINUE_ROUNDS} rounds — press the button again to go on.
            </p>
          )}
          {actionError && (
            <p className="text-xs text-red-600 mt-2">Plugin error: {actionError}</p>
          )}
        </>
      ) : null}
    </div>
  );
}
