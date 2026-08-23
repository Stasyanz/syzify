import {
  createContext,
  useContext,
  useEffect,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";
import L from "leaflet";
import "leaflet/dist/leaflet.css";

// Clean-room replacement for the subset of react-leaflet this app used.
// react-leaflet@3+ is licensed under Hippocratic-2.1, which is incompatible
// with the AGPL — so the declarative layer over Leaflet lives here, written
// against the Leaflet documentation only, without consulting react-leaflet's
// sources.

const MapCtx = createContext<L.Map | null>(null);
const LayerCtx = createContext<L.Layer | null>(null);

export function useMap(): L.Map {
  const map = useContext(MapCtx);
  if (!map) throw new Error("useMap must be used inside <MapContainer>");
  return map;
}

function useLayer(): L.Layer {
  const layer = useContext(LayerCtx);
  if (!layer) throw new Error("Popup/Tooltip must be a child of a Marker/CircleMarker");
  return layer;
}

/** Subscribes to an Evented target; handlers are read through a ref so that
 * their re-creation on every render does not churn the subscription. */
function useEvented(target: L.Evented | null, handlers?: L.LeafletEventHandlerFnMap) {
  const handlersRef = useRef(handlers);
  handlersRef.current = handlers;
  const keys = handlers ? Object.keys(handlers).sort().join(" ") : "";

  useEffect(() => {
    if (!target || !keys) return;
    const bound: Record<string, L.LeafletEventHandlerFn> = {};
    for (const key of keys.split(" ")) {
      bound[key] = (e) =>
        (handlersRef.current as Record<string, L.LeafletEventHandlerFn> | undefined)?.[key]?.(e);
    }
    target.on(bound);
    return () => {
      target.off(bound);
    };
  }, [target, keys]);
}

export function useMapEvents(handlers: L.LeafletEventHandlerFnMap): L.Map {
  const map = useMap();
  useEvented(map, handlers);
  return map;
}

interface MapContainerProps {
  center: L.LatLngExpression;
  zoom: number;
  className?: string;
  style?: CSSProperties;
  scrollWheelZoom?: boolean;
  children?: ReactNode;
}

export function MapContainer({
  center,
  zoom,
  className,
  style,
  scrollWheelZoom = true,
  children,
}: MapContainerProps) {
  const divRef = useRef<HTMLDivElement>(null);
  const [map, setMap] = useState<L.Map | null>(null);
  // center/zoom are only the map's initial state; from then on the user and
  // fitBounds drive it, so prop changes must not touch the instance.
  const initial = useRef({ center, zoom, scrollWheelZoom });

  useEffect(() => {
    const el = divRef.current;
    if (!el) return;
    const opts = initial.current;
    const m = L.map(el, { scrollWheelZoom: opts.scrollWheelZoom }).setView(opts.center, opts.zoom);
    setMap(m);
    return () => {
      setMap(null);
      m.remove();
    };
  }, []);

  return (
    <div ref={divRef} className={className} style={style}>
      {map && <MapCtx.Provider value={map}>{children}</MapCtx.Provider>}
    </div>
  );
}

export function TileLayer({ url, attribution }: { url: string; attribution?: string }) {
  const map = useMap();
  const layerRef = useRef<L.TileLayer | null>(null);

  useEffect(() => {
    const layer = L.tileLayer("", attribution ? { attribution } : undefined).addTo(map);
    layerRef.current = layer;
    return () => {
      layerRef.current = null;
      layer.remove();
    };
    // attribution only changes together with a layer switch, and both call
    // sites remount TileLayer via key when that happens.
  }, [map]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    layerRef.current?.setUrl(url);
  }, [url]);

  return null;
}

export function Polyline({
  positions,
  color,
  weight,
  opacity,
  className,
}: {
  positions: L.LatLngExpression[];
  color?: string;
  weight?: number;
  opacity?: number;
  /** CSS class on the SVG path (creation-time only — Leaflet can't restyle
   * it later, so pass a constant). */
  className?: string;
}) {
  const map = useMap();
  const lineRef = useRef<L.Polyline | null>(null);

  useEffect(() => {
    const line = L.polyline([], className ? { className } : {}).addTo(map);
    lineRef.current = line;
    return () => {
      lineRef.current = null;
      line.remove();
    };
    // className is creation-time only by contract — not a dep.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [map]);

  useEffect(() => {
    lineRef.current?.setLatLngs(positions);
  }, [positions]);

  useEffect(() => {
    lineRef.current?.setStyle({ color, weight, opacity });
  }, [color, weight, opacity]);

  return null;
}

interface MarkerProps {
  position: L.LatLngExpression;
  icon?: L.Icon | L.DivIcon;
  eventHandlers?: L.LeafletEventHandlerFnMap;
  children?: ReactNode;
}

export function Marker({ position, icon, eventHandlers, children }: MarkerProps) {
  const map = useMap();
  const [marker, setMarker] = useState<L.Marker | null>(null);
  const initial = useRef({ position, icon });

  useEffect(() => {
    const opts = initial.current;
    const m = L.marker(opts.position, opts.icon ? { icon: opts.icon } : undefined).addTo(map);
    setMarker(m);
    return () => {
      setMarker(null);
      m.remove();
    };
  }, [map]);

  // Position is compared component-wise: a [lat, lon] array is a fresh
  // reference every render, and depending on the reference would recreate
  // the marker.
  const ll = L.latLng(position);
  useEffect(() => {
    marker?.setLatLng([ll.lat, ll.lng]);
  }, [marker, ll.lat, ll.lng]);

  useEffect(() => {
    if (marker && icon) marker.setIcon(icon);
  }, [marker, icon]);

  useEvented(marker, eventHandlers);

  if (!marker || !children) return null;
  return <LayerCtx.Provider value={marker}>{children}</LayerCtx.Provider>;
}

interface CircleMarkerProps {
  center: L.LatLngExpression;
  radius?: number;
  color?: string;
  weight?: number;
  fillColor?: string;
  fillOpacity?: number;
  eventHandlers?: L.LeafletEventHandlerFnMap;
  children?: ReactNode;
}

export function CircleMarker({
  center,
  radius = 10,
  color,
  weight,
  fillColor,
  fillOpacity,
  eventHandlers,
  children,
}: CircleMarkerProps) {
  const map = useMap();
  const [marker, setMarker] = useState<L.CircleMarker | null>(null);
  const initial = useRef({ center, radius, color, weight, fillColor, fillOpacity });

  useEffect(() => {
    const { center, ...opts } = initial.current;
    const m = L.circleMarker(center, opts).addTo(map);
    setMarker(m);
    return () => {
      setMarker(null);
      m.remove();
    };
  }, [map]);

  const ll = L.latLng(center);
  useEffect(() => {
    marker?.setLatLng([ll.lat, ll.lng]);
  }, [marker, ll.lat, ll.lng]);

  useEffect(() => {
    if (!marker) return;
    marker.setRadius(radius);
    marker.setStyle({ color, weight, fillColor, fillOpacity });
  }, [marker, radius, color, weight, fillColor, fillOpacity]);

  useEvented(marker, eventHandlers);

  if (!marker || !children) return null;
  return <LayerCtx.Provider value={marker}>{children}</LayerCtx.Provider>;
}

interface PopupProps {
  children?: ReactNode;
  className?: string;
  closeButton?: boolean;
  offset?: [number, number];
  maxHeight?: number;
}

/** Binds a popup to the parent layer; the React content moves into its
 * container through a portal and stays in the tree (Router/Query contexts
 * keep working). */
export function Popup({ children, className, closeButton = true, offset, maxHeight }: PopupProps) {
  const layer = useLayer();
  const [container] = useState(() => document.createElement("div"));
  // offset is an array literal on every render; rebinding on it would close
  // an open popup, so options are read once per layer.
  const initial = useRef({ className, closeButton, offset, maxHeight });

  useEffect(() => {
    const opts = initial.current;
    layer.bindPopup(container, {
      className: opts.className,
      closeButton: opts.closeButton,
      maxHeight: opts.maxHeight,
      ...(opts.offset ? { offset: L.point(opts.offset) } : {}),
    });
    return () => {
      layer.unbindPopup();
    };
  }, [layer, container]);

  return createPortal(children, container);
}

interface PopupAtProps {
  position: L.LatLngExpression;
  onClose?: () => void;
  className?: string;
  offset?: [number, number];
  children?: ReactNode;
}

/** A popup opened at an arbitrary point on the map (not bound to a layer) —
 * for context menus. Closing by any means (click-away, Esc, another popup)
 * is reported through onClose so the state owner can unmount the component. */
export function PopupAt({ position, onClose, className, offset, children }: PopupAtProps) {
  const map = useMap();
  const [container] = useState(() => document.createElement("div"));
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;
  const initial = useRef({ className, offset });
  const ll = L.latLng(position);

  useEffect(() => {
    const opts = initial.current;
    const popup = L.popup({
      closeButton: false,
      className: opts.className,
      ...(opts.offset ? { offset: L.point(opts.offset) } : {}),
    })
      .setLatLng([ll.lat, ll.lng])
      .setContent(container)
      .openOn(map);
    const handleClose = (e: L.PopupEvent) => {
      if (e.popup === popup) onCloseRef.current?.();
    };
    map.on("popupclose", handleClose);
    return () => {
      map.off("popupclose", handleClose);
      map.closePopup(popup);
    };
  }, [map, container, ll.lat, ll.lng]);

  return createPortal(children, container);
}

interface TooltipProps {
  children?: ReactNode;
  permanent?: boolean;
  direction?: L.Direction;
  offset?: [number, number];
}

export function Tooltip({ children, permanent, direction, offset }: TooltipProps) {
  const layer = useLayer();
  const [container] = useState(() => document.createElement("div"));
  const initial = useRef({ permanent, direction, offset });

  useEffect(() => {
    const opts = initial.current;
    layer.bindTooltip(container, {
      permanent: opts.permanent,
      direction: opts.direction,
      ...(opts.offset ? { offset: L.point(opts.offset) } : {}),
    });
    return () => {
      layer.unbindTooltip();
    };
  }, [layer, container]);

  return createPortal(children, container);
}