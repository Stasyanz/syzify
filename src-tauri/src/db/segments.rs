use rusqlite::{params, Connection, Result};

use crate::db::trackpoints::haversine_m;
use crate::models::segment::{NewSegmentMeta, Segment, SegmentPoint, SimilarSegment};
use crate::models::trackpoint::TrackGeometry;

/// Two segments count as "similar" (likely duplicates) when both endpoints
/// land within this radius of each other and the lengths agree within the
/// tolerance. Direction matters: a reversed run of the same road has its
/// start at the other's end and won't match.
pub const SIMILAR_ENDPOINT_RADIUS_M: f64 = 50.0;
pub const SIMILAR_LENGTH_TOLERANCE: f64 = 0.10;

/// Max stored segment-name length. Enforced backend-side — the input field
/// is not the only client of the IPC surface.
pub const MAX_NAME_LEN: usize = 200;

/// Meters per degree of latitude on the same 6 371 km sphere `haversine_m`
/// uses (2π·R/360). The SQL box and the exact check must share one Earth
/// model, or the sliver between them silently drops boundary matches.
const M_PER_DEG_LAT: f64 = 111_194.93;

/// The SQL box over-selects by this factor; the exact haversine check trims
/// the excess. Over-selection is free — under-selection is a silent miss.
const BOX_MARGIN: f64 = 1.05;

/// Trimmed, validated segment name (non-empty, capped length).
pub fn validated_name(name: &str) -> std::result::Result<&str, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("segment name is empty".into());
    }
    if name.chars().count() > MAX_NAME_LEN {
        return Err(format!("segment name is longer than {MAX_NAME_LEN} characters"));
    }
    Ok(name)
}

/// Build a segment and its polyline from the GPS-bearing points of a
/// trackpoint slice `[a..=b]` (columnar lat/lon/altitude, full-activity
/// indices). Points without GPS are skipped; the polyline distance is
/// recomputed with haversine so it doesn't depend on how the activity's own
/// cumulative distance was derived. Errors when the slice has fewer than two
/// GPS points or zero length.
pub fn build_segment(
    meta: NewSegmentMeta,
    a: usize,
    b: usize,
    geo: &TrackGeometry,
) -> std::result::Result<(Segment, Vec<SegmentPoint>), String> {
    let (lat, lon, alt) = (&geo.lat, &geo.lon, &geo.altitude_m);
    let n = lat.len().min(lon.len());
    let (a, b) = (a.min(b), a.max(b));
    // No clamping: a range past the end means the caller's view of the track
    // is stale (e.g. re-imported shorter) — saving a silently different
    // slice than the user saw is worse than an error.
    if b >= n {
        return Err("selection is out of range".into());
    }

    let mut points: Vec<SegmentPoint> = Vec::new();
    let mut cum = 0.0f64;
    // Source indices of the first/last point actually copied — the GPS
    // filter can slide them inward from the raw selection bounds.
    let mut first_used: Option<usize> = None;
    let mut last_used = a;
    for i in a..=b {
        let (Some(la), Some(lo)) = (lat[i], lon[i]) else {
            continue;
        };
        if let Some(prev) = points.last() {
            cum += haversine_m(prev.lat, prev.lon, la, lo);
        }
        first_used.get_or_insert(i);
        last_used = i;
        points.push(SegmentPoint {
            lat: la,
            lon: lo,
            altitude_m: alt.get(i).copied().flatten(),
            distance_m: cum,
        });
    }

    if points.len() < 2 {
        return Err("selection has no GPS data".into());
    }
    if cum <= 0.0 {
        return Err("selection has no distance".into());
    }

    let (Some(first), Some(last)) = (points.first(), points.last()) else {
        return Err("selection has no GPS data".into());
    };
    // Net climb needs two distinct altitude readings — a single one would
    // report a measured 0 m instead of "unknown".
    let alt_first = points.iter().find_map(|p| p.altitude_m);
    let alt_last = points.iter().rev().find_map(|p| p.altitude_m);
    let alt_count = points.iter().filter(|p| p.altitude_m.is_some()).count();
    let elev_delta_m = match (alt_first, alt_last) {
        (Some(a0), Some(a1)) if alt_count >= 2 => Some(a1 - a0),
        _ => None,
    };
    let avg_grade_pct = elev_delta_m.map(|d| d / cum * 100.0);

    let mut min_lat = f64::INFINITY;
    let mut max_lat = f64::NEG_INFINITY;
    let mut min_lon = f64::INFINITY;
    let mut max_lon = f64::NEG_INFINITY;
    for p in &points {
        min_lat = min_lat.min(p.lat);
        max_lat = max_lat.max(p.lat);
        min_lon = min_lon.min(p.lon);
        max_lon = max_lon.max(p.lon);
    }

    let segment = Segment {
        id: meta.id.to_string(),
        name: meta.name.to_string(),
        sport: meta.sport.to_string(),
        source_activity_id: Some(meta.activity_id.to_string()),
        source_start_idx: Some(first_used.unwrap_or(a) as i64),
        source_end_idx: Some(last_used as i64),
        distance_m: cum,
        elev_delta_m,
        avg_grade_pct,
        start_lat: first.lat,
        start_lon: first.lon,
        end_lat: last.lat,
        end_lon: last.lon,
        min_lat,
        max_lat,
        min_lon,
        max_lon,
        created_at: meta.created_at.to_string(),
    };
    Ok((segment, points))
}

/// Insert a segment with its polyline in one transaction.
pub fn insert_segment(
    conn: &mut Connection,
    seg: &Segment,
    points: &[SegmentPoint],
) -> Result<()> {
    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO segment (id, name, sport, source_activity_id,
             source_start_idx, source_end_idx, distance_m, elev_delta_m,
             avg_grade_pct, start_lat, start_lon, end_lat, end_lon,
             min_lat, max_lat, min_lon, max_lon, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
        params![
            seg.id,
            seg.name,
            seg.sport,
            seg.source_activity_id,
            seg.source_start_idx,
            seg.source_end_idx,
            seg.distance_m,
            seg.elev_delta_m,
            seg.avg_grade_pct,
            seg.start_lat,
            seg.start_lon,
            seg.end_lat,
            seg.end_lon,
            seg.min_lat,
            seg.max_lat,
            seg.min_lon,
            seg.max_lon,
            seg.created_at,
        ],
    )?;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO segment_point (segment_id, seq, lat, lon, altitude_m, distance_m)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?;
        for (seq, p) in points.iter().enumerate() {
            stmt.execute(params![
                seg.id,
                seq as i64,
                p.lat,
                p.lon,
                p.altitude_m,
                p.distance_m
            ])?;
        }
    }
    tx.commit()
}

/// Segments of the same sport that look like duplicates of the candidate:
/// start within [`SIMILAR_ENDPOINT_RADIUS_M`], end within the same radius,
/// length within [`SIMILAR_LENGTH_TOLERANCE`]. The SQL narrows by a lat/lon
/// box around the start (cheap, indexed); the exact haversine checks run in
/// Rust on the survivors.
pub fn find_similar(
    conn: &Connection,
    sport: &str,
    start_lat: f64,
    start_lon: f64,
    end_lat: f64,
    end_lon: f64,
    distance_m: f64,
) -> Result<Vec<SimilarSegment>> {
    let dlat = BOX_MARGIN * SIMILAR_ENDPOINT_RADIUS_M / M_PER_DEG_LAT;
    // Longitude degrees shrink with latitude. Where a longitude box can't be
    // trusted — near the poles (cos→0) or when the window would cross the
    // antimeridian (BETWEEN doesn't wrap ±180°) — skip that predicate
    // entirely and let the exact haversine check filter; the latitude box
    // still bounds the scan, and over-selection is free.
    let coslat = start_lat.to_radians().cos();
    let dlon = if coslat > 0.01 {
        BOX_MARGIN * SIMILAR_ENDPOINT_RADIUS_M / (M_PER_DEG_LAT * coslat)
    } else {
        0.0
    };
    let (lon_lo, lon_hi) = (start_lon - dlon, start_lon + dlon);
    let skip_lon = coslat <= 0.01 || lon_lo < -180.0 || lon_hi > 180.0;

    let mut stmt = conn.prepare(
        "SELECT id, name, distance_m, start_lat, start_lon, end_lat, end_lon
         FROM segment
         WHERE sport = ?1
           AND start_lat BETWEEN ?2 AND ?3
           AND (?6 = 1 OR start_lon BETWEEN ?4 AND ?5)",
    )?;
    let rows = stmt.query_map(
        params![
            sport,
            start_lat - dlat,
            start_lat + dlat,
            lon_lo,
            lon_hi,
            skip_lon
        ],
        |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, f64>(2)?,
                r.get::<_, f64>(3)?,
                r.get::<_, f64>(4)?,
                r.get::<_, f64>(5)?,
                r.get::<_, f64>(6)?,
            ))
        },
    )?;

    let mut hits = Vec::new();
    for row in rows {
        let (id, name, dist, s_lat, s_lon, e_lat, e_lon) = row?;
        if haversine_m(start_lat, start_lon, s_lat, s_lon) > SIMILAR_ENDPOINT_RADIUS_M {
            continue;
        }
        if haversine_m(end_lat, end_lon, e_lat, e_lon) > SIMILAR_ENDPOINT_RADIUS_M {
            continue;
        }
        let longer = distance_m.max(dist);
        if (dist - distance_m).abs() > SIMILAR_LENGTH_TOLERANCE * longer {
            continue;
        }
        hits.push(SimilarSegment {
            id,
            name,
            distance_m: dist,
        });
    }
    Ok(hits)
}

/// Every saved segment with its effort aggregates, newest first.
pub fn list_segments(conn: &Connection) -> Result<Vec<crate::models::segment::SegmentSummaryRow>> {
    let mut stmt = conn.prepare(
        "SELECT s.id, s.name, s.sport, s.distance_m, s.avg_grade_pct, s.elev_delta_m,
                s.created_at,
                (SELECT COUNT(*) FROM segment_effort e
                 WHERE e.segment_id = s.id AND e.elapsed_s IS NOT NULL),
                (SELECT MIN(e.elapsed_s) FROM segment_effort e WHERE e.segment_id = s.id)
         FROM segment s
         ORDER BY s.created_at DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(crate::models::segment::SegmentSummaryRow {
            id: r.get(0)?,
            name: r.get(1)?,
            sport: r.get(2)?,
            distance_m: r.get(3)?,
            avg_grade_pct: r.get(4)?,
            elev_delta_m: r.get(5)?,
            created_at: r.get(6)?,
            effort_count: r.get(7)?,
            best_elapsed_s: r.get(8)?,
        })
    })?;
    rows.collect()
}

/// Rename a segment (name already validated by the caller). Errors when the
/// segment no longer exists — a stale UI must hear about it.
pub fn rename_segment(conn: &Connection, id: &str, name: &str) -> Result<()> {
    let n = conn.execute(
        "UPDATE segment SET name = ?2 WHERE id = ?1",
        params![id, name],
    )?;
    if n == 0 {
        return Err(rusqlite::Error::QueryReturnedNoRows);
    }
    Ok(())
}

/// Delete a segment; its polyline and efforts cascade with it.
pub fn delete_segment(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM segment WHERE id = ?1", params![id])?;
    Ok(())
}

/// The source activity's sport — segments inherit it. `None` when the
/// activity doesn't exist (a friendlier surface than QueryReturnedNoRows).
pub fn activity_sport(conn: &Connection, activity_id: &str) -> Result<Option<String>> {
    use rusqlite::OptionalExtension;
    conn.query_row(
        "SELECT sport_type FROM activity WHERE id = ?1",
        params![activity_id],
        |r| r.get(0),
    )
    .optional()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::models::activity::Activity;

    /// Flat-args convenience over `build_segment`'s grouped inputs.
    #[allow(clippy::too_many_arguments)]
    fn build(
        name: &str,
        sport: &str,
        activity_id: &str,
        a: usize,
        b: usize,
        lat: &[Option<f64>],
        lon: &[Option<f64>],
        alt: &[Option<f64>],
        id: &str,
        created_at: &str,
    ) -> std::result::Result<(Segment, Vec<SegmentPoint>), String> {
        build_segment(
            NewSegmentMeta { name, sport, activity_id, id, created_at },
            a,
            b,
            &TrackGeometry {
                t: Vec::new(),
                lat: lat.to_vec(),
                lon: lon.to_vec(),
                altitude_m: alt.to_vec(),
            },
        )
    }

    /// A straight ~334 m line north along a meridian: 4 points ~111 m apart.
    fn line() -> (Vec<Option<f64>>, Vec<Option<f64>>, Vec<Option<f64>>) {
        let lat = vec![Some(55.750), Some(55.751), Some(55.752), Some(55.753)];
        let lon = vec![Some(37.620); 4];
        let alt = vec![Some(100.0), Some(105.0), Some(112.0), Some(120.0)];
        (lat, lon, alt)
    }

    fn built() -> (Segment, Vec<SegmentPoint>) {
        let (lat, lon, alt) = line();
        build("Hill", "run", "act1", 0, 3, &lat, &lon, &alt, "seg1", "2026-08-24T10:00:00Z")
            .unwrap()
    }

    #[test]
    fn build_computes_polyline_stats() {
        let (seg, points) = built();
        assert_eq!(points.len(), 4);
        assert_eq!(points[0].distance_m, 0.0);
        // ~111.3 m per 0.001° of latitude.
        assert!((seg.distance_m - 333.9).abs() < 2.0, "got {}", seg.distance_m);
        assert_eq!(seg.start_lat, 55.750);
        assert_eq!(seg.end_lat, 55.753);
        assert_eq!(seg.elev_delta_m, Some(20.0));
        let grade = seg.avg_grade_pct.unwrap();
        assert!((grade - 6.0).abs() < 0.1, "got {grade}");
        assert_eq!((seg.min_lat, seg.max_lat), (55.750, 55.753));
        assert_eq!((seg.min_lon, seg.max_lon), (37.620, 37.620));
        assert_eq!(seg.source_start_idx, Some(0));
        assert_eq!(seg.source_end_idx, Some(3));
    }

    #[test]
    fn build_skips_gpsless_points_and_normalizes_order() {
        let (mut lat, mut lon, alt) = line();
        lat[1] = None;
        lon[1] = None;
        // Reversed selection order must be normalized, the hole skipped.
        let (seg, points) =
            build("x", "run", "a", 3, 0, &lat, &lon, &alt, "s", "t").unwrap();
        assert_eq!(points.len(), 3);
        assert!((seg.distance_m - 333.9).abs() < 2.0);
    }

    #[test]
    fn build_rejects_gpsless_and_degenerate_selections() {
        let none = vec![None; 4];
        let alt = vec![Some(1.0); 4];
        assert!(build("x", "run", "a", 0, 3, &none, &none, &alt, "s", "t").is_err());

        // A single GPS point is not a segment.
        let one_lat = vec![Some(55.75), None];
        let one_lon = vec![Some(37.62), None];
        assert!(build("x", "run", "a", 0, 1, &one_lat, &one_lon, &alt, "s", "t").is_err());

        // Identical coordinates → zero length.
        let same_lat = vec![Some(55.75); 3];
        let same_lon = vec![Some(37.62); 3];
        assert!(build("x", "run", "a", 0, 2, &same_lat, &same_lon, &alt, "s", "t").is_err());

        // Any range past the end is refused, not clamped — a partial overrun
        // means the caller's track view is stale and clamping would silently
        // save a different slice.
        let (lat, lon, alt) = line();
        assert!(build("x", "run", "a", 9, 12, &lat, &lon, &alt, "s", "t").is_err());
        assert!(build("x", "run", "a", 2, 9, &lat, &lon, &alt, "s", "t").is_err());
    }

    #[test]
    fn source_indices_point_at_copied_gps_points() {
        // GPS starts only at index 1 — the stored source range must follow
        // the copied points, not the raw selection bounds.
        let (mut lat, mut lon, alt) = line();
        lat[0] = None;
        lon[0] = None;
        let (seg, _) = build("x", "run", "a", 0, 3, &lat, &lon, &alt, "s", "t").unwrap();
        assert_eq!(seg.source_start_idx, Some(1));
        assert_eq!(seg.source_end_idx, Some(3));
    }

    #[test]
    fn single_altitude_reading_is_unknown_not_flat() {
        let (lat, lon, _) = line();
        let alt = vec![None, Some(100.0), None, None];
        let (seg, _) = build("x", "run", "a", 0, 3, &lat, &lon, &alt, "s", "t").unwrap();
        assert_eq!(seg.elev_delta_m, None);
        assert_eq!(seg.avg_grade_pct, None);
    }

    #[test]
    fn validated_name_trims_and_bounds() {
        assert_eq!(validated_name("  Big climb  ").unwrap(), "Big climb");
        assert!(validated_name("").is_err());
        assert!(validated_name("   ").is_err());
        assert!(validated_name(&"x".repeat(MAX_NAME_LEN)).is_ok());
        assert!(validated_name(&"x".repeat(MAX_NAME_LEN + 1)).is_err());
    }

    #[test]
    fn build_without_altitude_leaves_elevation_empty() {
        let (lat, lon, _) = line();
        let alt = vec![None; 4];
        let (seg, _) = build("x", "run", "a", 0, 3, &lat, &lon, &alt, "s", "t").unwrap();
        assert_eq!(seg.elev_delta_m, None);
        assert_eq!(seg.avg_grade_pct, None);
    }

    #[test]
    fn insert_then_delete_cascades_points() {
        let mut conn = db::test_db();
        let (seg, points) = built();
        // The built segment references activity "act1" — satisfy the FK.
        insert_test_activity(&conn, "act1");
        insert_segment(&mut conn, &seg, &points).unwrap();

        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM segment_point WHERE segment_id='seg1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 4);

        conn.execute("DELETE FROM segment WHERE id='seg1'", []).unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM segment_point", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn segment_survives_source_activity_deletion() {
        let mut conn = db::test_db();
        insert_test_activity(&conn, "act1");
        let (seg, points) = built();
        insert_segment(&mut conn, &seg, &points).unwrap();

        conn.execute("DELETE FROM activity WHERE id='act1'", []).unwrap();

        let (n, src): (i64, Option<String>) = conn
            .query_row(
                "SELECT COUNT(*), source_activity_id FROM segment WHERE id='seg1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(n, 1);
        assert_eq!(src, None);
    }

    fn insert_test_activity(conn: &Connection, id: &str) {
        let a = Activity {
            id: id.to_string(),
            start_time: "2026-08-01T08:00:00+00:00".to_string(),
            sport_type: "run".to_string(),
            ..Default::default()
        };
        db::activities::insert_activity(conn, &a).unwrap();
    }

    fn stored(id: &str, name: &str, sport: &str, end_lat: f64, distance_m: f64) -> Segment {
        Segment {
            id: id.to_string(),
            name: name.to_string(),
            sport: sport.to_string(),
            source_activity_id: None,
            source_start_idx: None,
            source_end_idx: None,
            distance_m,
            elev_delta_m: None,
            avg_grade_pct: None,
            start_lat: 55.750,
            start_lon: 37.620,
            end_lat,
            end_lon: 37.620,
            min_lat: 55.750,
            max_lat: end_lat,
            min_lon: 37.620,
            max_lon: 37.620,
            created_at: "2026-08-24T10:00:00Z".to_string(),
        }
    }

    fn two_points() -> Vec<SegmentPoint> {
        vec![
            SegmentPoint { lat: 55.750, lon: 37.620, altitude_m: None, distance_m: 0.0 },
            SegmentPoint { lat: 55.753, lon: 37.620, altitude_m: None, distance_m: 334.0 },
        ]
    }

    #[test]
    fn find_similar_matches_same_route_only() {
        let mut conn = db::test_db();
        insert_segment(&mut conn, &stored("s1", "Hill", "run", 55.753, 334.0), &two_points())
            .unwrap();

        // Same endpoints, same sport, same length → hit.
        let hits = find_similar(&conn, "run", 55.750, 37.620, 55.753, 37.620, 334.0).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "Hill");

        // ~22 m off at the start still matches (inside the 50 m radius).
        let hits = find_similar(&conn, "run", 55.7502, 37.620, 55.753, 37.620, 334.0).unwrap();
        assert_eq!(hits.len(), 1);

        // Another sport over the same road is a different segment.
        assert!(find_similar(&conn, "ride", 55.750, 37.620, 55.753, 37.620, 334.0)
            .unwrap()
            .is_empty());

        // Start ~111 m away → miss.
        assert!(find_similar(&conn, "run", 55.751, 37.620, 55.753, 37.620, 334.0)
            .unwrap()
            .is_empty());

        // Reversed direction: candidate start = stored end → miss.
        assert!(find_similar(&conn, "run", 55.753, 37.620, 55.750, 37.620, 334.0)
            .unwrap()
            .is_empty());

        // Same endpoints but a 30% longer path (a detour) → miss…
        assert!(find_similar(&conn, "run", 55.750, 37.620, 55.753, 37.620, 434.0)
            .unwrap()
            .is_empty());
        // …while a 5% difference stays within tolerance.
        let hits = find_similar(&conn, "run", 55.750, 37.620, 55.753, 37.620, 350.0).unwrap();
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn find_similar_at_the_radius_boundary() {
        let mut conn = db::test_db();
        insert_segment(&mut conn, &stored("s1", "Hill", "run", 55.753, 334.0), &two_points())
            .unwrap();

        // ~49.97 m offset (0.0004494°): inside the 50 m radius. Guards the
        // box/haversine Earth-model agreement — with the box computed on a
        // different sphere this sliver silently missed.
        let off = 0.0004494;
        let hits =
            find_similar(&conn, "run", 55.750 + off, 37.620, 55.753 + off, 37.620, 334.0).unwrap();
        assert_eq!(hits.len(), 1);

        // ~50.5 m: survives the padded SQL box, rejected by the exact check.
        let off = 0.00045416;
        assert!(
            find_similar(&conn, "run", 55.750 + off, 37.620, 55.753 + off, 37.620, 334.0)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn find_similar_wraps_the_antimeridian() {
        let mut conn = db::test_db();
        let seg = Segment {
            id: "s1".into(),
            name: "Taveuni".into(),
            sport: "run".into(),
            source_activity_id: None,
            source_start_idx: None,
            source_end_idx: None,
            distance_m: 111.2,
            elev_delta_m: None,
            avg_grade_pct: None,
            start_lat: 0.0,
            start_lon: -179.9998,
            end_lat: 0.001,
            end_lon: -179.9998,
            min_lat: 0.0,
            max_lat: 0.001,
            min_lon: -179.9998,
            max_lon: -179.9998,
            created_at: "2026-08-24T10:00:00Z".into(),
        };
        insert_segment(&mut conn, &seg, &two_points()).unwrap();

        // The candidate's start is ~44 m away — across ±180°. A plain
        // BETWEEN on longitude can never match this; the box must yield to
        // the haversine check there.
        let hits = find_similar(&conn, "run", 0.0, 179.9998, 0.001, -179.9998, 111.2).unwrap();
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn insert_rejects_unknown_source_activity_atomically() {
        let mut conn = db::test_db();
        let mut seg = stored("sX", "X", "run", 55.753, 334.0);
        seg.source_activity_id = Some("ghost".into());
        assert!(insert_segment(&mut conn, &seg, &two_points()).is_err());
        // The transaction leaves nothing behind — neither header nor points.
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM segment", [], |r| r.get(0))
            .unwrap();
        let np: i64 = conn
            .query_row("SELECT COUNT(*) FROM segment_point", [], |r| r.get(0))
            .unwrap();
        assert_eq!((n, np), (0, 0));
    }

    #[test]
    fn activity_sport_missing_is_none() {
        let conn = db::test_db();
        assert_eq!(activity_sport(&conn, "nope").unwrap(), None);
        insert_test_activity(&conn, "act1");
        assert_eq!(activity_sport(&conn, "act1").unwrap().as_deref(), Some("run"));
    }
}
