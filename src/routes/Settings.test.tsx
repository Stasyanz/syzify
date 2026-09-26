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
  },
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
vi.mock("../components/settings/PluginRegistry", () => ({ PluginRegistry: () => <div>PLUGIN REGISTRY</div> }));
vi.mock("../components/settings/UpdateCheck", () => ({ UpdateCheck: () => <div>UPDATE CHECK</div> }));
vi.mock("../components/settings/LegalModal", () => ({
  LegalModal: ({ doc }: { doc: string }) => <div>LEGAL {doc}</div>,
}));

import { SettingsPage } from "./Settings";
import { api } from "../lib/tauri";
import { useToastStore } from "../stores/toastStore";

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

  it("opens on General with the four tabs and only that section", async () => {
    renderAt("/settings");
    const tabs = screen.getAllByRole("tab");
    expect(tabs.map((t) => t.textContent)).toEqual(["General", "Vault", "Plugins", "About"]);
    expect(tabs.map((t) => t.getAttribute("aria-selected"))).toEqual(["true", "false", "false", "false"]);
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
    expect(screen.getAllByRole("tab").map((t) => t.tabIndex)).toEqual([0, -1, -1, -1]);
    expect(screen.getAllByRole("tab").map((t) => t.getAttribute("aria-controls"))).toEqual([
      screen.getByRole("tabpanel").id, null, null, null,
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
    await waitFor(() => expect(screen.getByText("Failed: Error: disk full")).toBeTruthy());
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

  it("treats an unknown tab as General", () => {
    renderAt("/settings?tab=nope");
    expect(screen.getByRole("tab", { name: "General" }).getAttribute("aria-selected")).toBe("true");
    expect(screen.getByText("Theme")).toBeTruthy();
  });
});
