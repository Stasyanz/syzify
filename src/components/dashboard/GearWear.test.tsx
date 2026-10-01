// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, Route, Routes, useLocation } from "react-router";
import type { GearItem } from "../../lib/types";

vi.mock("../../lib/tauri", () => ({ api: { listGear: vi.fn() } }));

import { GearWear } from "./GearWear";
import { api } from "../../lib/tauri";

const shoes = (id: string, name: string, km: number, limitKm: number | null, retired = false): GearItem => ({
  id,
  kind: "shoes",
  name,
  brand: null,
  model: null,
  purchased_at: null,
  initial_distance_m: 0,
  distance_limit_m: limitKm == null ? null : limitKm * 1000,
  retired_at: retired ? "2026-01-01T00:00:00" : null,
  notes: null,
  created_at: "2026-01-01T00:00:00",
  stats: { activities: 3, distance_m: km * 1000, duration_s: 0, elev_gain_m: 0, last_used: null },
  default_for: [],
  rules: [],
});

function Where() {
  const loc = useLocation();
  return <div data-testid="where">{loc.pathname + loc.search}</div>;
}

function renderIt() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter initialEntries={["/"]}>
        <Routes>
          <Route path="/" element={<GearWear />} />
          <Route path="/settings" element={<Where />} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("GearWear", () => {
  beforeEach(() => vi.clearAllMocks());
  afterEach(cleanup);

  it("shows nothing while no item in use is wearing out", async () => {
    vi.mocked(api.listGear).mockResolvedValue([shoes("a", "Fresh", 100, 800), shoes("b", "No limit", 5000, null), shoes("c", "Old", 900, 800, true)]);
    renderIt();
    await waitFor(() => expect(api.listGear).toHaveBeenCalledTimes(1));
    expect(screen.queryByTestId("gear-wear")).toBeNull();
  });

  it("lists the worn items most worn first, with their numbers and state, and a row opens the Garage", async () => {
    vi.mocked(api.listGear).mockResolvedValue([shoes("a", "Pegasus", 700, 800), shoes("b", "Vomero", 900, 800)]);
    renderIt();
    await waitFor(() => expect(screen.getByTestId("gear-wear")).toBeTruthy());
    const rows = screen.getAllByRole("button");
    expect(rows.map((r) => r.getAttribute("aria-label"))).toEqual([
      "Vomero: 900.00 km of 800.00 km · past the limit. Open the Garage",
      "Pegasus: 700.00 km of 800.00 km · 87 %. Open the Garage",
    ]);
    expect(rows[0].textContent).toContain("Worn out");
    expect(rows[1].textContent).toContain("Wearing out");
    const fills = screen.getAllByTestId("wear-fill").map((f) => f.style.background);
    expect(fills).toEqual(["var(--danger)", "var(--warn)"]);
    fireEvent.click(rows[1]);
    await waitFor(() => expect(screen.getByTestId("where").textContent).toBe("/settings?tab=garage"));
  });

  it("opens the Garage from the keyboard too — Enter or Space, nothing else", async () => {
    vi.mocked(api.listGear).mockResolvedValue([shoes("b", "Vomero", 900, 800)]);
    renderIt();
    await waitFor(() => expect(screen.getByTestId("gear-wear")).toBeTruthy());
    fireEvent.keyDown(screen.getByRole("button"), { key: "a" });
    await new Promise((r) => setTimeout(r, 20));
    expect(screen.queryByTestId("where")).toBeNull();
    // Space must not scroll the page on its way: the event is consumed.
    expect(fireEvent.keyDown(screen.getByRole("button"), { key: " " })).toBe(false);
    await waitFor(() => expect(screen.getByTestId("where").textContent).toBe("/settings?tab=garage"));
    cleanup();
    renderIt();
    await waitFor(() => expect(screen.getByTestId("gear-wear")).toBeTruthy());
    fireEvent.keyDown(screen.getByRole("button"), { key: "Enter" });
    await waitFor(() => expect(screen.getByTestId("where").textContent).toBe("/settings?tab=garage"));
  });

  it("shows nothing when the registry cannot be read", async () => {
    vi.mocked(api.listGear).mockRejectedValue(new Error("database is locked"));
    renderIt();
    await waitFor(() => expect(api.listGear).toHaveBeenCalledTimes(1));
    await new Promise((r) => setTimeout(r, 20));
    expect(screen.queryByTestId("gear-wear")).toBeNull();
  });
});
