// @vitest-environment happy-dom
import { describe, it, expect, afterEach, beforeEach, vi } from "vitest";
import { act, render, cleanup, fireEvent, screen, waitFor } from "@testing-library/react";
import { listen } from "@tauri-apps/api/event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "../../lib/tauri";
import { commandError } from "../../test/commandError";
import { confirmDialog } from "../../stores/confirmStore";
import { useToastStore } from "../../stores/toastStore";
import { useBusyStore } from "../../stores/busyStore";
import { t, translate, useLanguageStore, type Translator } from "../../lib/i18n";
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

afterEach(() => {
  cleanup();
  useLanguageStore.getState().setPref("system");
  // A successful switch or move stays busy until the restart.
  useBusyStore.setState({ running: {}, lastError: {} });
});
beforeEach(() => {
  vi.clearAllMocks();
  useToastStore.setState({ toasts: [] });
  mocked.getVaultPath.mockResolvedValue(CURRENT);
  mocked.restartApp.mockResolvedValue(undefined);
  vi.mocked(open).mockResolvedValue(OTHER);
});

describe("dialog copy", () => {
  it("warns only for macOS-protected folders", () => {
    expect(protectedFolderNote("/Users/me/Documents/Syzify", t)).toContain("Full Disk Access");
    expect(protectedFolderNote("/Users/me/Desktop", t)).toContain("Full Disk Access");
    expect(protectedFolderNote("/Users/me/Downloadsx", t)).toBe("");
    expect(protectedFolderNote("/Volumes/Data/Syzify", t)).toBe("");
  });

  it("switch says the current vault is left in place; move says files move", () => {
    const sw = switchVaultMessage(OTHER, CURRENT, t);
    expect(sw).toBe(
      `Open the vault in "${OTHER}"?\n\nThe current vault at "${CURRENT}" stays where it is — nothing is moved. The app will restart.`,
    );
    // Path still loading → no dangling quotes around "undefined".
    expect(switchVaultMessage(OTHER, undefined, t)).not.toContain("undefined");

    expect(moveVaultMessage(OTHER, t)).toBe(
      `Move the vault to "${OTHER}"?\n\nAll files will be moved there and the app will restart.`,
    );
    expect(moveVaultMessage("/Users/me/Documents/V", t)).toContain("Full Disk Access");
  });

  it("asks in the language it is given", () => {
    const ru: Translator = (key, ...args) => translate("ru", key, ...args);
    expect(switchVaultMessage(OTHER, CURRENT, ru)).toBe(
      `Открыть хранилище в «${OTHER}»?\n\nТекущее хранилище в «${CURRENT}» остаётся на месте — ничего не перемещается. Приложение перезапустится.`,
    );
    expect(switchVaultMessage(OTHER, undefined, ru)).toContain("Текущее хранилище остаётся на месте");
    expect(moveVaultMessage("/Users/me/Documents/V", ru)).toBe(
      "Переместить хранилище в «/Users/me/Documents/V»?\n\nВсе файлы будут перенесены туда, и приложение перезапустится." +
        "\n\nОбратите внимание: «/Users/me/Documents/V» — защищённая папка macOS. Чтобы она открылась при следующем запуске, дайте Syzify «Полный доступ к диску».",
    );
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
    mocked.switchVault.mockRejectedValue(
      commandError("vault.noVaultFound", 'No vault found in "/Volumes/Data/Syzify"', { path: "/Volumes/Data/Syzify" }),
    );
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

  it("surfaces a backend refusal as an error toast and re-enables the buttons — an Error carries its message only (#182)", async () => {
    vi.mocked(confirmDialog).mockResolvedValue(true);
    mocked.switchVault.mockRejectedValue(new Error("stale lock"));
    renderLocation();
    await screen.findByText(CURRENT);

    fireEvent.click(screen.getByRole("button", { name: /Open another…/ }));
    await waitFor(() =>
      expect(useToastStore.getState().toasts.map((t) => t.type)).toEqual(["error"])
    );
    expect(useToastStore.getState().toasts[0].message).toBe("Couldn't open vault: stale lock");
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

  it("keeps both buttons locked across a remount while a move runs (#235)", async () => {
    vi.mocked(confirmDialog).mockResolvedValue(true);
    let fail: (e: unknown) => void = () => {};
    mocked.relocateVault.mockReturnValue(
      new Promise((_, reject) => {
        fail = reject;
      }),
    );
    const first = renderLocation();
    await screen.findByText(CURRENT);
    fireEvent.click(screen.getByRole("button", { name: /Move…/ }));
    await screen.findByRole("button", { name: /Moving…/ });
    // A language change remounts the app; the move runs on.
    first.unmount();
    renderLocation();
    await screen.findByText(CURRENT);
    expect((screen.getByRole("button", { name: /Moving…/ }) as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByRole("button", { name: /Open another…/ }) as HTMLButtonElement).disabled).toBe(true);
    // The progress toast is the toast store's and outlives the remount too.
    expect(useToastStore.getState().toasts.map((x) => x.message)).toEqual(["Moving vault… 0%"]);

    fail("disk full");
    await waitFor(() => expect(screen.getByRole("button", { name: /Move…/ })).toBeTruthy());
    expect(useBusyStore.getState().running).toEqual({});
    expect(useToastStore.getState().toasts.map((x) => x.message)).toEqual(["Move failed: disk full"]);
  });

  it("unlocks everything when the progress listener cannot subscribe", async () => {
    vi.mocked(listen).mockRejectedValueOnce(new Error("event bus gone"));
    vi.mocked(confirmDialog).mockResolvedValue(true);
    renderLocation();
    await screen.findByText(CURRENT);
    fireEvent.click(screen.getByRole("button", { name: /Move…/ }));
    await waitFor(() => expect(useToastStore.getState().toasts.map((x) => x.message)).toEqual(["Move failed: event bus gone"]));
    expect(useBusyStore.getState().running).toEqual({});
    expect(mocked.relocateVault).not.toHaveBeenCalled();
    expect((screen.getByRole("button", { name: /Move…/ }) as HTMLButtonElement).disabled).toBe(false);
  });

  it("follows the move's progress in its toast", async () => {
    type Progress = { payload: { processed: number; total: number } };
    let progress: ((e: Progress) => void) | undefined;
    vi.mocked(listen).mockImplementationOnce(async (_event, cb) => {
      progress = cb as unknown as (e: Progress) => void;
      return () => {};
    });
    vi.mocked(confirmDialog).mockResolvedValue(true);
    mocked.relocateVault.mockReturnValue(new Promise(() => {}));
    renderLocation();
    await screen.findByText(CURRENT);
    fireEvent.click(screen.getByRole("button", { name: /Move…/ }));
    await waitFor(() => expect(progress).toBeDefined());
    act(() => progress!({ payload: { processed: 37, total: 100 } }));
    expect(useToastStore.getState().toasts.map((x) => x.message)).toEqual(["Moving vault… 37%"]);
    // Nothing to copy reads as done, not as a division by zero.
    act(() => progress!({ payload: { processed: 0, total: 0 } }));
    expect(useToastStore.getState().toasts.map((x) => x.message)).toEqual(["Moving vault… 100%"]);
  });

  it("speaks Russian", async () => {
    useLanguageStore.getState().setPref("ru");
    vi.mocked(confirmDialog).mockResolvedValue(true);
    mocked.relocateVault.mockReturnValue(new Promise(() => {}));
    renderLocation();
    await screen.findByText(CURRENT);
    screen.getByText("Расположение");
    screen.getByRole("button", { name: "Открыть другое…" });
    fireEvent.click(screen.getByRole("button", { name: "Переместить…" }));
    await screen.findByRole("button", { name: "Перемещение…" });
    expect(vi.mocked(confirmDialog).mock.calls[0][0]).toMatchObject({
      title: "Перемещение хранилища",
      confirmLabel: "Переместить",
    });
    expect(vi.mocked(open).mock.calls[0][0]).toMatchObject({ title: "Выберите новое место для хранилища" });
    expect(useToastStore.getState().toasts[0].message).toMatch(/^Перемещение хранилища… 0\s?%$/);
  });
});