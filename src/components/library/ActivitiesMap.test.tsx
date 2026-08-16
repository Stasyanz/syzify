// @vitest-environment happy-dom
import { StrictMode } from "react";
import { describe, it, expect, afterEach, beforeEach, vi } from "vitest";
import { render, cleanup, fireEvent, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import L from "leaflet";
import type { ActivityLocation } from "../../lib/types";
import { api } from "../../lib/tauri";
import { MapContainer, useMap } from "../map/leaflet";
import { ActivitiesMap, FitBounds, ViewTracker, lastMapView } from "./ActivitiesMap";

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

/** Exposes the Leaflet map instance — the clean-room MapContainer has no ref. */
function MapProbe({ onMap }: { onMap: (m: L.Map) => void }) {
  onMap(useMap());
  return null;
}

function renderView(
  positions: L.LatLngExpression[],
  sig = "sig-a",
  { strict = false } = {},
) {
  let map: L.Map | null = null;
  const tree = (
    <MapContainer center={[55.7, 37.6]} zoom={10}>
      <ViewTracker onZoom={() => {}} sig={sig} />
      <FitBounds positions={positions} sig={sig} />
      <MapProbe onMap={(m) => (map = m)} />
    </MapContainer>
  );
  const utils = render(strict ? <StrictMode>{tree}</StrictMode> : tree);
  return { ...utils, map: () => map! };
}

describe("map view persistence", () => {
  beforeEach(() => {
    lastMapView.current = null;
  });
  afterEach(() => {
    lastMapView.current = null;
  });

  it("records the view when the map moves", () => {
    const { map } = renderView([[55.7, 37.6]]);
    map().setView([48.85, 2.35], 13, { animate: false });

    expect(lastMapView.current?.zoom).toBe(13);
    expect(lastMapView.current?.center.lat).toBeCloseTo(48.85, 2);
    expect(lastMapView.current?.center.lng).toBeCloseTo(2.35, 2);
    expect(lastMapView.current?.sig).toBe("sig-a");
  });

  it("restores the recorded view on remount instead of refitting bounds", () => {
    const first = renderView([[55.7, 37.6]]);
    first.map().setView([48.85, 2.35], 13, { animate: false });
    first.unmount();

    const second = renderView([[55.7, 37.6]]);
    expect(second.map().getZoom()).toBe(13);
    const center = second.map().getCenter();
    expect(center.lat).toBeCloseTo(48.85, 2);
    expect(center.lng).toBeCloseTo(2.35, 2);
  });

  it("restores under StrictMode's double-mounted effects", () => {
    const first = renderView([[55.7, 37.6]]);
    first.map().setView([48.85, 2.35], 13, { animate: false });
    first.unmount();

    // A did-run ref would mark the first StrictMode effect pass as "restored"
    // and send the second pass into fitBounds, snapping the view back.
    const second = renderView([[55.7, 37.6]], "sig-a", { strict: true });
    expect(second.map().getZoom()).toBe(13);
  });

  it("refits instead of restoring when the dataset signature changed", () => {
    const first = renderView([[55.7, 37.6]]);
    first.map().setView([48.85, 2.35], 13, { animate: false });
    first.unmount();

    const second = renderView([[55.7, 37.6]], "sig-b");
    const center = second.map().getCenter();
    expect(center.lat).toBeCloseTo(55.7, 1);
    expect(center.lng).toBeCloseTo(37.6, 1);
  });

  it("fits the activity bounds when nothing was recorded", () => {
    const { map } = renderView([[55.7, 37.6]]);
    expect(map().getCenter().lat).toBeCloseTo(55.7, 1);
    expect(map().getCenter().lng).toBeCloseTo(37.6, 1);
  });
});