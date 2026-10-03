// @vitest-environment happy-dom
import { describe, it, expect, afterEach, vi } from "vitest";
import { render, cleanup, fireEvent, screen, waitFor } from "@testing-library/react";
import { api } from "../lib/tauri";
import { UnlockModal } from "./UnlockModal";

vi.mock("../lib/tauri", () => ({ api: { unlockVault: vi.fn() } }));

afterEach(cleanup);

describe("UnlockModal", () => {
  /** The backend refuses with a bare string; a JS-side failure is an
   * Error. Both read as their words alone (#182). */
  for (const [kind, rejection] of [
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
