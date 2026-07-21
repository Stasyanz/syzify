import { useEffect, useState } from "react";
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
export function PluginWidget({
  pluginId,
  name,
  point,
  context = "{}",
}: {
  pluginId: string;
  name: string;
  point: string;
  context?: string;
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

  async function onAction(action: string) {
    setActionError(null);
    setBusy(true);
    try {
      const base = JSON.parse(context || "{}");
      const next = await api.renderPluginView(
        pluginId,
        point,
        JSON.stringify({ ...base, action, values })
      );
      setLiveSpec(next);
    } catch (e) {
      setActionError(String(e));
    } finally {
      setBusy(false);
    }
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
          />
          {busy && <p className="text-xs text-faint mt-2">Working…</p>}
          {actionError && (
            <p className="text-xs text-red-600 mt-2">Plugin error: {actionError}</p>
          )}
        </>
      ) : null}
    </div>
  );
}
