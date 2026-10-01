// @vitest-environment happy-dom
import { describe, it, expect, afterEach, beforeEach, vi } from "vitest";
import { renderHook, cleanup, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactNode } from "react";
import { PLUGINS_IMPORTED_EVENT, usePluginImportRefresh } from "./usePluginImportRefresh";

const listeners = new Map<string, () => void>();
const unlisten = vi.fn();
// Each test decides how `listen` resolves: settled at once, held back
// (the unmount-before-resolve race), or failing (no event bus at all).
let resolveListen: ((fn: () => void) => void) | null = null;
let listenRejects = false;
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((name: string, cb: () => void) => {
    listeners.set(name, cb);
    if (listenRejects) return Promise.reject(new Error("no event bus"));
    return new Promise<() => void>((resolve) => {
      resolveListen = resolve;
      if (!holdListen) resolve(unlisten);
    });
  }),
}));
let holdListen = false;
vi.mock("../lib/activityInvalidation", () => ({
  invalidateActivityData: vi.fn(() => Promise.resolve()),
}));

import { invalidateActivityData } from "../lib/activityInvalidation";
const invalidateMock = vi.mocked(invalidateActivityData);

beforeEach(() => {
  listeners.clear();
  unlisten.mockClear();
  invalidateMock.mockClear();
  resolveListen = null;
  holdListen = false;
  listenRejects = false;
});
afterEach(cleanup);

function mountHook() {
  const client = new QueryClient();
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
  return { client, ...renderHook(() => usePluginImportRefresh(), { wrapper }) };
}

describe("usePluginImportRefresh", () => {
  it("invalidates the activity-derived queries and the open activity pages on the plugin event", async () => {
    const { client, unmount } = mountHook();
    // An open activity page: a plugin may have put it on gear (#169).
    client.setQueryData(["activity", "a-1"], { id: "a-1" });
    client.setQueryData(["setting", "map_layer"], "osm");
    await waitFor(() => expect(listeners.has(PLUGINS_IMPORTED_EVENT)).toBe(true));
    expect(invalidateMock).not.toHaveBeenCalled();

    listeners.get(PLUGINS_IMPORTED_EVENT)!();
    expect(invalidateMock).toHaveBeenCalledTimes(1);
    expect(invalidateMock).toHaveBeenCalledWith(client);
    expect(client.getQueryState(["activity", "a-1"])!.isInvalidated).toBe(true);
    expect(client.getQueryState(["setting", "map_layer"])!.isInvalidated).toBe(false);

    unmount();
    expect(unlisten).toHaveBeenCalledTimes(1);
  });

  it("unlistens right away when the listener resolves after the unmount", async () => {
    holdListen = true;
    const { unmount } = mountHook();
    await waitFor(() => expect(resolveListen).not.toBeNull());
    unmount();
    expect(unlisten).not.toHaveBeenCalled();

    resolveListen!(unlisten);
    await waitFor(() => expect(unlisten).toHaveBeenCalledTimes(1));
  });

  it("survives an environment without the event bus", async () => {
    listenRejects = true;
    const { unmount } = mountHook();
    await waitFor(() => expect(listeners.has(PLUGINS_IMPORTED_EVENT)).toBe(true));
    await Promise.resolve();
    unmount();
    expect(unlisten).not.toHaveBeenCalled();
  });
});
