// @vitest-environment happy-dom
import { describe, it, expect, afterEach, vi } from "vitest";
import { act, render, cleanup, screen, waitFor, fireEvent } from "@testing-library/react";
import type { RenderResult } from "@testing-library/react";
import { commandError } from "./test/commandError";
import type { CommandError } from "./lib/types";

vi.mock("./lib/tauri", () => ({
  api: {
    getEncryptionStatus: vi.fn(),
    getVaultError: vi.fn().mockResolvedValue(null),
    getVaultPath: vi.fn().mockResolvedValue("/Users/me/Syzify"),
    switchVault: vi.fn().mockResolvedValue(""),
    restartApp: vi.fn().mockResolvedValue(undefined),
    checkForUpdates: vi.fn(),
    installUpdate: vi.fn(),
  },
  isTauri: () => false,
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
}));

vi.mock("@tauri-apps/api/path", () => ({
  homeDir: vi.fn().mockResolvedValue("/Users/me"),
}));

// What the drop hook and the watch listener report; a test sets it and
// puts it back.
const drop = vi.hoisted(() => ({ dragging: false, kind: "workout" as "workout" | "photo" }));
const watch = vi.hoisted(() => ({ pendingFiles: [] as string[], importing: false }));

vi.mock("./hooks/useDropImport", () => ({
  useDropImport: () => ({ ...drop }),
}));

// Every `vaultOpen` the app hands the watch listener, render by render.
const watchVaultOpen = vi.hoisted(() => [] as boolean[]);

vi.mock("./hooks/useWatchFolderListener", () => ({
  useWatchFolderListener: (vaultOpen: boolean) => {
    watchVaultOpen.push(vaultOpen);
    return {
      pendingFiles: watch.pendingFiles,
      importing: watch.importing,
      handleImport: vi.fn(),
      handleDismiss: vi.fn(),
    };
  },
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

afterEach(cleanup);

// App.tsx holds a module-level QueryClient, so its cache would leak between
// tests — re-import the module fresh (mocks survive vi.resetModules).
async function bootApp(
  status: Promise<unknown>,
  vaultError: CommandError | null = null,
  vaultPath = "/Users/me/Syzify",
): Promise<RenderResult> {
  vi.resetModules();
  const { api } = await import("./lib/tauri");
  vi.mocked(api.getEncryptionStatus).mockReturnValue(
    status as ReturnType<typeof api.getEncryptionStatus>,
  );
  vi.mocked(api.getVaultError).mockResolvedValue(vaultError);
  vi.mocked(api.getVaultPath).mockClear().mockResolvedValue(vaultPath);
  const { default: App } = await import("./App");
  return render(<App />);
}

/** What `get_vault_error` says of a vault a newer build migrated (#244). */
const TOO_NEW = commandError(
  "vault.tooNew",
  "This vault was last used by a newer version of Syzify (data format 40, this app reads up to 34). Update the app to open it.",
  { vault: 40, app: 34 },
);
/** A database the app could not open, the library's words as the detail. */
const DB_DETAIL = "unable to open database file: /Users/me/Documents/Syzify/vault.db";
const DB_OPEN_FAILED = commandError("vault.dbOpenFailed", `Failed to open database: ${DB_DETAIL}`, {
  detail: DB_DETAIL,
});
const BOOM = commandError(null, "boom");
/** What a missing Full Disk Access looks like at boot: the first folder the
 * app makes is refused, and the refusal names no path. */
const NO_ACCESS = commandError("vault.createFolderFailed", "Failed to create vault/raw: Operation not permitted (os error 1)", {
  folder: "raw",
  detail: "Operation not permitted (os error 1)",
});

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

  /** The launch catch-up (#221) may run only on a vault the app shows:
   * loaded, unlocked, without an error. */
  it("tells the watch listener the vault is open only when it is", async () => {
    watchVaultOpen.length = 0;
    await bootApp(UNLOCKED);
    await waitFor(() => expect(watchVaultOpen).toContain(true));
    expect(watchVaultOpen[0]).toBe(false);

    cleanup();
    watchVaultOpen.length = 0;
    await bootApp(
      Promise.resolve({
        enabled: true,
        locked: true,
        scopes: { activities: true, database: true, photos: true },
      }),
    );
    await waitFor(() => expect(screen.queryByRole("status")).toBeNull());
    expect(watchVaultOpen).not.toContain(true);

    cleanup();
    watchVaultOpen.length = 0;
    await bootApp(UNLOCKED, TOO_NEW);
    await screen.findByText("Can't open your vault");
    expect(watchVaultOpen).not.toContain(true);
  });

  /** Half an answer is no answer: unlocked but the vault error still
   * pending, or the other way round, is not an open vault yet. */
  it("waits for both the encryption status and the vault error", async () => {
    watchVaultOpen.length = 0;
    vi.resetModules();
    const { api } = await import("./lib/tauri");
    vi.mocked(api.getEncryptionStatus).mockReturnValue(
      UNLOCKED as ReturnType<typeof api.getEncryptionStatus>,
    );
    vi.mocked(api.getVaultError).mockReturnValue(new Promise(() => {}));
    const { default: App } = await import("./App");
    render(<App />);
    await waitFor(() => expect(screen.queryByRole("status")).toBeNull());
    await new Promise((r) => setTimeout(r, 50));
    expect(watchVaultOpen).not.toContain(true);

    cleanup();
    watchVaultOpen.length = 0;
    await bootApp(new Promise(() => {}), null);
    await new Promise((r) => setTimeout(r, 50));
    expect(watchVaultOpen.length).toBeGreaterThan(0);
    expect(watchVaultOpen).not.toContain(true);
  });

  /** Dates, numbers and units are written when a screen renders — some in
   * memoized chart configs and uPlot axes no hook reaches. A language
   * switch remounts the screens so all of it is written again (#232). */
  it("remounts the screens on a language change, and only then", async () => {
    await bootApp(UNLOCKED);
    const main = await waitFor(() => {
      const m = document.querySelector("main");
      expect(m).toBeTruthy();
      return m!;
    });
    // bootApp re-imported the module graph: this is the store App reads.
    const { useLanguageStore } = await import("./lib/i18n");
    try {
      act(() => useLanguageStore.getState().setPref("en"));
      expect(document.querySelector("main")).toBe(main);
      act(() => useLanguageStore.getState().setPref("ru"));
      const remounted = document.querySelector("main");
      expect(remounted).toBeTruthy();
      expect(remounted).not.toBe(main);
    } finally {
      act(() => useLanguageStore.getState().setPref("system"));
    }
  });

  /** The text around the screens (#233): the watch-folder banner counts
   * its files in the number's form, the drop overlay names what it takes. */
  it("speaks the interface language around the screens", async () => {
    try {
      watch.pendingFiles = ["a.fit"];
      drop.dragging = true;
      drop.kind = "photo";
      await bootApp(UNLOCKED);
      // bootApp re-imported the module graph: this is the store App reads.
      const fresh = await import("./lib/i18n");
      act(() => fresh.useLanguageStore.getState().setPref("ru"));
      const banner = await waitFor(() => {
        const el = screen.getByText(/папках наблюдения/).closest("span");
        expect(el).toBeTruthy();
        return el!;
      });
      // One message, the bold number where Russian puts it.
      expect(banner.textContent).toBe("В папках наблюдения найден 1 новый файл тренировки");
      expect(banner.querySelector("strong")!.textContent).toBe("1");
      screen.getByText("Импортировать");
      screen.getByText("Скрыть");
      screen.getByText("Отпустите фото, чтобы добавить их к тренировке");
      screen.getByText("JPG, PNG, WebP, HEIC");

      cleanup();
      watch.pendingFiles = ["a.fit", "b.fit", "c.fit", "d.fit", "e.fit"];
      watch.importing = true;
      drop.kind = "workout";
      await bootApp(UNLOCKED);
      const again = await import("./lib/i18n");
      act(() => again.useLanguageStore.getState().setPref("ru"));
      await waitFor(() =>
        expect(screen.getByText(/папках наблюдения/).closest("span")!.textContent).toBe(
          "В папках наблюдения найдено 5 новых файлов тренировок",
        ),
      );
      screen.getByText("Импорт…");
      screen.getByText("Отпустите файлы тренировок, чтобы импортировать их");
      screen.getByText("GPX, FIT, TCX");
    } finally {
      watch.pendingFiles = [];
      watch.importing = false;
      drop.dragging = false;
      const last = await import("./lib/i18n");
      act(() => last.useLanguageStore.getState().setPref("system"));
    }
  });

  it("writes the vault error screen and the splash's name in the interface language", async () => {
    await bootApp(UNLOCKED, NO_ACCESS, "/Users/me/Documents/Syzify");
    await screen.findByText("Can't open your vault");
    screen.getByText("Failed to create vault/raw: Operation not permitted (os error 1)");
    const { useLanguageStore } = await import("./lib/i18n");
    try {
      act(() => useLanguageStore.getState().setPref("ru"));
      screen.getByText("Не удаётся открыть хранилище");
      // The reason in Russian, the system's words kept as they are.
      screen.getByText("Не удалось создать папку vault/raw: Operation not permitted (os error 1)");
      // The vault lies in ~/Documents: the hint, though the error names no path.
      await screen.findByText("Полный доступ к диску");
      expect(screen.getByText("Полный доступ к диску").parentElement!.textContent).toBe(
        "Хранилище лежит в защищённой папке. Дайте Syzify Полный доступ к диску в Системных настройках → Конфиденциальность и безопасность и откройте снова — или верните хранилище в незащищённое место.",
      );
      screen.getByText("Открыть снова");
      screen.getByText("Создать новое хранилище…");
      screen.getByText("Файлы текущего хранилища остаются на месте.");

      const { open } = await import("@tauri-apps/plugin-dialog");
      vi.mocked(open).mockResolvedValue(null);
      fireEvent.click(screen.getByText("Открыть другое хранилище…"));
      await waitFor(() =>
        expect(open).toHaveBeenLastCalledWith({ directory: true, title: "Откройте папку существующего хранилища" }),
      );
      fireEvent.click(screen.getByText("Создать новое хранилище…"));
      await waitFor(() =>
        expect(open).toHaveBeenLastCalledWith({ directory: true, title: "Выберите пустую папку для нового хранилища" }),
      );
      // Later tests count the picker's calls.
      vi.mocked(open).mockClear();
    } finally {
      act(() => useLanguageStore.getState().setPref("system"));
    }

    cleanup();
    await bootApp(UNLOCKED, DB_OPEN_FAILED);
    await screen.findByText("Can't open your vault");
    const db = await import("./lib/i18n");
    try {
      act(() => db.useLanguageStore.getState().setPref("ru"));
      // The reason in Russian, the library's words kept as they are.
      screen.getByText(`Не удалось открыть базу данных: ${DB_DETAIL}`);
    } finally {
      act(() => db.useLanguageStore.getState().setPref("system"));
    }

    cleanup();
    await bootApp(UNLOCKED, TOO_NEW);
    await screen.findByText("Can't open your vault");
    const again = await import("./lib/i18n");
    try {
      act(() => again.useLanguageStore.getState().setPref("ru"));
      screen.getByText(
        "Это хранилище последней открывала более новая версия Syzify (формат данных 40, это приложение читает до 34). Обновите приложение, чтобы открыть его.",
      );
      // A too-new vault is no protected folder, and reopening it is no fix.
      expect(screen.queryByText("Полный доступ к диску")).toBeNull();
      expect(screen.getByText("Открыть снова").className).toContain("ghost");
      act(() => again.useLanguageStore.getState().setPref("it"));
      screen.getByText(
        "Questo archivio è stato usato l'ultima volta da una versione più recente di Syzify (formato dati 40, questa app legge fino al formato 34). Aggiorna l'app per aprirlo.",
      );
    } finally {
      act(() => again.useLanguageStore.getState().setPref("system"));
    }

    cleanup();
    await bootApp(new Promise(() => {}));
    const splash = await import("./lib/i18n");
    try {
      act(() => splash.useLanguageStore.getState().setPref("ru"));
      expect(screen.getByRole("status").getAttribute("aria-label")).toBe("Загрузка");
    } finally {
      act(() => splash.useLanguageStore.getState().setPref("system"));
    }
  });

  it("vault error screen switches to a picked vault and restarts", async () => {
    await bootApp(UNLOCKED, TOO_NEW);
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

  /** The Full Disk Access hint goes by the vault's path, not the error's
   * text (#244): a path that merely says "Documents" or "Desktop" gets no
   * hint, nor does a refusal that is not about access. */
  it.each([
    ["an access refusal outside the guarded folders", NO_ACCESS, "/Volumes/Desktop Backup/Syzify"],
    ["a text naming Documents on a vault outside them", DB_OPEN_FAILED, "/Users/me/Syzify"],
    ["a refusal that is not about access", TOO_NEW, "/Users/me/Documents/Syzify"],
  ])("gives no Full Disk Access hint for %s", async (_case, error, vaultPath) => {
    await bootApp(UNLOCKED, error, vaultPath);
    await screen.findByText("Can't open your vault");
    const { api } = await import("./lib/tauri");
    if (error !== TOO_NEW) await waitFor(() => expect(api.getVaultPath).toHaveBeenCalled());
    await new Promise((r) => setTimeout(r, 20));
    expect(screen.queryByText("Full Disk Access")).toBeNull();
  });

  it("a too-new vault offers the updater", async () => {
    await bootApp(UNLOCKED, TOO_NEW);
    await screen.findByText("Can't open your vault");

    // The reason, versions and all; the update check is right there.
    screen.getByText(TOO_NEW.message);
    const { api } = await import("./lib/tauri");
    vi.mocked(api.checkForUpdates).mockResolvedValue({
      update_available: true,
      latest_version: "9.9.9",
      current_version: "0.3.0",
      release_url: "https://example.com",
    } as Awaited<ReturnType<typeof api.checkForUpdates>>);
    fireEvent.click(screen.getByText("Check for updates"));
    await screen.findByText("Install and restart");

    // The escape hatches stay available.
    screen.getByText("Open another vault…");
    screen.getByText("Reopen");
  });

  it("an uncoded vault error says its message, with no updater", async () => {
    await bootApp(UNLOCKED, BOOM);
    await screen.findByText("Can't open your vault");
    screen.getByText("boom");
    expect(screen.queryByText("Check for updates")).toBeNull();
  });

  it("create-new passes expect_existing=false; cancelling picks nothing", async () => {
    await bootApp(UNLOCKED, BOOM);
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
    await bootApp(UNLOCKED, BOOM);
    await screen.findByText("Can't open your vault");
    const { open } = await import("@tauri-apps/plugin-dialog");
    const { api } = await import("./lib/tauri");
    // The api mock instances survive vi.resetModules — drop calls leaked
    // from the earlier successful-switch tests.
    vi.mocked(api.restartApp).mockClear();
    vi.mocked(open).mockResolvedValue("/no/vault/here");
    vi.mocked(api.switchVault).mockRejectedValue(
      commandError("vault.noVaultFound", 'No vault found in "/no/vault/here"', { path: "/no/vault/here" }),
    );

    fireEvent.click(screen.getByText("Open another vault…"));

    await screen.findByText('No vault found in "/no/vault/here"');
    expect(api.restartApp).not.toHaveBeenCalled();
  });

  it("shows the switch error and does not restart when switching fails — an Error reads the same, without an Error: prefix (#182)", async () => {
    await bootApp(UNLOCKED, BOOM);
    await screen.findByText("Can't open your vault");
    const { open } = await import("@tauri-apps/plugin-dialog");
    const { api } = await import("./lib/tauri");
    // The api mock instances survive vi.resetModules — drop calls leaked
    // from the earlier successful-switch tests.
    vi.mocked(api.restartApp).mockClear();
    vi.mocked(open).mockResolvedValue("/no/vault/here");
    vi.mocked(api.switchVault).mockRejectedValue(new Error("the volume is not mounted"));

    fireEvent.click(screen.getByText("Open another vault…"));

    await screen.findByText("the volume is not mounted");
    expect(screen.queryByText(/Error:/)).toBeNull();
    expect(api.restartApp).not.toHaveBeenCalled();
  });
});