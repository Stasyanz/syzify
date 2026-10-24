// @vitest-environment happy-dom
import { describe, it, expect, afterEach, vi } from "vitest";
import { act, render, cleanup, fireEvent, screen, waitFor } from "@testing-library/react";
import { api } from "../lib/tauri";
import { UnlockModal } from "./UnlockModal";
import { useLanguageStore } from "../lib/i18n";
import { commandError } from "../test/commandError";

vi.mock("../lib/tauri", () => ({ api: { unlockVault: vi.fn() } }));
// The updater has its own tests and its own IPC; here only whether it shows.
vi.mock("./settings/UpdateCheck", () => ({ UpdateCheck: () => <div>UPDATE CHECK</div> }));

afterEach(cleanup);

describe("UnlockModal", () => {
  /** The backend refuses with a CommandError (#236), a plugin with a bare
   * string; a JS-side failure is an Error. All read as their words alone
   * (#182). */
  for (const [kind, rejection] of [
    ["a command error", commandError("vault.wrongPassword", "Wrong password")],
    ["a string", "Wrong password"],
    ["an Error", new Error("Wrong password")],
  ] as const) {
    it(`shows the refusal's words when the unlock fails with ${kind}`, async () => {
      vi.mocked(api.unlockVault).mockRejectedValue(rejection);
      const onUnlocked = vi.fn();
      render(<UnlockModal onUnlocked={onUnlocked} />);
      const input = screen.getByPlaceholderText("Password");
      fireEvent.change(input, { target: { value: "nope" } });
      fireEvent.keyDown(input, { key: "Enter" });
      await waitFor(() => expect(screen.getByText("Wrong password")).toBeTruthy());
      expect(screen.queryByText(/Error:/)).toBeNull();
      expect(onUnlocked).not.toHaveBeenCalled();
    });
  }

  it("asks in the interface language, switching as it changes (#233)", async () => {
    let done: () => void = () => {};
    vi.mocked(api.unlockVault).mockReturnValue(new Promise<void>((r) => (done = r)) as never);
    try {
      // Mounted in English, then switched: the modal sits above the keyed
      // AppShell, so only its own subscription re-renders it.
      render(<UnlockModal onUnlocked={vi.fn()} />);
      screen.getByPlaceholderText("Password");
      act(() => useLanguageStore.getState().setPref("ru"));
      screen.getByText("Хранилище зашифровано. Введите пароль, чтобы открыть его.");
      fireEvent.change(screen.getByPlaceholderText("Пароль"), { target: { value: "secret" } });
      fireEvent.click(screen.getByRole("button", { name: "Открыть" }));
      await screen.findByText("Открытие…");
      done();
    } finally {
      act(() => useLanguageStore.getState().setPref("system"));
    }
  });

  /** The right password on a vault a newer build migrated (#244): the
   * reason in the language, and the updater, the one way out. */
  it("offers the updater when the vault is too new, in Russian", async () => {
    vi.mocked(api.unlockVault).mockRejectedValue(
      commandError(
        "vault.tooNew",
        "This vault was last used by a newer version of Syzify (data format 40, this app reads up to 34). Update the app to open it.",
        { vault: 40, app: 34 },
      ),
    );
    act(() => useLanguageStore.getState().setPref("ru"));
    try {
      render(<UnlockModal onUnlocked={vi.fn()} />);
      expect(screen.queryByText("UPDATE CHECK")).toBeNull();
      fireEvent.change(screen.getByPlaceholderText("Пароль"), { target: { value: "secret" } });
      fireEvent.click(screen.getByRole("button", { name: "Открыть" }));
      await screen.findByText(
        "Это хранилище последней открывала более новая версия Syzify (формат данных 40, это приложение читает до 34). Обновите приложение, чтобы открыть его.",
      );
      screen.getByText("UPDATE CHECK");
    } finally {
      act(() => useLanguageStore.getState().setPref("system"));
    }
  });

  it("offers no updater for a wrong password", async () => {
    vi.mocked(api.unlockVault).mockRejectedValue(commandError("vault.wrongPassword", "Wrong password"));
    render(<UnlockModal onUnlocked={vi.fn()} />);
    fireEvent.change(screen.getByPlaceholderText("Password"), { target: { value: "nope" } });
    fireEvent.click(screen.getByRole("button", { name: /Unlock/ }));
    await screen.findByText("Wrong password");
    expect(screen.queryByText("UPDATE CHECK")).toBeNull();
  });

  it("unlocks and says nothing", async () => {
    vi.mocked(api.unlockVault).mockResolvedValue(undefined as never);
    const onUnlocked = vi.fn();
    render(<UnlockModal onUnlocked={onUnlocked} />);
    fireEvent.change(screen.getByPlaceholderText("Password"), { target: { value: "hunter2hunter2" } });
    fireEvent.click(screen.getByRole("button", { name: /Unlock/ }));
    await waitFor(() => expect(onUnlocked).toHaveBeenCalled());
    expect(api.unlockVault).toHaveBeenCalledWith("hunter2hunter2");
  });
});
