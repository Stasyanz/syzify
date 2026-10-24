// @vitest-environment happy-dom
import { afterEach, describe, expect, it } from "vitest";
import { act, renderHook } from "@testing-library/react";
import { anyBusy, useAnyBusy, useBusy, useBusyError, useBusyProgress, useBusyStore } from "./busyStore";

afterEach(() => useBusyStore.setState({ running: {}, lastError: {} }));

describe("busyStore (#235)", () => {
  it("tracks each operation apart, with its progress", () => {
    const store = useBusyStore.getState();
    store.start("vaultMove");
    store.start("updateInstall");
    store.progress("updateInstall", 42);
    expect(useBusyStore.getState().running).toEqual({ vaultMove: null, updateInstall: 42 });
    store.finish("vaultMove");
    expect(useBusyStore.getState().running).toEqual({ updateInstall: 42 });
    store.finish("updateInstall");
    expect(useBusyStore.getState().running).toEqual({});
  });

  it("ignores progress of an operation that is not running, so a late event cannot revive it", () => {
    const store = useBusyStore.getState();
    store.progress("updateInstall", 10);
    expect(useBusyStore.getState().running).toEqual({});
    store.start("updateInstall");
    store.finish("updateInstall");
    store.progress("updateInstall", 99);
    expect(useBusyStore.getState().running).toEqual({});
  });

  it("keeps what a run failed with until the next start, whatever the rejection was", () => {
    const store = useBusyStore.getState();
    const failure = renderHook(() => useBusyError("updateInstall"));
    expect(failure.result.current).toEqual({ failed: false, error: undefined });
    act(() => store.start("updateInstall"));
    act(() => store.finish("updateInstall", "bad signature"));
    expect(failure.result.current).toEqual({ failed: true, error: "bad signature" });
    // A promise may reject with undefined: still a failure.
    act(() => store.start("updateInstall"));
    expect(failure.result.current.failed).toBe(false);
    act(() => store.finish("updateInstall", undefined));
    expect(failure.result.current).toEqual({ failed: true, error: undefined });
    // A success says nothing and clears nothing of another operation.
    act(() => store.start("backup"));
    act(() => store.finish("backup"));
    expect(useBusyStore.getState().lastError).toEqual({ updateInstall: undefined });
    expect(anyBusy()).toBe(false);
  });

  it("feeds the hooks a section and the language picker read", () => {
    const busy = renderHook(() => useBusy("watchScan"));
    const progress = renderHook(() => useBusyProgress("watchScan"));
    const any = renderHook(() => useAnyBusy());
    expect([busy.result.current, progress.result.current, any.result.current]).toEqual([false, null, false]);
    act(() => useBusyStore.getState().start("watchScan"));
    expect([busy.result.current, progress.result.current, any.result.current]).toEqual([true, null, true]);
    act(() => useBusyStore.getState().progress("watchScan", 5));
    expect(progress.result.current).toBe(5);
    // Another section's operation is not this one's, but locks the picker.
    act(() => useBusyStore.getState().finish("watchScan"));
    act(() => useBusyStore.getState().start("monitoringDelete"));
    expect([busy.result.current, progress.result.current, any.result.current]).toEqual([false, null, true]);
  });
});
