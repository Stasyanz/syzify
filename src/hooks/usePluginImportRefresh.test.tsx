// @vitest-environment happy-dom
import { describe, it, expect, afterEach, beforeEach, vi } from "vitest";
import { renderHook, cleanup, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactNode } from "react";
import { PLUGINS_IMPORTED_EVENT, usePluginImportRefresh } from "./usePluginImportRefresh";

const listeners = new Map<string, () => void>();
const unlisten = vi.fn();
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((name: string, cb: () => void) => {
    listeners.set(name, cb);
    return Promise.resolve(unlisten);
  }),
}));
vi.mock("../lib/activityInvalidation", () => ({
  invalidateActivityData: vi.fn(() => Promise.resolve()),
}));

import { invalidateActivityData } from "../lib/activityInvalidation";
const invalidateMock = vi.mocked(invalidateActivityData);

beforeEach(() => {
  listeners.clear();
  unlisten.mockClear();
  invalidateMock.mockClear();
});
afterEach(cleanup);

describe("usePluginImportRefresh", () => {
  it("invalidates the activity-derived queries on the plugin import event", async () => {
    const client = new QueryClient();
    const wrapper = ({ children }: { children: ReactNode }) => (
      <QueryClientProvider client={client}>{children}</QueryClientProvider>
    );
    const { unmount } = renderHook(() => usePluginImportRefresh(), { wrapper });
    await waitFor(() => expect(listeners.has(PLUGINS_IMPORTED_EVENT)).toBe(true));
    expect(invalidateMock).not.toHaveBeenCalled();

    listeners.get(PLUGINS_IMPORTED_EVENT)!();
    expect(invalidateMock).toHaveBeenCalledTimes(1);
    expect(invalidateMock).toHaveBeenCalledWith(client);

    unmount();
    expect(unlisten).toHaveBeenCalledTimes(1);
  });
});
