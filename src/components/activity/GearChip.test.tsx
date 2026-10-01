// @vitest-environment happy-dom
import { describe, it, expect, afterEach, beforeEach, vi } from "vitest";
import { render, cleanup, screen, waitFor, fireEvent } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { GearItem } from "../../lib/types";

vi.mock("../../lib/tauri", () => ({ api: { listGear: vi.fn() } }));

import { GearChip } from "./GearChip";
import { api } from "../../lib/tauri";

const road: GearItem = {
  id: "g-road",
  kind: "bike",
  name: "Road",
  brand: "Canyon",
  model: "Ultimate",
  purchased_at: null,
  initial_distance_m: 0,
  distance_limit_m: null,
  retired_at: null,
  notes: null,
  created_at: "2026-10-01T10:00:00",
  stats: { activities: 1, distance_m: 0, duration_s: 0, elev_gain_m: 0, last_used: null },
  default_for: [],
};
const shoes: GearItem = { ...road, id: "g-shoes", kind: "shoes", name: "Pegasus", brand: null, model: null };

function renderIt(gearId: string | null, onClick = vi.fn()) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={qc}>
      <GearChip gearId={gearId} onClick={onClick} />
    </QueryClientProvider>,
  );
  return onClick;
}

describe("GearChip", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(api.listGear).mockResolvedValue([road, shoes]);
  });
  afterEach(cleanup);

  it("names the item with its kind's icon, the brand and model on hover, and opens the editor on click", async () => {
    const onClick = renderIt("g-road");
    await waitFor(() => expect(screen.getByTestId("gear-chip")).toBeTruthy());
    const chip = screen.getByTestId("gear-chip");
    expect(chip.textContent).toBe("Road");
    expect(chip.getAttribute("data-tip")).toBe("Road (Canyon Ultimate)");
    expect(chip.getAttribute("aria-label")).toBe("Gear: Road (Canyon Ultimate)");
    expect(chip.querySelector("svg")).toBeTruthy();
    fireEvent.click(chip);
    expect(onClick).toHaveBeenCalledTimes(1);
  });

  it("shows just the name when there is no brand or model", async () => {
    renderIt("g-shoes");
    await waitFor(() => expect(screen.getByTestId("gear-chip").getAttribute("data-tip")).toBe("Pegasus"));
  });

  it("renders nothing, and asks for nothing, without gear", async () => {
    renderIt(null);
    await new Promise((r) => setTimeout(r, 20));
    expect(screen.queryByTestId("gear-chip")).toBeNull();
    expect(api.listGear).not.toHaveBeenCalled();
  });

  it("renders nothing for an item the registry no longer has", async () => {
    renderIt("g-gone");
    await waitFor(() => expect(api.listGear).toHaveBeenCalledTimes(1));
    expect(screen.queryByTestId("gear-chip")).toBeNull();
  });
});
