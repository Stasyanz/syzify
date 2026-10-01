// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

vi.mock("../../lib/tauri", () => ({ api: { getActivityYearRange: vi.fn() } }));

import { DateField } from "./DateField";
import { api } from "../../lib/tauri";

function renderIt(props: Partial<Parameters<typeof DateField>[0]> = {}) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const onChange = vi.fn();
  render(
    <QueryClientProvider client={qc}>
      <DateField label="Purchased" value={undefined} onChange={onChange} {...props} />
    </QueryClientProvider>,
  );
  return onChange;
}

describe("DateField", () => {
  beforeEach(() => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    vi.setSystemTime(new Date(2026, 9, 1, 12, 0, 0));
    vi.mocked(api.getActivityYearRange).mockResolvedValue([2024, 2026]);
  });
  afterEach(() => {
    cleanup();
    vi.useRealTimers();
  });

  it("opens downward on request, shows its own placeholder and takes the year span it is given", () => {
    const onChange = renderIt({ drop: "down", yearSpan: [2001, 2026], placeholder: "Not set" });
    const field = screen.getByRole("button", { name: /Purchased/ });
    expect(field.textContent).toContain("Not set");
    fireEvent.click(field);
    expect(field.closest(".dp")!.className).toContain("dp-down");
    // Its own years, not the vault's: no request for the activity range.
    expect(api.getActivityYearRange).not.toHaveBeenCalled();
    fireEvent.click(screen.getByLabelText("Year"));
    const years = screen.getAllByRole("option").map((o) => o.textContent);
    expect(years[0]).toBe("2026");
    expect(years[years.length - 1]).toBe("2001");
    expect(years).toHaveLength(26);
    fireEvent.click(screen.getByRole("option", { name: "2026" }));
    fireEvent.click(screen.getByRole("button", { name: "15" }));
    expect(onChange).toHaveBeenCalledWith("2026-10-15");
  });

  it("opens upward by default, says Any, and asks the vault for its years", () => {
    renderIt();
    const field = screen.getByRole("button", { name: /Purchased/ });
    expect(field.textContent).toContain("Any");
    fireEvent.click(field);
    expect(field.closest(".dp")!.className).not.toContain("dp-down");
    expect(api.getActivityYearRange).toHaveBeenCalledTimes(1);
  });

  it("disables the days past max", () => {
    renderIt({ max: "2026-10-01" });
    fireEvent.click(screen.getByRole("button", { name: /Purchased/ }));
    expect((screen.getByRole("button", { name: "1" }) as HTMLButtonElement).disabled).toBe(false);
    expect((screen.getByRole("button", { name: "2" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("closes on a click outside or Escape, but not on a click in the portalled year menu", () => {
    renderIt({ yearSpan: [2024, 2026] });
    const open = () => fireEvent.click(screen.getByRole("button", { name: /Purchased/ }));
    open();
    expect(screen.getByLabelText("Month")).toBeTruthy();
    fireEvent.mouseDown(document.body);
    expect(screen.queryByLabelText("Month")).toBeNull();

    open();
    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByLabelText("Month")).toBeNull();

    open();
    fireEvent.click(screen.getByLabelText("Year"));
    // The year list lives in a portal outside the field: a press on it
    // must not count as "outside".
    fireEvent.mouseDown(screen.getByRole("option", { name: "2024" }));
    expect(screen.getByLabelText("Month")).toBeTruthy();
    // A press elsewhere inside the field keeps it open too.
    fireEvent.mouseDown(screen.getByLabelText("Next month"));
    expect(screen.getByLabelText("Month")).toBeTruthy();
  });

  it("pages across the year boundary both ways and jumps by the month menu", () => {
    renderIt({ value: "2026-01-10" });
    const field = screen.getByRole("button", { name: /Purchased/ });
    expect(field.textContent).toContain("10 Jan 2026");
    fireEvent.click(field);
    const shown = () => `${screen.getByLabelText("Month").textContent} ${screen.getByLabelText("Year").textContent}`;
    expect(shown()).toBe("January 2026");
    fireEvent.click(screen.getByLabelText("Previous month"));
    expect(shown()).toBe("December 2025");
    fireEvent.click(screen.getByLabelText("Next month"));
    expect(shown()).toBe("January 2026");
    fireEvent.click(screen.getByLabelText("Month"));
    fireEvent.click(screen.getByRole("option", { name: "December" }));
    expect(shown()).toBe("December 2026");
    fireEvent.click(screen.getByLabelText("Next month"));
    expect(shown()).toBe("January 2027");
  });

  it("clears a value, and Clear is inert without one", () => {
    const onChange = renderIt({ value: "2026-10-01" });
    fireEvent.click(screen.getByRole("button", { name: /Purchased/ }));
    const clear = screen.getByRole("button", { name: "Clear" }) as HTMLButtonElement;
    expect(clear.disabled).toBe(false);
    fireEvent.click(clear);
    expect(onChange).toHaveBeenCalledWith(undefined);
    expect(screen.queryByLabelText("Month")).toBeNull();
    cleanup();
    renderIt();
    fireEvent.click(screen.getByRole("button", { name: /Purchased/ }));
    expect((screen.getByRole("button", { name: "Clear" }) as HTMLButtonElement).disabled).toBe(true);
  });
});
