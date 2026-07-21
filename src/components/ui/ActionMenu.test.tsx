// @vitest-environment happy-dom
import { describe, it, expect, afterEach, vi } from "vitest";
import { render, cleanup, fireEvent, screen } from "@testing-library/react";
import { ActionMenu } from "./ActionMenu";

afterEach(cleanup);

function renderMenu(onOriginal = vi.fn(), onPrivate = vi.fn()) {
  render(
    <ActionMenu
      ariaLabel="Export GPX"
      items={[
        { label: "Original GPX", hint: "Full track as recorded", onSelect: onOriginal },
        { label: "With privacy zone", onSelect: onPrivate },
      ]}
    >
      ⬇
    </ActionMenu>,
  );
  return { onOriginal, onPrivate };
}

describe("ActionMenu", () => {
  it("opens on trigger click and runs the picked action once, then closes", () => {
    const { onOriginal, onPrivate } = renderMenu();

    expect(screen.queryByRole("menu")).toBeNull();
    fireEvent.click(screen.getByLabelText("Export GPX"));
    expect(screen.getByRole("menu")).toBeTruthy();

    fireEvent.click(screen.getByRole("menuitem", { name: /original gpx/i }));
    expect(onOriginal).toHaveBeenCalledTimes(1);
    expect(onPrivate).not.toHaveBeenCalled();
    // One-shot action: the menu closes after picking.
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("shows the hint line under an item that has one", () => {
    renderMenu();
    fireEvent.click(screen.getByLabelText("Export GPX"));
    expect(screen.getByText("Full track as recorded")).toBeTruthy();
  });

  it("closes on outside click and on Escape without firing actions", () => {
    const { onOriginal, onPrivate } = renderMenu();

    fireEvent.click(screen.getByLabelText("Export GPX"));
    fireEvent.mouseDown(document.body);
    expect(screen.queryByRole("menu")).toBeNull();

    fireEvent.click(screen.getByLabelText("Export GPX"));
    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByRole("menu")).toBeNull();

    expect(onOriginal).not.toHaveBeenCalled();
    expect(onPrivate).not.toHaveBeenCalled();
  });

  it("does not open while disabled", () => {
    render(
      <ActionMenu ariaLabel="Export GPX" disabled items={[{ label: "x", onSelect: vi.fn() }]}>
        ⬇
      </ActionMenu>,
    );
    fireEvent.click(screen.getByLabelText("Export GPX"));
    expect(screen.queryByRole("menu")).toBeNull();
  });
});
