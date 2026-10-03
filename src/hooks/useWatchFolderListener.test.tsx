// @vitest-environment happy-dom
import { describe, it, expect, afterEach, beforeEach, vi } from "vitest";
import { renderHook, act, waitFor, cleanup } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "../lib/tauri";
import { useToastStore } from "../stores/toastStore";
import { useWatchFolderListener } from "./useWatchFolderListener";

vi.mock("../lib/tauri", () => ({ api: { getSetting: vi.fn(), importFiles: vi.fn() } }));
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
  const hook = renderHook(() => useWatchFolderListener(), { wrapper });
  await waitFor(() => expect(handler).toBeTruthy());
  return { hook, fire: (files: string[]) => act(async () => handler!({ payload: { files } })) };
}

const lastToast = () => useToastStore.getState().toasts.slice(-1)[0];

afterEach(cleanup);

beforeEach(() => {
  useToastStore.setState({ toasts: [] });
  vi.mocked(api.getSetting).mockReset();
  vi.mocked(api.importFiles).mockReset();
});

describe("useWatchFolderListener", () => {
  /** The backend refuses with a bare string; a JS-side failure is an
   * Error. The toast carries the words alone either way (#182). */
  for (const [kind, rejection] of [
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
});
