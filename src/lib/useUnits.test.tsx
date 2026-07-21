// @vitest-environment happy-dom
import { describe, it, expect, afterEach } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useUnits, useUnitsStore } from "./units";

describe("useUnits", () => {
  afterEach(() => useUnitsStore.setState({ mode: "metric" }));

  it("re-renders the subscriber when the units mode flips", () => {
    const { result } = renderHook(() => useUnits());
    expect(result.current).toBe("metric");

    act(() => useUnitsStore.getState().setMode("imperial"));
    expect(result.current).toBe("imperial");

    act(() => useUnitsStore.getState().setMode("metric"));
    expect(result.current).toBe("metric");
  });
});
