import { errorCode } from "./errors";

/** A newer build has migrated this vault past what this binary knows — the
 * only real fix is updating the app, so a screen that meets it (the vault
 * error screen, the unlock prompt) offers the updater. Told by the code
 * (#244), never the text. */
export function isVaultTooNew(error: unknown): boolean {
  return errorCode(error) === "vault.tooNew";
}

/** The refusals that a missing Full Disk Access shows up as: the vault's
 * folders, its database or a folder above it cannot be reached. */
const ACCESS_REFUSALS: ReadonlySet<string> = new Set([
  "vault.createFolderFailed",
  "vault.dbOpenFailed",
  "vault.checkFailed",
]);

/** Whether `error` may be macOS keeping the app out of the vault's folder —
 * worth the Full Disk Access hint only when the vault is in one of the
 * folders macOS guards. */
export function isAccessRefusal(error: unknown): boolean {
  return ACCESS_REFUSALS.has(errorCode(error) ?? "");
}

/** The folders of the home that macOS guards (TCC): a vault in one of them
 * opens only with Full Disk Access. Their names on disk are English in
 * every language — Finder only shows them translated. */
const PROTECTED_FOLDERS = ["Documents", "Desktop", "Downloads"];

/** Whether the vault lies in (or is) one of the home's guarded folders.
 * Decided by the vault's path, never by an error's text: the refusal
 * rarely names the path ("Operation not permitted"), and a "Desktop" in
 * another place ("/Volumes/Desktop Backup") is no guarded folder. APFS
 * compares names without case, so does this. */
export function inProtectedFolder(vaultPath: string, home: string): boolean {
  const path = vaultPath.toLowerCase();
  const base = home.replace(/\/+$/, "").toLowerCase();
  if (!base) return false;
  return PROTECTED_FOLDERS.some((name) => {
    const folder = `${base}/${name.toLowerCase()}`;
    return path === folder || path.startsWith(`${folder}/`);
  });
}
