// @vitest-environment happy-dom
import { describe, it, expect, afterEach, vi } from "vitest";
import { render, cleanup, screen, waitFor, fireEvent } from "@testing-library/react";
import type { RenderResult } from "@testing-library/react";

vi.mock("./lib/tauri", () => ({
  api: {
    getEncryptionStatus: vi.fn(),
    getVaultError: vi.fn().mockResolvedValue(null),
    switchVault: vi.fn().mockResolvedValue(""),
    restartApp: vi.fn().mockResolvedValue(undefined),
  },
  isTauri: () => false,
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
}));

vi.mock("./hooks/useDropImport", () => ({
  useDropImport: () => ({ dragging: false }),
}));

vi.mock("./hooks/useWatchFolderListener", () => ({
  useWatchFolderListener: () => ({
    pendingFiles: [],
    importing: false,
    handleImport: vi.fn(),
    handleDismiss: vi.fn(),
  }),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

afterEach(cleanup);

// App.tsx holds a module-level QueryClient, so its cache would leak between
// tests — re-import the module fresh (mocks survive vi.resetModules).
async function bootApp(
  status: Promise<unknown>,
  vaultError: string | null = null,
): Promise<RenderResult> {
  vi.resetModules();
  const { api } = await import("./lib/tauri");
  vi.mocked(api.getEncryptionStatus).mockReturnValue(
    status as ReturnType<typeof api.getEncryptionStatus>,
  );
  vi.mocked(api.getVaultError).mockResolvedValue(vaultError);
  const { default: App } = await import("./App");
  return render(<App />);
}

const UNLOCKED = Promise.resolve({
  enabled: false,
  locked: false,
  scopes: { activities: false, database: false, photos: false },
});

// bootApp re-imports the whole App module graph (vi.resetModules), which
// can take seconds under a full-suite run — hence the raised timeouts.
describe("App boot", { timeout: 20_000 }, () => {
  it("shows the pulsing logo while the encryption status is loading", async () => {
    await bootApp(new Promise(() => {}));
    const status = screen.getByRole("status");
    expect(status.querySelector("svg.boot-logo")).toBeTruthy();
  });

  it("replaces the splash once the status resolves (locked vault)", async () => {
    await bootApp(
      Promise.resolve({
        enabled: true,
        locked: true,
        scopes: { activities: true, database: true, photos: true },
      }),
    );
    await waitFor(() => expect(screen.queryByRole("status")).toBeNull());
  });

  it("vault error screen switches to a picked vault and restarts", async () => {
    await bootApp(UNLOCKED, "Failed to run migrations: DatabaseTooFarAhead");
    await screen.findByText("Can't open your vault");

    const { open } = await import("@tauri-apps/plugin-dialog");
    vi.mocked(open).mockResolvedValue("/picked/vault");
    const { api } = await import("./lib/tauri");
    fireEvent.click(screen.getByText("Open another vault…"));

    await waitFor(() =>
      expect(api.switchVault).toHaveBeenCalledWith("/picked/vault", true),
    );
    await waitFor(() => expect(api.restartApp).toHaveBeenCalled());
  });

  it("create-new passes expect_existing=false; cancelling picks nothing", async () => {
    await bootApp(UNLOCKED, "boom");
    await screen.findByText("Can't open your vault");
    const { open } = await import("@tauri-apps/plugin-dialog");
    const { api } = await import("./lib/tauri");

    vi.mocked(open).mockResolvedValue("/picked/new");
    fireEvent.click(screen.getByText("Create new vault…"));
    await waitFor(() =>
      expect(api.switchVault).toHaveBeenCalledWith("/picked/new", false),
    );

    // Cancelled picker (null) must not switch again.
    vi.mocked(api.switchVault).mockClear();
    vi.mocked(open).mockResolvedValue(null);
    fireEvent.click(screen.getByText("Open another vault…"));
    await waitFor(() => expect(open).toHaveBeenCalledTimes(2));
    expect(api.switchVault).not.toHaveBeenCalled();
  });

  it("shows the switch error and does not restart when switching fails", async () => {
    await bootApp(UNLOCKED, "boom");
    await screen.findByText("Can't open your vault");
    const { open } = await import("@tauri-apps/plugin-dialog");
    const { api } = await import("./lib/tauri");
    // The api mock instances survive vi.resetModules — drop calls leaked
    // from the earlier successful-switch tests.
    vi.mocked(api.restartApp).mockClear();
    vi.mocked(open).mockResolvedValue("/no/vault/here");
    vi.mocked(api.switchVault).mockRejectedValue('No vault found in "/no/vault/here"');

    fireEvent.click(screen.getByText("Open another vault…"));

    await screen.findByText('No vault found in "/no/vault/here"');
    expect(api.restartApp).not.toHaveBeenCalled();
  });
});