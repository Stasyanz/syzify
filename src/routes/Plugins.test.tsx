// @vitest-environment happy-dom
import { describe, it, expect, afterEach, vi } from "vitest";
import { render, cleanup, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router";
import { api } from "../lib/tauri";
import type { PluginInfo } from "../lib/types";
import { PluginsPage, secretsDisclosure, secretsSealed } from "./Plugins";

vi.mock("../lib/tauri", () => ({
  api: {
    getPlugins: vi.fn(),
    getEncryptionStatus: vi.fn(),
  },
}));
vi.mock("../stores/confirmStore", () => ({ confirmDialog: vi.fn() }));

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
        <PluginsPage />
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

describe("PluginsPage", () => {
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
