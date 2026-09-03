// @vitest-environment happy-dom
import { describe, it, expect, afterEach, beforeEach, vi } from "vitest";
import { render, cleanup, fireEvent, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "../../lib/tauri";
import { confirmDialog } from "../../stores/confirmStore";
import { useToastStore } from "../../stores/toastStore";
import {
  VaultLocation,
  moveVaultMessage,
  protectedFolderNote,
  switchVaultMessage,
} from "./VaultLocation";

vi.mock("../../lib/tauri", () => ({
  api: {
    getVaultPath: vi.fn(),
    switchVault: vi.fn(),
    relocateVault: vi.fn(),
    restartApp: vi.fn(),
  },
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));
vi.mock("../../stores/confirmStore", () => ({ confirmDialog: vi.fn() }));

const mocked = vi.mocked(api);
const CURRENT = "/Users/me/Syzify";
const OTHER = "/Volumes/Data/Syzify";

function renderLocation() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <VaultLocation />
    </QueryClientProvider>
  );
}

afterEach(cleanup);
beforeEach(() => {
  vi.clearAllMocks();
  useToastStore.setState({ toasts: [] });
  mocked.getVaultPath.mockResolvedValue(CURRENT);
  mocked.restartApp.mockResolvedValue(undefined);
  vi.mocked(open).mockResolvedValue(OTHER);
});

describe("dialog copy", () => {
  it("warns only for macOS-protected folders", () => {
    expect(protectedFolderNote("/Users/me/Documents/Syzify")).toContain("Full Disk Access");
    expect(protectedFolderNote("/Users/me/Desktop")).toContain("Full Disk Access");
    expect(protectedFolderNote("/Users/me/Downloadsx")).toBe("");
    expect(protectedFolderNote("/Volumes/Data/Syzify")).toBe("");
  });

  it("switch says the current vault is left in place; move says files move", () => {
    const sw = switchVaultMessage(OTHER, CURRENT);
    expect(sw).toContain(`Open the vault in "${OTHER}"`);
    expect(sw).toContain(`"${CURRENT}" stays where it is`);
    expect(sw).toContain("nothing is moved");
    // Path still loading → no dangling quotes around "undefined".
    expect(switchVaultMessage(OTHER, undefined)).not.toContain("undefined");

    expect(moveVaultMessage(OTHER)).toContain("All files will be moved");
    expect(moveVaultMessage("/Users/me/Documents/V")).toContain("Full Disk Access");
  });
});

describe("<VaultLocation>", () => {
  it("shows the current path with both actions", async () => {
    renderLocation();
    expect(await screen.findByText(CURRENT)).toBeTruthy();
    expect(screen.getByRole("button", { name: /Open another…/ })).toBeTruthy();
    expect(screen.getByRole("button", { name: /Move…/ })).toBeTruthy();
  });

  it("Open another… switches to an EXISTING vault only after confirmation, then restarts", async () => {
    vi.mocked(confirmDialog).mockResolvedValue(true);
    mocked.switchVault.mockResolvedValue(OTHER);
    renderLocation();
    await screen.findByText(CURRENT);

    fireEvent.click(screen.getByRole("button", { name: /Open another…/ }));
    await waitFor(() => expect(mocked.switchVault).toHaveBeenCalledTimes(1));
    // expect_existing = true: Settings never creates a vault by accident.
    expect(mocked.switchVault).toHaveBeenCalledWith(OTHER, true);
    expect(mocked.relocateVault).not.toHaveBeenCalled();
    expect(vi.mocked(confirmDialog).mock.calls[0][0]).toMatchObject({
      title: "Open another vault",
      confirmLabel: "Open",
    });

    // Busy state and the restart that follows the toast.
    expect(screen.getByRole("button", { name: /Opening…/ })).toBeTruthy();
    expect(useToastStore.getState().toasts.map((t) => t.type)).toEqual(["success"]);
    await waitFor(() => expect(mocked.restartApp).toHaveBeenCalledTimes(1), {
      timeout: 3000,
    });
  });

  it("does nothing when the picker or the confirm is cancelled", async () => {
    vi.mocked(open).mockResolvedValueOnce(null);
    renderLocation();
    await screen.findByText(CURRENT);

    fireEvent.click(screen.getByRole("button", { name: /Open another…/ }));
    await waitFor(() => expect(open).toHaveBeenCalledTimes(1));
    expect(confirmDialog).not.toHaveBeenCalled();

    vi.mocked(confirmDialog).mockResolvedValueOnce(false);
    fireEvent.click(screen.getByRole("button", { name: /Open another…/ }));
    await waitFor(() => expect(confirmDialog).toHaveBeenCalledTimes(1));
    expect(mocked.switchVault).not.toHaveBeenCalled();
    expect(mocked.restartApp).not.toHaveBeenCalled();
  });

  it("surfaces a backend refusal as an error toast and re-enables the buttons", async () => {
    vi.mocked(confirmDialog).mockResolvedValue(true);
    mocked.switchVault.mockRejectedValue('No vault found in "/Volumes/Data/Syzify"');
    renderLocation();
    await screen.findByText(CURRENT);

    fireEvent.click(screen.getByRole("button", { name: /Open another…/ }));
    await waitFor(() =>
      expect(useToastStore.getState().toasts.map((t) => t.type)).toEqual(["error"])
    );
    expect(useToastStore.getState().toasts[0].message).toContain("No vault found");
    expect(
      (screen.getByRole("button", { name: /Open another…/ }) as HTMLButtonElement).disabled
    ).toBe(false);
    expect(mocked.restartApp).not.toHaveBeenCalled();
  });

  it("says so when the restart itself fails instead of hanging on 'restarting…'", async () => {
    vi.mocked(confirmDialog).mockResolvedValue(true);
    mocked.switchVault.mockResolvedValue(OTHER);
    mocked.restartApp.mockRejectedValue("no relaunch");
    renderLocation();
    await screen.findByText(CURRENT);

    fireEvent.click(screen.getByRole("button", { name: /Open another…/ }));
    await waitFor(
      () => expect(useToastStore.getState().toasts.map((t) => t.type)).toEqual(["success", "error"]),
      { timeout: 3000 }
    );
    expect(useToastStore.getState().toasts[1].message).toContain("Quit and reopen");
  });

  it("Move… relocates (not switches) and restarts on success", async () => {
    vi.mocked(confirmDialog).mockResolvedValue(true);
    mocked.relocateVault.mockResolvedValue(OTHER);
    renderLocation();
    await screen.findByText(CURRENT);

    fireEvent.click(screen.getByRole("button", { name: /Move…/ }));
    await waitFor(() => expect(mocked.relocateVault).toHaveBeenCalledWith(OTHER));
    expect(mocked.switchVault).not.toHaveBeenCalled();
    expect(vi.mocked(confirmDialog).mock.calls[0][0]).toMatchObject({
      title: "Move vault",
      confirmLabel: "Move",
    });
    // The sticky progress toast is replaced by the success one.
    await waitFor(() =>
      expect(useToastStore.getState().toasts.map((t) => t.type)).toEqual(["success"])
    );
    await waitFor(() => expect(mocked.restartApp).toHaveBeenCalledTimes(1), {
      timeout: 3000,
    });
  });
});