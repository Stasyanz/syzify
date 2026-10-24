import { describe, it, expect } from "vitest";
import { commandError } from "../test/commandError";
import { inProtectedFolder, isAccessRefusal, isVaultTooNew } from "./vaultError";

describe("the vault's boot error (#244)", () => {
  it("tells a too-new vault by its code, never its text", () => {
    const tooNew = commandError("vault.tooNew", "newer", { vault: 40, app: 34 });
    expect(isVaultTooNew(tooNew)).toBe(true);
    expect(isVaultTooNew(commandError("vault.restartRequired", tooNew.message))).toBe(false);
    // The old raw migration text is no code: nothing in it is read any more.
    expect(isVaultTooNew(commandError(null, "Failed to run migrations: DatabaseTooFarAhead"))).toBe(false);
    expect(isVaultTooNew("vault.tooNew")).toBe(false);
    expect(isVaultTooNew(null)).toBe(false);
  });

  it("takes only the refusals of a folder out of reach for access ones", () => {
    const detail = { folder: "raw", detail: "Operation not permitted (os error 1)" };
    expect(isAccessRefusal(commandError("vault.createFolderFailed", "Failed to create vault/raw: Operation not permitted (os error 1)", detail))).toBe(true);
    expect(isAccessRefusal(commandError("vault.dbOpenFailed", "Failed to open database: x", { detail: "x" }))).toBe(true);
    expect(isAccessRefusal(commandError("vault.checkFailed", "Couldn't check \"/a\": x", { path: "/a", detail: "x" }))).toBe(true);
    expect(isAccessRefusal(commandError("vault.tooNew", "newer", { vault: 40, app: 34 }))).toBe(false);
    expect(isAccessRefusal(commandError("vault.restartRequired", "restart"))).toBe(false);
    expect(isAccessRefusal(commandError(null, "Permission denied: /Users/me/Documents"))).toBe(false);
  });

  it("finds the guarded folders by the vault's path under the home", () => {
    const home = "/Users/me";
    expect(inProtectedFolder("/Users/me/Documents/Syzify", home)).toBe(true);
    expect(inProtectedFolder("/Users/me/Desktop", home)).toBe(true);
    expect(inProtectedFolder("/Users/me/Downloads/a/b", "/Users/me/")).toBe(true);
    // APFS compares names without case.
    expect(inProtectedFolder("/Users/me/documents/Syzify", home)).toBe(true);
    // The same names elsewhere are no guarded folders.
    expect(inProtectedFolder("/Volumes/Desktop Backup/Syzify", home)).toBe(false);
    expect(inProtectedFolder("/Users/other/Documents/Syzify", home)).toBe(false);
    expect(inProtectedFolder("/Users/me/DocumentsArchive/Syzify", home)).toBe(false);
    expect(inProtectedFolder("/Users/me/Syzify", home)).toBe(false);
    // No home known: no guess.
    expect(inProtectedFolder("/Documents/Syzify", "")).toBe(false);
  });
});
