import { useNavigate } from "react-router";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { confirmDialog } from "../../stores/confirmStore";
import { Puzzle, Plus, Trash2, Globe, ShieldCheck, Fingerprint } from "lucide-react";
import type { EncryptionStatus, PluginInfo } from "../../lib/types";
import { api } from "../../lib/tauri";
import { useToastStore } from "../../stores/toastStore";
import { errorText } from "../../lib/errors";
import { useRichT, useT, type Translator } from "../../lib/i18n";
import type { MessageKey } from "../../locales/types";

/** The message of each permission the host knows; an unknown one is shown
 * as the raw string the manifest asks for. */
const PERMISSION_LABELS = {
  "read:activities": "settings.plugins.perm.readActivities",
  "read:trackpoints": "settings.plugins.perm.readTrackpoints",
  "read:hrv": "settings.plugins.perm.readHrv",
  "read:laps": "settings.plugins.perm.readLaps",
  "read:dashboard": "settings.plugins.perm.readDashboard",
  "data:own": "settings.plugins.perm.dataOwn",
  "data:secret": "settings.plugins.perm.dataSecret",
  "import:files": "settings.plugins.perm.importFiles",
  "gear:write": "settings.plugins.perm.gearWrite",
} as const satisfies Record<string, MessageKey>;

/** Human-readable label for a raw permission string. Own keys only: a
 * manifest asking for "constructor" is shown as it asked. */
export function permissionLabel(perm: string, translate: Translator): string {
  if (perm.startsWith("net:host=")) {
    return translate("settings.plugins.perm.network", { host: perm.slice("net:host=".length) });
  }
  return Object.prototype.hasOwnProperty.call(PERMISSION_LABELS, perm)
    ? translate(PERMISSION_LABELS[perm as keyof typeof PERMISSION_LABELS])
    : perm;
}

/** The note under a plugin holding `data:secret` while the vault is not
 * encrypted: its tokens sit in the clear until encryption is on. `sealed`
 * is the host's own predicate — any encrypted scope, not the mere presence
 * of a lock — and `null` while it is still unknown (nothing is shown then). */
export function secretsDisclosure(permissions: string[], sealed: boolean | null, translate: Translator): string | null {
  if (sealed !== false || !permissions.includes("data:secret")) return null;
  return translate("settings.plugins.secretsUnencrypted");
}

/** Whether plugin secrets are sealed, from the encryption status: any scope
 * on. Unknown while loading; a failed status query counts as NOT sealed, so
 * the disclosure fails towards the warning. */
export function secretsSealed(status: EncryptionStatus | undefined, failed: boolean): boolean | null {
  if (status) return status.scopes.activities || status.scopes.database || status.scopes.photos;
  return failed ? false : null;
}

/** The plugin registry, as the Plugins tab of Settings: sideload, enable,
 * review permissions, open a plugin's page or sync. Plugins install disabled
 * and every network host they ask for is listed on their card. */
export function PluginRegistry() {
  const t = useT();
  const rt = useRichT();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const addToast = useToastStore((s) => s.addToast);

  const { data: plugins = [], isLoading } = useQuery({
    queryKey: ["plugins"],
    queryFn: () => api.getPlugins(),
  });
  // Whether a plugin's secrets are sealed: any encrypted scope seals them.
  const { data: encryption, isError: encryptionUnknown } = useQuery({
    queryKey: ["encryptionStatus"],
    queryFn: () => api.getEncryptionStatus(),
  });
  const sealed = secretsSealed(encryption, encryptionUnknown);

  // Invalidate the registry list *and* the contribution/render caches, so host
  // pages (Dashboard, activity detail) pick up an install/enable/uninstall on
  // next mount instead of only after a full reload.
  const refresh = () => {
    queryClient.invalidateQueries({ queryKey: ["plugins"] });
    queryClient.invalidateQueries({ queryKey: ["pluginContributions"] });
    queryClient.invalidateQueries({ queryKey: ["pluginView"] });
  };

  async function handleInstall() {
    // The dialog's own failure (a plugin error, an Error on the JS side)
    // is reported like a refused install, not left as an unhandled rejection.
    try {
      await installFromDialog();
    } catch (e) {
      addToast("error", t("settings.plugins.installFailed", { error: errorText(e) }));
    }
  }

  async function installFromDialog() {
    const path = await open({
      multiple: false,
      filters: [
        { name: t("plugins.page.fallbackName"), extensions: ["syzify-ext", "json"] },
        { name: t("settings.plugins.signedPackage"), extensions: ["syzify-ext"] },
        { name: t("settings.plugins.unsignedManifest"), extensions: ["json"] },
      ],
    });
    if (!path || typeof path !== "string") return;
    // A signed package is verified; a bare manifest is an unsigned dev sideload.
    const info = path.endsWith(".syzify-ext")
      ? await api.installPluginFromPackage(path)
      : await api.installPluginFromFile(path);
    await refresh();
    addToast("success", t("settings.plugins.installed", { name: info.name }));
  }

  async function handleToggle(plugin: PluginInfo) {
    try {
      await api.setPluginEnabled(plugin.id, !plugin.enabled);
      await refresh();
    } catch (e) {
      addToast(
        "error",
        t(plugin.enabled ? "settings.plugins.disableFailed" : "settings.plugins.enableFailed", {
          error: errorText(e),
        }),
      );
    }
  }

  async function handleUninstall(plugin: PluginInfo) {
    const ok = await confirmDialog({
      title: t("settings.plugins.uninstallTitle"),
      message: t("settings.plugins.uninstallMessage", { name: plugin.name }),
      confirmLabel: t("settings.plugins.uninstall"),
      danger: true,
    });
    if (!ok) return;
    try {
      await api.uninstallPlugin(plugin.id);
      await refresh();
      addToast("success", t("settings.plugins.uninstalled", { name: plugin.name }));
    } catch (e) {
      addToast("error", t("settings.plugins.uninstallFailed", { error: errorText(e) }));
    }
  }

  return (
    <section className="card">
      <div className="flex items-center justify-between gap-4">
        <p className="sd !mt-0">{t("settings.plugins.intro")}</p>
        <button onClick={handleInstall} className="btn primary shrink-0">
          <Plus size={15} />
          {t("settings.plugins.install")}
        </button>
      </div>
      <div className="mt-4">
        {isLoading ? (
          <p className="text-sm text-faint">{t("common.loading")}</p>
        ) : plugins.length === 0 ? (
          <div className="border border-dashed border-border rounded-lg p-8 text-center">
            <Puzzle size={28} className="text-faint mx-auto mb-2" />
            <p className="text-sm text-muted">{t("settings.plugins.empty")}</p>
            <p className="text-xs text-faint mt-1">
              {rt("settings.plugins.emptyHint", {
                file: (
                  <code className="font-mono" translate="no">
                    plugin.json
                  </code>
                ),
              })}
            </p>
          </div>
        ) : (
          <ul className="space-y-3">
            {plugins.map((p) => (
              <li key={p.id} className="bg-card-2 rounded-lg p-4 space-y-3">
                <div>
                  <div className="flex items-start justify-between gap-3">
                    {/* The badges wrap under the name rather than squeeze it:
                        Russian labels are twice the English ones. */}
                    <div className="min-w-0">
                      <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
                        <span className="font-semibold text-ink truncate max-w-full">{p.name}</span>
                        {/* "v1.2.0" reads the same in every language. */}
                        <span className="text-xs text-faint whitespace-nowrap" translate="no">
                          v{p.version}
                        </span>
                        {p.enabled ? (
                          <span className="text-[10px] uppercase tracking-wide font-semibold text-good bg-good-soft px-1.5 py-0.5 rounded whitespace-nowrap">
                            {t("settings.plugins.enabled")}
                          </span>
                        ) : (
                          <span className="text-[10px] uppercase tracking-wide font-semibold text-muted bg-border px-1.5 py-0.5 rounded whitespace-nowrap">
                            {t("settings.plugins.disabled")}
                          </span>
                        )}
                        {p.signed ? (
                          <span
                            title={t("settings.plugins.selfSignedTip", { key: p.key_fingerprint ?? "?" })}
                            className="text-[10px] uppercase tracking-wide font-semibold text-muted flex items-center gap-0.5 whitespace-nowrap"
                          >
                            <Fingerprint size={11} /> {t("settings.plugins.selfSigned")}
                            {p.key_fingerprint && (
                              <span className="font-mono normal-case text-faint">· {p.key_fingerprint.slice(0, 8)}</span>
                            )}
                          </span>
                        ) : (
                          <span
                            title={t("settings.plugins.unsignedTip")}
                            className="text-[10px] uppercase tracking-wide font-semibold text-warn whitespace-nowrap"
                          >
                            {t("settings.plugins.unsigned")}
                          </span>
                        )}
                      </div>
                      {p.author && (
                        <p className="text-xs text-faint mt-0.5">{t("settings.plugins.by", { author: p.author })}</p>
                      )}
                    </div>
                    <div className="flex items-center gap-2 shrink-0">
                      {p.enabled && p.contributes.includes("route.planner") && (
                        <button
                          onClick={() => navigate(`/plugin/${p.id}`)}
                          className="px-3 py-1 rounded text-xs font-medium border border-border-2 text-ink hover:bg-card-2"
                        >
                          {t("common.open")}
                        </button>
                      )}
                      {p.enabled && p.contributes.includes("sync.source") && (
                        <button
                          onClick={() => navigate(`/plugin/${p.id}/sync`)}
                          className="px-3 py-1 rounded text-xs font-medium border border-border-2 text-ink hover:bg-card-2"
                        >
                          {t("settings.plugins.sync")}
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
                        {p.enabled ? t("settings.plugins.disable") : t("settings.plugins.enable")}
                      </button>
                      <button
                        onClick={() => handleUninstall(p)}
                        aria-label={t("settings.plugins.uninstall")}
                        data-tip={t("settings.plugins.uninstall")}
                        className="p-1.5 rounded text-faint hover:text-danger hover:bg-danger-soft"
                      >
                        <Trash2 size={15} />
                      </button>
                    </div>
                  </div>
                  {/* Full card width, not the column beside the buttons. */}
                  {p.description && (
                    <p className="text-sm text-muted mt-2">{p.description}</p>
                  )}
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
                          {permissionLabel(perm, t)}
                        </span>
                      ))}
                    </div>
                  </div>
                )}

                {/* Ink on the warn tint, the colour on the icon: the warn
                    tone itself is under AA as 12px text (see .sd.warn). */}
                {secretsDisclosure(p.permissions, sealed, t) && (
                  <div className="flex items-start gap-2 text-ink bg-warn-soft border border-warn/30 rounded px-2 py-1.5">
                    <ShieldCheck size={14} className="mt-0.5 shrink-0 text-warn" />
                    <div className="text-xs">{secretsDisclosure(p.permissions, sealed, t)}</div>
                  </div>
                )}

                {p.network_hosts.length > 0 && (
                  <div className="flex items-start gap-2 text-ink bg-warn-soft border border-warn/30 rounded px-2 py-1.5">
                    <Globe size={14} className="mt-0.5 shrink-0 text-warn" />
                    <div className="text-xs">
                      <span className="font-medium">{t("settings.plugins.connectsTo")} </span>
                      {p.network_hosts.join(", ")}
                    </div>
                  </div>
                )}
              </li>
            ))}
          </ul>
        )}
      </div>
    </section>
  );
}
