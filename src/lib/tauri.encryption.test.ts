import { describe, it, expect, vi, beforeEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { api } from "./tauri";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const mocked = vi.mocked(invoke);

beforeEach(() => {
  vi.clearAllMocks();
  mocked.mockResolvedValue(undefined);
});

describe("encryption api wrappers", () => {
  // #209: a route is stored in its raw file AND in the database, so a
  // partial set leaves it readable. The wrapper takes no scopes at all —
  // the frontend cannot ask for less than everything.
  it("enabling always asks for all three scopes", async () => {
    await api.enableEncryption("hunter2hunter2");
    expect(mocked).toHaveBeenCalledWith("enable_encryption", {
      password: "hunter2hunter2",
      scopes: { activities: true, database: true, photos: true },
    });
    expect(api.enableEncryption).toHaveLength(1);
  });

  // #244: why the vault would not open — locked or not — comes back coded,
  // as the backend wrote it, for the vault error screen's errorText.
  it("the vault's boot error comes back as the backend's CommandError", async () => {
    const tooNew = { code: "vault.tooNew", params: { vault: 40, app: 34 }, message: "newer" };
    mocked.mockResolvedValueOnce(tooNew);
    expect(await api.getVaultError()).toEqual(tooNew);
    expect(mocked).toHaveBeenCalledWith("get_vault_error");
  });

  it("unlocking and disabling carry only the password", async () => {
    await api.unlockVault("pw");
    expect(mocked).toHaveBeenCalledWith("unlock_vault", { password: "pw" });
    await api.disableEncryption("pw");
    expect(mocked).toHaveBeenCalledWith("disable_encryption", { password: "pw" });
  });
});
