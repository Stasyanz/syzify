import { useNavigate } from "react-router";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { ask, open } from "@tauri-apps/plugin-dialog";
import { Puzzle, Plus, Trash2, Globe, ShieldCheck, Fingerprint } from "lucide-react";
import type { PluginInfo } from "../lib/types";
import { api } from "../lib/tauri";
import { useToastStore } from "../stores/toastStore";

// Human-readable label for a raw permission string.
function permissionLabel(perm: string): string {
  if (perm.startsWith("net:host=")) return `Network: ${perm.slice("net:host=".length)}`;
  const map: Record<string, string> = {
    "read:activities": "Read activities",
    "read:trackpoints": "Read track points",
    "read:hrv": "Read HRV",
    "read:laps": "Read laps",
    "read:dashboard": "Read dashboard",
    "data:own": "Private storage",
  };
  return map[perm] ?? perm;
}

export function PluginsPage() {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const addToast = useToastStore((s) => s.addToast);

  const { data: plugins = [], isLoading } = useQuery({
    queryKey: ["plugins"],
    queryFn: () => api.getPlugins(),
  });

  // Invalidate the registry list *and* the contribution/render caches, so host
  // pages (Dashboard, activity detail) pick up an install/enable/uninstall on
  // next mount instead of only after a full reload.
  const refresh = () => {
    queryClient.invalidateQueries({ queryKey: ["plugins"] });
    queryClient.invalidateQueries({ queryKey: ["pluginContributions"] });
    queryClient.invalidateQueries({ queryKey: ["pluginView"] });
  };

  async function handleInstall() {
    const path = await open({
      multiple: false,
      filters: [
        { name: "Plugin", extensions: ["syzify-ext", "json"] },
        { name: "Signed package", extensions: ["syzify-ext"] },
        { name: "Manifest (unsigned)", extensions: ["json"] },
      ],
    });
    if (!path || typeof path !== "string") return;
    try {
      // A signed package is verified; a bare manifest is an unsigned dev sideload.
      const info = path.endsWith(".syzify-ext")
        ? await api.installPluginFromPackage(path)
        : await api.installPluginFromFile(path);
      await refresh();
      addToast("success", `Installed “${info.name}”. Review its access, then enable it.`);
    } catch (e) {
      addToast("error", `Install failed: ${String(e)}`);
    }
  }

  async function handleToggle(plugin: PluginInfo) {
    try {
      await api.setPluginEnabled(plugin.id, !plugin.enabled);
      await refresh();
    } catch (e) {
      addToast("error", `Could not ${plugin.enabled ? "disable" : "enable"}: ${String(e)}`);
    }
  }

  async function handleUninstall(plugin: PluginInfo) {
    // NOT window.confirm: Tauri's shim returns a Promise (always truthy),
    // so the uninstall ran before the user answered.
    const ok = await ask(
      `Uninstall “${plugin.name}”? Its stored data will be removed too.`,
      { title: "Uninstall plugin", kind: "warning" }
    );
    if (!ok) return;
    try {
      await api.uninstallPlugin(plugin.id);
      await refresh();
      addToast("success", `Uninstalled “${plugin.name}”.`);
    } catch (e) {
      addToast("error", `Uninstall failed: ${String(e)}`);
    }
  }

  return (
    <div className="h-full overflow-y-auto">
      <div className="max-w-2xl mx-auto p-6 space-y-6">
        <div>
          <button
            onClick={() => navigate("/settings")}
            className="text-sm text-faint hover:text-muted mb-2 inline-block"
          >
            &larr; Back to settings
          </button>
          <div className="flex items-center justify-between">
            <h1 className="text-2xl font-bold text-ink flex items-center gap-2">
              <Puzzle size={22} className="text-faint" />
              Plugins
            </h1>
            <button
              onClick={handleInstall}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded bg-accent text-white text-sm font-medium hover:bg-accent-2"
            >
              <Plus size={15} />
              Install plugin
            </button>
          </div>
          <p className="text-xs text-faint mt-2">
            Plugins extend Syzify locally. They run with only the access you grant and
            are disabled until you turn them on. No plugin can reach the network unless
            its requested hosts are shown below.
          </p>
        </div>

        {isLoading ? (
          <p className="text-sm text-faint">Loading…</p>
        ) : plugins.length === 0 ? (
          <div className="border border-dashed border-border rounded-lg p-8 text-center">
            <Puzzle size={28} className="text-faint mx-auto mb-2" />
            <p className="text-sm text-muted">No plugins installed yet.</p>
            <p className="text-xs text-faint mt-1">
              Install a <code className="font-mono">plugin.json</code> manifest to get started.
            </p>
          </div>
        ) : (
          <ul className="space-y-3">
            {plugins.map((p) => (
              <li key={p.id} className="bg-card-2 rounded-lg p-4 space-y-3">
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0">
                    <div className="flex items-center gap-2">
                      <span className="font-semibold text-ink truncate">{p.name}</span>
                      <span className="text-xs text-faint">v{p.version}</span>
                      {p.enabled ? (
                        <span className="text-[10px] uppercase tracking-wide font-semibold text-green-700 bg-green-100 px-1.5 py-0.5 rounded">
                          Enabled
                        </span>
                      ) : (
                        <span className="text-[10px] uppercase tracking-wide font-semibold text-muted bg-border px-1.5 py-0.5 rounded">
                          Disabled
                        </span>
                      )}
                      {p.signed ? (
                        <span
                          title={`Self-signed package — integrity verified, but the author is self-asserted (not vetted). Key ${p.key_fingerprint ?? "?"}`}
                          className="text-[10px] uppercase tracking-wide font-semibold text-muted flex items-center gap-0.5"
                        >
                          <Fingerprint size={11} /> Self-signed
                          {p.key_fingerprint && (
                            <span className="font-mono normal-case text-faint">· {p.key_fingerprint.slice(0, 8)}</span>
                          )}
                        </span>
                      ) : (
                        <span
                          title="Unsigned dev sideload — integrity not verified"
                          className="text-[10px] uppercase tracking-wide font-semibold text-amber-600"
                        >
                          Unsigned
                        </span>
                      )}
                    </div>
                    {p.author && <p className="text-xs text-faint mt-0.5">by {p.author}</p>}
                    {p.description && (
                      <p className="text-sm text-muted mt-1">{p.description}</p>
                    )}
                  </div>
                  <div className="flex items-center gap-2 shrink-0">
                    {p.enabled && p.contributes.includes("route.planner") && (
                      <button
                        onClick={() => navigate(`/plugin/${p.id}`)}
                        className="px-3 py-1 rounded text-xs font-medium border border-border-2 text-ink hover:bg-card-2"
                      >
                        Open
                      </button>
                    )}
                    <button
                      onClick={() => handleToggle(p)}
                      className={`px-3 py-1 rounded text-xs font-medium ${
                        p.enabled
                          ? "border border-border-2 text-muted hover:bg-card-2"
                          : "bg-accent text-accent-ink hover:bg-accent-2"
                      }`}
                    >
                      {p.enabled ? "Disable" : "Enable"}
                    </button>
                    <button
                      onClick={() => handleUninstall(p)}
                      title="Uninstall"
                      className="p-1.5 rounded text-faint hover:text-red-600 hover:bg-red-50"
                    >
                      <Trash2 size={15} />
                    </button>
                  </div>
                </div>

                {p.permissions.length > 0 && (
                  <div className="flex items-start gap-2">
                    <ShieldCheck size={14} className="text-faint mt-0.5 shrink-0" />
                    <div className="flex flex-wrap gap-1.5">
                      {p.permissions.map((perm) => (
                        <span
                          key={perm}
                          className="text-[11px] text-muted bg-card border border-border px-1.5 py-0.5 rounded"
                        >
                          {permissionLabel(perm)}
                        </span>
                      ))}
                    </div>
                  </div>
                )}

                {p.network_hosts.length > 0 && (
                  <div className="flex items-start gap-2 text-amber-700 bg-amber-50 border border-amber-200 rounded px-2 py-1.5">
                    <Globe size={14} className="mt-0.5 shrink-0" />
                    <div className="text-xs">
                      <span className="font-medium">Connects to: </span>
                      {p.network_hosts.join(", ")}
                    </div>
                  </div>
                )}
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}
