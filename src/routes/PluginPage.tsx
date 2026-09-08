import { useNavigate, useParams } from "react-router";
import { useQuery } from "@tanstack/react-query";
import { ArrowLeft } from "lucide-react";
import { api } from "../lib/tauri";
import { PluginWidget } from "../components/plugins/PluginWidget";

/** The full-page contribution points and the export each maps to. */
export type PagePoint = "route.planner" | "sync.source";

// Full-page host for a plugin's `route.planner` or `sync.source` contribution.
// The plugin's output is rendered with the same safe ViewSpec primitives as a
// widget, just full-width. Only the sync page runs the continue loop: a
// planner the user approved as a page must not gain rounds of network time
// without asking.
export function PluginPage({ point = "route.planner" }: { point?: PagePoint }) {
  const { pluginId } = useParams<{ pluginId: string }>();
  const navigate = useNavigate();

  const { data: plugins = [] } = useQuery({
    queryKey: ["plugins"],
    queryFn: () => api.getPlugins(),
  });
  const plugin = plugins.find((p) => p.id === pluginId);

  return (
    <div className="h-full overflow-y-auto">
      <div className="max-w-3xl mx-auto p-6 space-y-4">
        <button
          onClick={() => navigate("/plugins")}
          className="flex items-center gap-1 text-sm text-faint hover:text-muted"
        >
          <ArrowLeft size={14} /> Plugins
        </button>
        <h1 className="text-2xl font-bold text-ink">
          {plugin?.name ?? "Plugin"}
          {point === "sync.source" && <span className="text-faint font-normal"> · Sync</span>}
        </h1>
        {pluginId && (
          <PluginWidget
            pluginId={pluginId}
            name={plugin?.name ?? pluginId}
            point={point}
            context="{}"
            followContinue={point === "sync.source"}
          />
        )}
      </div>
    </div>
  );
}
