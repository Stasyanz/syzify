// @vitest-environment happy-dom
import { describe, it, expect, afterEach, beforeEach, vi } from "vitest";
import { renderHook, cleanup, waitFor, act } from "@testing-library/react";
import { TRAINING_LIVE_EVENT, useTrainingLive } from "./useTrainingLive";
import { api } from "../lib/tauri";
import type { TrainingLive } from "../lib/types";

type Handler = (event: { payload: TrainingLive | null }) => void;
const listeners = new Map<string, Handler>();
const unlisten = vi.fn();
let resolveListen: ((fn: () => void) => void) | null = null;
let holdListen = false;
let listenRejects = false;
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((name: string, cb: Handler) => {
    listeners.set(name, cb);
    if (listenRejects) return Promise.reject(new Error("no event bus"));
    return new Promise<() => void>((resolve) => {
      resolveListen = resolve;
      if (!holdListen) resolve(unlisten);
    });
  }),
}));
vi.mock("../lib/tauri", () => ({ api: { trainingLive: vi.fn() } }));
const mocked = vi.mocked(api);

function live(over: Partial<TrainingLive> = {}): TrainingLive {
  return {
    phase: "riding",
    ride_s: 10,
    lap_s: 10,
    lap: 1,
    watts: 200,
    power_from: "pedals",
    trainer_watts: 210,
    cadence_rpm: 90,
    heart_rate: 140,
    speed_kmh: 30,
    distance_m: 80,
    avg_watts: 200,
    profile: null,
    links: [],
    warnings: [],
    slept_s: 0,
    erg: null,
    release_unconfirmed: false,
    power_match: null,
    ...over,
  };
}

beforeEach(() => {
  mocked.trainingLive.mockReset();
  listeners.clear();
  unlisten.mockClear();
  resolveListen = null;
  holdListen = false;
  listenRejects = false;
});
afterEach(cleanup);

describe("useTrainingLive", () => {
  it("starts from what the backend last showed, then follows the event", async () => {
    mocked.trainingLive.mockResolvedValue(live({ ride_s: 10 }));
    const { result, unmount } = renderHook(() => useTrainingLive());
    expect(result.current).toBeUndefined();
    await waitFor(() => expect(result.current?.ride_s).toBe(10));
    await waitFor(() => expect(listeners.has(TRAINING_LIVE_EVENT)).toBe(true));
    act(() => listeners.get(TRAINING_LIVE_EVENT)!({ payload: live({ ride_s: 11 }) }));
    expect(result.current?.ride_s).toBe(11);
    act(() => listeners.get(TRAINING_LIVE_EVENT)!({ payload: null }));
    expect(result.current).toBeNull();
    unmount();
    expect(unlisten).toHaveBeenCalledTimes(1);
  });

  it("keeps an event that came before the slower first answer", async () => {
    let answer: (v: TrainingLive | null) => void = () => {};
    mocked.trainingLive.mockReturnValue(new Promise((resolve) => (answer = resolve)));
    const { result } = renderHook(() => useTrainingLive());
    await waitFor(() => expect(listeners.has(TRAINING_LIVE_EVENT)).toBe(true));
    act(() => listeners.get(TRAINING_LIVE_EVENT)!({ payload: live({ ride_s: 99 }) }));
    await act(async () => answer(live({ ride_s: 1 })));
    expect(result.current?.ride_s).toBe(99);
  });

  it("listens before it asks, so a session ending in between is heard", async () => {
    holdListen = true;
    mocked.trainingLive.mockResolvedValue(live());
    const { result } = renderHook(() => useTrainingLive());
    await waitFor(() => expect(resolveListen).not.toBeNull());
    expect(mocked.trainingLive).not.toHaveBeenCalled();
    resolveListen!(unlisten);
    await waitFor(() => expect(result.current?.ride_s).toBe(10));
  });

  it("lets nothing in after the unmount, nor before it subscribed", async () => {
    // Gone before the event bus was even loaded: it never subscribes.
    mocked.trainingLive.mockResolvedValue(null);
    const early = renderHook(() => useTrainingLive());
    early.unmount();
    await Promise.resolve();
    await new Promise((r) => setTimeout(r, 0));
    expect(listeners.has(TRAINING_LIVE_EVENT)).toBe(false);
    expect(mocked.trainingLive).not.toHaveBeenCalled();

    // Gone after: a late event and a late answer change nothing.
    let fail: (e: unknown) => void = () => {};
    mocked.trainingLive.mockReturnValue(new Promise((_, reject) => (fail = reject)));
    const { result, unmount } = renderHook(() => useTrainingLive());
    await waitFor(() => expect(listeners.has(TRAINING_LIVE_EVENT)).toBe(true));
    act(() => listeners.get(TRAINING_LIVE_EVENT)!({ payload: live({ ride_s: 7 }) }));
    await act(async () => fail(new Error("late")));
    // The event stands: a failed answer after it is not a null session.
    expect(result.current?.ride_s).toBe(7);
    unmount();
    act(() => listeners.get(TRAINING_LIVE_EVENT)!({ payload: live({ ride_s: 8 }) }));
    expect(result.current?.ride_s).toBe(7);
  });

  it("reads a failed first answer as no session", async () => {
    mocked.trainingLive.mockRejectedValue(new Error("no backend"));
    const { result } = renderHook(() => useTrainingLive());
    await waitFor(() => expect(result.current).toBeNull());
  });

  it("unlistens right away when the listener resolves after the unmount", async () => {
    holdListen = true;
    mocked.trainingLive.mockResolvedValue(null);
    const { unmount } = renderHook(() => useTrainingLive());
    await waitFor(() => expect(resolveListen).not.toBeNull());
    unmount();
    resolveListen!(unlisten);
    await waitFor(() => expect(unlisten).toHaveBeenCalledTimes(1));
  });

  it("survives an environment without the event bus", async () => {
    listenRejects = true;
    mocked.trainingLive.mockResolvedValue(null);
    const { result, unmount } = renderHook(() => useTrainingLive());
    await waitFor(() => expect(result.current).toBeNull());
    unmount();
    expect(unlisten).not.toHaveBeenCalled();
  });
});
