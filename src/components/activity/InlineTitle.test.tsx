// @vitest-environment happy-dom
import { describe, it, expect, afterEach, vi } from "vitest";
import { render, cleanup, screen, fireEvent } from "@testing-library/react";
import { InlineTitle } from "./InlineTitle";

afterEach(cleanup);

function renderTitle(onSave = vi.fn()) {
  render(<InlineTitle title="Morning Ride" onSave={onSave} />);
  return onSave;
}

function startEditing(): HTMLInputElement {
  fireEvent.click(screen.getByRole("heading"));
  return screen.getByLabelText("Activity title") as HTMLInputElement;
}

describe("InlineTitle", () => {
  it("click edits, Enter saves the trimmed new title", () => {
    const onSave = renderTitle();
    const input = startEditing();
    expect(input.value).toBe("Morning Ride");

    fireEvent.change(input, { target: { value: "  Alanya Gravel  " } });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(onSave).toHaveBeenCalledWith("Alanya Gravel");
    // Back to the heading (with the old prop until the query refetches).
    expect(screen.getByRole("heading")).toBeTruthy();
  });

  it("Escape cancels; unchanged or emptied drafts never save", () => {
    const onSave = renderTitle();

    let input = startEditing();
    fireEvent.change(input, { target: { value: "Changed" } });
    fireEvent.keyDown(input, { key: "Escape" });
    expect(onSave).not.toHaveBeenCalled();

    // Unchanged draft → no save on Enter.
    input = startEditing();
    fireEvent.keyDown(input, { key: "Enter" });
    // Emptied draft → cancel (update_activity can't clear a title).
    input = startEditing();
    fireEvent.change(input, { target: { value: "   " } });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(onSave).not.toHaveBeenCalled();
  });

  it("clamps overlong input in code and warns the user", () => {
    // maxLength only limits keystrokes — a pre-existing overlong value or a
    // paste sails past it, so the clamp lives in onChange/save.
    const onSave = renderTitle();
    const input = startEditing();
    fireEvent.change(input, { target: { value: "x".repeat(150) } });
    expect(input.value).toHaveLength(100);
    // The "can't type further" notice is visible.
    expect(screen.getByRole("status").textContent).toContain("Max 100 characters");

    fireEvent.keyDown(input, { key: "Enter" });
    expect(onSave).toHaveBeenCalledWith("x".repeat(100));
  });

  it("blur saves once — not again after Enter already did", () => {
    const onSave = renderTitle();
    const input = startEditing();
    fireEvent.change(input, { target: { value: "Via Blur" } });
    fireEvent.blur(input);
    expect(onSave).toHaveBeenCalledTimes(1);
    expect(onSave).toHaveBeenCalledWith("Via Blur");

    const input2 = startEditing();
    fireEvent.change(input2, { target: { value: "Via Enter" } });
    fireEvent.keyDown(input2, { key: "Enter" });
    fireEvent.blur(input2);
    expect(onSave).toHaveBeenCalledTimes(2); // Enter's save only, blur skipped
  });
});