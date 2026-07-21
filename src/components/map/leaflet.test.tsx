// @vitest-environment happy-dom
import { describe, it, expect, afterEach, vi } from "vitest";
import { render, cleanup } from "@testing-library/react";
import { useEffect } from "react";
import L from "leaflet";
import {
  MapContainer,
  TileLayer,
  Polyline,
  Marker,
  CircleMarker,
  Tooltip,
  Popup,
  PopupAt,
  useMap,
  useMapEvents,
} from "./leaflet";

afterEach(cleanup);

function GrabMap({ onMap }: { onMap: (m: L.Map) => void }) {
  const map = useMap();
  useEffect(() => {
    onMap(map);
  }, [map, onMap]);
  return null;
}

function ZoomSub({ onZoomEnd }: { onZoomEnd: () => void }) {
  useMapEvents({ zoomend: onZoomEnd });
  return null;
}

const icon = L.divIcon({ className: "", iconSize: [10, 10], html: "<b>m</b>" });

describe("map/leaflet wrapper", () => {
  it("creates a map and mounts tile layer, polyline, markers and tooltip", () => {
    const { container } = render(
      <MapContainer center={[55.75, 37.62]} zoom={13} className="h-64">
        <TileLayer url="https://tiles.test/{z}/{x}/{y}.png" attribution="test" />
        <Polyline
          positions={[
            [55.75, 37.62],
            [55.76, 37.63],
          ]}
          color="#d8521d"
          weight={3}
        />
        <Marker position={[55.75, 37.62]} icon={icon} />
        <CircleMarker center={[55.76, 37.63]} radius={8} fillColor="#ef4444">
          <Tooltip permanent direction="top">
            <span>tip-content</span>
          </Tooltip>
        </CircleMarker>
      </MapContainer>,
    );

    expect(container.querySelector(".leaflet-container")).toBeTruthy();
    expect(container.querySelector(".leaflet-tile-pane")).toBeTruthy();
    expect(container.querySelector("path.leaflet-interactive")).toBeTruthy();
    expect(container.querySelector(".leaflet-marker-icon")).toBeTruthy();
    // Permanent tooltip opens on add; its React content arrives via portal.
    expect(container.querySelector(".leaflet-tooltip")?.textContent).toBe("tip-content");
  });

  it("moves a marker on position change without recreating its DOM node", () => {
    const { container, rerender } = render(
      <MapContainer center={[55.75, 37.62]} zoom={13}>
        <Marker position={[55.75, 37.62]} icon={icon} />
      </MapContainer>,
    );
    const el = container.querySelector(".leaflet-marker-icon");
    expect(el).toBeTruthy();

    rerender(
      <MapContainer center={[55.75, 37.62]} zoom={13}>
        <Marker position={[55.9, 37.7]} icon={icon} />
      </MapContainer>,
    );
    // Same node ⇒ setLatLng path, not remove+add (recreation would close
    // any popup the marker had open).
    expect(container.querySelector(".leaflet-marker-icon")).toBe(el);
  });

  it("useMapEvents subscribes to map events and unsubscribes on unmount", () => {
    let map!: L.Map;
    const spy = vi.fn();
    const { rerender } = render(
      <MapContainer center={[55.75, 37.62]} zoom={13}>
        <GrabMap onMap={(m) => (map = m)} />
        <ZoomSub onZoomEnd={spy} />
      </MapContainer>,
    );

    map.fire("zoomend");
    expect(spy).toHaveBeenCalledTimes(1);

    rerender(
      <MapContainer center={[55.75, 37.62]} zoom={13}>
        <GrabMap onMap={(m) => (map = m)} />
      </MapContainer>,
    );
    map.fire("zoomend");
    expect(spy).toHaveBeenCalledTimes(1);
  });

  it("marker eventHandlers receive events with the marker as target", () => {
    let map!: L.Map;
    const clicks: L.LeafletEvent[] = [];
    render(
      <MapContainer center={[55.75, 37.62]} zoom={13}>
        <GrabMap onMap={(m) => (map = m)} />
        <Marker
          position={[55.75, 37.62]}
          icon={icon}
          eventHandlers={{ click: (e) => clicks.push(e) }}
        />
      </MapContainer>,
    );

    map.eachLayer((l) => {
      if (l instanceof L.Marker) l.fire("click");
    });
    expect(clicks).toHaveLength(1);
    expect(clicks[0].target).toBeInstanceOf(L.Marker);
  });

  it("PopupAt opens a popup with React content and reports close", () => {
    let map!: L.Map;
    const onClose = vi.fn();
    const { container, rerender } = render(
      <MapContainer center={[55.75, 37.62]} zoom={13}>
        <GrabMap onMap={(m) => (map = m)} />
        <PopupAt position={[55.75, 37.62]} onClose={onClose}>
          <button>Set as destination point</button>
        </PopupAt>
      </MapContainer>,
    );

    expect(container.querySelector(".leaflet-popup")?.textContent).toContain(
      "Set as destination point",
    );

    map.closePopup();
    expect(onClose).toHaveBeenCalledTimes(1);

    // The owner reacts to onClose by unmounting the popup — no crash, no
    // stray popup left behind.
    rerender(
      <MapContainer center={[55.75, 37.62]} zoom={13}>
        <GrabMap onMap={(m) => (map = m)} />
      </MapContainer>,
    );
    expect(container.querySelector(".leaflet-popup")).toBeFalsy();
  });

  it("opens a bound Popup when the marker is clicked", () => {
    let map!: L.Map;
    const { container } = render(
      <MapContainer center={[55.75, 37.62]} zoom={13}>
        <GrabMap onMap={(m) => (map = m)} />
        <Marker position={[55.75, 37.62]} icon={icon}>
          <Popup className="mp-popup" closeButton={false}>
            <span>member-list</span>
          </Popup>
        </Marker>
      </MapContainer>,
    );
    let marker!: L.Marker;
    map.eachLayer((l) => {
      if (l instanceof L.Marker) marker = l;
    });
    marker.fire("click", { latlng: marker.getLatLng() });
    expect(marker.isPopupOpen()).toBe(true);
    expect(container.querySelector(".leaflet-popup")?.textContent).toContain("member-list");
  });

  it("tears the map down on unmount", () => {
    const { container, unmount } = render(
      <MapContainer center={[55.75, 37.62]} zoom={13}>
        <TileLayer url="https://tiles.test/{z}/{x}/{y}.png" />
      </MapContainer>,
    );
    expect(container.querySelector(".leaflet-container")).toBeTruthy();
    unmount();
    expect(container.querySelector(".leaflet-container")).toBeFalsy();
  });
});