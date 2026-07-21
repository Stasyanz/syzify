// @vitest-environment happy-dom
import { describe, it, expect, afterEach } from "vitest";
import { render, cleanup } from "@testing-library/react";
import { SportGlyph, SportIcon } from "./SportIcon";
import { getSportColor } from "../../lib/sportColors";

afterEach(cleanup);

describe("SportGlyph", () => {
  it("renders a filled glyph (matches the design's solid icons)", () => {
    const { container } = render(<SportGlyph sport="run" />);
    const svg = container.querySelector("svg")!;
    expect(svg.getAttribute("fill")).toBe("currentColor");
    expect(svg.getAttribute("stroke")).toBeNull();
  });

  it("tilts the paddle glyph via a transform group", () => {
    const { container } = render(<SportGlyph sport="paddle" />);
    expect(container.innerHTML).toContain('transform="rotate(-32 256 256)"');
  });

  it("falls back to the neutral glyph for an unknown sport", () => {
    const unknown = render(<SportGlyph sport="quidditch" />).container.innerHTML;
    const other = render(<SportGlyph sport="other" />).container.innerHTML;
    expect(unknown).toBe(other);
  });
});

describe("SportIcon", () => {
  it("paints the tile with the sport's identity color", () => {
    const { container } = render(<SportIcon sport="ride" />);
    const tile = container.querySelector("span")!;
    // happy-dom normalizes the hex to rgb, so compare via a probe element.
    const probe = document.createElement("span");
    probe.style.background = getSportColor("ride");
    expect(tile.style.background).toBe(probe.style.background);
  });
});
