// @vitest-environment happy-dom
import { useState } from "react";
import { describe, it, expect, afterEach, vi } from "vitest";
import { render, cleanup, fireEvent, screen } from "@testing-library/react";
import { Checkbox } from "./Checkbox";

afterEach(cleanup);

describe("Checkbox", () => {
  it("reports the new checked state on change", () => {
    const onChange = vi.fn();
    render(<Checkbox checked={false} onChange={onChange} ariaLabel="Route map" />);
    fireEvent.click(screen.getByLabelText("Route map"));
    expect(onChange).toHaveBeenCalledWith(true);
  });

  it("does not fire when disabled", () => {
    const onChange = vi.fn();
    render(<Checkbox checked={false} disabled onChange={onChange} ariaLabel="Route map" />);
    fireEvent.click(screen.getByLabelText("Route map"));
    expect(onChange).not.toHaveBeenCalled();
  });

  it("toggles via a wrapping label, like the share panel uses it", () => {
    function Host() {
      const [on, setOn] = useState(false);
      return (
        <label>
          <Checkbox checked={on} onChange={setOn} />
          Title &amp; date
        </label>
      );
    }
    render(<Host />);
    const input = screen.getByRole("checkbox") as HTMLInputElement;
    expect(input.checked).toBe(false);
    fireEvent.click(screen.getByText(/title & date/i));
    expect(input.checked).toBe(true);
  });
});