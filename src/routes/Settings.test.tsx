// @vitest-environment happy-dom
import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation, useNavigate } from "react-router";
import { QueryClient, QueryClientProvider, useQuery } from "@tanstack/react-query";
import { errorText } from "../lib/errors";

vi.mock("../lib/tauri", () => ({
  api: {
    getSetting: vi.fn(),
    setSetting: vi.fn(),
    getImportDatasources: vi.fn(),
    getTileCacheInfo: vi.fn(),
    getEncryptionStatus: vi.fn(),
    clearTileCache: vi.fn(),
    enableEncryption: vi.fn(),
    disableEncryption: vi.fn(),
    startGeocoding: vi.fn(),
    runImportDatasource: vi.fn(),
    backupVault: vi.fn(),
    restoreVault: vi.fn(),
    getVaultError: vi.fn(),
    restartApp: vi.fn(async () => {}),
  },
}));
vi.mock("../stores/confirmStore", () => ({ confirmDialog: vi.fn(async () => true) }));
const contact = vi.hoisted(() => ({ email: "hi@example.com" }));
vi.mock("../lib/contact", () => ({
  get CONTACT_EMAIL() {
    return contact.email;
  },
  GITHUB_ISSUES_URL: "https://example.com/issues",
}));
vi.mock("../stores/toastStore", () => {
  const addToast = vi.fn(() => "t1");
  const state = { addToast, updateToast: vi.fn(), removeToast: vi.fn() };
  const useToastStore = Object.assign((sel: (s: typeof state) => unknown) => sel(state), {
    getState: () => state,
  });
  return { useToastStore };
});
vi.mock("@tauri-apps/api/app", () => ({ getVersion: vi.fn(async () => "9.9.9") }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
// The sections' own widgets have their own tests and their own IPC; here
// only which of them a tab shows matters.
vi.mock("../components/settings/VaultLocation", () => ({ VaultLocation: () => <div>VAULT LOCATION</div> }));
vi.mock("../components/settings/MonitoringData", () => ({ MonitoringData: () => <div>MONITORING DATA</div> }));
vi.mock("../components/settings/WatchFolders", () => ({ WatchFolders: () => <div>WATCH FOLDERS</div> }));
vi.mock("../components/settings/Garage", () => ({ Garage: () => <div>GARAGE</div> }));
vi.mock("../components/settings/PluginRegistry", () => ({ PluginRegistry: () => <div>PLUGIN REGISTRY</div> }));
vi.mock("../components/settings/UpdateCheck", () => ({ UpdateCheck: () => <div>UPDATE CHECK</div> }));
vi.mock("../components/settings/LegalModal", () => ({
  LegalModal: ({ doc }: { doc: string }) => <div>LEGAL {doc}</div>,
}));

import { SettingsPage } from "./Settings";
import { api } from "../lib/tauri";
import { useToastStore } from "../stores/toastStore";
import { useThemeStore } from "../lib/theme";
import { useUnitsStore } from "../lib/units";
import { useLanguageStore } from "../lib/i18n";
import { commandError } from "../test/commandError";
import { useFeedbackStore } from "../stores/feedbackStore";
import { useBusyStore } from "../stores/busyStore";
import { open, save } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";

/** Every row label on the open tab, in order — a row that drifts to another
 * tab or vanishes changes this list. */
const rows = () => Array.from(document.querySelectorAll(".sl")).map((el) => el.textContent);
const selected = () =>
  screen.getAllByRole("tab").filter((t) => t.getAttribute("aria-selected") === "true").map((t) => t.textContent);

function Probe() {
  const { pathname, search } = useLocation();
  const navigate = useNavigate();
  return (
    <>
      <div data-testid="url">{pathname + search}</div>
      <button onClick={() => navigate("/away")}>leave settings</button>
    </>
  );
}

function renderAt(path: string, { strict = false } = {}) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  // StrictMode (the app's own wrapper, src/main.tsx) double-invokes effects
  // in dev: mount → cleanup → mount. Opt in where that matters.
  const Wrap = strict ? StrictMode : ({ children }: { children: React.ReactNode }) => <>{children}</>;
  return render(
    <Wrap>
      <QueryClientProvider client={qc}>
        <MemoryRouter initialEntries={[path]}>
          <Routes>
            <Route
              path="/settings"
              element={
                <>
                  <SettingsPage />
                  <Probe />
                </>
              }
            />
            <Route path="/away" element={<div>AWAY</div>} />
          </Routes>
        </MemoryRouter>
      </QueryClientProvider>
    </Wrap>,
  );
}

const url = () => screen.getByTestId("url").textContent;

describe("SettingsPage tabs", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(api.getSetting).mockResolvedValue(null);
    vi.mocked(api.getImportDatasources).mockResolvedValue([]);
    vi.mocked(api.getTileCacheInfo).mockResolvedValue({ size_bytes: 2048, size_display: "2 KB" });
    vi.mocked(api.getEncryptionStatus).mockResolvedValue({
      enabled: false,
      locked: false,
      scopes: { activities: false, database: false, photos: false },
    });
  });
  afterEach(() => {
    cleanup();
    useBusyStore.setState({ running: {}, lastError: {} });
  });

  it("opens on General with the five tabs and only that section", async () => {
    renderAt("/settings");
    const tabs = screen.getAllByRole("tab");
    expect(tabs.map((t) => t.textContent)).toEqual(["General", "Vault", "Garage", "Plugins", "About"]);
    expect(tabs.map((t) => t.getAttribute("aria-selected"))).toEqual(["true", "false", "false", "false", "false"]);
    expect(rows()).toEqual([
      "Theme",
      "Language",
      "Units",
      "Activities per page",
      "Automatic location names",
      "Import Activities",
      "Map tile cache",
    ]);
    await waitFor(() => expect(screen.getByText(/Cached map tiles on disk: 2 KB/)).toBeTruthy());
    expect(screen.getByRole("button", { name: /Clear Cache/ })).toBeTruthy();
    // Only the open tab is in the Tab order; the panel is tied to its tab.
    expect(screen.getAllByRole("tab").map((t) => t.tabIndex)).toEqual([0, -1, -1, -1, -1]);
    expect(screen.getAllByRole("tab").map((t) => t.getAttribute("aria-controls"))).toEqual([
      screen.getByRole("tabpanel").id, null, null, null, null,
    ]);
    const panel = screen.getByRole("tabpanel");
    expect(panel.getAttribute("aria-labelledby")).toBe(screen.getByRole("tab", { name: "General" }).id);
    expect(screen.queryByText(/Version/)).toBeNull();
  });

  it("lists each import data source as a row of General", async () => {
    vi.mocked(api.getImportDatasources).mockResolvedValue([
      { id: "runkeeper", name: "Runkeeper", description: "Runkeeper export", extensions: ["zip"] },
    ]);
    renderAt("/settings");
    await waitFor(() => expect(screen.getByText("Runkeeper import")).toBeTruthy());
    expect(rows()).toEqual([
      "Theme",
      "Language",
      "Units",
      "Activities per page",
      "Automatic location names",
      "Import Activities",
      "Runkeeper import",
      "Map tile cache",
    ]);
    expect(screen.getByText(/Runkeeper export · \.zip/)).toBeTruthy();
  });

  it("switches sections on click and writes the tab into the URL, General without a param", () => {
    renderAt("/settings");
    fireEvent.click(screen.getByRole("tab", { name: "Vault" }));
    expect(url()).toBe("/settings?tab=vault");
    expect(selected()).toEqual(["Vault"]);
    expect(rows()).toEqual(["Encryption", "Backup & Restore"]);
    expect(screen.getByText("VAULT LOCATION")).toBeTruthy();
    expect(screen.getByText("MONITORING DATA")).toBeTruthy();
    expect(screen.getByText("WATCH FOLDERS")).toBeTruthy();
    expect(screen.getAllByRole("tabpanel")).toHaveLength(1);

    fireEvent.click(screen.getByRole("tab", { name: "Garage" }));
    expect(url()).toBe("/settings?tab=garage");
    expect(screen.getByRole("tabpanel").textContent).toBe("GARAGE");
    expect(rows()).toEqual([]);

    fireEvent.click(screen.getByRole("tab", { name: "Plugins" }));
    expect(url()).toBe("/settings?tab=plugins");
    expect(screen.getByRole("tabpanel").textContent).toBe("PLUGIN REGISTRY");
    expect(rows()).toEqual([]);

    fireEvent.click(screen.getByRole("tab", { name: "General" }));
    expect(url()).toBe("/settings");
    expect(screen.getByText("Theme")).toBeTruthy();
  });

  it("changes only the tab key of the query and leaves the rest alone", () => {
    renderAt("/settings?tab=about&x=1");
    fireEvent.click(screen.getByRole("tab", { name: "Plugins" }));
    expect(url()).toBe("/settings?tab=plugins&x=1");
    fireEvent.click(screen.getByRole("tab", { name: "General" }));
    expect(url()).toBe("/settings?x=1");
  });

  it("asks only for what the open tab shows", async () => {
    renderAt("/settings?tab=about");
    await waitFor(() => expect(screen.getByText("Version 9.9.9")).toBeTruthy());
    expect(api.getTileCacheInfo).not.toHaveBeenCalled();
    expect(api.getEncryptionStatus).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("tab", { name: "General" }));
    await waitFor(() => expect(api.getTileCacheInfo).toHaveBeenCalledTimes(1));
    expect(api.getEncryptionStatus).not.toHaveBeenCalled();
  });

  it("About carries the license links, the version and the update check", async () => {
    renderAt("/settings?tab=about");
    await waitFor(() => expect(screen.getByText("Version 9.9.9")).toBeTruthy());
    expect(screen.getByText("UPDATE CHECK")).toBeTruthy();
    expect(rows()).toEqual(["Syzify", "License", "Feedback"]);
    expect(screen.getByLabelText("Send feedback")).toBeTruthy();
    expect(screen.getByRole("button", { name: "AGPL-3.0" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Third-party notices" }));
    expect(screen.getByText("LEGAL notices")).toBeTruthy();
  });

  it("moves between tabs with the arrow keys, wrapping at the ends, and Home/End", () => {
    renderAt("/settings");
    const list = screen.getByRole("tablist");
    fireEvent.keyDown(list, { key: "ArrowLeft" });
    expect(selected()).toEqual(["About"]);
    expect(url()).toBe("/settings?tab=about");
    expect(document.activeElement).toBe(screen.getByRole("tab", { name: "About" }));
    fireEvent.keyDown(list, { key: "ArrowRight" });
    expect(selected()).toEqual(["General"]);
    fireEvent.keyDown(list, { key: "End" });
    expect(selected()).toEqual(["About"]);
    fireEvent.keyDown(list, { key: "Home" });
    expect(selected()).toEqual(["General"]);
    fireEvent.keyDown(list, { key: "ArrowDown" });
    expect(selected()).toEqual(["General"]);
  });

  async function startEnablingEncryption({ strict = false } = {}) {
    renderAt("/settings?tab=vault", { strict });
    await waitFor(() => expect(screen.getByLabelText("Enable encryption")).toBeTruthy());
    fireEvent.click(screen.getByLabelText("Enable encryption"));
    fireEvent.change(screen.getByPlaceholderText("Password (min 8 characters)"), { target: { value: "hunter2hunter2" } });
    fireEvent.change(screen.getByPlaceholderText("Confirm password"), { target: { value: "hunter2hunter2" } });
    fireEvent.click(screen.getByRole("button", { name: "Enable Encryption" }));
  }

  it("turning encryption on asks for a password and nothing else — there is no scope to tick (#209)", async () => {
    // Still running, so the dialog is still there to read.
    vi.mocked(api.enableEncryption).mockReturnValue(new Promise(() => {}));
    await startEnablingEncryption();
    expect(screen.queryAllByRole("checkbox")).toEqual([]);
    expect(
      screen.getByText(
        /^Raw files \(activities and monitoring\), the database and photos\s+will be encrypted\. You will need this password to open your\s+vault, and it cannot be recovered$/,
      ),
    ).toBeTruthy();
    // Which scopes go to the backend is the bridge's business (tauri.encryption.test.ts).
    await waitFor(() => expect(api.enableEncryption).toHaveBeenCalledWith("hunter2hunter2"));
  });

  /** Until the status answers, the row reads exactly as it does for a vault
   * with encryption off — so "the off wording is on screen" proves nothing
   * about what the answer did. Wait for the answer itself. */
  async function statusDelivered() {
    await waitFor(() => expect(api.getEncryptionStatus).toHaveBeenCalled());
    await act(async () => {
      await vi.mocked(api.getEncryptionStatus).mock.results[0].value;
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
  }

  it.each([
    [{ activities: true, database: false, photos: false }, "the database, photos"],
    [{ activities: false, database: true, photos: true }, "raw files (activities and monitoring)"],
    // A lock whose enable died before any scope was done: nothing is covered.
    [
      { activities: false, database: false, photos: false },
      "raw files (activities and monitoring), the database, photos",
    ],
  ])("names what a vault encrypted with %j still leaves readable, and what covering it takes (#209)", async (scopes, parts) => {
    vi.mocked(api.getEncryptionStatus).mockResolvedValue({ enabled: true, locked: false, scopes });
    renderAt("/settings?tab=vault");
    await waitFor(() =>
      expect(screen.getByText("Raw files, the database and photos are not all encrypted at rest")).toBeTruthy(),
    );
    const warning = screen.getByText(/^Not encrypted:/);
    expect(warning.textContent?.replace(/\s+/g, " ")).toBe(
      `Not encrypted: ${parts}. To cover all three, back up the vault, then turn encryption off and on again. ` +
        "The vault is unencrypted in between, and a pre-restore folder inside it has to be moved out first",
    );
    expect(warning.parentElement?.className).toBe("sd warn");
    // The scopes are not offered for editing: covering the rest is off and on.
    expect(screen.queryAllByRole("checkbox")).toEqual([]);
  });

  it("says a fully encrypted vault is encrypted, with no warning (#209)", async () => {
    vi.mocked(api.getEncryptionStatus).mockResolvedValue({
      enabled: true,
      locked: false,
      scopes: { activities: true, database: true, photos: true },
    });
    renderAt("/settings?tab=vault");
    await waitFor(() =>
      expect(
        screen.getByText("Raw files (activities and monitoring), the database and photos are encrypted at rest"),
      ).toBeTruthy(),
    );
    expect(screen.queryByText(/Not encrypted/)).toBeNull();
  });

  it("offers encryption while it is off, with no warning about a vault that was never encrypted (#209)", async () => {
    renderAt("/settings?tab=vault");
    await statusDelivered();
    expect(
      screen.getByText("Encrypt raw files (activities and monitoring), the database and photos at rest"),
    ).toBeTruthy();
    expect(screen.queryByText(/Not encrypted/)).toBeNull();
  });

  it("shows a failed encryption switch in the dialog while the Vault tab is open, without a toast", async () => {
    vi.mocked(api.enableEncryption).mockRejectedValue(new Error("disk full"));
    // Under StrictMode: a mounted-flag that only its cleanup writes would
    // read "unmounted" for good after the dev double-mount.
    await startEnablingEncryption({ strict: true });
    await waitFor(() => expect(screen.getByText("Failed: disk full")).toBeTruthy());
    expect(useToastStore.getState().addToast).not.toHaveBeenCalled();
  });

  it("shows a failed encryption switch in the dialog while the Vault tab is open, without a toast — the same words when the backend refuses with a plain string (#182)", async () => {
    vi.mocked(api.enableEncryption).mockRejectedValue("disk full");
    // Under StrictMode: a mounted-flag that only its cleanup writes would
    // read "unmounted" for good after the dev double-mount.
    await startEnablingEncryption({ strict: true });
    await waitFor(() => expect(screen.getByText("Failed: disk full")).toBeTruthy());
    expect(useToastStore.getState().addToast).not.toHaveBeenCalled();
  });

  it.each([
    ["another tab", () => fireEvent.click(screen.getByRole("tab", { name: "About" }))],
    ["another page", () => fireEvent.click(screen.getByRole("button", { name: "leave settings" }))],
  ])("falls back to a toast when the user went to %s before the switch failed", async (_where, leave) => {
    // The call is still in flight while the user wanders off.
    let fail!: (e: Error) => void;
    vi.mocked(api.enableEncryption).mockReturnValue(new Promise((_, reject) => (fail = reject)));
    await startEnablingEncryption();
    leave();
    expect(screen.queryByPlaceholderText("Confirm password")).toBeNull();
    fail(new Error("disk full"));
    await waitFor(() =>
      expect(useToastStore.getState().addToast).toHaveBeenCalledWith("error", expect.stringMatching(/Encryption failed.*disk full/)),
    );
  });

  it("reserves the scrollbar gutter on every tab, so a tall tab is not narrower", () => {
    renderAt("/settings");
    expect(screen.getByTestId("settings-scroll").className).toContain("overflow-y-scroll");
  });

  it("treats an unknown tab as General", () => {
    renderAt("/settings?tab=nope");
    expect(screen.getByRole("tab", { name: "General" }).getAttribute("aria-selected")).toBe("true");
    expect(screen.getByText("Theme")).toBeTruthy();
  });

  it("General writes theme, units and the geocoding opt-in through", async () => {
    vi.mocked(api.setSetting).mockResolvedValue(undefined);
    vi.mocked(api.startGeocoding).mockResolvedValue(undefined);
    renderAt("/settings");
    try {
      fireEvent.click(screen.getByRole("button", { name: "Dark" }));
      expect(useThemeStore.getState().mode).toBe("dark");
      fireEvent.click(screen.getByRole("button", { name: "Imperial · mi" }));
      expect(useUnitsStore.getState().mode).toBe("imperial");
      fireEvent.click(screen.getByRole("button", { name: "Metric · km" }));
      expect(useUnitsStore.getState().mode).toBe("metric");
    } finally {
      useThemeStore.getState().setMode("system");
      useUnitsStore.getState().setMode("metric");
    }
    // Off → on: stored, re-read, and existing activities get named now.
    fireEvent.click(screen.getByLabelText("Automatic location names"));
    await waitFor(() => expect(api.setSetting).toHaveBeenCalledWith("geocoding_enabled", "true"));
    await waitFor(() => expect(api.startGeocoding).toHaveBeenCalledTimes(1));
  });

  it("runs an import data source on the file picked in the dialog and clears the tile cache", async () => {
    vi.mocked(api.getImportDatasources).mockResolvedValue([
      { id: "runkeeper", name: "Runkeeper", description: "Runkeeper export", extensions: ["zip"] },
    ]);
    vi.mocked(open).mockResolvedValueOnce(null).mockResolvedValueOnce("/x/export.zip");
    vi.mocked(api.runImportDatasource).mockResolvedValue({ imported: 2, skipped: 1, failed: [] } as never);
    vi.mocked(api.clearTileCache).mockResolvedValue(undefined);
    renderAt("/settings");
    const importBtn = await screen.findByRole("button", { name: /^Import$/ });
    fireEvent.click(importBtn);
    await waitFor(() => expect(open).toHaveBeenCalledTimes(1));
    expect(api.runImportDatasource).not.toHaveBeenCalled();
    fireEvent.click(importBtn);
    await waitFor(() => expect(api.runImportDatasource).toHaveBeenCalledWith("runkeeper", "/x/export.zip"));
    await waitFor(() =>
      expect(useToastStore.getState().addToast).toHaveBeenCalledWith("success", "Runkeeper — imported 2, skipped 1"),
    );
    fireEvent.click(await screen.findByRole("button", { name: /Clear Cache/ }));
    await waitFor(() => expect(api.clearTileCache).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(useToastStore.getState().addToast).toHaveBeenCalledWith("success", "Tile cache cleared"));
  });

  it("switches the interface language from General, and General speaks it at once (#231)", async () => {
    renderAt("/settings");
    try {
      fireEvent.click(screen.getByRole("button", { name: "Language" }));
      // Each language is named in its own words, whatever the current one.
      expect(screen.getAllByRole("option").map((o) => o.textContent)).toEqual([
        "System", "English", "Русский", "Italiano",
      ]);
      fireEvent.click(screen.getByRole("option", { name: "Русский" }));
      expect(useLanguageStore.getState().pref).toBe("ru");
      expect(document.documentElement.lang).toBe("ru");
      expect(rows()).toEqual([
        "Тема",
        "Язык",
        "Единицы измерения",
        "Тренировок на странице",
        "Автоматические названия мест",
        "Импорт тренировок",
        "Кэш карты",
      ]);
      await waitFor(() => expect(screen.getByText("Тайлы карты на диске: 2 KB")).toBeTruthy());
      // Dates and numbers follow the language too (#232), and the hint says so.
      expect(screen.getByText("Язык интерфейса, дат и чисел")).toBeTruthy();
      fireEvent.click(screen.getByRole("button", { name: "Язык" }));
      fireEvent.click(screen.getByRole("option", { name: "Italiano" }));
      expect(screen.getByRole("button", { name: "Scuro" })).toBeTruthy();
    } finally {
      act(() => useLanguageStore.getState().setPref("system"));
    }
  });

  it("gives the focus back to the language picker after the remount a new language brings (#232)", () => {
    const picker = (name: string) => screen.getByRole("button", { name });
    const choose = (picker: HTMLElement, option: string) => {
      fireEvent.click(picker);
      fireEvent.click(screen.getByRole("option", { name: option }));
    };
    // App remounts the whole shell under a new language; here a fresh render
    // of the page stands in for it.
    const remount = () => {
      cleanup();
      renderAt("/settings");
    };
    try {
      renderAt("/settings");
      // English is what "System" already shows: nothing remounts, nothing to refocus.
      choose(picker("Language"), "English");
      remount();
      expect(document.activeElement).not.toBe(picker("Language"));
      choose(picker("Language"), "Русский");
      remount();
      expect(document.activeElement).toBe(picker("Язык"));
      // Once only: the next visit to Settings leaves the focus alone.
      remount();
      expect(document.activeElement).not.toBe(picker("Язык"));
    } finally {
      act(() => useLanguageStore.getState().setPref("system"));
    }
  });

  describe("locks the language picker while an operation of the page runs (#232)", () => {
    const picker = () => screen.getByRole("button", { name: "Language" }) as HTMLButtonElement;
    const toGeneral = () => fireEvent.click(screen.getByRole("tab", { name: "General" }));

    it("clearing the tile cache", async () => {
      let finish!: () => void;
      vi.mocked(api.clearTileCache).mockReturnValue(new Promise<void>((r) => (finish = r)));
      renderAt("/settings");
      expect(picker().disabled).toBe(false);
      // Enabled once the cache size is known.
      await waitFor(() => expect(screen.getByText(/2 KB/)).toBeTruthy());
      fireEvent.click(screen.getByRole("button", { name: /Clear Cache/ }));
      await waitFor(() => expect(picker().disabled).toBe(true));
      await act(async () => finish());
      await waitFor(() => expect(picker().disabled).toBe(false));
    });

    it("making a backup", async () => {
      vi.mocked(listen).mockResolvedValue(() => {});
      vi.mocked(save).mockResolvedValue("/tmp/backup.zip");
      let finish!: () => void;
      vi.mocked(api.backupVault).mockReturnValue(new Promise<void>((r) => (finish = r)));
      renderAt("/settings?tab=vault");
      fireEvent.click(screen.getByRole("button", { name: /Backup/ }));
      await waitFor(() => expect(api.backupVault).toHaveBeenCalled());
      toGeneral();
      expect(picker().disabled).toBe(true);
      await act(async () => finish());
      await waitFor(() => expect(picker().disabled).toBe(false));
    });

    it("restoring a backup", async () => {
      vi.mocked(open).mockResolvedValue("/tmp/backup.zip");
      let fail!: (e: unknown) => void;
      vi.mocked(api.restoreVault).mockReturnValue(new Promise((_, reject) => (fail = reject)));
      renderAt("/settings?tab=vault");
      fireEvent.click(screen.getByRole("button", { name: /Restore/ }));
      await waitFor(() => expect(api.restoreVault).toHaveBeenCalled());
      toGeneral();
      expect(picker().disabled).toBe(true);
      await act(async () => fail("disk full"));
      await waitFor(() => expect(picker().disabled).toBe(false));
    });

    it("switching encryption", async () => {
      let fail!: (e: unknown) => void;
      vi.mocked(api.enableEncryption).mockReturnValue(new Promise((_, reject) => (fail = reject)));
      renderAt("/settings?tab=vault");
      await waitFor(() => expect(screen.getByLabelText("Enable encryption")).toBeTruthy());
      fireEvent.click(screen.getByLabelText("Enable encryption"));
      fireEvent.change(screen.getByPlaceholderText("Password (min 8 characters)"), { target: { value: "hunter2hunter2" } });
      fireEvent.change(screen.getByPlaceholderText("Confirm password"), { target: { value: "hunter2hunter2" } });
      fireEvent.click(screen.getByRole("button", { name: "Enable Encryption" }));
      await waitFor(() => expect(api.enableEncryption).toHaveBeenCalled());
      toGeneral();
      expect(picker().disabled).toBe(true);
      await act(async () => fail("bad key"));
      await waitFor(() => expect(picker().disabled).toBe(false));
    });

    // The sections' own operations live in the busy store (#235): a vault
    // switch or move, a watch-folder preview or scan, an update install, a
    // monitoring delete — started on another tab, they lock the picker too.
    it.each(["vaultSwitch", "vaultMove", "watchPreview", "watchScan", "updateInstall", "monitoringDelete"] as const)(
      "a section's %s",
      (op) => {
        renderAt("/settings");
        expect(picker().disabled).toBe(false);
        act(() => useBusyStore.getState().start(op));
        expect(picker().disabled).toBe(true);
        act(() => useBusyStore.getState().finish(op));
        expect(picker().disabled).toBe(false);
      },
    );
  });

  it("speaks Russian on every tab of its own", async () => {
    act(() => useLanguageStore.getState().setPref("ru"));
    try {
      vi.mocked(api.getEncryptionStatus).mockResolvedValue({
        enabled: true,
        locked: false,
        scopes: { activities: true, database: false, photos: false },
      });
      renderAt("/settings?tab=vault");
      expect(screen.getAllByRole("tab").map((x) => x.textContent)).toEqual([
        "Общие",
        "Хранилище",
        "Гараж",
        "Плагины",
        "О программе",
      ]);
      expect(screen.getByRole("tablist").getAttribute("aria-label")).toBe("Разделы настроек");
      expect(rows()).toEqual(["Шифрование", "Резервное копирование"]);
      // An early, partly encrypted vault names what is left, in Russian.
      expect(await screen.findByText(/^Не зашифровано: база данных, фото\./)).toBeTruthy();
      screen.getByRole("button", { name: "Восстановить" });

      fireEvent.click(screen.getByRole("tab", { name: "О программе" }));
      expect(rows()).toEqual(["Syzify", "Лицензия", "Обратная связь"]);
      screen.getByText("Локальное хранилище тренировок");
      expect(await screen.findByText("Версия 9.9.9")).toBeTruthy();
      const license = screen.getByText("AGPL-3.0");
      expect(license.getAttribute("translate")).toBe("no");
      expect(license.parentElement?.textContent).toBe(
        "© 2026 Stanislav Zainullin · AGPL-3.0 с исключением для плагинов · Сторонние лицензии",
      );
      expect(screen.getByText("hi@example.com").parentElement?.textContent).toBe(
        "Сообщить об ошибке или предложить идею можно в GitHub Issues или письмом на hi@example.com",
      );
      // The tracker's name is a name: the same words, marked untranslatable.
      expect(screen.getByText("GitHub Issues", { selector: "a" }).getAttribute("translate")).toBe("no");
      screen.getByLabelText("GitHub Issues");
      screen.getByLabelText("Отправить отзыв");
      fireEvent.click(screen.getByText("исключением для плагинов"));
      screen.getByText("LEGAL exception");
    } finally {
      act(() => useLanguageStore.getState().setPref("system"));
    }
  });

  it("names the documents of the licence line and drops the email without an address", () => {
    renderAt("/settings?tab=about");
    fireEvent.click(screen.getByText("AGPL-3.0"));
    screen.getByText("LEGAL license");
    expect(screen.getByText("GitHub Issues", { selector: "a" }).parentElement?.textContent).toBe(
      "Report a bug or suggest a feature on GitHub Issues, or email hi@example.com",
    );
    cleanup();
    contact.email = "";
    try {
      renderAt("/settings?tab=about");
      expect(screen.getByText("GitHub Issues", { selector: "a" }).parentElement?.textContent).toBe(
        "Report a bug or suggest a feature on GitHub Issues",
      );
    } finally {
      contact.email = "hi@example.com";
    }
  });

  it("says when a backup failed", async () => {
    vi.mocked(listen).mockResolvedValue(() => {});
    vi.mocked(save).mockResolvedValue("/tmp/backup.zip");
    vi.mocked(api.backupVault).mockRejectedValue(commandError(null, "disk full"));
    renderAt("/settings?tab=vault");
    fireEvent.click(screen.getByRole("button", { name: "Backup" }));
    await waitFor(() =>
      expect(useToastStore.getState().addToast).toHaveBeenCalledWith("error", "Backup failed: disk full"),
    );
  });

  it.each([
    ["a string", "cache dir is busy", "en", "Could not clear the tile cache: cache dir is busy"],
    ["an Error", new Error("cache dir is busy"), "ru", "Не удалось очистить кэш карты: cache dir is busy"],
    [
      "a command error",
      commandError("tiles.clearFailed", "Failed to clear cache: busy", { detail: "busy" }),
      "ru",
      "Не удалось очистить кэш карты: Не удалось очистить кэш: busy",
    ],
  ] as const)("says why clearing the tile cache failed with %s, and unlocks", async (_kind, rejection, lang, text) => {
    act(() => useLanguageStore.getState().setPref(lang));
    try {
      vi.mocked(api.clearTileCache).mockRejectedValue(rejection);
      renderAt("/settings");
      await waitFor(() => expect(screen.getByText(/2 KB/)).toBeTruthy());
      fireEvent.click(screen.getByRole("button", { name: lang === "en" ? "Clear Cache" : "Очистить кэш" }));
      await waitFor(() => expect(useToastStore.getState().addToast).toHaveBeenCalledWith("error", text));
      expect(useBusyStore.getState().running).toEqual({});
    } finally {
      act(() => useLanguageStore.getState().setPref("system"));
    }
  });

  it("unlocks the page when the backup's progress listener cannot subscribe", async () => {
    vi.mocked(listen).mockRejectedValueOnce("event bus gone");
    vi.mocked(save).mockResolvedValue("/tmp/backup.zip");
    renderAt("/settings?tab=vault");
    fireEvent.click(screen.getByRole("button", { name: "Backup" }));
    await waitFor(() =>
      expect(useToastStore.getState().addToast).toHaveBeenCalledWith("error", "Backup failed: event bus gone"),
    );
    expect(useBusyStore.getState().running).toEqual({});
    expect(api.backupVault).not.toHaveBeenCalled();
    expect(useToastStore.getState().removeToast).toHaveBeenCalledWith("t1");
  });

  it("keeps a backup's indicator and the locks when the user leaves Settings and comes back (#235)", async () => {
    vi.mocked(listen).mockResolvedValue(() => {});
    vi.mocked(save).mockResolvedValue("/tmp/backup.zip");
    let finish!: () => void;
    vi.mocked(api.backupVault).mockReturnValue(new Promise<void>((r) => (finish = r)));
    const first = renderAt("/settings?tab=vault");
    fireEvent.click(screen.getByRole("button", { name: "Backup" }));
    await waitFor(() => expect(api.backupVault).toHaveBeenCalled());
    first.unmount();
    renderAt("/settings?tab=vault");
    const busy = screen.getByRole("button", { name: "Creating…" }) as HTMLButtonElement;
    expect(busy.disabled).toBe(true);
    expect((screen.getByRole("button", { name: "Restore" }) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getByRole("tab", { name: "General" }));
    expect((screen.getByRole("button", { name: "Language" }) as HTMLButtonElement).disabled).toBe(true);
    await act(async () => finish());
    await waitFor(() => expect((screen.getByRole("button", { name: "Language" }) as HTMLButtonElement).disabled).toBe(false));
  });

  it("says an encryption switch is still running when the page comes back without its dialog", async () => {
    let fail!: (e: unknown) => void;
    vi.mocked(api.enableEncryption).mockReturnValue(new Promise((_, reject) => (fail = reject)));
    const first = renderAt("/settings?tab=vault");
    await waitFor(() => expect(screen.getByLabelText("Enable encryption")).toBeTruthy());
    fireEvent.click(screen.getByLabelText("Enable encryption"));
    fireEvent.change(screen.getByPlaceholderText("Password (min 8 characters)"), { target: { value: "hunter2hunter2" } });
    fireEvent.change(screen.getByPlaceholderText("Confirm password"), { target: { value: "hunter2hunter2" } });
    fireEvent.click(screen.getByRole("button", { name: "Enable Encryption" }));
    await waitFor(() => expect(api.enableEncryption).toHaveBeenCalled());
    first.unmount();
    renderAt("/settings?tab=vault");
    expect(screen.getByRole("status").textContent).toBe("Encrypting…");
    expect((screen.getByLabelText("Enable encryption") as HTMLButtonElement).disabled).toBe(true);
    await act(async () => fail("disk full"));
    await waitFor(() => expect(screen.queryByRole("status")).toBeNull());
    expect((screen.getByLabelText("Enable encryption") as HTMLButtonElement).disabled).toBe(false);
    // The old page is gone, so only a toast can say why.
    expect(useToastStore.getState().addToast).toHaveBeenCalledWith("error", "Encryption failed: disk full");
  });

  it("says the same for a switch turning encryption off", async () => {
    vi.mocked(api.getEncryptionStatus).mockResolvedValue({
      enabled: true,
      locked: false,
      scopes: { activities: true, database: true, photos: true },
    });
    act(() => useBusyStore.getState().start("decrypt"));
    renderAt("/settings?tab=vault");
    expect(screen.getByRole("status").textContent).toBe("Decrypting…");
    await waitFor(() => expect((screen.getByLabelText("Disable encryption") as HTMLButtonElement).disabled).toBe(true));
  });

  it("unlocks a restore the backend did not start", async () => {
    vi.mocked(open).mockResolvedValue("/tmp/backup.zip");
    vi.mocked(api.restoreVault).mockResolvedValue({ restored: false, error: null, preserved_at: null });
    renderAt("/settings?tab=vault");
    fireEvent.click(screen.getByRole("button", { name: "Restore" }));
    await waitFor(() => expect(api.restoreVault).toHaveBeenCalled());
    await waitFor(() => expect(useBusyStore.getState().running).toEqual({}));
    expect(api.restartApp).not.toHaveBeenCalled();
  });

  it.each([
    [{ restored: true, error: null, preserved_at: null }, "success", "Vault restored. Restarting…"],
    [
      { restored: true, error: null, preserved_at: "/v/pre-restore-1" },
      "success",
      "Vault restored. Previous vault kept in /v/pre-restore-1. Restarting…",
    ],
    [
      {
        restored: true,
        error: commandError("restore.extractFailed", "Failed to extract photos/x.jpg: short read", {
          name: "photos/x.jpg",
          detail: "short read",
        }),
        preserved_at: "/v/pre-restore-1",
      },
      "warning",
      "Restore incomplete: Failed to extract photos/x.jpg: short read. Previous vault kept in /v/pre-restore-1. Restarting…",
    ],
  ] as const)("says how a restore went, then restarts: %j", async (result, level, text) => {
    vi.mocked(open).mockResolvedValue("/tmp/backup.zip");
    vi.mocked(api.restoreVault).mockResolvedValue(result);
    renderAt("/settings?tab=vault");
    fireEvent.click(screen.getByRole("button", { name: "Restore" }));
    await waitFor(() => expect(useToastStore.getState().addToast).toHaveBeenCalledWith(level, text));
    await waitFor(() => expect(api.restartApp).toHaveBeenCalled(), { timeout: 2000 });
  });

  /** Past a restore's point of no return the backend demands a restart
   * through the vault error: Settings has it read again, so a failed
   * restart lands on the vault error screen, said in the language (#244). */
  it("has the restart-required vault error read again after a restore", async () => {
    vi.mocked(open).mockResolvedValue("/tmp/backup.zip");
    vi.mocked(api.restoreVault).mockResolvedValue({ restored: true, error: null, preserved_at: null });
    vi.mocked(api.getVaultError).mockResolvedValueOnce(null).mockResolvedValue(
      commandError(
        "vault.restartRequired",
        "The vault was restored from a backup — the app must restart to reopen it.",
      ),
    );
    function VaultErrorProbe() {
      const { data } = useQuery({ queryKey: ["vaultError"], queryFn: () => api.getVaultError() });
      return <div data-testid="vault-error">{data === undefined ? "" : data ? errorText(data) : "none"}</div>;
    }
    act(() => useLanguageStore.getState().setPref("ru"));
    try {
      const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
      render(
        <QueryClientProvider client={qc}>
          <MemoryRouter initialEntries={["/settings?tab=vault"]}>
            <SettingsPage />
            <VaultErrorProbe />
          </MemoryRouter>
        </QueryClientProvider>,
      );
      await waitFor(() => expect(screen.getByTestId("vault-error").textContent).toBe("none"));
      fireEvent.click(screen.getByRole("button", { name: "Восстановить" }));
      await waitFor(() =>
        expect(screen.getByTestId("vault-error").textContent).toBe(
          "Хранилище восстановлено из резервной копии — приложение нужно перезапустить, чтобы открыть его снова.",
        ),
      );
      await waitFor(() => expect(api.restartApp).toHaveBeenCalled(), { timeout: 2000 });
    } finally {
      act(() => useLanguageStore.getState().setPref("system"));
    }
  });

  it("falls back to a toast when encryption could not be turned off after the user left the tab", async () => {
    vi.mocked(api.getEncryptionStatus).mockResolvedValue({
      enabled: true,
      locked: false,
      scopes: { activities: true, database: true, photos: true },
    });
    let fail!: (e: unknown) => void;
    vi.mocked(api.disableEncryption).mockReturnValue(new Promise((_, reject) => (fail = reject)));
    renderAt("/settings?tab=vault");
    await waitFor(() => expect(screen.getByLabelText("Disable encryption")).toBeTruthy());
    fireEvent.click(screen.getByLabelText("Disable encryption"));
    fireEvent.change(screen.getByPlaceholderText("Current password"), { target: { value: "hunter2hunter2" } });
    fireEvent.click(screen.getByRole("button", { name: "Disable Encryption" }));
    await waitFor(() => expect(api.disableEncryption).toHaveBeenCalled());
    fireEvent.click(screen.getByRole("tab", { name: "About" }));
    await act(async () => fail("wrong password"));
    await waitFor(() =>
      expect(useToastStore.getState().addToast).toHaveBeenCalledWith(
        "error",
        "Disabling encryption failed: wrong password",
      ),
    );
  });

  it("asks for a long enough password in the words of the language", async () => {
    act(() => useLanguageStore.getState().setPref("ru"));
    try {
      renderAt("/settings?tab=vault");
      await waitFor(() => expect(screen.getByLabelText("Включить шифрование")).toBeTruthy());
      fireEvent.click(screen.getByLabelText("Включить шифрование"));
      fireEvent.change(screen.getByPlaceholderText("Пароль (не меньше 8 символов)"), { target: { value: "short" } });
      fireEvent.change(screen.getByPlaceholderText("Повторите пароль"), { target: { value: "short" } });
      fireEvent.click(screen.getByRole("button", { name: "Включить шифрование" }));
      expect(await screen.findByText("Пароль должен быть не короче 8 символов.")).toBeTruthy();
      expect(api.enableEncryption).not.toHaveBeenCalled();
    } finally {
      act(() => useLanguageStore.getState().setPref("system"));
    }
  });

  it("reports a data source import that failed outright or for some files", async () => {
    vi.mocked(api.getImportDatasources).mockResolvedValue([
      { id: "runkeeper", name: "Runkeeper", description: "Runkeeper export", extensions: ["zip"] },
    ]);
    vi.mocked(open).mockResolvedValue("/x/export.zip");
    vi.mocked(api.runImportDatasource)
      .mockResolvedValueOnce({ imported: 1, skipped: 0, failed: ["a.gpx", "b.gpx"] } as never)
      .mockRejectedValueOnce("archive is damaged");
    renderAt("/settings");
    const importBtn = await screen.findByRole("button", { name: /^Import$/ });
    fireEvent.click(importBtn);
    await waitFor(() =>
      expect(useToastStore.getState().addToast).toHaveBeenCalledWith(
        "success",
        "Runkeeper — imported 1, skipped 0, failed 2",
      ),
    );
    fireEvent.click(importBtn);
    await waitFor(() =>
      expect(useToastStore.getState().addToast).toHaveBeenCalledWith(
        "error",
        "Runkeeper import failed: archive is damaged",
      ),
    );
  });

  it("validates the encryption password dialog and lets it be cancelled", async () => {
    renderAt("/settings?tab=vault");
    await waitFor(() => expect(screen.getByLabelText("Enable encryption")).toBeTruthy());
    fireEvent.click(screen.getByLabelText("Enable encryption"));
    const enable = () => fireEvent.click(screen.getByRole("button", { name: "Enable Encryption" }));
    enable();
    expect(screen.getByText("Password must be at least 8 characters.")).toBeTruthy();
    fireEvent.change(screen.getByPlaceholderText("Password (min 8 characters)"), { target: { value: "hunter2hunter2" } });
    fireEvent.change(screen.getByPlaceholderText("Confirm password"), { target: { value: "different" } });
    enable();
    expect(screen.getByText("Passwords do not match.")).toBeTruthy();
    expect(api.enableEncryption).not.toHaveBeenCalled();
    // Cancelling closes the dialog with nothing sent.
    fireEvent.change(screen.getByPlaceholderText("Confirm password"), { target: { value: "hunter2hunter2" } });
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByPlaceholderText("Confirm password")).toBeNull();
    expect(api.enableEncryption).not.toHaveBeenCalled();
  });

  it("opens the disable dialog for an encrypted vault, reports a wrong password and cancels", async () => {
    vi.mocked(api.getEncryptionStatus).mockResolvedValue({
      enabled: true,
      locked: false,
      scopes: { activities: true, database: false, photos: false },
    });
    vi.mocked(api.disableEncryption).mockRejectedValue(new Error("bad key"));
    renderAt("/settings?tab=vault");
    await waitFor(() => expect(screen.getByLabelText("Disable encryption")).toBeTruthy());
    fireEvent.click(screen.getByLabelText("Disable encryption"));
    fireEvent.change(screen.getByPlaceholderText("Current password"), { target: { value: "nope" } });
    fireEvent.click(screen.getByRole("button", { name: "Disable Encryption" }));
    await waitFor(() => expect(screen.getByText(/Wrong password or error/)).toBeTruthy());
    // The Vault tab is still open: the dialog shows it, no toast.
    expect(useToastStore.getState().addToast).not.toHaveBeenCalled();
    // The backend's own "wrong password" says so alone (#236).
    vi.mocked(api.disableEncryption).mockRejectedValue(commandError("vault.wrongPassword", "Wrong password"));
    fireEvent.click(screen.getByRole("button", { name: "Disable Encryption" }));
    await waitFor(() => expect(screen.getByText("Wrong password")).toBeTruthy());
    expect(screen.queryByText(/Wrong password or error/)).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByPlaceholderText("Current password")).toBeNull();
  });

  it("counts the backup's megabytes in the interface language (#232)", async () => {
    act(() => useLanguageStore.getState().setPref("ru"));
    try {
      let progress: ((e: { payload: { processed: number; total: number } }) => void) | undefined;
      vi.mocked(listen).mockImplementation(async (_event, handler) => {
        progress = handler as typeof progress;
        return () => {};
      });
      vi.mocked(save).mockResolvedValue("/tmp/backup.zip");
      let finish!: () => void;
      vi.mocked(api.backupVault).mockReturnValue(new Promise<void>((r) => (finish = r)));
      renderAt("/settings?tab=vault");
      fireEvent.click(screen.getByRole("button", { name: "Создать копию" }));
      await waitFor(() => expect(progress).toBeDefined());
      expect(useToastStore.getState().addToast).toHaveBeenCalledWith(
        "info",
        expect.stringMatching(/^Создание резервной копии… 0\s%$/),
        undefined,
        true,
      );
      progress!({ payload: { processed: 1.5 * 1048576, total: 12345 * 1048576 } });
      expect(useToastStore.getState().updateToast).toHaveBeenCalledWith("t1", {
        message: expect.stringMatching(/^Создание резервной копии… 0\s% \(2\/12\xa0345 МБ\)$/),
      });
      await act(async () => finish());
    } finally {
      act(() => useLanguageStore.getState().setPref("system"));
    }
  });

  it("About opens the feedback form and routes the email link through the opener", async () => {
    vi.mocked(openUrl).mockResolvedValue(undefined);
    renderAt("/settings?tab=about");
    fireEvent.click(screen.getByLabelText("Send feedback"));
    expect(useFeedbackStore.getState().isOpen).toBe(true);
    fireEvent.click(screen.getByText("hi@example.com"));
    await waitFor(() => expect(openUrl).toHaveBeenCalledWith("mailto:hi@example.com"));
  });

  /** The opener refuses with a string (Tauri) or an Error (its JS side):
   * the toast says the words alone (#182). */
  for (const [kind, rejection] of [
    ["a string", "no mail client"],
    ["an Error", new Error("no mail client")],
  ] as const) {
    it(`says why the About email link did not open with ${kind}`, async () => {
      vi.mocked(openUrl).mockRejectedValueOnce(rejection);
      renderAt("/settings?tab=about");
      fireEvent.click(screen.getByText("hi@example.com"));
      await waitFor(() =>
        expect(useToastStore.getState().addToast).toHaveBeenCalledWith("error", "Failed to open email client: no mail client"),
      );
    });
  }
});
