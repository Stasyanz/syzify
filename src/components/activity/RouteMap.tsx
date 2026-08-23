import { useState, useEffect, useMemo, useCallback, useRef } from "react";
import { MapContainer, TileLayer, Polyline, Marker, CircleMarker, Tooltip, PopupAt, useMap, useMapEvents } from "../map/leaflet";
import L from "leaflet";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { Map, Mountain, Bike, Satellite, Moon, Layers, Maximize, Minimize, MapPin } from "lucide-react";
import type { TrackPointColumns } from "../../lib/types";
import { useActivityStore } from "../../stores/activityStore";
import { formatDistance, formatElevation, formatPaceOrSpeed, formatHR } from "../../lib/format";
import { useUnits } from "../../lib/units";
import { api, isTauri } from "../../lib/tauri";
import { protocolBase } from "../../lib/protocolUrl";

interface Props {
  trackpoints: TrackPointColumns;
  sport: string;
  activityId: string;
}

// No Leaflet default markers anywhere (every marker is a CSS divIcon), so
// there is no unpkg.com icon fixup here — it was an undisclosed network
// fetch, and dropping it lets the CSP forbid https: images entirely.

const startIcon = L.divIcon({
  className: "",
  iconSize: [18, 18],
  iconAnchor: [9, 9],
  html: '<svg width="18" height="18" viewBox="0 0 18 18"><circle cx="9" cy="9" r="8" fill="#22c55e" stroke="#fff" stroke-width="2"/></svg>',
});

const finishIcon = L.divIcon({
  className: "",
  iconSize: [18, 18],
  iconAnchor: [9, 9],
  html: `<svg width="18" height="18" viewBox="0 0 18 18">
    <defs>
      <clipPath id="fin-clip"><circle cx="9" cy="9" r="7"/></clipPath>
    </defs>
    <circle cx="9" cy="9" r="8" fill="#ef4444" stroke="#fff" stroke-width="2"/>
    <g clip-path="url(#fin-clip)" opacity="0.4">
      <rect x="2" y="2" width="3.5" height="3.5" fill="#fff"/>
      <rect x="9" y="2" width="3.5" height="3.5" fill="#fff"/>
      <rect x="5.5" y="5.5" width="3.5" height="3.5" fill="#fff"/>
      <rect x="12.5" y="5.5" width="3.5" height="3.5" fill="#fff"/>
      <rect x="2" y="9" width="3.5" height="3.5" fill="#fff"/>
      <rect x="9" y="9" width="3.5" height="3.5" fill="#fff"/>
      <rect x="5.5" y="12.5" width="3.5" height="3.5" fill="#fff"/>
      <rect x="12.5" y="12.5" width="3.5" height="3.5" fill="#fff"/>
    </g>
  </svg>`,
});

function FitBounds({ positions }: { positions: L.LatLngExpression[] }) {
  const map = useMap();
  useEffect(() => {
    if (positions.length > 0) {
      const bounds = L.latLngBounds(positions);
      map.fitBounds(bounds, { padding: [30, 30] });
    }
  }, [map, positions]);
  return null;
}

/** The hover tooltip's metric lines for one trackpoint — extracted pure for
 * tests. Sport-aware: pace for foot sports, /100 pace for swims, speed
 * otherwise; near-stops read as "--" pace (formatPace guards <= 0). */
export function hoverLines(tp: TrackPointColumns, i: number, sport: string): string[] {
  const dist = tp.distance_m[i];
  const ele = tp.altitude_m[i];
  const speed = tp.speed_mps[i];
  const hr = tp.hr[i];

  const lines: string[] = [];
  if (dist != null) lines.push(formatDistance(dist));
  if (ele != null) lines.push(formatElevation(ele));
  if (speed != null) lines.push(formatPaceOrSpeed(sport, speed));
  if (hr != null) lines.push(formatHR(hr));
  return lines;
}

function HoverMarker({ trackpoints, sport }: { trackpoints: TrackPointColumns; sport: string }) {
  const hoveredIndex = useActivityStore((s) => s.hoveredPointIndex);

  if (hoveredIndex == null) return null;
  const lat = trackpoints.lat[hoveredIndex];
  const lon = trackpoints.lon[hoveredIndex];
  if (lat == null || lon == null) return null;

  const lines = hoverLines(trackpoints, hoveredIndex, sport);

  return (
    <CircleMarker
      center={[lat, lon]}
      radius={8}
      color="#ffffff"
      weight={2}
      fillColor="#ef4444"
      fillOpacity={1}
    >
      {lines.length > 0 && (
        <Tooltip permanent direction="top" offset={[0, -10]}>
          <span style={{ fontSize: "11px", lineHeight: "1.4" }}>
            {lines.join(" · ")}
          </span>
        </Tooltip>
      )}
    </CircleMarker>
  );
}

/** Valid GPS positions of the trackpoint range [a, b] (inclusive, clamped) —
 * the piece of route a chart drag-selection maps to. Exported pure for
 * tests. Fewer than 2 usable points can't form a segment → null. */
export function segmentPositions(
  trackpoints: TrackPointColumns,
  range: [number, number],
): L.LatLngExpression[] | null {
  const from = Math.max(0, Math.min(range[0], range[1]));
  const to = Math.min(trackpoints.lat.length - 1, Math.max(range[0], range[1]));
  const pts: L.LatLngExpression[] = [];
  for (let i = from; i <= to; i++) {
    const lat = trackpoints.lat[i];
    const lon = trackpoints.lon[i];
    if (lat != null && lon != null) pts.push([lat, lon]);
  }
  return pts.length >= 2 ? pts : null;
}

/** The elevation chart's drag-selected span, drawn over the base route as
 * a slightly thicker line with a white casing — the outline separates the
 * highlight from the 3px base route on any tile style. Order matters: the
 * white line mounts first, so the colored core renders on top of it. */
function SelectedSegment({ trackpoints }: { trackpoints: TrackPointColumns }) {
  const range = useActivityStore((s) => s.selectedRange);
  const segment = useMemo(
    () => (range ? segmentPositions(trackpoints, range) : null),
    [range, trackpoints],
  );
  if (!segment) return null;
  return (
    <>
      <Polyline positions={segment} color="#ffffff" weight={7} />
      <Polyline positions={segment} color="#d8521d" weight={4} />
    </>
  );
}

/** Index of the route point nearest to `latlng`, or -1 when the closest one
 * is farther than `maxDistanceM` (clicks off the route shouldn't snap).
 * Exported pure for tests. */
export function findNearestPointIndex(
  trackpoints: TrackPointColumns,
  latlng: L.LatLng,
  maxDistanceM = 50,
): number {
  let minDist = Infinity;
  let minIdx = -1;
  for (let i = 0; i < trackpoints.lat.length; i++) {
    const lat = trackpoints.lat[i];
    const lon = trackpoints.lon[i];
    if (lat == null || lon == null) continue;
    const d = latlng.distanceTo(L.latLng(lat, lon));
    if (d < minDist) {
      minDist = d;
      minIdx = i;
    }
  }
  return minDist < maxDistanceM ? minIdx : -1;
}

function MapClickHandler({
  trackpoints,
  onRouteContextMenu,
}: {
  trackpoints: TrackPointColumns;
  onRouteContextMenu: (point: [number, number] | null) => void;
}) {
  const setHoveredPointIndex = useActivityStore((s) => s.setHoveredPointIndex);

  useMapEvents({
    click(e) {
      const idx = findNearestPointIndex(trackpoints, e.latlng);
      setHoveredPointIndex(idx >= 0 ? idx : null);
    },
    mousemove(e) {
      const idx = findNearestPointIndex(trackpoints, e.latlng);
      setHoveredPointIndex(idx >= 0 ? idx : null);
    },
    contextmenu(e) {
      const idx = findNearestPointIndex(trackpoints, e.latlng);
      const lat = idx >= 0 ? trackpoints.lat[idx] : null;
      const lon = idx >= 0 ? trackpoints.lon[idx] : null;
      onRouteContextMenu(lat != null && lon != null ? [lat, lon] : null);
    },
  });

  return null;
}

function InvalidateSize({ isFullscreen, height }: { isFullscreen: boolean; height: number }) {
  const map = useMap();
  useEffect(() => {
    requestAnimationFrame(() => map.invalidateSize());
  }, [map, isFullscreen, height]);
  return null;
}

// The map can be stretched vertically by its bottom-right grip: from the
// default 320px (the old fixed h-80) up to +50%.
export const MAP_DEFAULT_HEIGHT_PX = 320;
export const MAP_MAX_HEIGHT_PX = MAP_DEFAULT_HEIGHT_PX * 1.5;

/** Clamp a dragged/persisted map height into [default, default × 1.5].
 * Exported pure for tests. */
export function clampMapHeight(px: number): number {
  if (!Number.isFinite(px)) return MAP_DEFAULT_HEIGHT_PX;
  return Math.min(MAP_MAX_HEIGHT_PX, Math.max(MAP_DEFAULT_HEIGHT_PX, Math.round(px)));
}

const MAP_LAYERS = [
  { id: "osm", name: "Standard", icon: Map },
  { id: "topo", name: "Topo", icon: Mountain },
  { id: "cycling", name: "Cycling", icon: Bike },
  { id: "satellite", name: "Satellite", icon: Satellite },
  { id: "dark", name: "Dark", icon: Moon },
] as const;

type LayerId = (typeof MAP_LAYERS)[number]["id"];

const BROWSER_TILE_URLS: Record<LayerId, string> = {
  osm: "https://tile.openstreetmap.org/{z}/{x}/{y}.png",
  topo: "https://tile.opentopomap.org/{z}/{x}/{y}.png",
  cycling: "https://a.tile-cyclosm.openstreetmap.fr/cyclosm/{z}/{x}/{y}.png",
  satellite: "https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{z}/{y}/{x}",
  dark: "https://basemaps.cartocdn.com/dark_all/{z}/{x}/{y}@2x.png",
};

const ATTRIBUTIONS: Record<LayerId, string> = {
  osm: '&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a>',
  topo: '&copy; <a href="https://opentopomap.org">OpenTopoMap</a>',
  cycling: '&copy; <a href="https://www.cyclosm.org">CyclOSM</a>',
  satellite: '&copy; <a href="https://www.esri.com">Esri</a>',
  dark: '&copy; <a href="https://carto.com">CartoDB</a>',
};

function LayerSwitcher({ layer, onSelect }: { layer: LayerId; onSelect: (id: LayerId) => void }) {
  const [open, setOpen] = useState(false);

  return (
    <div
      className="absolute top-2 right-12 z-[1000]"
      onMouseEnter={() => setOpen(true)}
      onMouseLeave={() => setOpen(false)}
    >
      <button
        onClick={() => setOpen((o) => !o)}
        className="bg-card rounded-md shadow-md p-1.5 hover:bg-card-2 border border-border-2"
        title="Change map layer"
      >
        <Layers size={18} className="text-muted" />
      </button>
      {open && (
        <div className="absolute top-0 right-0 bg-card rounded-md shadow-lg border border-border py-1 min-w-[140px]">
          {MAP_LAYERS.map((l) => {
            const Icon = l.icon;
            const selected = l.id === layer;
            return (
              <button
                key={l.id}
                onClick={() => {
                  onSelect(l.id);
                  setOpen(false);
                }}
                className={`flex items-center gap-2 w-full px-3 py-1.5 text-left text-sm hover:bg-card-2 ${
                  selected ? "font-medium text-accent-2 bg-accent-soft" : "text-ink"
                }`}
              >
                <Icon size={14} />
                {l.name}
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}

/** Collect all valid GPS positions into a single continuous polyline */
function useRoutePositions(trackpoints: TrackPointColumns) {
  return useMemo(() => {
    const positions: L.LatLngExpression[] = [];

    for (let i = 0; i < trackpoints.lat.length; i++) {
      const lat = trackpoints.lat[i];
      const lon = trackpoints.lon[i];
      if (lat != null && lon != null) {
        positions.push([lat, lon]);
      }
    }

    return positions;
  }, [trackpoints]);
}

export function RouteMap({ trackpoints, sport, activityId }: Props) {
  useUnits();
  const positions = useRoutePositions(trackpoints);

  if (positions.length === 0) {
    // No GPS track — don't reserve a full map canvas; a slim notice is enough.
    return (
      <div className="bg-card-2 rounded-card flex items-center justify-center h-16 text-sm text-faint">
        Indoor activity — no route data
      </div>
    );
  }

  const { data: savedLayer } = useQuery({
    queryKey: ["setting", "map_layer"],
    queryFn: () => api.getSetting("map_layer"),
  });
  const { data: savedHeight } = useQuery({
    queryKey: ["setting", "map_height"],
    queryFn: () => api.getSetting("map_height"),
  });

  const [isFullscreen, setIsFullscreen] = useState(false);
  const [layer, setLayer] = useState<LayerId>("osm");
  const [height, setHeight] = useState(MAP_DEFAULT_HEIGHT_PX);
  // Right-click on the route: the snapped track point the menu was opened on.
  const [menuPoint, setMenuPoint] = useState<[number, number] | null>(null);

  const queryClient = useQueryClient();
  const setDestination = useMutation({
    mutationFn: ([lat, lon]: [number, number]) =>
      api.setActivityLocationPoint(activityId, lat, lon),
    onSuccess: () => {
      setMenuPoint(null);
      queryClient.invalidateQueries({ queryKey: ["activity", activityId] });
      queryClient.invalidateQueries({ queryKey: ["activities"] });
      queryClient.invalidateQueries({ queryKey: ["activity-locations"] });
    },
  });

  useEffect(() => {
    if (savedLayer && MAP_LAYERS.some((l) => l.id === savedLayer)) {
      setLayer(savedLayer as LayerId);
    }
  }, [savedLayer]);

  useEffect(() => {
    if (savedHeight != null) setHeight(clampMapHeight(Number(savedHeight)));
  }, [savedHeight]);

  // Drag state for the bottom-right resize grip; pointer capture keeps the
  // drag alive even when the cursor leaves the 20×20 handle.
  const dragRef = useRef<{ startY: number; startHeight: number } | null>(null);

  const handleResizeStart = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    e.preventDefault();
    e.currentTarget.setPointerCapture(e.pointerId);
    dragRef.current = { startY: e.clientY, startHeight: height };
  }, [height]);

  const handleResizeMove = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current;
    if (!drag) return;
    setHeight(clampMapHeight(drag.startHeight + (e.clientY - drag.startY)));
  }, []);

  const handleResizeEnd = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    if (!dragRef.current) return;
    dragRef.current = null;
    e.currentTarget.releasePointerCapture(e.pointerId);
    setHeight((h) => {
      api.setSetting("map_height", String(h)).catch(() => {});
      return h;
    });
  }, []);

  useEffect(() => {
    if (!isFullscreen) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") setIsFullscreen(false);
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [isFullscreen]);

  const handleLayerSelect = useCallback((id: LayerId) => {
    setLayer(id);
    api.setSetting("map_layer", id).catch(() => {});
  }, []);

  const tileUrl = isTauri()
    ? `${protocolBase("tile")}${layer}/{z}/{x}/{y}.png`
    : BROWSER_TILE_URLS[layer];

  const start = positions[0];
  const end = positions[positions.length - 1];

  return (
    <div
      className={isFullscreen ? "fixed inset-0 z-50 bg-bg" : "relative"}
      style={isFullscreen ? undefined : { height }}
    >
      <MapContainer
        center={start}
        zoom={13}
        className="h-full w-full rounded-lg z-0"
      >
        <TileLayer
          key={layer}
          attribution={ATTRIBUTIONS[layer]}
          url={tileUrl}
        />
        <Polyline positions={positions} color="#d8521d" weight={3} />
        <SelectedSegment trackpoints={trackpoints} />
        <Marker position={start} icon={startIcon} />
        <Marker position={end} icon={finishIcon} />
        <HoverMarker trackpoints={trackpoints} sport={sport} />
        <MapClickHandler trackpoints={trackpoints} onRouteContextMenu={setMenuPoint} />
        {menuPoint && (
          <PopupAt
            position={menuPoint}
            className="mp-popup"
            offset={[0, -6]}
            onClose={() => setMenuPoint(null)}
          >
            <div className="map-menu">
              <button
                type="button"
                className="cal-pop-row disabled:opacity-60"
                disabled={setDestination.isPending}
                onClick={() => setDestination.mutate(menuPoint)}
              >
                <MapPin size={15} className="text-muted flex-shrink-0" />
                <span className="cal-pop-t">
                  {setDestination.isPending ? "Saving…" : "Set as destination point"}
                </span>
              </button>
              {setDestination.isError && (
                <p className="px-2 pt-1 text-xs text-red-500 max-w-56">
                  {String(setDestination.error)}
                </p>
              )}
            </div>
          </PopupAt>
        )}
        <FitBounds positions={positions} />
        <InvalidateSize isFullscreen={isFullscreen} height={height} />
      </MapContainer>
      <LayerSwitcher layer={layer} onSelect={handleLayerSelect} />
      <button
        onClick={() => setIsFullscreen((f) => !f)}
        className="absolute top-2 right-2 z-[1000] bg-card rounded-md shadow-md p-1.5 hover:bg-card-2 border border-border-2"
        title={isFullscreen ? "Exit fullscreen" : "Fullscreen map"}
      >
        {isFullscreen ? (
          <Minimize size={18} className="text-muted" />
        ) : (
          <Maximize size={18} className="text-muted" />
        )}
      </button>
      {!isFullscreen && (
        // No data-tip here: its `position: relative` rule (App.css) beats the
        // layered `absolute` utility, and a bottom-edge tooltip would dangle
        // outside the map anyway — the ns-resize cursor is the affordance.
        <div
          className="absolute bottom-0 right-0 z-[1000] h-5 w-5 cursor-ns-resize touch-none text-muted hover:text-ink"
          onPointerDown={handleResizeStart}
          onPointerMove={handleResizeMove}
          onPointerUp={handleResizeEnd}
          onPointerCancel={handleResizeEnd}
        >
          <svg
            width="10"
            height="10"
            viewBox="0 0 10 10"
            className="pointer-events-none absolute bottom-1 right-1"
          >
            <path
              d="M9 1L1 9M9 5L5 9"
              stroke="currentColor"
              strokeWidth="1.5"
              strokeLinecap="round"
            />
          </svg>
        </div>
      )}
    </div>
  );
}
