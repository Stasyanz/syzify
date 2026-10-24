// @vitest-environment happy-dom
import { describe, it, expect, afterEach, beforeEach, vi } from "vitest";
import { renderHook, act, waitFor, cleanup } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "../lib/tauri";
import { useToastStore } from "../stores/toastStore";
import { useWatchFolderListener } from "./useWatchFolderListener";
import { useLanguageStore } from "../lib/i18n";
import { commandError } from "../test/commandError";

vi.mock("../lib/tauri", () => ({
  api: { getSetting: vi.fn(), importFiles: vi.fn(), catchUpWatchFiles: vi.fn() },
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
vi.mock("../lib/activityInvalidation", () => ({ invalidateActivityData: vi.fn() }));

type Handler = (event: { payload: { files: string[] } }) => Promise<void> | void;

/** Mounts the hook and hands back the `watch:files-detected` handler it
 * registered, so a detection can be fired without a Tauri event bus. */
async function mount() {
  let handler: Handler | undefined;
  vi.mocked(listen).mockImplementation(async (_name, h) => {
    handler = h as unknown as Handler;
    return () => {};
  });
  const qc = new QueryClient();
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={qc}>{children}</QueryClientProvider>
  );
  const hook = renderHook(() => useWatchFolderListener(false), { wrapper });
  await waitFor(() => expect(handler).toBeTruthy());
  return { hook, fire: (files: string[]) => act(async () => handler!({ payload: { files } })) };
}

const lastToast = () => useToastStore.getState().toasts.slice(-1)[0];

afterEach(cleanup);

beforeEach(() => {
  useToastStore.setState({ toasts: [] });
  vi.mocked(listen).mockClear();
  vi.mocked(api.getSetting).mockReset();
  vi.mocked(api.importFiles).mockReset();
  vi.mocked(api.catchUpWatchFiles).mockReset();
});

/** Mounts the hook with the vault closed, so a test can open it later. */
function mountClosed() {
  vi.mocked(listen).mockResolvedValue(() => {});
  const qc = new QueryClient();
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={qc}>{children}</QueryClientProvider>
  );
  return renderHook(({ open }) => useWatchFolderListener(open), {
    wrapper,
    initialProps: { open: false },
  });
}

describe("useWatchFolderListener", () => {
  /** The backend refuses with a CommandError (#236) or, from a plugin, a
   * bare string; a JS-side failure is an Error. The toast carries the
   * words alone either way (#182). */
  for (const [kind, rejection] of [
    ["an uncoded command error", commandError(null, "vault locked")],
    ["a string", "vault locked"],
    ["an Error", new Error("vault locked")],
  ] as const) {
    it(`says why an auto-import failed with ${kind}`, async () => {
      vi.mocked(api.getSetting).mockResolvedValue("auto");
      vi.mocked(api.importFiles).mockRejectedValue(rejection);
      const { fire } = await mount();
      await fire(["/w/ride.fit"]);
      expect(lastToast()).toMatchObject({ type: "error", message: "Auto-import failed: vault locked" });
    });

    it(`says why a confirmed import failed with ${kind}`, async () => {
      vi.mocked(api.getSetting).mockResolvedValue(null);
      vi.mocked(api.importFiles).mockRejectedValue(rejection);
      const { hook, fire } = await mount();
      await fire(["/w/ride.fit"]);
      expect(hook.result.current.pendingFiles).toEqual(["/w/ride.fit"]);
      await act(async () => hook.result.current.handleImport());
      expect(lastToast()).toMatchObject({ type: "error", message: "Import failed: vault locked" });
      expect(hook.result.current.importing).toBe(false);
    });
  }

  it("unsubscribes on unmount", async () => {
    const unlisten = vi.fn();
    vi.mocked(listen).mockResolvedValue(unlisten);
    const qc = new QueryClient();
    const wrapper = ({ children }: { children: ReactNode }) => (
      <QueryClientProvider client={qc}>{children}</QueryClientProvider>
    );
    const hook = renderHook(() => useWatchFolderListener(false), { wrapper });
    await waitFor(() => expect(listen).toHaveBeenCalledTimes(1));
    await act(async () => {});
    expect(unlisten).not.toHaveBeenCalled();
    hook.unmount();
    expect(unlisten).toHaveBeenCalledTimes(1);
  });

  /** The cleanup ran while `listen` was still on its IPC round trip: the
   * subscription that arrives afterwards belongs to a dead hook and must be
   * dropped at once (#194). */
  it("drops a subscription that arrives after unmount", async () => {
    const unlisten = vi.fn();
    let resolveListen: ((u: () => void) => void) | undefined;
    vi.mocked(listen).mockImplementation(
      () => new Promise<() => void>((resolve) => { resolveListen = resolve; }),
    );
    const qc = new QueryClient();
    const wrapper = ({ children }: { children: ReactNode }) => (
      <QueryClientProvider client={qc}>{children}</QueryClientProvider>
    );
    const hook = renderHook(() => useWatchFolderListener(false), { wrapper });
    await waitFor(() => expect(resolveListen).toBeDefined());
    hook.unmount();
    expect(unlisten).not.toHaveBeenCalled();
    await act(async () => {
      resolveListen!(unlisten);
    });
    expect(unlisten).toHaveBeenCalledTimes(1);
  });

  describe("launch catch-up (#221)", () => {
    it("waits for the vault, then asks once and queues the files in ask mode", async () => {
      vi.mocked(api.getSetting).mockResolvedValue(null);
      vi.mocked(api.catchUpWatchFiles).mockResolvedValue(["/w/a.fit", "/w/b.fit"]);
      const hook = mountClosed();
      await act(async () => {});
      expect(api.catchUpWatchFiles).not.toHaveBeenCalled();

      hook.rerender({ open: true });
      await waitFor(() => expect(hook.result.current.pendingFiles).toEqual(["/w/a.fit", "/w/b.fit"]));
      expect(api.importFiles).not.toHaveBeenCalled();

      // Re-renders, and the vault closing and opening again, do not ask twice.
      hook.rerender({ open: false });
      hook.rerender({ open: true });
      await act(async () => {});
      expect(api.catchUpWatchFiles).toHaveBeenCalledTimes(1);
    });

    it("imports what it finds in auto mode", async () => {
      vi.mocked(api.getSetting).mockResolvedValue("auto");
      vi.mocked(api.catchUpWatchFiles).mockResolvedValue(["/w/a.fit"]);
      vi.mocked(api.importFiles).mockResolvedValue({
        imported: 1,
        skipped: 0,
        failed: [],
        monitoring_files: 0,
        monitoring_days: 0,
        monitoring_night: false,
        monitoring_range: null,
      });
      const hook = mountClosed();
      hook.rerender({ open: true });
      await waitFor(() => expect(api.importFiles).toHaveBeenCalledWith(["/w/a.fit"]));
      expect(hook.result.current.pendingFiles).toEqual([]);
    });

    it("stays quiet when nothing arrived", async () => {
      vi.mocked(api.getSetting).mockResolvedValue("auto");
      vi.mocked(api.catchUpWatchFiles).mockResolvedValue([]);
      const hook = mountClosed();
      hook.rerender({ open: true });
      await waitFor(() => expect(api.catchUpWatchFiles).toHaveBeenCalledTimes(1));
      await act(async () => {});
      expect(api.importFiles).not.toHaveBeenCalled();
      expect(hook.result.current.pendingFiles).toEqual([]);
      expect(lastToast()).toBeUndefined();
    });

    it("says why the check failed", async () => {
      vi.mocked(api.catchUpWatchFiles).mockRejectedValue("The vault is locked — unlock it first");
      const hook = mountClosed();
      hook.rerender({ open: true });
      await waitFor(() =>
        expect(lastToast()).toMatchObject({
          type: "error",
          message: "Could not check for new files: The vault is locked — unlock it first",
        }),
      );
    });

    /** main.tsx renders under StrictMode: its doubled effect pass must not
     * ask — and auto-import — twice. */
    it("asks once under StrictMode", async () => {
      vi.mocked(api.getSetting).mockResolvedValue("auto");
      vi.mocked(api.catchUpWatchFiles).mockResolvedValue([]);
      vi.mocked(listen).mockResolvedValue(() => {});
      const qc = new QueryClient();
      const wrapper = ({ children }: { children: ReactNode }) => (
        <QueryClientProvider client={qc}>{children}</QueryClientProvider>
      );
      renderHook(() => useWatchFolderListener(true), { wrapper, reactStrictMode: true });
      await waitFor(() => expect(api.catchUpWatchFiles).toHaveBeenCalled());
      await act(async () => {});
      expect(api.catchUpWatchFiles).toHaveBeenCalledTimes(1);
    });
  });

  describe("in Russian (#235)", () => {
    afterEach(() => useLanguageStore.getState().setPref("system"));
    const result = {
      imported: 2,
      skipped: 0,
      failed: [],
      monitoring_files: 0,
      monitoring_days: 0,
      monitoring_night: false,
      monitoring_range: null,
    };

    it("writes the toast in the language of the moment it is shown, not of the subscription", async () => {
      vi.mocked(api.getSetting).mockResolvedValue("auto");
      vi.mocked(api.importFiles).mockResolvedValueOnce(result).mockRejectedValueOnce("vault locked");
      const { fire } = await mount();
      act(() => useLanguageStore.getState().setPref("ru"));
      await fire(["/w/ride.fit"]);
      expect(lastToast()).toMatchObject({ type: "success", message: "Автоимпорт: 2 тренировки" });
      await fire(["/w/run.fit"]);
      expect(lastToast()).toMatchObject({ type: "error", message: "Автоимпорт не удался: vault locked" });
    });

    it("sums up and refuses a confirmed import in the interface language", async () => {
      useLanguageStore.getState().setPref("ru");
      vi.mocked(api.getSetting).mockResolvedValue(null);
      vi.mocked(api.importFiles).mockResolvedValueOnce(result).mockRejectedValueOnce("vault locked");
      const { hook, fire } = await mount();
      await fire(["/w/ride.fit"]);
      await act(async () => hook.result.current.handleImport());
      expect(lastToast()).toMatchObject({ type: "success", message: "Импортировано: 2 тренировки" });
      await fire(["/w/run.fit"]);
      await act(async () => hook.result.current.handleImport());
      expect(lastToast()).toMatchObject({ type: "error", message: "Импорт не удался: vault locked" });
    });

    it("says why the launch check failed in the interface language", async () => {
      useLanguageStore.getState().setPref("ru");
      vi.mocked(api.catchUpWatchFiles).mockRejectedValue("The vault is locked");
      const hook = mountClosed();
      hook.rerender({ open: true });
      await waitFor(() =>
        expect(lastToast()).toMatchObject({
          type: "error",
          message: "Не удалось проверить новые файлы: The vault is locked",
        }),
      );
    });
  });
});
