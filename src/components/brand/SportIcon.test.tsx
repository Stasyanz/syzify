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

  it("draws the trainer rides with glyphs of their own (#189)", () => {
    const indoor = render(<SportGlyph sport="indoor_ride" />).container.innerHTML;
    const virtual = render(<SportGlyph sport="virtual_ride" />).container.innerHTML;
    const other = render(<SportGlyph sport="other" />).container.innerHTML;
    const ride = render(<SportGlyph sport="ride" />).container.innerHTML;
    expect(indoor).not.toBe(other);
    expect(virtual).not.toBe(other);
    expect(indoor).not.toBe(virtual);
    expect(virtual).not.toBe(ride);
    // Each is drawn in its own box, and the path is a closed filled figure.
    const svgOf = (sport: string) => render(<SportGlyph sport={sport} />).container.querySelector("svg")!;
    expect(svgOf("indoor_ride").getAttribute("viewBox")).toBe("0 0 512 512");
    expect(svgOf("virtual_ride").getAttribute("viewBox")).toBe("0 0 640 512");
    for (const sport of ["indoor_ride", "virtual_ride"]) {
      const d = svgOf(sport).querySelector("path")!.getAttribute("d")!;
      expect(d.startsWith("M")).toBe(true);
      expect(d.endsWith("z")).toBe(true);
      expect(d.length).toBeGreaterThan(500);
    }
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
