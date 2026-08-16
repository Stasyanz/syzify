import { useState, useEffect, useCallback, useMemo } from "react";
import { useNavigate } from "react-router";
import { useQuery } from "@tanstack/react-query";
import {
  MapContainer,
  TileLayer,
  CircleMarker,
  Marker,
  Popup,
  useMap,
  useMapEvents,
} from "../map/leaflet";
import L from "leaflet";
import { Map, Mountain, Bike, Satellite, Moon, Layers, ChevronRight, Maximize, Minimize } from "lucide-react";
import { api, isTauri } from "../../lib/tauri";
import { useActivityStore } from "../../stores/activityStore";
import { getSportColor } from "../../lib/sportColors";
import { SportGlyph } from "../brand/SportIcon";
import { SPORT_LABELS, type ActivityLocation, type SportType } from "../../lib/types";
import { formatDistance, formatDuration } from "../../lib/format";
import { useUnits } from "../../lib/units";
import {
  clusterLocations,
  clusterSpreadMeters,
  CLUSTER_SPLIT_MAX_ZOOM,
  type Cluster,
} from "./clusterLocations";

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

/** Last view the user left the map in, tagged with the dataset signature it
 * was seen on. Module scope on purpose: the map unmounts when navigating
 * into a workout, and this survives the round trip (but resets with the
 * app, unlike a persisted setting). */
export const lastMapView: {
  current: { center: L.LatLng; zoom: number; sig: string } | null;
} = { current: null };

export function FitBounds({
  positions,
  sig,
}: {
  positions: L.LatLngExpression[];
  sig: string;
}) {
  const map = useMap();
  useEffect(() => {
    // Restore while the dataset is the one the view was left on; a changed
    // sig (filters) refits instead. Signature matching, not a did-run ref:
    // restoring is idempotent, so StrictMode's double-effect can't flip the
    // second run into the fitBounds branch.
    const saved = lastMapView.current;
    if (saved && saved.sig === sig) {
      map.setView(saved.center, saved.zoom, { animate: false });
      return;
    }
    if (positions.length > 0) {
      const bounds = L.latLngBounds(positions);
      map.fitBounds(bounds, { padding: [40, 40] });
    }
  }, [map, positions, sig]);
  return null;
}

/** Leaflet measures its container once — poke it when fullscreen resizes it. */
function InvalidateSize({ isFullscreen }: { isFullscreen: boolean }) {
  const map = useMap();
  useEffect(() => {
    requestAnimationFrame(() => map.invalidateSize());
  }, [map, isFullscreen]);
  return null;
}

/** Report the live zoom level so clustering can re-bucket on every zoom,
 * and record the view for restoring after a detail-page round trip. */
export function ViewTracker({
  onZoom,
  sig,
}: {
  onZoom: (zoom: number) => void;
  sig: string;
}) {
  const map = useMapEvents({
    zoomend: () => onZoom(map.getZoom()),
    // moveend also fires after every zoom, so it alone keeps the record fresh.
    moveend: () => {
      lastMapView.current = { center: map.getCenter(), zoom: map.getZoom(), sig };
      onZoom(map.getZoom());
    },
  });
  useEffect(() => onZoom(map.getZoom()), [map, onZoom]);
  return null;
}

/** Popup row shared by single markers and cluster lists. */
function ActivityPopupRow({
  loc,
  onOpen,
}: {
  loc: ActivityLocation;
  onOpen: (id: string) => void;
}) {
  const color = getSportColor(loc.sport_type);
  const label = SPORT_LABELS[loc.sport_type as SportType] ?? loc.sport_type;
  return (
    <button className="cal-pop-row" onClick={() => onOpen(loc.id)}>
      <span className="cal-pop-ic" style={{ background: color }}>
        <SportGlyph sport={loc.sport_type} size={14} />
      </span>
      <span className="cal-pop-t">{loc.title ?? label}</span>
      <span className="cal-pop-m">
        {loc.distance_m != null
          ? formatDistance(loc.distance_m)
          : loc.duration_s != null
            ? formatDuration(loc.duration_s)
            : ""}
      </span>
      <ChevronRight size={14} className="cal-pop-arrow" />
    </button>
  );
}

/**
 * The count bubble for an aggregated area. Clicking zooms to the members'
 * bounds so the cluster splits; when it can't split (max zoom reached, or
 * every workout starts from the same spot), it opens the member list instead.
 * The popup is only BOUND in the unsplittable state — binding it always and
 * closing on click made the list flash for a moment on every zoom step (the
 * wrapper registers eventHandlers before bindPopup, so Leaflet's popup-open
 * click fires after our close).
 */
function ClusterMarker({
  cluster,
  zoom,
  onOpen,
}: {
  cluster: Cluster<ActivityLocation>;
  zoom: number;
  onOpen: (id: string) => void;
}) {
  const map = useMap();
  const n = cluster.members.length;
  // Bucketed size like markercluster: bigger crowds get a bigger bubble.
  const size = n < 10 ? 34 : n < 100 ? 42 : 50;

  const icon = useMemo(
    () =>
      L.divIcon({
        // `n` is a number — nothing user-controlled reaches this HTML.
        html: `<div class="mcl">${n}</div>`,
        className: "mcl-wrap",
        iconSize: [size, size],
      }),
    [n, size],
  );

  const splittable =
    zoom < CLUSTER_SPLIT_MAX_ZOOM && clusterSpreadMeters(cluster.members) > 50;

  const onClick = () => {
    if (splittable) {
      map.fitBounds(
        L.latLngBounds(cluster.members.map((m) => [m.lat, m.lon])),
        { padding: [60, 60], maxZoom: CLUSTER_SPLIT_MAX_ZOOM },
      );
    }
  };

  // Newest first, like the library list.
  const listed = [...cluster.members].sort((a, b) =>
    b.start_time.localeCompare(a.start_time),
  );

  return (
    <Marker
      position={[cluster.lat, cluster.lon]}
      icon={icon}
      eventHandlers={{ click: onClick }}
    >
      {/* Every member is reachable: an unsplittable cluster (all workouts
          from one spot, max zoom) has ONLY this list — a truncated "+N more"
          would strand the rest. maxHeight makes Leaflet scroll the content
          (it also stops wheel events from zooming the map underneath). */}
      {!splittable && (
        <Popup className="mp-popup" closeButton={false} offset={[0, -4]} maxHeight={280}>
          <div className="mcl-list">
            {listed.map((m) => (
              <ActivityPopupRow key={m.id} loc={m} onOpen={onOpen} />
            ))}
          </div>
        </Popup>
      )}
    </Marker>
  );
}

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

export function ActivitiesMap() {
  useUnits();
  const navigate = useNavigate();
  const filters = useActivityStore((s) => s.filters);
  // Zoom drives the aggregation level (real-estate-map style): far out one
  // bubble sums a whole city, zooming in splits it until singles remain.
  const [zoom, setZoom] = useState(10);
  const [isFullscreen, setIsFullscreen] = useState(false);

  useEffect(() => {
    if (!isFullscreen) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") setIsFullscreen(false);
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [isFullscreen]);

  const { data: locations = [], isLoading } = useQuery({
    queryKey: ["activity-locations", filters],
    queryFn: () => api.getActivityLocations(filters),
  });

  const clusters = useMemo(
    () => clusterLocations(locations, zoom),
    [locations, zoom],
  );

  // Stable identity per dataset: FitBounds effects on this array, and the
  // zoom-state re-renders MUST NOT re-trigger it — a fresh array every render
  // would snap the view back to the all-activities bounds after every zoom
  // (zoomend → setZoom → render → new array → fitBounds).
  const positions: L.LatLngExpression[] = useMemo(
    () => locations.map((l) => [l.lat, l.lon] as [number, number]),
    [locations],
  );

  // Cheap content signature of the dataset (count + endpoints): ties the
  // saved view to the data it was seen on, so a refetch of identical data
  // restores while a filter change refits.
  const viewSig = useMemo(
    () =>
      `${positions.length}:${positions[0] ?? ""}:${positions[positions.length - 1] ?? ""}`,
    [positions],
  );

  const { data: savedLayer } = useQuery({
    queryKey: ["setting", "map_layer"],
    queryFn: () => api.getSetting("map_layer"),
  });

  const [layer, setLayer] = useState<LayerId>("osm");

  useEffect(() => {
    if (savedLayer && MAP_LAYERS.some((l) => l.id === savedLayer)) {
      setLayer(savedLayer as LayerId);
    }
  }, [savedLayer]);

  const handleLayerSelect = useCallback((id: LayerId) => {
    setLayer(id);
    api.setSetting("map_layer", id).catch(() => {});
  }, []);

  const tileUrl = isTauri()
    ? `tile://localhost/${layer}/{z}/{x}/{y}.png`
    : BROWSER_TILE_URLS[layer];

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-full text-faint">
        Loading map...
      </div>
    );
  }

  if (locations.length === 0) {
    return (
      <div className="flex items-center justify-center h-full text-faint">
        No activities with GPS data
      </div>
    );
  }

  return (
    <div className={isFullscreen ? "fixed inset-0 z-50 bg-bg" : "relative h-full"}>
      <MapContainer
        center={positions[0]}
        zoom={10}
        className="h-full w-full z-0"
      >
        <TileLayer
          key={layer}
          attribution={ATTRIBUTIONS[layer]}
          url={tileUrl}
        />
        <ViewTracker onZoom={setZoom} sig={viewSig} />
        {clusters.map((cluster, i) =>
          cluster.members.length === 1 ? (
            (() => {
              const loc = cluster.members[0];
              return (
                <CircleMarker
                  key={loc.id}
                  center={[loc.lat, loc.lon]}
                  radius={7}
                  color="#fff"
                  weight={2}
                  fillColor={getSportColor(loc.sport_type)}
                  fillOpacity={0.9}
                  eventHandlers={{ mouseover: (e) => e.target.openPopup() }}
                >
                  <Popup className="mp-popup" closeButton={false} offset={[0, -2]}>
                    <ActivityPopupRow
                      loc={loc}
                      onOpen={(id) => navigate(`/activity/${id}`)}
                    />
                  </Popup>
                </CircleMarker>
              );
            })()
          ) : (
            <ClusterMarker
              key={`${zoom}-${i}-${cluster.members.length}`}
              cluster={cluster}
              zoom={zoom}
              onOpen={(id) => navigate(`/activity/${id}`)}
            />
          ),
        )}
        <FitBounds positions={positions} sig={viewSig} />
        <InvalidateSize isFullscreen={isFullscreen} />
      </MapContainer>
      <LayerSwitcher layer={layer} onSelect={handleLayerSelect} />
      {/* [data-tip] forces position:relative (unlayered CSS beats the
          `absolute` utility), so the tooltip button needs an absolute
          wrapper — same shape as LayerSwitcher. */}
      <div className="absolute top-2 right-2 z-[1000]">
        <button
          onClick={() => setIsFullscreen((f) => !f)}
          className="bg-card rounded-md shadow-md p-1.5 hover:bg-card-2 border border-border-2 tip-left"
          data-tip={isFullscreen ? "Exit fullscreen" : "Fullscreen map"}
          aria-label={isFullscreen ? "Exit fullscreen" : "Fullscreen map"}
        >
          {isFullscreen ? (
            <Minimize size={18} className="text-muted" />
          ) : (
            <Maximize size={18} className="text-muted" />
          )}
        </button>
      </div>
    </div>
  );
}
