// @vitest-environment happy-dom
import { describe, it, expect, afterEach, vi } from "vitest";
import { render, cleanup, screen, waitFor } from "@testing-library/react";
import type { RenderResult } from "@testing-library/react";

vi.mock("./lib/tauri", () => ({
  api: {
    getEncryptionStatus: vi.fn(),
    getVaultError: vi.fn().mockResolvedValue(null),
  },
  isTauri: () => false,
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
async function bootApp(status: Promise<unknown>): Promise<RenderResult> {
  vi.resetModules();
  const { api } = await import("./lib/tauri");
  vi.mocked(api.getEncryptionStatus).mockReturnValue(
    status as ReturnType<typeof api.getEncryptionStatus>,
  );
  const { default: App } = await import("./App");
  return render(<App />);
}

describe("App boot", () => {
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
});