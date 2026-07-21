// @vitest-environment happy-dom
import { useState } from "react";
import { describe, it, expect, afterEach, vi } from "vitest";
import { render, cleanup, fireEvent, screen } from "@testing-library/react";
import { Select } from "./Select";

afterEach(cleanup);

const options = [
  { value: "", label: "All sports" },
  { value: "run", label: "Run" },
  { value: "ride", label: "Ride" },
];

describe("Select", () => {
  it("shows the selected option's label on the trigger", () => {
    render(<Select value="ride" options={options} onChange={() => {}} />);
    expect(screen.getByRole("button", { name: /ride/i })).toBeTruthy();
  });

  it("opens the menu and selects an option", () => {
    const onChange = vi.fn();
    render(<Select value="" options={options} onChange={onChange} ariaLabel="Sport" />);

    // Menu closed initially.
    expect(screen.queryByRole("listbox")).toBeNull();

    fireEvent.click(screen.getByLabelText("Sport"));
    expect(screen.getByRole("listbox")).toBeTruthy();

    fireEvent.click(screen.getByRole("option", { name: "Run" }));
    expect(onChange).toHaveBeenCalledWith("run");
    // Menu closes after picking.
    expect(screen.queryByRole("listbox")).toBeNull();
  });

  it("renders option icons before the labels", () => {
    const withIcons = [
      { value: "run", label: "Run", icon: <span data-testid="ic-run">R</span> },
      { value: "ride", label: "Ride" },
    ];
    render(<Select value="run" options={withIcons} onChange={() => {}} ariaLabel="Sport" />);
    fireEvent.click(screen.getByLabelText("Sport"));
    // The icon lives inside its option row; icon-less options still render.
    expect(screen.getByRole("option", { name: /run/i }).contains(screen.getByTestId("ic-run"))).toBe(
      true,
    );
    expect(screen.getByRole("option", { name: "Ride" })).toBeTruthy();
  });

  it("marks the current value as selected in the menu", () => {
    render(<Select value="run" options={options} onChange={() => {}} ariaLabel="Sport" />);
    fireEvent.click(screen.getByLabelText("Sport"));
    const run = screen.getByRole("option", { name: "Run" });
    expect(run.getAttribute("aria-selected")).toBe("true");
  });

  it("closes on Escape", () => {
    render(<Select value="" options={options} onChange={() => {}} ariaLabel="Sport" />);
    fireEvent.click(screen.getByLabelText("Sport"));
    expect(screen.getByRole("listbox")).toBeTruthy();
    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByRole("listbox")).toBeNull();
  });
});

describe("Select multiple", () => {
  const sports = [
    { value: "run", label: "Run" },
    { value: "ride", label: "Ride" },
    { value: "swim", label: "Swim" },
  ];

  it("toggles options without closing and keeps the options' order", () => {
    const onChange = vi.fn();
    // Controlled harness: the component only reports; the state must flow
    // back in for the next toggle to build on the previous one.
    function Harness() {
      const [values, setValues] = useState<string[]>(["swim"]);
      return (
        <Select
          multiple
          values={values}
          options={sports}
          onChange={(vs) => {
            onChange(vs);
            setValues(vs);
          }}
          placeholder="All sports"
          ariaLabel="Sport"
        />
      );
    }
    render(<Harness />);
    fireEvent.click(screen.getByLabelText("Sport"));

    // Adding a sport reports [run, swim] in OPTIONS order, not click order.
    fireEvent.click(screen.getByRole("option", { name: "Run" }));
    expect(onChange).toHaveBeenLastCalledWith(["run", "swim"]);
    // The menu stays open for further toggles.
    expect(screen.getByRole("listbox")).toBeTruthy();

    // Clicking a selected option removes it.
    fireEvent.click(screen.getByRole("option", { name: /Swim/ }));
    expect(onChange).toHaveBeenLastCalledWith(["run"]);
  });

  it("shows the placeholder when empty and the clear row resets", () => {
    const onChange = vi.fn();
    render(
      <Select
        multiple
        values={[]}
        options={sports}
        onChange={onChange}
        placeholder="All sports"
        clearLabel="All sports"
        ariaLabel="Sport"
      />,
    );
    // Empty selection → placeholder on the trigger.
    expect(screen.getByLabelText("Sport").textContent).toContain("All sports");

    fireEvent.click(screen.getByLabelText("Sport"));
    // The clear row is checked while nothing is selected...
    const clear = screen.getByRole("option", { name: /All sports/ });
    expect(clear.getAttribute("aria-selected")).toBe("true");
    // ...and clicking it reports an empty selection and closes.
    fireEvent.click(clear);
    expect(onChange).toHaveBeenCalledWith([]);
    expect(screen.queryByRole("listbox")).toBeNull();
  });
});
