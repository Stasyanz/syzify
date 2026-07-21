// @vitest-environment happy-dom
import { describe, it, expect, vi, afterEach } from "vitest";
import { render, fireEvent, cleanup } from "@testing-library/react";
import { CropOverlay } from "./CropOverlay";

afterEach(cleanup);

describe("CropOverlay move", () => {
  it("moving a quarter-turned oversized frame keeps its size (regression)", () => {
    // Portrait photo 400×600; frame straightened 90° with local width 500 px —
    // normalized w = 1.25 is legal for a rotated frame. Moving must translate
    // only: the old axis clamp snapped w back to 1.0 on the first move drag.
    const onChange = vi.fn();
    const crop = { x: -0.125, y: 175 / 600, w: 1.25, h: 250 / 600, straighten: 90, orientation: 0 };
    const { container } = render(
      <CropOverlay boxW={400} boxH={600} crop={crop} ratio={null} onChange={onChange} />
    );

    const moveTarget = Array.from(container.querySelectorAll("div")).find(
      (el) => el.style.cursor === "move"
    )!;
    expect(moveTarget).toBeTruthy();

    fireEvent.pointerDown(moveTarget, { clientX: 200, clientY: 300 });
    fireEvent.pointerMove(window, { clientX: 210, clientY: 300 });

    expect(onChange).toHaveBeenCalled();
    const emitted = onChange.mock.calls[onChange.mock.calls.length - 1][0];
    expect(emitted.w).toBeCloseTo(1.25, 6);
    expect(emitted.h).toBeCloseTo(250 / 600, 6);
    // Translated by 10 px right (10/400 in normalized x).
    expect(emitted.x).toBeCloseTo(-0.125 + 10 / 400, 6);
  });
});

describe("CropOverlay children", () => {
  function renderWithChild(crop: {
    x: number;
    y: number;
    w: number;
    h: number;
    straighten: number;
    orientation: number;
  }) {
    const { container } = render(
      <CropOverlay boxW={400} boxH={600} crop={crop} ratio={null} onChange={() => {}}>
        <span data-testid="wm">WM</span>
      </CropOverlay>
    );
    const frame = Array.from(container.querySelectorAll("div")).find(
      (el) => el.style.cursor === "move"
    )!;
    const wrapper = frame.querySelector('[data-testid="wm"]')!.parentElement as HTMLElement;
    return { frame, wrapper };
  }

  it("renders children inside the selection frame (the watermark stays while cropping)", () => {
    const { frame, wrapper } = renderWithChild({
      x: 0.1, y: 0.1, w: 0.5, h: 0.5, straighten: 15, orientation: 0,
    });
    // Inside the frame div, so it inherits the frame's tilt transform; the
    // output wrapper is frame-sized and unrotated at orientation 0.
    expect(frame.contains(wrapper)).toBe(true);
    expect(wrapper.style.width).toBe("200px"); // 0.5 * 400
    expect(wrapper.style.height).toBe("300px"); // 0.5 * 600
    expect(wrapper.style.transform).toBe("rotate(0deg)");
  });

  it("orientation 90: children live in the OUTPUT's coordinate system (WYSIWYG)", () => {
    // Frame 200×100 px; the exported image is 100×200 (quarter-turn swap).
    // The wrapper must be output-sized, centered on the frame and
    // counter-rotated, so right/bottom-pinned content lands exactly where it
    // will on the export.
    const { wrapper } = renderWithChild({
      x: 0, y: 0, w: 0.5, h: 100 / 600, straighten: 0, orientation: 90,
    });
    expect(wrapper.style.width).toBe("100px");
    expect(wrapper.style.height).toBe("200px");
    expect(wrapper.style.left).toBe("50px"); // (200 - 100) / 2
    expect(wrapper.style.top).toBe("-50px"); // (100 - 200) / 2
    expect(wrapper.style.transform).toBe("rotate(-90deg)");
    // The quarter-fold at 45° is discrete — the wrapper eases it instead of clicking.
    expect(wrapper.style.transition).toContain("transform");
  });

  it("the rotation knob uses the custom rotate cursor (grab fallback)", () => {
    const { frame } = renderWithChild({
      x: 0.1, y: 0.1, w: 0.5, h: 0.5, straighten: 0, orientation: 0,
    });
    const knob = frame.querySelector('[title^="Drag to rotate"]') as HTMLElement;
    expect(knob).toBeTruthy();
    expect(knob.style.cursor).toContain("data:image/svg+xml");
    expect(knob.style.cursor).toContain("grab");
  });

  it("orientation 180: no swap, only the counter-rotation", () => {
    const { wrapper } = renderWithChild({
      x: 0, y: 0, w: 0.5, h: 100 / 600, straighten: 0, orientation: 180,
    });
    expect(wrapper.style.width).toBe("200px");
    expect(wrapper.style.height).toBe("100px");
    expect(wrapper.style.transform).toBe("rotate(-180deg)");
  });
});
