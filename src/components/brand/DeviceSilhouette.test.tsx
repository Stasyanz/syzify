// @vitest-environment happy-dom
import { describe, it, expect, afterEach } from "vitest";
import { render, cleanup } from "@testing-library/react";
import { DeviceSilhouette } from "./DeviceSilhouette";
import type { DeviceForm } from "../../lib/devices";

afterEach(cleanup);

const FORMS: DeviceForm[] = ["watch_multi", "watch_round", "watch_instinct", "watch_rect", "bike_computer", "generic"];

describe("DeviceSilhouette", () => {
  it.each(FORMS)("draws %s as a line drawing in the parent's color", (form) => {
    const { container } = render(<DeviceSilhouette form={form} />);
    const svg = container.querySelector("svg")!;
    expect(svg.getAttribute("aria-hidden")).toBe("true");
    expect(svg.getAttribute("data-form")).toBe(form);
    expect(svg.getAttribute("width")).toBe("28");
    // Strokes and fills both come from currentColor: no baked-in palette.
    expect(container.innerHTML).not.toMatch(/#[0-9a-f]{3,6}/i);
    expect(container.innerHTML).toContain('stroke="currentColor"');
  });

  it("gives every form factor its own drawing", () => {
    const drawings = FORMS.map((form) => render(<DeviceSilhouette form={form} />).container.innerHTML);
    expect(new Set(drawings).size).toBe(FORMS.length);
  });

  it("tells the watches apart by their details, not only by name", () => {
    const html = (form: DeviceForm) => render(<DeviceSilhouette form={form} />).container.innerHTML;
    // The Instinct has its sub-dial: one more circle than the rugged watch.
    expect((html("watch_instinct").match(/<circle/g) ?? []).length).toBe((html("watch_multi").match(/<circle/g) ?? []).length + 1);
    // The rugged watch has five buttons, the plain round one three.
    const buttons = (form: DeviceForm) => (html(form).match(/<rect[^>]*(?:x="5\.3"|x="5\.6"|x="25\.2"|x="25\.3")/g) ?? []).length;
    expect(buttons("watch_multi")).toBe(5);
    expect(buttons("watch_round")).toBe(3);
    // Straps mark a watch; the head unit and the generic gadget have none.
    expect(html("bike_computer")).not.toContain('y="1.5"');
    expect(html("generic")).not.toContain('y="1.5"');
    expect(html("watch_rect")).toContain('y="1.5"');
  });

  it("honours a custom size", () => {
    const { container } = render(<DeviceSilhouette form="generic" size={20} />);
    expect(container.querySelector("svg")!.getAttribute("height")).toBe("20");
  });
});
