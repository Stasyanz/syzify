import { useNavigate, useParams } from "react-router";
import { useQuery } from "@tanstack/react-query";
import { ArrowLeft } from "lucide-react";
import { api } from "../lib/tauri";
import { PluginWidget } from "../components/plugins/PluginWidget";

// Full-page host for a plugin's `route.planner` contribution. The plugin's
// output is rendered with the same safe ViewSpec primitives as a widget, just
// full-width.
export function PluginPage() {
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
        <h1 className="text-2xl font-bold text-ink">{plugin?.name ?? "Plugin"}</h1>
        {pluginId && (
          <PluginWidget
            pluginId={pluginId}
            name={plugin?.name ?? pluginId}
            point="route.planner"
            context="{}"
          />
        )}
      </div>
    </div>
  );
}
