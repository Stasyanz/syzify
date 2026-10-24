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
import { t, translate, useLanguageStore, type Translator } from "../../lib/i18n";
import { PluginRegistry, permissionLabel, secretsDisclosure, secretsSealed } from "./PluginRegistry";

const ru: Translator = (key, ...args) => translate("ru", key, ...args);

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

afterEach(() => {
  cleanup();
  useLanguageStore.getState().setPref("system");
});

describe("secretsDisclosure", () => {
  it("speaks only for a secret-holding plugin in a plaintext vault", () => {
    expect(secretsDisclosure(["data:secret"], false, t)).toMatch(/unencrypted/);
    expect(secretsDisclosure(["data:secret"], true, t)).toBeNull();
    expect(secretsDisclosure(["data:secret"], null, t)).toBeNull();
    expect(secretsDisclosure(["data:own"], false, t)).toBeNull();
  });

  it("is said in the language it is given", () => {
    expect(secretsDisclosure(["data:secret"], false, ru)).toBe(
      "Секреты хранятся незашифрованными, пока не включено шифрование хранилища (Настройки → Хранилище → Шифрование).",
    );
  });
});

describe("permissionLabel", () => {
  it("names the permissions the host knows and shows any other as asked", () => {
    expect(permissionLabel("read:hrv", t)).toBe("Read HRV");
    expect(permissionLabel("net:host=api.example.com", t)).toBe("Network: api.example.com");
    expect(permissionLabel("net:host=api.example.com", ru)).toBe("Сеть: api.example.com");
    expect(permissionLabel("gear:write", ru)).toMatch(/^Изменение снаряжения/);
    expect(permissionLabel("write:everything", ru)).toBe("write:everything");
    // Own keys only: an object's built-in names are not labels.
    expect(permissionLabel("constructor", t)).toBe("constructor");
    expect(permissionLabel("toString", ru)).toBe("toString");
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

  it("speaks Russian around the plugin's own name and words", async () => {
    useLanguageStore.getState().setPref("ru");
    mocked.getPlugins.mockResolvedValue([
      plugin({ author: "Ann", contributes: ["sync.source"] }),
      plugin({
        id: "com.test.signed",
        name: "Signed",
        enabled: false,
        signed: true,
        key_fingerprint: "ab:cd",
        permissions: ["read:laps"],
        network_hosts: [],
      }),
    ]);
    mocked.getEncryptionStatus.mockResolvedValue({
      enabled: false,
      locked: false,
      scopes: { activities: false, database: false, photos: false },
    });
    renderPage();
    await screen.findByText("автор: Ann");
    screen.getByText(/^Плагины расширяют Syzify локально/);
    screen.getByText("Включён");
    screen.getByText("Выключен");
    screen.getByText("Без подписи");
    expect(screen.getByText("Самоподписанный").closest("[title]")?.getAttribute("title")).toBe(
      "Самоподписанный пакет — целостность проверена, но имя автора заявлено самим автором и не проверено. Ключ ab:cd",
    );
    screen.getByRole("button", { name: "Синхронизировать" });
    screen.getByRole("button", { name: "Включить" });
    screen.getByText("Хранит секреты (токены)");
    screen.getByText("Чтение кругов");
    screen.getByText("Подключается к:");
    // The version is the plugin's own and reads the same in every language.
    expect(screen.getAllByText("v0.1.0")[0].getAttribute("translate")).toBe("no");
  });

  it("says what an install, an uninstall and a refused enable did", async () => {
    useToastStore.setState({ toasts: [] });
    mocked.getPlugins.mockResolvedValue([
      plugin({ enabled: false, signed: true, key_fingerprint: null, permissions: [], network_hosts: [] }),
    ]);
    mocked.getEncryptionStatus.mockResolvedValue({
      enabled: false,
      locked: false,
      scopes: { activities: false, database: false, photos: false },
    });
    vi.mocked(open).mockResolvedValue("/tmp/plugin.json");
    mocked.installPluginFromFile.mockResolvedValue(plugin({ name: "Route planner" }));
    mocked.setPluginEnabled.mockRejectedValue("manifest changed");
    mocked.uninstallPlugin.mockResolvedValue(undefined);
    vi.mocked(confirmDialog).mockResolvedValue(true);
    renderPage();
    // A signed package without a fingerprint still says whose key it is not.
    const badge = await screen.findByText("Self-signed");
    expect(badge.closest("[title]")?.getAttribute("title")).toMatch(/Key \?$/);
    const messages = () => useToastStore.getState().toasts.map((x) => x.message);

    fireEvent.click(screen.getByRole("button", { name: /Install plugin/ }));
    await waitFor(() => expect(messages()).toContain("Installed “Route planner”. Review its access, then enable it."));
    fireEvent.click(screen.getByRole("button", { name: "Enable" }));
    await waitFor(() => expect(messages()).toContain("Could not enable: manifest changed"));
    fireEvent.click(screen.getByRole("button", { name: "Uninstall" }));
    await waitFor(() => expect(messages()).toContain("Uninstalled “Sync”."));
    expect(vi.mocked(confirmDialog)).toHaveBeenCalledWith(
      expect.objectContaining({
        title: "Uninstall plugin",
        message: "Uninstall “Sync”? Its stored data will be removed too.",
        confirmLabel: "Uninstall",
      }),
    );
  });

  it("says how to start when nothing is installed", async () => {
    useLanguageStore.getState().setPref("ru");
    mocked.getPlugins.mockResolvedValue([]);
    mocked.getEncryptionStatus.mockResolvedValue({
      enabled: false,
      locked: false,
      scopes: { activities: false, database: false, photos: false },
    });
    renderPage();
    await screen.findByText("Плагинов пока нет.");
    const file = screen.getByText("plugin.json");
    expect(file.getAttribute("translate")).toBe("no");
    expect(file.parentElement?.textContent).toBe("Чтобы начать, установите манифест plugin.json.");
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
