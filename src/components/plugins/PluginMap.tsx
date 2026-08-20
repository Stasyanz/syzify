import { useEffect } from "react";
import { MapContainer, TileLayer, Polyline, useMap } from "../map/leaflet";
import L from "leaflet";
import { protocolBase } from "../../lib/protocolUrl";

// A plugin-provided polyline overlay. Points are [lat, lon] pairs; the host
// owns the map and tile source, so a plugin only supplies geometry.
function FitBounds({ positions }: { positions: [number, number][] }) {
  const map = useMap();
  useEffect(() => {
    if (positions.length > 0) {
      map.fitBounds(L.latLngBounds(positions), { padding: [20, 20] });
    }
  }, [map, positions]);
  return null;
}

// Cap how much geometry a plugin can push at the map, and drop out-of-range
// coordinates, so a malicious/buggy plugin can't freeze the UI with millions of
// points or invalid lat/lon.
const MAX_POINTS = 10_000;

export function PluginMap({ points }: { points: [number, number][] }) {
  const safe = points
    .filter(
      ([lat, lon]) =>
        Number.isFinite(lat) &&
        Number.isFinite(lon) &&
        lat >= -90 &&
        lat <= 90 &&
        lon >= -180 &&
        lon <= 180
    )
    .slice(0, MAX_POINTS);

  if (safe.length === 0) return null;
  return (
    <div className="h-64 rounded overflow-hidden border border-border">
      <MapContainer
        center={safe[0]}
        zoom={13}
        style={{ height: "100%", width: "100%" }}
        scrollWheelZoom={false}
      >
        <TileLayer url={`${protocolBase("tile")}osm/{z}/{x}/{y}.png`} />
        <Polyline positions={safe} color="#2563eb" weight={4} />
        <FitBounds positions={safe} />
      </MapContainer>
    </div>
  );
}
