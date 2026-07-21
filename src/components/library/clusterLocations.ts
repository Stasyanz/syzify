import L from "leaflet";

/** Two points closer than this on screen collapse into one cluster. */
export const CLUSTER_RADIUS_PX = 60;
/** Clicking a cluster zooms no deeper than this; past it (or when the
 * members share one spot) the cluster opens a list instead. */
export const CLUSTER_SPLIT_MAX_ZOOM = 16;

export interface ClusterPoint {
  lat: number;
  lon: number;
}

export interface Cluster<T extends ClusterPoint> {
  /** Centroid of the members — where the marker is drawn. */
  lat: number;
  lon: number;
  members: T[];
}

/**
 * Greedy screen-space clustering, the zoom-dependent aggregation real-estate
 * maps use: project every point to pixel space at the given zoom
 * (EPSG3857 math only — no live map needed, so this stays pure and testable)
 * and absorb each point into the first cluster anchored within
 * CLUSTER_RADIUS_PX, else start a new one. Zooming in spreads the pixels,
 * so clusters split into smaller ones until single activities remain.
 */
export function clusterLocations<T extends ClusterPoint>(
  points: T[],
  zoom: number,
): Cluster<T>[] {
  const anchors: { x: number; y: number; members: T[] }[] = [];
  const r2 = CLUSTER_RADIUS_PX * CLUSTER_RADIUS_PX;

  for (const p of points) {
    const px = L.CRS.EPSG3857.latLngToPoint(L.latLng(p.lat, p.lon), zoom);
    const hit = anchors.find(
      (c) => (c.x - px.x) ** 2 + (c.y - px.y) ** 2 <= r2,
    );
    if (hit) {
      hit.members.push(p);
    } else {
      anchors.push({ x: px.x, y: px.y, members: [p] });
    }
  }

  return anchors.map((c) => ({
    lat: c.members.reduce((s, m) => s + m.lat, 0) / c.members.length,
    lon: c.members.reduce((s, m) => s + m.lon, 0) / c.members.length,
    members: c.members,
  }));
}

/** Largest member-to-member distance in meters — a cluster whose spread is
 * tiny (every run starts at the same door) can never split by zooming. */
export function clusterSpreadMeters(members: ClusterPoint[]): number {
  let max = 0;
  for (let i = 0; i < members.length; i++) {
    for (let j = i + 1; j < members.length; j++) {
      const d = L.latLng(members[i].lat, members[i].lon).distanceTo(
        L.latLng(members[j].lat, members[j].lon),
      );
      if (d > max) max = d;
    }
  }
  return max;
}
