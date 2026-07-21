import { useQuery } from "@tanstack/react-query";
import { api } from "../../lib/tauri";
import { PluginWidget } from "./PluginWidget";

// Renders every enabled plugin contributing to `point`, each as an isolated
// widget fed `context` (an opaque JSON string). Renders nothing when there are
// no contributions, so host pages are unchanged without plugins.
export function PluginContributions({
  point,
  context = "{}",
  className = "grid grid-cols-1 lg:grid-cols-2 gap-6",
}: {
  point: string;
  context?: string;
  className?: string;
}) {
  const { data: contributions = [] } = useQuery({
    queryKey: ["pluginContributions", point],
    queryFn: () => api.getPluginContributions(point),
  });

  if (contributions.length === 0) return null;

  return (
    <div className={className}>
      {contributions.map((c) => (
        <PluginWidget
          key={c.plugin_id}
          pluginId={c.plugin_id}
          name={c.name}
          point={point}
          context={context}
        />
      ))}
    </div>
  );
}
