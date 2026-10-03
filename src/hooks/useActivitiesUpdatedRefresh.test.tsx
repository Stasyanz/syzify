// @vitest-environment happy-dom
import { describe, it, expect, afterEach, beforeEach, vi } from "vitest";
import { renderHook, cleanup, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactNode } from "react";
import { ACTIVITIES_UPDATED_EVENT, useActivitiesUpdatedRefresh } from "./useActivitiesUpdatedRefresh";

const listeners = new Map<string, () => void>();
const unlisten = vi.fn();
// Each test decides how `listen` resolves: settled at once, held back
// (the unmount-before-resolve race), or failing (no event bus at all).
let resolveListen: ((fn: () => void) => void) | null = null;
let holdListen = false;
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

beforeEach(() => {
  listeners.clear();
  unlisten.mockClear();
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
  return { client, ...renderHook(() => useActivitiesUpdatedRefresh(), { wrapper }) };
}

describe("useActivitiesUpdatedRefresh", () => {
  it("invalidates the list and the open activity pages on the event, and unlistens on unmount", async () => {
    const { client, unmount } = mountHook();
    client.setQueryData(["activities", { limit: 20 }], []);
    client.setQueryData(["activity", "a-1"], { id: "a-1" });
    client.setQueryData(["setting", "map_layer"], "osm");
    await waitFor(() => expect(listeners.has(ACTIVITIES_UPDATED_EVENT)).toBe(true));
    await waitFor(() => expect(resolveListen).not.toBeNull());

    listeners.get(ACTIVITIES_UPDATED_EVENT)!();
    expect(client.getQueryState(["activities", { limit: 20 }])!.isInvalidated).toBe(true);
    expect(client.getQueryState(["activity", "a-1"])!.isInvalidated).toBe(true);
    expect(client.getQueryState(["setting", "map_layer"])!.isInvalidated).toBe(false);

    // Let the resolved subscription land before the cleanup runs.
    await Promise.resolve();
    unmount();
    expect(unlisten).toHaveBeenCalledTimes(1);
  });

  /** The cleanup ran while `listen` was on its IPC round trip: the
   * subscription that lands afterwards is let go at once (#194). */
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
    await waitFor(() => expect(listeners.has(ACTIVITIES_UPDATED_EVENT)).toBe(true));
    unmount();
    expect(unlisten).not.toHaveBeenCalled();
  });
});
