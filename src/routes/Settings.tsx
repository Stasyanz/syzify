import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { useSearchParams } from "react-router";
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
  AlertTriangle,
} from "lucide-react";
import { api } from "../lib/tauri";
import { confirmDialog } from "../stores/confirmStore";
import { invalidateActivityData } from "../lib/activityInvalidation";
import { useToastStore } from "../stores/toastStore";
import { useFeedbackStore } from "../stores/feedbackStore";
import { useThemeStore, type ThemeMode } from "../lib/theme";
import { useUnitsStore } from "../lib/units";
import {
  LANGUAGES,
  LANGUAGE_NAMES,
  currentLanguage,
  formatNumber,
  formatPercent,
  useLanguageStore,
  useRichT,
  useT,
  type LanguagePref,
} from "../lib/i18n";
import { useAnyBusy, useBusy, useBusyStore } from "../stores/busyStore";
import type { MessageKey } from "../locales/types";
import { Select } from "../components/ui/Select";
import { Toggle } from "../components/ui/Toggle";
import { LegalModal, type LegalDoc } from "../components/settings/LegalModal";
import { UpdateCheck } from "../components/settings/UpdateCheck";
import { VaultLocation } from "../components/settings/VaultLocation";
import { MonitoringData } from "../components/settings/MonitoringData";
import { WatchFolders } from "../components/settings/WatchFolders";
import { Garage } from "../components/settings/Garage";
import { PluginRegistry } from "../components/settings/PluginRegistry";
import {
  DEFAULT_SETTINGS_TAB,
  SETTINGS_TABS,
  nextSettingsTab,
  settingsTabFrom,
  type SettingsTab,
} from "../lib/settingsTabs";
import { CONTACT_EMAIL, GITHUB_ISSUES_URL } from "../lib/contact";
import { errorCode, errorText } from "../lib/errors";
import { unencryptedParts } from "../lib/encryptionScopes";

const THEME_MODES = [
  ["light", "settings.general.themeLight"],
  ["dark", "settings.general.themeDark"],
  ["system", "settings.general.themeSystem"],
] as const satisfies readonly (readonly [ThemeMode, MessageKey])[];

/** Set when the language picker changed the interface language: the app
 * remounts under the new language (#232), and the new Settings page puts the
 * focus back on the picker the keyboard left. */
let refocusLanguagePicker = false;

/** The shortest encryption password the vault takes. */
const MIN_PASSWORD_LENGTH = 8;

/** The issue tracker's own name, the same in every language. */
const GITHUB_ISSUES_NAME = "GitHub Issues";

export function SettingsPage() {
  const t = useT();
  const rt = useRichT();
  // A long operation of the page or of a section (a backup, a vault move, a
  // scan, an update, a monitoring delete) locks the language picker (#235).
  const anyBusy = useAnyBusy();
  const queryClient = useQueryClient();
  const addToast = useToastStore((s) => s.addToast);
  const updateToast = useToastStore((s) => s.updateToast);
  const removeToast = useToastStore((s) => s.removeToast);
  const themeMode = useThemeStore((s) => s.mode);
  const setThemeMode = useThemeStore((s) => s.setMode);
  const unitsMode = useUnitsStore((s) => s.mode);
  const setUnitsMode = useUnitsStore((s) => s.setMode);
  const languagePref = useLanguageStore((s) => s.pref);
  const setLanguagePref = useLanguageStore((s) => s.setPref);
  const languageRowRef = useRef<HTMLDivElement>(null);
  function chooseLanguage(pref: LanguagePref) {
    const before = currentLanguage();
    setLanguagePref(pref);
    // "System" may resolve to the language already shown: no remount then.
    if (currentLanguage() !== before) refocusLanguagePicker = true;
  }
  useEffect(() => {
    if (!refocusLanguagePicker) return;
    refocusLanguagePicker = false;
    languageRowRef.current?.querySelector("button")?.focus();
  }, []);

  // The open tab lives in the URL (`?tab=vault`): a plugin page links back
  // to Plugins, and a reload keeps the section. General has no param.
  const [searchParams, setSearchParams] = useSearchParams();
  const tab = settingsTabFrom(searchParams.get("tab"));
  // Whether the encryption dialogs can still show an error when a crypto
  // call settles: they live in the Vault section, which another tab — or
  // leaving Settings altogether — unmounts. Then only a toast reaches the user.
  const tabRef = useRef(tab);
  tabRef.current = tab;
  const mountedRef = useRef(true);
  useEffect(() => {
    // Set on the way in too: StrictMode runs mount → cleanup → mount, and a
    // cleanup-only effect would leave the flag false for good in dev.
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);
  const encryptionDialogGone = () => !mountedRef.current || tabRef.current !== "vault";
  const selectTab = (next: SettingsTab) =>
    setSearchParams(
      (prev) => {
        // Only the tab key changes; any other query the page grows keeps.
        const params = new URLSearchParams(prev);
        if (next === DEFAULT_SETTINGS_TAB) params.delete("tab");
        else params.set("tab", next);
        return params;
      },
      { replace: true },
    );
  // A tablist promises arrow keys: Left/Right cycle, Home/End jump, and only
  // the open tab sits in the Tab order (roving tabindex).
  function onTabKeyDown(e: KeyboardEvent<HTMLDivElement>) {
    const next = nextSettingsTab(tab, e.key);
    if (!next) return;
    e.preventDefault();
    selectTab(next);
    (e.currentTarget.querySelector(`#settings-tab-${next}`) as HTMLElement | null)?.focus();
  }

  // Legal-document viewer (Settings → About)
  const [legalDoc, setLegalDoc] = useState<LegalDoc | null>(null);

  // App version (About card)
  const { data: appVersion } = useQuery({
    queryKey: ["appVersion"],
    queryFn: () => getVersion(),
    enabled: tab === "about",
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

  // Tile cache info: a directory walk on disk, only for the tab that shows it
  const { data: cacheInfo, refetch: refetchCache } = useQuery({
    queryKey: ["tileCacheInfo"],
    queryFn: () => api.getTileCacheInfo(),
    enabled: tab === "general",
  });

  // Encryption status
  const { data: encryptionStatus } = useQuery({
    queryKey: ["encryptionStatus"],
    queryFn: () => api.getEncryptionStatus(),
    enabled: tab === "vault",
  });

  // The page's own long operations live in the busy store with the
  // sections' (#235): leaving Settings and coming back while one runs keeps
  // its indicator and its buttons off.
  const clearing = useBusy("clearTiles");
  const backingUp = useBusy("backup");
  const restoring = useBusy("restore");
  const encrypting = useBusy("encrypt");
  const decrypting = useBusy("decrypt");
  const encBusy = encrypting || decrypting;

  // Encryption UI state
  const [showEncryptDialog, setShowEncryptDialog] = useState(false);
  const [encPassword, setEncPassword] = useState("");
  const [encConfirm, setEncConfirm] = useState("");
  const [encError, setEncError] = useState<string | null>(null);
  const [showDecryptDialog, setShowDecryptDialog] = useState(false);
  const [decPassword, setDecPassword] = useState("");
  const [decError, setDecError] = useState<string | null>(null);
  // What a vault encrypted before #209 left out; changing scopes in place is
  // a separate op, so the row can only say how to cover the rest.
  const leftReadable = encryptionStatus?.enabled ? unencryptedParts(encryptionStatus.scopes, t) : [];

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
    const ops = useBusyStore.getState();
    ops.start("clearTiles");
    try {
      await api.clearTileCache();
      refetchCache();
      addToast("success", t("settings.general.cacheCleared"));
    } catch (e) {
      addToast("error", t("settings.general.clearCacheFailed", { error: errorText(e) }));
    } finally {
      ops.finish("clearTiles");
    }
  }

  async function handleBackup() {
    const dest = await save({
      defaultPath: "Syzify_backup.zip",
      filters: [{ name: t("settings.vault.zipFilter"), extensions: ["zip"] }],
    });
    if (!dest) return;
    const ops = useBusyStore.getState();
    ops.start("backup");
    const toastId = addToast(
      "info",
      t("settings.vault.backupStarting", { percent: formatPercent(0, 0) }),
      undefined,
      true,
    );
    // Everything after `start` is inside the try: a listener that cannot
    // subscribe must not leave the flag (and the language picker) locked.
    let unlisten = () => {};
    try {
      unlisten = await listen<{ processed: number; total: number }>("backup:progress", (e) => {
        const { processed, total } = e.payload;
        const pct = total > 0 ? Math.floor((processed / total) * 100) : 100;
        updateToast(toastId, {
          message: t("settings.vault.backupProgress", {
            percent: formatPercent(pct, 0),
            done: formatNumber(processed / 1048576, 0),
            total: formatNumber(total / 1048576, 0),
          }),
        });
      });
      await api.backupVault(dest);
      removeToast(toastId);
      addToast("success", t("settings.vault.backupCreated"));
    } catch (e) {
      removeToast(toastId);
      addToast("error", t("settings.vault.backupFailed", { error: errorText(e) }));
    } finally {
      unlisten();
      ops.finish("backup");
    }
  }

  async function handleRestore() {
    const selected = await open({
      multiple: false,
      filters: [{ name: t("settings.vault.zipFilter"), extensions: ["zip"] }],
    });
    if (!selected) return;
    // Restoring replaces the whole vault, so the app must reboot to reopen
    // the DB from the restored files (the backup may have a different
    // encryption state). The backend closes the live DB first; on success we
    // restart, exactly like relocate. The replaced data isn't destroyed — it
    // moves into a pre-restore folder inside the vault.
    const confirmed = await confirmDialog({
      title: t("settings.vault.restoreTitle"),
      message: t("settings.vault.restoreMessage"),
      confirmLabel: t("settings.vault.restore"),
    });
    if (!confirmed) return;
    const ops = useBusyStore.getState();
    // Busy until the restart, like a vault move: the vault is not the live
    // one any more.
    ops.start("restore");
    try {
      // `error` here is the backend's own report of a partial extraction: a
      // CommandError in the outcome, said like a rejection (#236).
      const { restored, error: partial, preserved_at } = await api.restoreVault(selected as string);
      // Once the vault was touched the live DB is a placeholder — restart even
      // if extraction failed partway (error set), so the app reopens from disk.
      if (restored) {
        const sentences = [
          partial ? t("settings.vault.restoreIncomplete", { error: errorText(partial) }) : t("settings.vault.restored"),
          preserved_at ? t("settings.vault.restoreKept", { path: preserved_at }) : null,
          t("settings.vault.restarting"),
        ];
        addToast(partial ? "warning" : "success", sentences.filter(Boolean).join(" "));
        // The backend set vault_error ("restart required") the moment the live
        // DB became a placeholder. Refetch it so that if the restart below
        // fails (fire-and-forget, flaky in tauri dev), the blocking
        // VaultErrorScreen replaces the UI instead of the app looking healthy
        // while every write vanishes into the in-memory placeholder.
        queryClient.invalidateQueries({ queryKey: ["vaultError"] });
        setTimeout(() => {
          api.restartApp().catch(() => {});
        }, 600);
      } else {
        // Nothing was touched: no restart comes, so nothing stays locked.
        ops.finish("restore");
      }
    } catch (e) {
      // Pre-flight rejection (bad/missing archive) — the live DB is untouched.
      addToast("error", t("settings.vault.restoreFailed", { error: errorText(e) }));
      ops.finish("restore");
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
      const counts = { name: ds.name, imported: r.imported, skipped: r.skipped };
      addToast(
        "success",
        r.failed.length
          ? t("settings.general.sourceImportedFailed", { ...counts, failed: r.failed.length })
          : t("settings.general.sourceImported", counts),
      );
    } catch (e) {
      addToast("error", t("settings.general.sourceImportFailed", { name: ds.name, error: errorText(e) }));
    }
  }

  async function handleEnableEncryption() {
    setEncError(null);
    if (encPassword.length < MIN_PASSWORD_LENGTH) {
      setEncError(t("settings.vault.passwordTooShort", { count: MIN_PASSWORD_LENGTH }));
      return;
    }
    if (encPassword !== encConfirm) {
      setEncError(t("settings.vault.passwordMismatch"));
      return;
    }
    const ops = useBusyStore.getState();
    ops.start("encrypt");
    try {
      await api.enableEncryption(encPassword);
      setShowEncryptDialog(false);
      setEncPassword("");
      setEncConfirm("");
    } catch (e) {
      setEncError(t("settings.vault.encryptFailedInline", { error: errorText(e) }));
      if (encryptionDialogGone()) addToast("error", t("settings.vault.encryptFailed", { error: errorText(e) }));
    } finally {
      ops.finish("encrypt");
      // Re-read status on EVERY outcome: a failed transition can settle the
      // vault to LOCKED (backend cleared the key + swapped in a placeholder),
      // and only refreshing encryptionStatus makes the app surface the
      // UnlockModal instead of silently serving an empty DB.
      refreshAfterCryptoChange();
    }
  }

  async function handleDisableEncryption() {
    setDecError(null);
    const ops = useBusyStore.getState();
    ops.start("decrypt");
    try {
      await api.disableEncryption(decPassword);
      setShowDecryptDialog(false);
      setDecPassword("");
    } catch (e) {
      // A wrong password says so alone — not "Wrong password or error: Wrong password".
      setDecError(
        errorCode(e) === "vault.wrongPassword"
          ? errorText(e)
          : t("settings.vault.decryptFailedInline", { error: errorText(e) }),
      );
      if (encryptionDialogGone()) addToast("error", t("settings.vault.decryptFailed", { error: errorText(e) }));
    } finally {
      ops.finish("decrypt");
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
    // overflow-y-scroll, not auto: the themed scrollbar takes 10 px of
    // layout, and only the tallest tab would get it — every tab reserves
    // the gutter, so switching tabs never shifts the cards.
    <div className="h-full overflow-y-scroll scroll-themed" data-testid="settings-scroll">
      <div className="max-w-[760px] mx-auto p-6 flex flex-col gap-5">
        {/* The tab strip is the page's heading: the tab names say where the
            user is, so no title and no card headings repeating them. */}
        <div
          className="seg self-center"
          role="tablist"
          aria-label={t("settings.sections")}
          onKeyDown={onTabKeyDown}
        >
          {SETTINGS_TABS.map((def) => (
            <button
              key={def.id}
              id={`settings-tab-${def.id}`}
              role="tab"
              aria-selected={tab === def.id}
              // Only the open panel exists in the DOM; a dangling id is worse
              // than none.
              aria-controls={tab === def.id ? `settings-panel-${def.id}` : undefined}
              tabIndex={tab === def.id ? 0 : -1}
              className={tab === def.id ? "on" : ""}
              onClick={() => selectTab(def.id)}
            >
              {t(def.labelKey)}
            </button>
          ))}
        </div>

        {tab === "general" && (
          <section
            className="card"
            role="tabpanel"
            id="settings-panel-general"
            aria-labelledby="settings-tab-general"
          >
            <div className="set-row">
              <div>
                <div className="sl">{t("settings.general.theme")}</div>
                <div className="sd">{t("settings.general.themeHint")}</div>
              </div>
              <div className="seg">
                {THEME_MODES.map(([m, label]) => (
                  <button
                    key={m}
                    onClick={() => setThemeMode(m)}
                    className={themeMode === m ? "on" : ""}
                  >
                    {t(label)}
                  </button>
                ))}
              </div>
            </div>
            <div className="set-row" ref={languageRowRef}>
              <div>
                <div className="sl">{t("settings.general.language")}</div>
                <div className="sd">{t("settings.general.languageHint")}</div>
              </div>
              <Select
                ariaLabel={t("settings.general.language")}
                className="w-40"
                // The remount a new language brings would cut short the
                // indicator of a long operation: every one of them, the
                // page's and its sections', keeps its flag in the busy store.
                disabled={anyBusy}
                value={languagePref}
                onChange={(v) => chooseLanguage(v as LanguagePref)}
                options={[
                  { value: "system", label: t("settings.general.languageSystem") },
                  ...LANGUAGES.map((l) => ({ value: l, label: LANGUAGE_NAMES[l] })),
                ]}
              />
            </div>
            <div className="set-row">
              <div>
                <div className="sl">{t("settings.general.units")}</div>
                <div className="sd">{t("settings.general.unitsHint")}</div>
              </div>
              <div className="seg">
                <button
                  onClick={() => setUnitsMode("metric")}
                  className={unitsMode === "metric" ? "on" : ""}
                >
                  {t("settings.general.unitsMetric")}
                </button>
                <button
                  onClick={() => setUnitsMode("imperial")}
                  className={unitsMode === "imperial" ? "on" : ""}
                >
                  {t("settings.general.unitsImperial")}
                </button>
              </div>
            </div>
            <div className="set-row">
              <div>
                <div className="sl">{t("settings.general.pageSize")}</div>
                <div className="sd">{t("settings.general.pageSizeHint")}</div>
              </div>
              <Select
                ariaLabel={t("settings.general.pageSize")}
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
                <div className="sl">{t("settings.general.geocoding")}</div>
                <div className="sd">{t("settings.general.geocodingHint")}</div>
              </div>
              <Toggle
                on={geocodingEnabled}
                onToggle={handleGeocodingToggle}
                ariaLabel={t("settings.general.geocoding")}
              />
            </div>
            <div className="set-row">
              <div>
                <div className="sl">{t("settings.general.import")}</div>
                <div className="sd">{t("settings.general.importHint")}</div>
              </div>
            </div>
            {importSources.map((ds) => (
              <div key={ds.id} className="set-row">
                <div>
                  <div className="sl">{t("settings.general.sourceImport", { name: ds.name })}</div>
                  <div className="sd">
                    {ds.description} · .{ds.extensions.join(" .")}
                  </div>
                </div>
                <button
                  onClick={() => handleImportDatasource(ds)}
                  className="btn ghost"
                >
                  <Upload size={15} />
                  {t("settings.general.sourceImportButton")}
                </button>
              </div>
            ))}
            <div className="set-row">
              <div>
                <div className="sl">{t("settings.general.tileCache")}</div>
                <div className="sd">
                  {t("settings.general.tileCacheHint", { size: cacheInfo?.size_display ?? "…" })}
                </div>
              </div>
              <button
                onClick={handleClearCache}
                disabled={clearing || (cacheInfo?.size_bytes ?? 0) === 0}
                className="btn ghost"
              >
                <Trash2 size={15} />
                {clearing ? t("settings.general.clearingCache") : t("settings.general.clearCache")}
              </button>
            </div>
          </section>
        )}

        {tab === "vault" && (
          <section
            className="card"
            role="tabpanel"
            id="settings-panel-vault"
            aria-labelledby="settings-tab-vault"
          >
            <VaultLocation />
            <div className="set-row">
              <div className="flex-1">
                <div className="sl">{t("settings.vault.encryption")}</div>
                <div className="sd">
                  {!encryptionStatus?.enabled
                    ? t("settings.vault.encryptionOff")
                    : leftReadable.length === 0
                      ? t("settings.vault.encryptionOn")
                      : t("settings.vault.encryptionPartial")}
                </div>
                {leftReadable.length > 0 && (
                  // The way out is disable + enable, and each half can strand
                  // the vault: everything is readable in between, a failed
                  // disable leaves it half-decrypted, and enable refuses
                  // while a pre-restore folder is inside. Say so up front.
                  <div className="sd warn">
                    <AlertTriangle size={13} aria-hidden="true" />
                    <span>{t("settings.vault.notEncrypted", { parts: leftReadable.join(", ") })}</span>
                  </div>
                )}
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
                    ? t("settings.vault.disableEncryptionLabel")
                    : t("settings.vault.enableEncryptionLabel")
                }
                disabled={encBusy}
              />
            </div>
            {/* Back on the tab while a switch runs: its dialog is gone with
                the old page, the switch is not. */}
            {encBusy && !showEncryptDialog && !showDecryptDialog && (
              <p className="sd flex items-center gap-1.5 pb-3" role="status">
                <Loader2 size={13} className="animate-spin" aria-hidden="true" />
                {encrypting ? t("settings.vault.encrypting") : t("settings.vault.decrypting")}
              </p>
            )}

            {/* Enable encryption dialog */}
            {showEncryptDialog && (
              <div className="bg-card-2 rounded-[9px] p-4 space-y-3 mb-3">
                <p className="text-sm font-semibold text-ink">{t("settings.vault.setPassword")}</p>
                <p className="sd">{t("settings.vault.setPasswordHint")}</p>
                <input
                  type="password"
                  placeholder={t("settings.vault.passwordPlaceholder", { count: MIN_PASSWORD_LENGTH })}
                  value={encPassword}
                  onChange={(e) => setEncPassword(e.target.value)}
                  className="w-full text-sm bg-card border border-border-2 rounded-[9px] px-3 py-2 outline-none focus:border-accent"
                />
                <input
                  type="password"
                  placeholder={t("settings.vault.confirmPassword")}
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
                    {encBusy ? t("settings.vault.encrypting") : t("settings.vault.enableEncryption")}
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
                    {t("common.cancel")}
                  </button>
                </div>
              </div>
            )}

            {/* Disable encryption dialog */}
            {showDecryptDialog && (
              <div className="bg-card-2 rounded-[9px] p-4 space-y-3 mb-3">
                <p className="text-sm font-semibold text-ink">{t("settings.vault.disablePrompt")}</p>
                <input
                  type="password"
                  placeholder={t("settings.vault.currentPassword")}
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
                    {encBusy ? t("settings.vault.decrypting") : t("settings.vault.disableEncryption")}
                  </button>
                  <button
                    onClick={() => {
                      setShowDecryptDialog(false);
                      setDecPassword("");
                      setDecError(null);
                    }}
                    className="btn ghost"
                  >
                    {t("common.cancel")}
                  </button>
                </div>
              </div>
            )}

            <div className="set-row">
              <div>
                <div className="sl">{t("settings.vault.backupRestore")}</div>
                <div className="sd">{t("settings.vault.backupRestoreHint")}</div>
              </div>
              <div className="flex gap-2">
                <button
                  onClick={handleBackup}
                  disabled={backingUp || restoring}
                  className="btn ghost"
                >
                  <Archive size={15} />
                  {backingUp ? t("settings.vault.creating") : t("settings.vault.backup")}
                </button>
                <button
                  onClick={handleRestore}
                  disabled={backingUp || restoring}
                  className="btn ghost"
                >
                  <Upload size={15} />
                  {restoring ? t("settings.vault.restoring") : t("settings.vault.restore")}
                </button>
              </div>
            </div>
            <MonitoringData />
            <WatchFolders />
          </section>
        )}

        {tab === "garage" && (
          <div role="tabpanel" id="settings-panel-garage" aria-labelledby="settings-tab-garage">
            <Garage />
          </div>
        )}

        {tab === "plugins" && (
          <div role="tabpanel" id="settings-panel-plugins" aria-labelledby="settings-tab-plugins">
            <PluginRegistry />
          </div>
        )}

        {tab === "about" && (
          <section
            className="card"
            role="tabpanel"
            id="settings-panel-about"
            aria-labelledby="settings-tab-about"
          >
            <div className="set-row">
              <div>
                <div className="sl">Syzify</div>
                <div className="sd">{t("settings.about.tagline")}</div>
              </div>
              <div className="flex flex-col items-end gap-0.5">
                {appVersion && <span className="sd !mt-0">{t("settings.about.version", { version: appVersion })}</span>}
                <UpdateCheck />
              </div>
            </div>
            <div className="set-row">
              <div>
                <div className="sl">{t("settings.about.license")}</div>
                <div className="sd">
                  {/* The copyright line and the licence's name are names;
                      the documents themselves stay English (LegalModal). */}
                  <span translate="no">© 2026 Stanislav Zainullin</span>
                  {" · "}
                  {rt("settings.about.licenseLine", {
                    license: (
                      <button
                        onClick={() => setLegalDoc("license")}
                        className="text-accent-2 hover:underline"
                        translate="no"
                      >
                        AGPL-3.0
                      </button>
                    ),
                    exception: (
                      <button
                        onClick={() => setLegalDoc("exception")}
                        className="text-accent-2 hover:underline"
                      >
                        {t("settings.about.pluginException")}
                      </button>
                    ),
                  })}
                  {" · "}
                  <button
                    onClick={() => setLegalDoc("notices")}
                    className="text-accent-2 hover:underline"
                  >
                    {t("settings.about.notices")}
                  </button>
                </div>
              </div>
            </div>
            <div className="set-row">
              <div>
                <div className="sl">{t("settings.about.feedback")}</div>
                <div className="sd">
                  {(() => {
                    const github = (
                      <a
                        href={GITHUB_ISSUES_URL}
                        target="_blank"
                        rel="noopener noreferrer"
                        className="text-accent-2 hover:underline"
                        translate="no"
                      >
                        {GITHUB_ISSUES_NAME}
                      </a>
                    );
                    if (!CONTACT_EMAIL) return rt("settings.about.report", { github });
                    const email = (
                      <a
                        href={`mailto:${CONTACT_EMAIL}`}
                        className="text-accent-2 hover:underline"
                        onClick={(e) => {
                          // WKWebView doesn't handle mailto: itself and the
                          // external-link interceptor skips non-http(s)
                          // schemes, so route it through the opener plugin.
                          e.preventDefault();
                          openUrl(`mailto:${CONTACT_EMAIL}`).catch((err) => {
                            addToast("error", t("feedback.openFailed", { error: errorText(err) }));
                          });
                        }}
                      >
                        {CONTACT_EMAIL}
                      </a>
                    );
                    return rt("settings.about.reportOrEmail", { github, email });
                  })()}
                </div>
              </div>
              <div className="flex gap-2">
                <a
                  href={GITHUB_ISSUES_URL}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="iconbtn no-underline"
                  data-tip={GITHUB_ISSUES_NAME}
                  aria-label={GITHUB_ISSUES_NAME}
                >
                  <Github size={15} />
                </a>
                <button
                  onClick={() => useFeedbackStore.getState().open()}
                  className="iconbtn"
                  data-tip={t("settings.about.sendFeedback")}
                  aria-label={t("settings.about.sendFeedback")}
                >
                  <Mail size={15} />
                </button>
              </div>
            </div>
          </section>
        )}

        {legalDoc && (
          <LegalModal doc={legalDoc} onClose={() => setLegalDoc(null)} />
        )}
      </div>
    </div>
  );
}
