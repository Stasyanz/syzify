import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { open, save } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { listen } from "@tauri-apps/api/event";
import { getVersion } from "@tauri-apps/api/app";
import {
  Trash2,
  Archive,
  Upload,
  Github,
  Mail,
  Loader2,
} from "lucide-react";
import { api } from "../lib/tauri";
import { confirmDialog } from "../stores/confirmStore";
import { invalidateActivityData } from "../lib/activityInvalidation";
import { useToastStore } from "../stores/toastStore";
import { useFeedbackStore } from "../stores/feedbackStore";
import { useThemeStore, type ThemeMode } from "../lib/theme";
import { useUnitsStore } from "../lib/units";
import { Select } from "../components/ui/Select";
import { Toggle } from "../components/ui/Toggle";
import { Checkbox } from "../components/ui/Checkbox";
import { LegalModal, type LegalDoc } from "../components/settings/LegalModal";
import { UpdateCheck } from "../components/settings/UpdateCheck";
import { VaultLocation } from "../components/settings/VaultLocation";
import { CONTACT_EMAIL, GITHUB_ISSUES_URL } from "../lib/contact";

/** What each encryption scope covers. Raw files include Garmin monitoring
 * (night heart rate, stress, SpO2) — the same `raw/` machinery, so the
 * same scope (ADR 0002). */
const SCOPE_LABELS = {
  activities: "Raw files (activities & monitoring)",
  database: "Database",
  photos: "Photos",
} as const;

const THEME_MODES: ThemeMode[] = ["light", "dark", "system"];

export function SettingsPage() {
  const queryClient = useQueryClient();
  const addToast = useToastStore((s) => s.addToast);
  const updateToast = useToastStore((s) => s.updateToast);
  const removeToast = useToastStore((s) => s.removeToast);
  const themeMode = useThemeStore((s) => s.mode);
  const setThemeMode = useThemeStore((s) => s.setMode);
  const unitsMode = useUnitsStore((s) => s.mode);
  const setUnitsMode = useUnitsStore((s) => s.setMode);

  // Legal-document viewer (Settings → General → License)
  const [legalDoc, setLegalDoc] = useState<LegalDoc | null>(null);

  // App version (About card)
  const { data: appVersion } = useQuery({
    queryKey: ["appVersion"],
    queryFn: () => getVersion(),
  });

  // Import data sources (Runkeeper, …)
  const { data: importSources = [] } = useQuery({
    queryKey: ["importDatasources"],
    queryFn: () => api.getImportDatasources(),
  });

  // Page size setting
  const { data: pageSize } = useQuery({
    queryKey: ["setting", "page_size"],
    queryFn: () => api.getSetting("page_size"),
  });

  // Online geocoding opt-in (off until the user flips it — privacy default)
  const { data: geocodingSetting } = useQuery({
    queryKey: ["setting", "geocoding_enabled"],
    queryFn: () => api.getSetting("geocoding_enabled"),
  });
  const geocodingEnabled = geocodingSetting === "true";

  // Tile cache info
  const { data: cacheInfo, refetch: refetchCache } = useQuery({
    queryKey: ["tileCacheInfo"],
    queryFn: () => api.getTileCacheInfo(),
  });

  // Encryption status
  const { data: encryptionStatus } = useQuery({
    queryKey: ["encryptionStatus"],
    queryFn: () => api.getEncryptionStatus(),
  });

  const [clearing, setClearing] = useState(false);
  const [backingUp, setBackingUp] = useState(false);
  const [restoring, setRestoring] = useState(false);

  // Encryption UI state
  const [showEncryptDialog, setShowEncryptDialog] = useState(false);
  const [encPassword, setEncPassword] = useState("");
  const [encConfirm, setEncConfirm] = useState("");
  const [encError, setEncError] = useState<string | null>(null);
  const [encBusy, setEncBusy] = useState(false);
  const [showDecryptDialog, setShowDecryptDialog] = useState(false);
  const [decPassword, setDecPassword] = useState("");
  const [decError, setDecError] = useState<string | null>(null);
  // What to encrypt when enabling. When already enabled the row reflects the
  // active scopes read-only (changing scopes in place is a separate op).
  const [encScopes, setEncScopes] = useState({
    activities: false,
    database: false,
    photos: false,
  });

  async function handlePageSizeChange(value: string) {
    await api.setSetting("page_size", value);
    queryClient.invalidateQueries({ queryKey: ["setting", "page_size"] });
    queryClient.invalidateQueries({ queryKey: ["activities"] });
  }

  async function handleGeocodingToggle() {
    const turningOn = !geocodingEnabled;
    await api.setSetting("geocoding_enabled", turningOn ? "true" : "false");
    queryClient.invalidateQueries({ queryKey: ["setting", "geocoding_enabled"] });
    if (turningOn) {
      // Name existing activities right away, not on the next restart/import.
      api.startGeocoding().catch(() => {});
    }
  }

  async function handleClearCache() {
    setClearing(true);
    try {
      await api.clearTileCache();
      refetchCache();
      addToast("success", "Tile cache cleared");
    } finally {
      setClearing(false);
    }
  }

  async function handleBackup() {
    const dest = await save({
      defaultPath: "Syzify_backup.zip",
      filters: [{ name: "ZIP Archive", extensions: ["zip"] }],
    });
    if (!dest) return;
    setBackingUp(true);
    const toastId = addToast("info", "Creating backup… 0%", undefined, true);
    const unlisten = await listen<{ processed: number; total: number }>(
      "backup:progress",
      (e) => {
        const { processed, total } = e.payload;
        const pct = total > 0 ? Math.floor((processed / total) * 100) : 100;
        const mb = (processed / 1048576).toFixed(0);
        const totalMb = (total / 1048576).toFixed(0);
        updateToast(toastId, {
          message: `Creating backup… ${pct}% (${mb}/${totalMb} MB)`,
        });
      }
    );
    try {
      await api.backupVault(dest);
      removeToast(toastId);
      addToast("success", "Vault backup created");
    } catch (e) {
      removeToast(toastId);
      addToast("error", `Backup failed: ${e}`);
    } finally {
      unlisten();
      setBackingUp(false);
    }
  }

  async function handleRestore() {
    const selected = await open({
      multiple: false,
      filters: [{ name: "ZIP Archive", extensions: ["zip"] }],
    });
    if (!selected) return;
    // Restoring replaces the whole vault, so the app must reboot to reopen
    // the DB from the restored files (the backup may have a different
    // encryption state). The backend closes the live DB first; on success we
    // restart, exactly like relocate. The replaced data isn't destroyed — it
    // moves into a pre-restore folder inside the vault.
    const confirmed = await confirmDialog({
      title: "Restore backup",
      message:
        "Restore the vault from this backup?\n\nYour current vault will be replaced and the app will restart. The replaced data is kept in a “pre-restore” folder inside the vault until you delete it.",
      confirmLabel: "Restore",
    });
    if (!confirmed) return;
    setRestoring(true);
    try {
      const { restored, error, preserved_at } = await api.restoreVault(selected as string);
      // Once the vault was touched the live DB is a placeholder — restart even
      // if extraction failed partway (error set), so the app reopens from disk.
      if (restored) {
        const kept = preserved_at ? ` Previous vault kept in ${preserved_at}.` : "";
        addToast(
          error ? "warning" : "success",
          error
            ? `Restore incomplete: ${error}.${kept} Restarting…`
            : `Vault restored.${kept} Restarting…`
        );
        // The backend set vault_error ("restart required") the moment the live
        // DB became a placeholder. Refetch it so that if the restart below
        // fails (fire-and-forget, flaky in tauri dev), the blocking
        // VaultErrorScreen replaces the UI instead of the app looking healthy
        // while every write vanishes into the in-memory placeholder.
        queryClient.invalidateQueries({ queryKey: ["vaultError"] });
        setTimeout(() => {
          api.restartApp().catch(() => {});
        }, 600);
      }
    } catch (e) {
      // Pre-flight rejection (bad/missing archive) — the live DB is untouched.
      addToast("error", `Restore failed: ${e}`);
      setRestoring(false);
    }
  }

  async function handleImportDatasource(ds: { id: string; name: string; extensions: string[] }) {
    const path = await open({
      multiple: false,
      filters: [{ name: ds.name, extensions: ds.extensions }],
    });
    if (!path || typeof path !== "string") return;
    try {
      const r = await api.runImportDatasource(ds.id, path);
      invalidateActivityData(queryClient);
      addToast(
        "success",
        `${ds.name} — imported ${r.imported}, skipped ${r.skipped}${
          r.failed.length ? `, failed ${r.failed.length}` : ""
        }`
      );
    } catch (e) {
      addToast("error", `${ds.name} import failed: ${e}`);
    }
  }

  async function handleEnableEncryption() {
    setEncError(null);
    if (encPassword.length < 8) {
      setEncError("Password must be at least 8 characters.");
      return;
    }
    if (encPassword !== encConfirm) {
      setEncError("Passwords do not match.");
      return;
    }
    if (!encScopes.activities && !encScopes.database && !encScopes.photos) {
      setEncError("Select at least one thing to encrypt.");
      return;
    }
    setEncBusy(true);
    try {
      await api.enableEncryption(encPassword, encScopes);
      setShowEncryptDialog(false);
      setEncPassword("");
      setEncConfirm("");
    } catch (e) {
      setEncError(`Failed: ${e}`);
    } finally {
      setEncBusy(false);
      // Re-read status on EVERY outcome: a failed transition can settle the
      // vault to LOCKED (backend cleared the key + swapped in a placeholder),
      // and only refreshing encryptionStatus makes the app surface the
      // UnlockModal instead of silently serving an empty DB.
      refreshAfterCryptoChange();
    }
  }

  async function handleDisableEncryption() {
    setDecError(null);
    setEncBusy(true);
    try {
      await api.disableEncryption(decPassword);
      setShowDecryptDialog(false);
      setDecPassword("");
    } catch (e) {
      setDecError(`Wrong password or error: ${e}`);
    } finally {
      setEncBusy(false);
      refreshAfterCryptoChange();
    }
  }

  // A crypto transition can change the DB and the lock state on any outcome.
  function refreshAfterCryptoChange() {
    queryClient.invalidateQueries({ queryKey: ["encryptionStatus"] });
    queryClient.invalidateQueries({ queryKey: ["activities"] });
    queryClient.invalidateQueries({ queryKey: ["dashboard"] });
  }

  return (
    <div className="h-full overflow-y-auto scroll-themed">
      <div className="max-w-[760px] mx-auto p-6 flex flex-col gap-5">
        <h1 className="page-title">Settings</h1>

        {/* General */}
        <section className="card">
          <h3>General</h3>
          <div className="set-row">
            <div>
              <div className="sl">Syzify</div>
              <div className="sd">A local-first training vault</div>
              <div className="sd">
                © 2026 Stanislav Zainullin ·{" "}
                <button
                  onClick={() => setLegalDoc("license")}
                  className="text-accent-2 hover:underline"
                >
                  AGPL-3.0
                </button>{" "}
                with the{" "}
                <button
                  onClick={() => setLegalDoc("exception")}
                  className="text-accent-2 hover:underline"
                >
                  Plugin Exception
                </button>{" "}
                ·{" "}
                <button
                  onClick={() => setLegalDoc("notices")}
                  className="text-accent-2 hover:underline"
                >
                  Third-party notices
                </button>
              </div>
            </div>
            {/* Stretch to the text block's height: Version sits on the title
                line, Check for updates on the license line. */}
            <div className="flex flex-col items-end justify-between self-stretch">
              {appVersion && <span className="sd !mt-0">Version {appVersion}</span>}
              <UpdateCheck />
            </div>
          </div>
          <div className="set-row">
            <div>
              <div className="sl">Theme</div>
              <div className="sd">Warm light, warm dark, or follow the system</div>
            </div>
            <div className="seg">
              {THEME_MODES.map((m) => (
                <button
                  key={m}
                  onClick={() => setThemeMode(m)}
                  className={`capitalize${themeMode === m ? " on" : ""}`}
                >
                  {m}
                </button>
              ))}
            </div>
          </div>
          <div className="set-row">
            <div>
              <div className="sl">Units</div>
              <div className="sd">Distance, pace and elevation</div>
            </div>
            <div className="seg">
              <button
                onClick={() => setUnitsMode("metric")}
                className={unitsMode === "metric" ? "on" : ""}
              >
                Metric · km
              </button>
              <button
                onClick={() => setUnitsMode("imperial")}
                className={unitsMode === "imperial" ? "on" : ""}
              >
                Imperial · mi
              </button>
            </div>
          </div>
          <div className="set-row">
            <div>
              <div className="sl">Activities per page</div>
              <div className="sd">
                Number of activities loaded at once in the list view
              </div>
            </div>
            <Select
              ariaLabel="Activities per page"
              className="w-24"
              value={pageSize ?? "20"}
              onChange={handlePageSizeChange}
              options={[
                { value: "10", label: "10" },
                { value: "20", label: "20" },
                { value: "50", label: "50" },
                { value: "100", label: "100" },
              ]}
            />
          </div>
          <div className="set-row">
            <div>
              <div className="sl">Automatic location names</div>
              <div className="sd">
                Look up a city name for each imported activity via
                nominatim.openstreetmap.org — the activity's start coordinates
                are sent to that OpenStreetMap service. When off, nothing is
                sent; searching a location by name when editing an activity
                uses the same service.
              </div>
            </div>
            <Toggle
              on={geocodingEnabled}
              onToggle={handleGeocodingToggle}
              ariaLabel="Automatic location names"
            />
          </div>
          <div className="set-row">
            <div>
              <div className="sl">Import Activities</div>
              <div className="sd">
                Drop GPX/FIT/TCX files anywhere in the app to import
              </div>
            </div>
          </div>
          {importSources.map((ds) => (
            <div key={ds.id} className="set-row">
              <div>
                <div className="sl">{ds.name} import</div>
                <div className="sd">
                  {ds.description} · .{ds.extensions.join(" .")}
                </div>
              </div>
              <button
                onClick={() => handleImportDatasource(ds)}
                className="btn ghost"
              >
                <Upload size={15} />
                Import
              </button>
            </div>
          ))}
          <div className="set-row">
            <div>
              <div className="sl">Feedback</div>
              <div className="sd">
                Report a bug or suggest a feature on{" "}
                <a
                  href={GITHUB_ISSUES_URL}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="text-accent-2 hover:underline"
                >
                  GitHub Issues
                </a>
                {CONTACT_EMAIL && (
                  <>
                    , or email{" "}
                    <a
                      href={`mailto:${CONTACT_EMAIL}`}
                      className="text-accent-2 hover:underline"
                      onClick={(e) => {
                        // WKWebView doesn't handle mailto: itself and the
                        // external-link interceptor skips non-http(s)
                        // schemes, so route it through the opener plugin.
                        e.preventDefault();
                        openUrl(`mailto:${CONTACT_EMAIL}`).catch((err) => {
                          addToast("error", `Failed to open email client: ${err}`);
                        });
                      }}
                    >
                      {CONTACT_EMAIL}
                    </a>
                  </>
                )}
              </div>
            </div>
            <div className="flex gap-2">
              <a
                href={GITHUB_ISSUES_URL}
                target="_blank"
                rel="noopener noreferrer"
                className="iconbtn no-underline"
                data-tip="GitHub Issues"
                aria-label="GitHub Issues"
              >
                <Github size={15} />
              </a>
              <button
                onClick={() => useFeedbackStore.getState().open()}
                className="iconbtn"
                data-tip="Send feedback"
                aria-label="Send feedback"
              >
                <Mail size={15} />
              </button>
            </div>
          </div>
        </section>


        {/* Vault */}
        <section className="card">
          <h3>Vault</h3>
          <VaultLocation />
          <div className="set-row">
            <div className="flex-1">
              <div className="sl">Encryption</div>
              <div className="sd">
                Encrypt selected data at rest with AES-256-GCM
              </div>
              <div className="flex flex-wrap gap-x-4 gap-y-1.5 mt-2.5">
                {(["activities", "database", "photos"] as const).map((scope) => {
                  const enabled = !!encryptionStatus?.enabled;
                  const checked = enabled
                    ? !!encryptionStatus?.scopes?.[scope]
                    : encScopes[scope];
                  return (
                    <label
                      key={scope}
                      className={`flex items-center gap-1.5 text-[13px] ${
                        enabled ? "cursor-default text-muted" : "cursor-pointer text-ink"
                      }`}
                    >
                      <Checkbox
                        checked={checked}
                        disabled={enabled}
                        onChange={() =>
                          setEncScopes((s) => ({ ...s, [scope]: !s[scope] }))
                        }
                      />
                      {SCOPE_LABELS[scope]}
                    </label>
                  );
                })}
              </div>
            </div>
            <Toggle
              on={!!encryptionStatus?.enabled}
              onToggle={() =>
                encryptionStatus?.enabled
                  ? setShowDecryptDialog(true)
                  : setShowEncryptDialog(true)
              }
              ariaLabel={
                encryptionStatus?.enabled
                  ? "Disable encryption"
                  : "Enable encryption"
              }
            />
          </div>

          {/* Enable encryption dialog */}
          {showEncryptDialog && (
            <div className="bg-card-2 rounded-[9px] p-4 space-y-3 mb-3">
              <p className="text-sm font-semibold text-ink">
                Set encryption password
              </p>
              <p className="sd">
                The selected data will be encrypted. You will need this password
                to access your vault
              </p>
              <input
                type="password"
                placeholder="Password (min 8 characters)"
                value={encPassword}
                onChange={(e) => setEncPassword(e.target.value)}
                className="w-full text-sm bg-card border border-border-2 rounded-[9px] px-3 py-2 outline-none focus:border-accent"
              />
              <input
                type="password"
                placeholder="Confirm password"
                value={encConfirm}
                onChange={(e) => setEncConfirm(e.target.value)}
                className="w-full text-sm bg-card border border-border-2 rounded-[9px] px-3 py-2 outline-none focus:border-accent"
              />
              {encError && <p className="text-xs text-red-500">{encError}</p>}
              <div className="flex gap-2">
                <button
                  onClick={handleEnableEncryption}
                  disabled={encBusy}
                  className="btn primary"
                >
                  {encBusy && <Loader2 size={15} className="animate-spin" />}
                  {encBusy ? "Encrypting…" : "Enable Encryption"}
                </button>
                <button
                  onClick={() => {
                    setShowEncryptDialog(false);
                    setEncPassword("");
                    setEncConfirm("");
                    setEncError(null);
                  }}
                  className="btn ghost"
                >
                  Cancel
                </button>
              </div>
            </div>
          )}

          {/* Disable encryption dialog */}
          {showDecryptDialog && (
            <div className="bg-card-2 rounded-[9px] p-4 space-y-3 mb-3">
              <p className="text-sm font-semibold text-ink">
                Enter password to disable encryption
              </p>
              <input
                type="password"
                placeholder="Current password"
                value={decPassword}
                onChange={(e) => setDecPassword(e.target.value)}
                className="w-full text-sm bg-card border border-border-2 rounded-[9px] px-3 py-2 outline-none focus:border-accent"
              />
              {decError && <p className="text-xs text-red-500">{decError}</p>}
              <div className="flex gap-2">
                <button
                  onClick={handleDisableEncryption}
                  disabled={encBusy}
                  className="btn danger"
                >
                  {encBusy && <Loader2 size={15} className="animate-spin" />}
                  {encBusy ? "Decrypting…" : "Disable Encryption"}
                </button>
                <button
                  onClick={() => {
                    setShowDecryptDialog(false);
                    setDecPassword("");
                    setDecError(null);
                  }}
                  className="btn ghost"
                >
                  Cancel
                </button>
              </div>
            </div>
          )}

          <div className="set-row">
            <div>
              <div className="sl">Backup &amp; Restore</div>
              <div className="sd">Back up or restore your entire vault</div>
            </div>
            <div className="flex gap-2">
              <button
                onClick={handleBackup}
                disabled={backingUp || restoring}
                className="btn ghost"
              >
                <Archive size={15} />
                {backingUp ? "Creating…" : "Backup"}
              </button>
              <button
                onClick={handleRestore}
                disabled={backingUp || restoring}
                className="btn ghost"
              >
                <Upload size={15} />
                {restoring ? "Restoring…" : "Restore"}
              </button>
            </div>
          </div>
          <div className="set-row">
            <div>
              <div className="sl">Map tile cache</div>
              <div className="sd">
                Cached map tiles on disk: {cacheInfo?.size_display ?? "…"}
              </div>
            </div>
            <button
              onClick={handleClearCache}
              disabled={clearing || (cacheInfo?.size_bytes ?? 0) === 0}
              className="btn ghost"
            >
              <Trash2 size={15} />
              {clearing ? "Clearing…" : "Clear Cache"}
            </button>
          </div>
        </section>

        {legalDoc && (
          <LegalModal doc={legalDoc} onClose={() => setLegalDoc(null)} />
        )}
      </div>
    </div>
  );
}
