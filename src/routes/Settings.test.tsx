// @vitest-environment happy-dom
import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation, useNavigate } from "react-router";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

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
  },
}));
vi.mock("../lib/contact", () => ({ CONTACT_EMAIL: "hi@example.com", GITHUB_ISSUES_URL: "https://example.com/issues" }));
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
import { useFeedbackStore } from "../stores/feedbackStore";
import { open } from "@tauri-apps/plugin-dialog";
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
  afterEach(cleanup);

  it("opens on General with the five tabs and only that section", async () => {
    renderAt("/settings");
    const tabs = screen.getAllByRole("tab");
    expect(tabs.map((t) => t.textContent)).toEqual(["General", "Vault", "Garage", "Plugins", "About"]);
    expect(tabs.map((t) => t.getAttribute("aria-selected"))).toEqual(["true", "false", "false", "false", "false"]);
    expect(rows()).toEqual([
      "Theme",
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
    fireEvent.click(screen.getByLabelText("Raw files (activities & monitoring)"));
    fireEvent.click(screen.getByRole("button", { name: "Enable Encryption" }));
  }

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
    fireEvent.click(screen.getByRole("button", { name: "dark" }));
    expect(useThemeStore.getState().mode).toBe("dark");
    fireEvent.click(screen.getByRole("button", { name: "Imperial · mi" }));
    expect(useUnitsStore.getState().mode).toBe("imperial");
    useThemeStore.getState().setMode("system");
    useUnitsStore.getState().setMode("metric");
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
    fireEvent.change(screen.getByPlaceholderText("Confirm password"), { target: { value: "hunter2hunter2" } });
    enable();
    expect(screen.getByText("Select at least one thing to encrypt.")).toBeTruthy();
    expect(api.enableEncryption).not.toHaveBeenCalled();
    // Ticking a scope and cancelling closes the dialog with nothing sent.
    fireEvent.click(screen.getByLabelText("Photos"));
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
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByPlaceholderText("Current password")).toBeNull();
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
