// @vitest-environment happy-dom
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { renderHook, act, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactNode } from "react";

vi.mock("../../lib/tauri", () => ({
  api: { getSetting: vi.fn(async () => null), setSetting: vi.fn(async () => undefined) },
}));

import { api } from "../../lib/tauri";
import { useResizeGrip } from "./useResizeGrip";

const clamp = (px: number) => (Number.isFinite(px) ? Math.min(300, Math.max(200, Math.round(px))) : 200);

function mount() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={qc}>{children}</QueryClientProvider>
  );
  return { ...renderHook(() => useResizeGrip(200, clamp, "k"), { wrapper }), qc };
}

/** A pointer event against a handle that records capture calls. */
function pointer(clientY: number, captured = true) {
  const target = {
    setPointerCapture: vi.fn(),
    hasPointerCapture: vi.fn(() => captured),
    releasePointerCapture: vi.fn(),
  };
  return {
    e: {
      pointerId: 7,
      clientY,
      preventDefault: vi.fn(),
      stopPropagation: vi.fn(),
      currentTarget: target,
    } as unknown as React.PointerEvent<HTMLElement>,
    target,
  };
}

describe("useResizeGrip", () => {
  beforeEach(() => vi.mocked(api.setSetting).mockClear());
  afterEach(() => vi.mocked(api.getSetting).mockImplementation(async () => null));

  it("drags from the height at pointer-down, clamped, and persists once on release", () => {
    const { result } = mount();
    expect(result.current.height).toBe(200);
    const down = pointer(100);
    act(() => result.current.grip.onPointerDown(down.e));
    expect(down.target.setPointerCapture).toHaveBeenCalledWith(7);
    expect((down.e as unknown as { stopPropagation: () => void }).stopPropagation).toHaveBeenCalled();
    act(() => result.current.grip.onPointerMove(pointer(150).e));
    expect(result.current.height).toBe(250);
    act(() => result.current.grip.onPointerMove(pointer(900).e));
    expect(result.current.height).toBe(300);
    expect(api.setSetting).not.toHaveBeenCalled();
    const up = pointer(900);
    act(() => result.current.grip.onPointerUp(up.e));
    expect(up.target.releasePointerCapture).toHaveBeenCalledWith(7);
    expect(api.setSetting).toHaveBeenCalledTimes(1);
    expect(api.setSetting).toHaveBeenCalledWith("k", "300");
    // After release a move is not a drag, and a second drag starts from the
    // new height, not from the old one.
    act(() => result.current.grip.onPointerMove(pointer(0).e));
    expect(result.current.height).toBe(300);
    act(() => result.current.grip.onPointerDown(pointer(0).e));
    act(() => result.current.grip.onPointerMove(pointer(-40).e));
    expect(result.current.height).toBe(260);
  });

  it("ignores a release with no drag, and does not release an uncaptured pointer", () => {
    const { result } = mount();
    const up = pointer(50);
    act(() => result.current.grip.onPointerUp(up.e));
    expect(up.target.releasePointerCapture).not.toHaveBeenCalled();
    expect(api.setSetting).not.toHaveBeenCalled();
    act(() => result.current.grip.onPointerDown(pointer(0).e));
    const cancel = pointer(0, false);
    act(() => result.current.grip.onPointerCancel(cancel.e));
    expect(cancel.target.releasePointerCapture).not.toHaveBeenCalled();
    expect(api.setSetting).toHaveBeenCalledWith("k", "200");
  });

  it("restores the persisted height through the clamp, unless a drag is already in progress", async () => {
    vi.mocked(api.getSetting).mockImplementation(async (key: string) => (key === "k" ? "9999" : null));
    const { result } = mount();
    await waitFor(() => expect(result.current.height).toBe(300));

    // A setting that resolves after pointer-down leaves the drag alone.
    let resolve: (v: string) => void = () => {};
    vi.mocked(api.getSetting).mockImplementation(() => new Promise<string | null>((r) => (resolve = r)));
    const late = mount();
    expect(late.result.current.height).toBe(200);
    act(() => late.result.current.grip.onPointerDown(pointer(0).e));
    act(() => late.result.current.grip.onPointerMove(pointer(30).e));
    await act(async () => resolve("280"));
    // The setting did arrive — and the height is still the dragged one.
    await waitFor(() => expect(late.qc.getQueryData(["setting", "k"])).toBe("280"));
    expect(late.result.current.height).toBe(230);
  });
});
