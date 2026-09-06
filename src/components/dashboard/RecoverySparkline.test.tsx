// @vitest-environment happy-dom
import { describe, it, expect, vi, afterEach } from "vitest";
import { render, cleanup, fireEvent, act } from "@testing-library/react";
import { RecoverySparkline } from "./RecoverySparkline";

const history = [
  { date: "2026-06-09", index: 90, band: "intervals_ok" as const },
  { date: "2026-06-10", index: 55, band: "rest" as const },
];

afterEach(cleanup);

describe("RecoverySparkline", () => {
  it("draws the nights on their own days, joined only when consecutive", () => {
    const { container } = render(
      <RecoverySparkline
        history={[...history, { date: "2026-05-30", index: 70, band: "easy_day" }]}
        computedFor="2026-06-10"
      />,
    );
    // Three nights in the window → three points; one segment (09→10) + two guides.
    const circles = container.querySelectorAll("circle");
    expect(circles).toHaveLength(3);
    expect(container.querySelectorAll("svg line")).toHaveLength(3);
    // Point colors come from the backend's band, not a frontend re-derivation.
    expect(circles[2].getAttribute("fill")).toBe("var(--danger)");
    expect(container.querySelector("svg")?.getAttribute("aria-label")).toBe(
      "Recovery index, last 28 days: 3 nights, latest 55 on Jun 10",
    );
    expect(container.textContent).toContain("4 weeks ago");
    expect(container.textContent).toContain("Today");
  });

  it("labels a hovered point with its night and index", () => {
    const { container, queryByTestId, getByTestId } = render(
      <RecoverySparkline history={history} computedFor="2026-06-10" />,
    );
    expect(queryByTestId("spark-tip")).toBeNull();
    const circles = container.querySelectorAll("circle");
    fireEvent.mouseEnter(circles[1]);
    expect(getByTestId("spark-tip").textContent).toBe("Jun 10 · 55");
    fireEvent.mouseLeave(circles[1]);
    expect(queryByTestId("spark-tip")).toBeNull();
  });

  it("says so instead of drawing an empty frame", () => {
    const { container, getByTestId } = render(
      <RecoverySparkline
        history={[{ date: "2026-04-01", index: 88, band: "intervals_ok" }]}
        computedFor="2026-06-10"
      />,
    );
    expect(container.querySelector("svg")).toBeNull();
    expect(getByTestId("spark-empty").textContent).toBe("No nights in the last 28 days");
  });

  it("stretches the time axis to the width it is given", () => {
    // happy-dom has no ResizeObserver; stand one in that reports on observe.
    let callback: ResizeObserverCallback | undefined;
    const disconnect = vi.fn();
    class FakeRO {
      constructor(cb: ResizeObserverCallback) {
        callback = cb;
      }
      observe() {
        callback?.([{ contentRect: { width: 420 } } as ResizeObserverEntry], this as never);
      }
      unobserve() {}
      disconnect = disconnect;
    }
    vi.stubGlobal("ResizeObserver", FakeRO);
    try {
      const { container, unmount } = render(
        <RecoverySparkline history={history} computedFor="2026-06-10" />,
      );
      const svg = container.querySelector("svg")!;
      expect(svg.getAttribute("width")).toBe("420");
      // The last night sits at the right edge of whatever width it got.
      expect(container.querySelectorAll("circle")[1].getAttribute("cx")).toBe(String(420 - 5));
      // A zero-width measurement (hidden container) keeps the last width.
      act(() => callback?.([{ contentRect: { width: 0 } } as ResizeObserverEntry], {} as never));
      expect(svg.getAttribute("width")).toBe("420");
      unmount();
      expect(disconnect).toHaveBeenCalled();
    } finally {
      vi.unstubAllGlobals();
    }
  });
});
