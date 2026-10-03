// @vitest-environment happy-dom
import { describe, it, expect, afterEach, vi } from "vitest";
import { render, cleanup, screen, waitFor, fireEvent } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, Route, Routes } from "react-router";
import { api } from "../../lib/tauri";
import { open } from "@tauri-apps/plugin-dialog";
import { confirmDialog } from "../../stores/confirmStore";
import { useToastStore } from "../../stores/toastStore";
import type { PluginInfo } from "../../lib/types";
import { PluginRegistry, secretsDisclosure, secretsSealed } from "./PluginRegistry";

vi.mock("../../lib/tauri", () => ({
  api: {
    getPlugins: vi.fn(),
    getEncryptionStatus: vi.fn(),
    setPluginEnabled: vi.fn(),
    uninstallPlugin: vi.fn(),
    installPluginFromFile: vi.fn(),
    installPluginFromPackage: vi.fn(),
  },
}));
vi.mock("../../stores/confirmStore", () => ({ confirmDialog: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));

const mocked = vi.mocked(api);

function plugin(over: Partial<PluginInfo> = {}): PluginInfo {
  return {
    id: "com.test.sync",
    name: "Sync",
    version: "0.1.0",
    author: null,
    description: null,
    enabled: true,
    contributes: ["route.planner"],
    permissions: ["data:secret", "net:host=sso.example.com"],
    network_hosts: ["sso.example.com"],
    signed: false,
    key_fingerprint: null,
    ...over,
  } as PluginInfo;
}

function renderPage() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter>
        <PluginRegistry />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

afterEach(cleanup);

describe("secretsDisclosure", () => {
  it("speaks only for a secret-holding plugin in a plaintext vault", () => {
    expect(secretsDisclosure(["data:secret"], false)).toMatch(/unencrypted/);
    expect(secretsDisclosure(["data:secret"], true)).toBeNull();
    expect(secretsDisclosure(["data:secret"], null)).toBeNull();
    expect(secretsDisclosure(["data:own"], false)).toBeNull();
  });

  it("judges by the scopes, not by the presence of a lock, and fails towards the warning", () => {
    const off = { activities: false, database: false, photos: false };
    // A lock with every scope off (a disable that stopped half-way): not sealed.
    expect(secretsSealed({ enabled: true, locked: false, scopes: off }, false)).toBe(false);
    expect(secretsSealed({ enabled: true, locked: true, scopes: { ...off, photos: true } }, false)).toBe(true);
    expect(secretsSealed(undefined, false)).toBeNull();
    expect(secretsSealed(undefined, true)).toBe(false);
  });
});

describe("PluginRegistry", () => {
  it("opens the sync page of an enabled plugin contributing sync.source", async () => {
    mocked.getPlugins.mockResolvedValue([
      plugin({ id: "com.test.sync", contributes: ["sync.source"] }),
      plugin({ id: "com.test.off", name: "Off", contributes: ["sync.source"], enabled: false }),
      plugin({ id: "com.test.widget", name: "Widget", contributes: ["dashboard.widget"] }),
    ]);
    mocked.getEncryptionStatus.mockResolvedValue({
      enabled: true,
      locked: false,
      scopes: { activities: true, database: false, photos: false },
    });
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(
      <QueryClientProvider client={qc}>
        <MemoryRouter initialEntries={["/settings?tab=plugins"]}>
          <Routes>
            <Route path="/settings" element={<PluginRegistry />} />
            <Route path="/plugin/:pluginId/sync" element={<div>SYNC PAGE</div>} />
          </Routes>
        </MemoryRouter>
      </QueryClientProvider>,
    );
    const sync = await screen.findAllByRole("button", { name: "Sync" });
    expect(sync).toHaveLength(1);
    fireEvent.click(sync[0]);
    await waitFor(() => expect(screen.getByText("SYNC PAGE")).toBeTruthy());
  });

  it("labels the permission and discloses plain secrets until encryption is on", async () => {
    mocked.getPlugins.mockResolvedValue([plugin()]);
    mocked.getEncryptionStatus.mockResolvedValue({
      enabled: false,
      locked: false,
      scopes: { activities: false, database: false, photos: false },
    });
    renderPage();
    await waitFor(() => expect(screen.getByText("Stores secrets (tokens)")).toBeTruthy());
    await waitFor(() => expect(screen.getByText(/stored unencrypted/)).toBeTruthy());
  });

  it("warns when the status cannot be read at all", async () => {
    mocked.getPlugins.mockResolvedValue([plugin()]);
    mocked.getEncryptionStatus.mockRejectedValue(new Error("ipc down"));
    renderPage();
    await waitFor(() => expect(screen.getByText(/stored unencrypted/)).toBeTruthy());
  });

  it("drops the note once any scope is encrypted", async () => {
    mocked.getPlugins.mockResolvedValue([plugin()]);
    mocked.getEncryptionStatus.mockResolvedValue({
      enabled: true,
      locked: false,
      scopes: { activities: true, database: false, photos: false },
    });
    renderPage();
    await waitFor(() => expect(screen.getByText("Stores secrets (tokens)")).toBeTruthy());
    expect(screen.queryByText(/stored unencrypted/)).toBeNull();
  });
});

describe("PluginRegistry refusals (#182)", () => {
  const lastToast = () => useToastStore.getState().toasts.slice(-1)[0];

  /** The backend refuses with a bare string; a dialog or JS failure is an
   * Error. Every toast carries the words alone. */
  for (const [kind, make] of [
    ["a string", (m: string) => m],
    ["an Error", (m: string) => new Error(m)],
  ] as const) {
    it(`names the reason when a toggle, an uninstall or an install fails with ${kind}`, async () => {
      useToastStore.setState({ toasts: [] });
      mocked.getPlugins.mockResolvedValue([plugin()]);
      mocked.getEncryptionStatus.mockResolvedValue({
        enabled: false,
        locked: false,
        scopes: { activities: false, database: false, photos: false },
      });
      mocked.setPluginEnabled.mockRejectedValue(make("plugin is mid-sync"));
      mocked.uninstallPlugin.mockRejectedValue(make("vault locked"));
      mocked.installPluginFromFile.mockRejectedValue(make("manifest is not JSON"));
      vi.mocked(confirmDialog).mockResolvedValue(true);
      vi.mocked(open).mockResolvedValue("/tmp/plugin.json");
      renderPage();
      await waitFor(() => expect(screen.getByText("Sync")).toBeTruthy());

      fireEvent.click(screen.getByRole("button", { name: "Disable" }));
      await waitFor(() => expect(lastToast()?.message).toBe("Could not disable: plugin is mid-sync"));
      fireEvent.click(screen.getByRole("button", { name: "Uninstall" }));
      await waitFor(() => expect(lastToast()?.message).toBe("Uninstall failed: vault locked"));
      fireEvent.click(screen.getByRole("button", { name: /Install plugin/ }));
      await waitFor(() => expect(lastToast()?.message).toBe("Install failed: manifest is not JSON"));
      // A signed package goes through the other install call.
      vi.mocked(open).mockResolvedValue("/tmp/sync.syzify-ext");
      mocked.installPluginFromPackage.mockRejectedValue(make("signature does not verify"));
      fireEvent.click(screen.getByRole("button", { name: /Install plugin/ }));
      await waitFor(() => expect(lastToast()?.message).toBe("Install failed: signature does not verify"));
      // The file dialog itself failing is an install failure too, not an
      // unhandled rejection.
      vi.mocked(open).mockRejectedValue(make("dialog unavailable"));
      fireEvent.click(screen.getByRole("button", { name: /Install plugin/ }));
      await waitFor(() => expect(lastToast()?.message).toBe("Install failed: dialog unavailable"));
      expect(useToastStore.getState().toasts.every((t) => t.type === "error" && !t.message.includes("Error:"))).toBe(true);
    });
  }
});
