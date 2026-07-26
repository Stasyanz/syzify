// @vitest-environment happy-dom
import { describe, it, expect, afterEach } from "vitest";
import { render, cleanup, screen, fireEvent, waitFor } from "@testing-library/react";
import { ConfirmDialogHost } from "./ConfirmDialog";
import { confirmDialog, useConfirmStore } from "../../stores/confirmStore";

afterEach(() => {
  cleanup();
  useConfirmStore.getState().settle(false); // never leak an open dialog
});

describe("ConfirmDialogHost", () => {
  it("resolves true on confirm and false on cancel", async () => {
    render(<ConfirmDialogHost />);

    const p1 = confirmDialog({ title: "Delete photo", message: "Sure?", confirmLabel: "Delete", danger: true });
    const del = await waitFor(() => screen.getByText("Delete"));
    fireEvent.click(del);
    await expect(p1).resolves.toBe(true);

    const p2 = confirmDialog({ title: "Delete photo", message: "Sure?" });
    fireEvent.click(await waitFor(() => screen.getByText("Cancel")));
    await expect(p2).resolves.toBe(false);
    // Dialog is gone after settling.
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });

  it("Escape cancels without reaching page-level handlers", async () => {
    render(<ConfirmDialogHost />);
    let pageSawEscape = false;
    const pageHandler = (e: KeyboardEvent) => {
      if (e.key === "Escape") pageSawEscape = true;
    };
    document.addEventListener("keydown", pageHandler);

    const p = confirmDialog({ title: "T", message: "M" });
    await waitFor(() => screen.getByRole("alertdialog"));
    fireEvent.keyDown(document.body, { key: "Escape" });
    await expect(p).resolves.toBe(false);
    expect(pageSawEscape).toBe(false);

    document.removeEventListener("keydown", pageHandler);
  });

  it("a second request settles the first as cancelled instead of hanging it", async () => {
    render(<ConfirmDialogHost />);
    const first = confirmDialog({ title: "First", message: "M" });
    const second = confirmDialog({ title: "Second", message: "M" });
    await expect(first).resolves.toBe(false);

    fireEvent.click(await waitFor(() => screen.getByText("Confirm")));
    await expect(second).resolves.toBe(true);
  });
});