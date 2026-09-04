// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook } from "@testing-library/react";
import { StrictMode, type ReactNode } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { useInvalidateOnNewDay, useMonthView, useToday, useTodayKey } from "./useToday";
import { dayKey } from "../lib/calendar";

// 30 s before midnight, local time.
const nearMidnight = new Date(2026, 8, 3, 23, 59, 30);

describe("useToday", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(nearMidnight);
  });
  afterEach(() => {
    cleanup();
    vi.useRealTimers();
  });

  it("starts on the current local day", () => {
    const { result } = renderHook(() => useToday());
    expect(dayKey(result.current)).toBe("2026-09-03");
  });

  it("rolls over to the new day on the next minute tick", () => {
    const { result } = renderHook(() => useToday());
    const before = result.current;
    act(() => {
      vi.advanceTimersByTime(60_000);
    });
    expect(dayKey(result.current)).toBe("2026-09-04");
    expect(result.current).not.toBe(before);
  });

  it("keeps the same Date instance while the day has not changed", () => {
    vi.setSystemTime(new Date(2026, 8, 3, 12, 0, 0));
    const { result } = renderHook(() => useToday());
    const before = result.current;
    act(() => {
      vi.advanceTimersByTime(5 * 60_000);
    });
    expect(result.current).toBe(before);
  });

  it("re-checks immediately when the window becomes visible or focused", () => {
    const { result } = renderHook(() => useToday());
    // The clock jumps past midnight without a tick (laptop asleep).
    vi.setSystemTime(new Date(2026, 8, 4, 8, 0, 0));
    expect(dayKey(result.current)).toBe("2026-09-03");
    act(() => {
      document.dispatchEvent(new Event("visibilitychange"));
    });
    expect(dayKey(result.current)).toBe("2026-09-04");

    vi.setSystemTime(new Date(2026, 8, 5, 8, 0, 0));
    act(() => {
      window.dispatchEvent(new Event("focus"));
    });
    expect(dayKey(result.current)).toBe("2026-09-05");
  });

  it("ignores a visibility event while the window is hidden", () => {
    const { result } = renderHook(() => useToday());
    vi.setSystemTime(new Date(2026, 8, 4, 8, 0, 0));
    const state = vi
      .spyOn(document, "visibilityState", "get")
      .mockReturnValue("hidden");
    act(() => {
      document.dispatchEvent(new Event("visibilitychange"));
    });
    expect(dayKey(result.current)).toBe("2026-09-03");
    state.mockRestore();
  });

  it("shares one ticker and one value across subscribers", () => {
    const a = renderHook(() => useTodayKey());
    const b = renderHook(() => useTodayKey());
    expect(vi.getTimerCount()).toBe(1);
    act(() => {
      vi.advanceTimersByTime(60_000);
    });
    expect(a.result.current).toBe("2026-09-04");
    expect(b.result.current).toBe("2026-09-04");
    a.unmount();
    expect(vi.getTimerCount()).toBe(1);
    b.unmount();
    expect(vi.getTimerCount()).toBe(0);
  });

  it("catches up on a day that passed while nobody was subscribed", () => {
    const first = renderHook(() => useTodayKey());
    expect(first.result.current).toBe("2026-09-03");
    first.unmount();
    vi.setSystemTime(new Date(2026, 8, 6, 9, 0, 0));
    const second = renderHook(() => useTodayKey());
    expect(second.result.current).toBe("2026-09-06");
  });

  it("removes the visibility and focus listeners with the last subscriber", () => {
    const docRemove = vi.spyOn(document, "removeEventListener");
    const winRemove = vi.spyOn(window, "removeEventListener");
    const { unmount } = renderHook(() => useTodayKey());
    expect(docRemove).not.toHaveBeenCalledWith("visibilitychange", expect.any(Function));
    unmount();
    expect(docRemove).toHaveBeenCalledWith("visibilitychange", expect.any(Function));
    expect(winRemove).toHaveBeenCalledWith("focus", expect.any(Function));
    docRemove.mockRestore();
    winRemove.mockRestore();
  });

  it("under StrictMode keeps one ticker and a stable Date across re-renders", () => {
    const { result, rerender } = renderHook(() => useToday(), { wrapper: StrictMode });
    expect(vi.getTimerCount()).toBe(1);
    const before = result.current;
    rerender();
    expect(result.current).toBe(before);
    act(() => {
      vi.advanceTimersByTime(60_000);
    });
    expect(dayKey(result.current)).toBe("2026-09-04");
    expect(vi.getTimerCount()).toBe(1);
  });
});

describe("useInvalidateOnNewDay", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(nearMidnight);
  });
  afterEach(() => {
    cleanup();
    vi.useRealTimers();
  });

  function setup() {
    const qc = new QueryClient();
    const spy = vi.spyOn(qc, "invalidateQueries");
    const wrapper = ({ children }: { children: ReactNode }) => (
      <QueryClientProvider client={qc}>{children}</QueryClientProvider>
    );
    const hook = renderHook(() => useInvalidateOnNewDay(["dashboard"]), { wrapper });
    return { spy, hook };
  }

  it("does nothing on mount or on re-renders within the day", () => {
    const { spy, hook } = setup();
    hook.rerender();
    act(() => {
      vi.advanceTimersByTime(20_000);
    });
    expect(spy).not.toHaveBeenCalled();
  });

  it("invalidates the key once when the day changes", () => {
    const { spy } = setup();
    act(() => {
      vi.advanceTimersByTime(60_000);
    });
    expect(spy).toHaveBeenCalledTimes(1);
    expect(spy).toHaveBeenCalledWith({ queryKey: ["dashboard"] });
    act(() => {
      vi.advanceTimersByTime(60_000);
    });
    expect(spy).toHaveBeenCalledTimes(1);
  });
});

describe("useMonthView", () => {
  afterEach(cleanup);

  const sep3 = new Date(2026, 8, 3);
  const sep30 = new Date(2026, 8, 30);
  const oct1 = new Date(2026, 9, 1);

  it("seeds the view with today's month", () => {
    const { result } = renderHook(({ today }) => useMonthView(today), {
      initialProps: { today: sep3 },
    });
    expect(result.current).toMatchObject({ year: 2026, month: 9 });
  });

  it("stays put on a day change within the month", () => {
    const { result, rerender } = renderHook(({ today }) => useMonthView(today), {
      initialProps: { today: sep3 },
    });
    rerender({ today: sep30 });
    expect(result.current).toMatchObject({ year: 2026, month: 9 });
  });

  it("follows a month rollover while showing today's month", () => {
    const { result, rerender } = renderHook(({ today }) => useMonthView(today), {
      initialProps: { today: sep30 },
    });
    rerender({ today: oct1 });
    expect(result.current).toMatchObject({ year: 2026, month: 10 });
  });

  it("follows a year rollover too", () => {
    const { result, rerender } = renderHook(({ today }) => useMonthView(today), {
      initialProps: { today: new Date(2026, 11, 31) },
    });
    rerender({ today: new Date(2027, 0, 1) });
    expect(result.current).toMatchObject({ year: 2027, month: 1 });
  });

  it("leaves a user browsing another month alone", () => {
    const { result, rerender } = renderHook(({ today }) => useMonthView(today), {
      initialProps: { today: sep30 },
    });
    act(() => {
      result.current.setMonth(3);
    });
    rerender({ today: oct1 });
    expect(result.current).toMatchObject({ year: 2026, month: 3 });
  });

  it("does not snap back when the user pages away on the same day", () => {
    const { result, rerender } = renderHook(({ today }) => useMonthView(today), {
      initialProps: { today: sep3 },
    });
    act(() => {
      result.current.setMonth(8);
    });
    rerender({ today: sep3 });
    expect(result.current).toMatchObject({ year: 2026, month: 8 });
  });

  it("under StrictMode a paged-away month survives the doubled effect", () => {
    const { result, rerender } = renderHook(({ today }) => useMonthView(today), {
      initialProps: { today: sep30 },
      wrapper: StrictMode,
    });
    act(() => {
      result.current.setMonth(3);
    });
    rerender({ today: oct1 });
    expect(result.current).toMatchObject({ year: 2026, month: 3 });
  });
});
