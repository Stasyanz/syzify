// @vitest-environment happy-dom
import { describe, it, expect, afterEach, vi } from "vitest";
import { render, cleanup, screen, fireEvent } from "@testing-library/react";
import type { ViewSpec } from "../../lib/types";
import { PluginViewRenderer } from "./PluginViewRenderer";

afterEach(cleanup);

describe("PluginViewRenderer", () => {
  it("renders each safe primitive from a ViewSpec", () => {
    const spec: ViewSpec = {
      title: "Consistency · last 4 weeks",
      elements: [
        { type: "heading", text: "Summary" },
        { type: "text", text: "Some note" },
        { type: "stat", label: "Streak", value: "5 weeks" },
        {
          type: "stat_grid",
          stats: [
            { label: "Activities", value: "8" },
            { label: "Per week", value: "2.0" },
          ],
        },
        { type: "table", headers: ["Day", "km"], rows: [["Mon", "10"]] },
        { type: "divider" },
      ],
    };

    const { container } = render(<PluginViewRenderer spec={spec} />);

    expect(screen.getByText("Consistency · last 4 weeks")).toBeTruthy();
    expect(screen.getByText("Summary")).toBeTruthy();
    expect(screen.getByText("Streak")).toBeTruthy();
    expect(screen.getByText("5 weeks")).toBeTruthy();
    expect(screen.getByText("Activities")).toBeTruthy();
    expect(screen.getByText("2.0")).toBeTruthy();
    expect(screen.getByText("Mon")).toBeTruthy();
    expect(container.querySelector("table")).toBeTruthy();
    expect(container.querySelector("hr")).toBeTruthy();
  });

  it("treats values as text — no HTML injection", () => {
    const spec: ViewSpec = {
      title: null,
      elements: [{ type: "text", text: "<img src=x onerror=alert(1)>" }],
    };
    const { container } = render(<PluginViewRenderer spec={spec} />);
    // The string is rendered verbatim as text, not parsed into an <img> element.
    expect(container.querySelector("img")).toBeNull();
    expect(screen.getByText("<img src=x onerror=alert(1)>")).toBeTruthy();
  });

  it("drives input and button through host callbacks", () => {
    const onChange = vi.fn();
    const onAction = vi.fn();
    const spec: ViewSpec = {
      title: null,
      elements: [
        { type: "input", id: "distance", label: "Distance (km)", value: "8", input_type: "number" },
        { type: "select", id: "sport", label: "Sport", options: ["run", "ride"], value: "run" },
        { type: "button", label: "Plan", action: "plan" },
      ],
    };
    render(
      <PluginViewRenderer
        spec={spec}
        values={{ distance: "8", sport: "run" }}
        onChange={onChange}
        onAction={onAction}
      />
    );

    fireEvent.change(screen.getByDisplayValue("8"), { target: { value: "12" } });
    expect(onChange).toHaveBeenCalledWith("distance", "12");

    fireEvent.click(screen.getByText("Plan"));
    expect(onAction).toHaveBeenCalledWith("plan");
  });

  it("ignores unknown element types from newer plugins", () => {
    const spec = {
      title: null,
      elements: [{ type: "future_thing", whatever: 1 }],
    } as unknown as ViewSpec;
    const { container } = render(<PluginViewRenderer spec={spec} />);
    // Renders the wrapper but no crash and no content for the unknown element.
    expect(container).toBeTruthy();
  });
});
