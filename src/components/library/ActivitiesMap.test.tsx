// @vitest-environment happy-dom
import { describe, it, expect, afterEach, vi } from "vitest";
import { render, cleanup, fireEvent, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ActivityLocation } from "../../lib/types";
import { api } from "../../lib/tauri";
import { ActivitiesMap } from "./ActivitiesMap";

vi.mock("../../lib/tauri", () => ({
  api: {
    getActivityLocations: vi.fn(),
    getSetting: vi.fn().mockResolvedValue(null),
  },
  isTauri: () => false,
}));

afterEach(cleanup);

const loc: ActivityLocation = {
  id: "a-1",
  start_time: "2026-07-01T08:00:00+00:00",
  sport_type: "run",
  title: "Morning run",
  distance_m: 5000,
  duration_s: 1800,
  lat: 55.7,
  lon: 37.6,
};

function renderMap() {
  vi.mocked(api.getActivityLocations).mockResolvedValue([loc]);
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter>
        <ActivitiesMap />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("ActivitiesMap fullscreen", () => {
  it("toggles fullscreen from the corner button and exits on Escape", async () => {
    const { container } = renderMap();
    const button = await waitFor(() => screen.getByLabelText("Fullscreen map"));
    const wrapper = container.firstElementChild!;
    expect(wrapper.className).toContain("relative h-full");

    fireEvent.click(button);
    expect(wrapper.className).toContain("fixed inset-0");
    expect(screen.getByLabelText("Exit fullscreen")).toBeTruthy();

    fireEvent.keyDown(document, { key: "Escape" });
    expect(wrapper.className).toContain("relative h-full");
    expect(screen.getByLabelText("Fullscreen map")).toBeTruthy();
  });
});