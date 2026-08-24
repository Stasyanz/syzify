use rusqlite::{params, Connection, Result};

use crate::import::segment_matching::{find_efforts, EffortMatch};
use crate::models::segment::SegmentEffortRow;
use crate::models::trackpoint::TrackGeometry;

/// One effort ready for storage — an [`EffortMatch`] plus times derived from
/// the track's timestamps (both stay NULL for timeless GPX). Start time is
/// epoch seconds, NOT a local-time string — the app's TEXT timestamps carry
/// the recording's local offset, which this layer can't reconstruct from the
/// parsed track, and a UTC string here would silently disagree with them.
struct NewEffort {
    start_idx: usize,
    end_idx: usize,
    distance_m: f64,
    start_time_epoch_s: Option<f64>,
    elapsed_s: Option<f64>,
}

fn to_new_efforts(matches: &[EffortMatch], t: &[Option<f64>]) -> Vec<NewEffort> {
    matches
        .iter()
        .map(|m| {
            let t0 = t.get(m.start_idx).copied().flatten();
            let t1 = t.get(m.end_idx).copied().flatten();
            let elapsed_s = match (t0, t1) {
                (Some(a), Some(b)) if b > a => Some(b - a),
                _ => None,
            };
            NewEffort {
                start_idx: m.start_idx,
                end_idx: m.end_idx,
                distance_m: m.distance_m,
                start_time_epoch_s: t0,
                elapsed_s,
            }
        })
        .collect()
}

/// Replace the stored efforts for one (segment, activity) pair — rematching
/// is a full refresh, so stale passes from an older algorithm never linger.
/// Transactional: a failure mid-insert must not leave the pair half-refreshed.
fn set_efforts(
    conn: &Connection,
    segment_id: &str,
    activity_id: &str,
    efforts: &[NewEffort],
) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "DELETE FROM segment_effort WHERE segment_id = ?1 AND activity_id = ?2",
        params![segment_id, activity_id],
    )?;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO segment_effort (segment_id, activity_id, start_idx, end_idx,
                 start_time_epoch_s, elapsed_s, distance_m)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )?;
        for e in efforts {
            stmt.execute(params![
                segment_id,
                activity_id,
                e.start_idx as i64,
                e.end_idx as i64,
                e.start_time_epoch_s,
                e.elapsed_s,
                e.distance_m,
            ])?;
        }
    }
    tx.commit()
}

/// The segment's stored polyline as dense parallel columns for the matcher.
fn segment_polyline(conn: &Connection, segment_id: &str) -> Result<(Vec<f64>, Vec<f64>, Vec<f64>)> {
    let mut stmt = conn.prepare(
        "SELECT lat, lon, distance_m FROM segment_point WHERE segment_id = ?1 ORDER BY seq ASC",
    )?;
    let mut lat = Vec::new();
    let mut lon = Vec::new();
    let mut cum = Vec::new();
    let mut rows = stmt.query(params![segment_id])?;
    while let Some(row) = rows.next()? {
        lat.push(row.get(0)?);
        lon.push(row.get(1)?);
        cum.push(row.get(2)?);
    }
    Ok((lat, lon, cum))
}

fn match_geometry_against_segment(
    conn: &Connection,
    segment_id: &str,
    activity_id: &str,
    geo: &TrackGeometry,
) -> Result<usize> {
    let (slat, slon, scum) = segment_polyline(conn, segment_id)?;
    let matches = find_efforts(&slat, &slon, &scum, &geo.lat, &geo.lon);
    let n = matches.len();
    set_efforts(conn, segment_id, activity_id, &to_new_efforts(&matches, &geo.t))?;
    Ok(n)
}

/// Padding for the bbox pre-reject, in degrees (~220 m) — generously wider
/// than every matcher radius, so it can only skip true non-overlaps.
const BBOX_PAD_DEG: f64 = 0.002;

/// Match one activity against every saved segment of its sport. Runs on
/// import; a full pair refresh, so it's idempotent. Returns the effort count.
/// Segments whose (padded) bbox doesn't touch the track's bbox are skipped
/// without a polyline load — with many segments that's most of them.
pub fn match_activity(conn: &Connection, activity_id: &str) -> Result<usize> {
    let Some(sport) = crate::db::segments::activity_sport(conn, activity_id)? else {
        return Ok(0);
    };
    let geo = crate::db::trackpoints::get_track_geometry(conn, activity_id)?;
    let mut t_bbox: Option<(f64, f64, f64, f64)> = None; // min/max lat, min/max lon
    for i in 0..geo.lat.len().min(geo.lon.len()) {
        let (Some(la), Some(lo)) = (geo.lat[i], geo.lon[i]) else {
            continue;
        };
        let b = t_bbox.get_or_insert((la, la, lo, lo));
        b.0 = b.0.min(la);
        b.1 = b.1.max(la);
        b.2 = b.2.min(lo);
        b.3 = b.3.max(lo);
    }
    let Some(tb) = t_bbox else {
        return Ok(0); // no GPS at all
    };

    let mut stmt = conn.prepare(
        "SELECT id, min_lat, max_lat, min_lon, max_lon FROM segment WHERE sport = ?1",
    )?;
    let segs: Vec<(String, f64, f64, f64, f64)> = stmt
        .query_map(params![sport], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })?
        .collect::<Result<_>>()?;
    let mut n = 0;
    for (sid, min_lat, max_lat, min_lon, max_lon) in &segs {
        let lat_apart = max_lat + BBOX_PAD_DEG < tb.0 || min_lat - BBOX_PAD_DEG > tb.1;
        // A longitude comparison is only meaningful away from the ±180 seam.
        let lon_safe = min_lon.abs() < 179.9 && max_lon.abs() < 179.9
            && tb.2.abs() < 179.9 && tb.3.abs() < 179.9;
        let lon_apart =
            lon_safe && (max_lon + BBOX_PAD_DEG < tb.2 || min_lon - BBOX_PAD_DEG > tb.3);
        if lat_apart || lon_apart {
            // Skipping the match must still honor the full-refresh contract:
            // a track that MOVED away from a segment sheds its old efforts.
            conn.execute(
                "DELETE FROM segment_effort WHERE segment_id = ?1 AND activity_id = ?2",
                params![sid, activity_id],
            )?;
            continue;
        }
        n += match_geometry_against_segment(conn, sid, activity_id, &geo)?;
    }
    Ok(n)
}

/// The segment's sport, `None` when the segment no longer exists.
pub fn segment_sport(conn: &Connection, segment_id: &str) -> Result<Option<String>> {
    use rusqlite::OptionalExtension;
    conn.query_row(
        "SELECT sport FROM segment WHERE id = ?1",
        params![segment_id],
        |r| r.get(0),
    )
    .optional()
}

/// Every activity id of a sport (merged-triathlon legs included — they are
/// activities with their own tracks and sports).
pub fn activity_ids_for_sport(conn: &Connection, sport: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT id FROM activity WHERE sport_type = ?1")?;
    let ids = stmt.query_map(params![sport], |r| r.get(0))?.collect();
    ids
}

/// Match ONE (segment, activity) pair, loading the activity's geometry.
/// The save-time backfill worker calls this per activity so the DB mutex is
/// released between activities and the UI never stalls behind a long scan.
pub fn match_pair(conn: &Connection, segment_id: &str, activity_id: &str) -> Result<usize> {
    let geo = crate::db::trackpoints::get_track_geometry(conn, activity_id)?;
    if geo.lat.is_empty() {
        return Ok(0);
    }
    match_geometry_against_segment(conn, segment_id, activity_id, &geo)
}

/// Match one segment against every activity of its sport, in one sitting.
/// A broken activity is logged and skipped, never fatal for the rest.
/// Returns the effort count (0 if the segment vanished meanwhile).
pub fn backfill_segment(conn: &Connection, segment_id: &str) -> Result<usize> {
    let Some(sport) = segment_sport(conn, segment_id)? else {
        return Ok(0);
    };
    let mut n = 0;
    for aid in &activity_ids_for_sport(conn, &sport)? {
        match match_pair(conn, segment_id, aid) {
            Ok(k) => n += k,
            Err(e) => eprintln!("Segment backfill: skipping activity {aid}: {e}"),
        }
    }
    Ok(n)
}

/// Backfill every saved segment — for segments created before the matching
/// engine existed. Iterates ACTIVITIES (one geometry load each, matching all
/// segments of that sport via `match_activity`) — the inverted loop would be
/// O(segments × activities) geometry loads. Broken activities are logged and
/// skipped. Idempotent; used by the one-time startup backfill.
pub fn backfill_all_segments(conn: &Connection) -> Result<usize> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT a.id FROM activity a JOIN segment s ON s.sport = a.sport_type",
    )?;
    let act_ids: Vec<String> = stmt
        .query_map([], |r| r.get(0))?
        .collect::<Result<_>>()?;
    let mut n = 0;
    for aid in &act_ids {
        match match_activity(conn, aid) {
            Ok(k) => n += k,
            Err(e) => eprintln!("Segment backfill: skipping activity {aid}: {e}"),
        }
    }
    Ok(n)
}

/// Drop every effort of the activity and rematch under its CURRENT sport.
/// The sport-change path: `match_activity` alone only refreshes pairs with
/// same-sport segments, so efforts earned under the old sport would linger
/// and poison that segment's ranks.
pub fn rematch_activity(conn: &Connection, activity_id: &str) -> Result<usize> {
    conn.execute(
        "DELETE FROM segment_effort WHERE activity_id = ?1",
        params![activity_id],
    )?;
    match_activity(conn, activity_id)
}

/// The activity page's efforts, in track order, each with its standing among
/// the segment's TIMED efforts (rank/count NULL-safe: timeless efforts get
/// no rank and don't dilute the count).
pub fn efforts_for_activity(conn: &Connection, activity_id: &str) -> Result<Vec<SegmentEffortRow>> {
    let mut stmt = conn.prepare(
        "SELECT se.id, se.segment_id, s.name, se.start_idx, se.end_idx,
                se.distance_m, se.elapsed_s, s.avg_grade_pct,
                CASE WHEN se.elapsed_s IS NULL THEN NULL ELSE
                    (SELECT COUNT(*) + 1 FROM segment_effort b
                     WHERE b.segment_id = se.segment_id
                       AND b.elapsed_s IS NOT NULL AND b.elapsed_s < se.elapsed_s)
                END,
                (SELECT COUNT(*) FROM segment_effort b
                 WHERE b.segment_id = se.segment_id AND b.elapsed_s IS NOT NULL)
         FROM segment_effort se
         JOIN segment s ON s.id = se.segment_id
         WHERE se.activity_id = ?1
         ORDER BY se.start_idx ASC",
    )?;
    let rows = stmt.query_map(params![activity_id], |r| {
        Ok(SegmentEffortRow {
            id: r.get(0)?,
            segment_id: r.get(1)?,
            segment_name: r.get(2)?,
            start_idx: r.get(3)?,
            end_idx: r.get(4)?,
            distance_m: r.get(5)?,
            elapsed_s: r.get(6)?,
            avg_grade_pct: r.get(7)?,
            rank: r.get(8)?,
            effort_count: r.get(9)?,
        })
    })?;
    rows.collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::models::activity::Activity;
    use crate::models::segment::NewSegmentMeta;
    use crate::models::trackpoint::TrackPoint;

    const STEP_DEG: f64 = 0.0001; // ≈11.1 m of latitude per point
    const T0: i64 = 1_750_000_000;

    fn insert_activity(conn: &Connection, id: &str, sport: &str) {
        let a = Activity {
            id: id.to_string(),
            start_time: "2026-08-01T08:00:00+00:00".to_string(),
            sport_type: sport.to_string(),
            ..Default::default()
        };
        db::activities::insert_activity(conn, &a).unwrap();
    }

    /// Straight north track: `n` points, `secs_per_point` apart (0 = timeless).
    fn insert_track(conn: &Connection, id: &str, n: usize, secs_per_point: i64) {
        let tps: Vec<TrackPoint> = (0..n)
            .map(|i| TrackPoint {
                activity_id: id.to_string(),
                t: (secs_per_point > 0).then(|| {
                    chrono::DateTime::from_timestamp(T0 + i as i64 * secs_per_point, 0)
                        .unwrap()
                        .to_rfc3339()
                }),
                lat: Some(55.0 + i as f64 * STEP_DEG),
                lon: Some(37.0),
                altitude_m: None, speed_mps: None,
                hr: None, cadence: None, power_w: None, temperature_c: None,
                vertical_oscillation_mm: None, stance_time_ms: None, stance_time_percent: None,
                step_length_mm: None, grade_percent: None,
                left_right_balance: None, left_torque_effectiveness: None,
                right_torque_effectiveness: None, left_pedal_smoothness: None,
                right_pedal_smoothness: None,
            })
            .collect();
        db::trackpoints::insert_trackpoints(conn, &tps).unwrap();
    }

    /// Save a segment cut from the canonical straight line, indices a..=b.
    fn insert_line_segment(conn: &mut Connection, seg_id: &str, source: &str, a: usize, b: usize) {
        let geo = db::trackpoints::get_track_geometry(conn, source).unwrap();
        let (seg, points) = db::segments::build_segment(
            NewSegmentMeta {
                name: "Line",
                sport: "ride",
                activity_id: source,
                id: seg_id,
                created_at: "2026-08-24T10:00:00Z",
            },
            a,
            b,
            &geo,
        )
        .unwrap();
        db::segments::insert_segment(conn, &seg, &points).unwrap();
    }

    fn setup(conn: &mut Connection) {
        insert_activity(conn, "a1", "ride");
        insert_track(conn, "a1", 100, 10);
        insert_line_segment(conn, "seg1", "a1", 20, 50);
    }

    #[test]
    fn match_activity_finds_and_times_the_pass() {
        let mut conn = db::test_db();
        setup(&mut conn);

        assert_eq!(match_activity(&conn, "a1").unwrap(), 1);
        let rows = efforts_for_activity(&conn, "a1").unwrap();
        assert_eq!(rows.len(), 1);
        let e = &rows[0];
        assert_eq!(e.segment_name, "Line");
        assert_eq!((e.start_idx, e.end_idx), (20, 50));
        // 30 hops × 10 s.
        assert_eq!(e.elapsed_s, Some(300.0));
        assert_eq!(e.rank, Some(1));
        assert_eq!(e.effort_count, 1);
        assert!(e.distance_m > 0.0);

        // Rematching is a full pair refresh — no duplicate rows.
        assert_eq!(match_activity(&conn, "a1").unwrap(), 1);
        assert_eq!(efforts_for_activity(&conn, "a1").unwrap().len(), 1);
    }

    #[test]
    fn ranks_order_by_elapsed_across_activities() {
        let mut conn = db::test_db();
        setup(&mut conn);
        match_activity(&conn, "a1").unwrap();

        // A second, faster ride over the same line (5 s per point).
        insert_activity(&conn, "a2", "ride");
        insert_track(&conn, "a2", 100, 5);
        assert_eq!(match_activity(&conn, "a2").unwrap(), 1);

        let fast = &efforts_for_activity(&conn, "a2").unwrap()[0];
        let slow = &efforts_for_activity(&conn, "a1").unwrap()[0];
        assert_eq!(fast.rank, Some(1));
        assert_eq!(slow.rank, Some(2));
        assert_eq!(fast.effort_count, 2);
        assert_eq!(slow.effort_count, 2);
    }

    #[test]
    fn timeless_track_matches_without_rank() {
        let mut conn = db::test_db();
        setup(&mut conn);
        match_activity(&conn, "a1").unwrap();

        insert_activity(&conn, "a3", "ride");
        insert_track(&conn, "a3", 100, 0); // GPX without timestamps
        assert_eq!(match_activity(&conn, "a3").unwrap(), 1);

        let e = &efforts_for_activity(&conn, "a3").unwrap()[0];
        assert_eq!(e.elapsed_s, None);
        assert_eq!(e.rank, None);
        // The timed count excludes the timeless pass.
        assert_eq!(e.effort_count, 1);
    }

    #[test]
    fn other_sport_does_not_match() {
        let mut conn = db::test_db();
        setup(&mut conn);
        insert_activity(&conn, "r1", "run");
        insert_track(&conn, "r1", 100, 10);
        assert_eq!(match_activity(&conn, "r1").unwrap(), 0);
        assert!(efforts_for_activity(&conn, "r1").unwrap().is_empty());
    }

    #[test]
    fn backfill_covers_existing_activities() {
        let mut conn = db::test_db();
        insert_activity(&conn, "a1", "ride");
        insert_track(&conn, "a1", 100, 10);
        insert_activity(&conn, "a2", "ride");
        insert_track(&conn, "a2", 100, 5);
        insert_line_segment(&mut conn, "seg1", "a1", 20, 50);

        assert_eq!(backfill_segment(&conn, "seg1").unwrap(), 2);
        assert_eq!(efforts_for_activity(&conn, "a1").unwrap().len(), 1);
        assert_eq!(efforts_for_activity(&conn, "a2").unwrap().len(), 1);
    }

    #[test]
    fn backfill_all_segments_covers_every_segment() {
        let mut conn = db::test_db();
        insert_activity(&conn, "a1", "ride");
        insert_track(&conn, "a1", 100, 10);
        insert_line_segment(&mut conn, "seg1", "a1", 20, 50);
        insert_line_segment(&mut conn, "seg2", "a1", 55, 90);

        assert_eq!(backfill_all_segments(&conn).unwrap(), 2);
        assert_eq!(efforts_for_activity(&conn, "a1").unwrap().len(), 2);
        // Idempotent — a rerun refreshes, never duplicates.
        assert_eq!(backfill_all_segments(&conn).unwrap(), 2);
        assert_eq!(efforts_for_activity(&conn, "a1").unwrap().len(), 2);
    }

    #[test]
    fn sport_change_rematches_and_drops_stale_efforts() {
        let mut conn = db::test_db();
        setup(&mut conn);
        match_activity(&conn, "a1").unwrap();
        assert_eq!(efforts_for_activity(&conn, "a1").unwrap().len(), 1);

        // The ride becomes a run: its ride-segment efforts must vanish.
        conn.execute("UPDATE activity SET sport_type='run' WHERE id='a1'", [])
            .unwrap();
        assert_eq!(rematch_activity(&conn, "a1").unwrap(), 0);
        assert!(efforts_for_activity(&conn, "a1").unwrap().is_empty());

        // And back: rematching restores the effort.
        conn.execute("UPDATE activity SET sport_type='ride' WHERE id='a1'", [])
            .unwrap();
        assert_eq!(rematch_activity(&conn, "a1").unwrap(), 1);
    }

    #[test]
    fn rank_ties_share_the_place() {
        let mut conn = db::test_db();
        setup(&mut conn);
        match_activity(&conn, "a1").unwrap();
        // An identical-speed twin of a1.
        insert_activity(&conn, "a2", "ride");
        insert_track(&conn, "a2", 100, 10);
        match_activity(&conn, "a2").unwrap();

        let e1 = &efforts_for_activity(&conn, "a1").unwrap()[0];
        let e2 = &efforts_for_activity(&conn, "a2").unwrap()[0];
        assert_eq!(e1.rank, Some(1));
        assert_eq!(e2.rank, Some(1));
        assert_eq!(e1.effort_count, 2);
    }

    #[test]
    fn rematch_compacts_efforts_that_no_longer_match() {
        let mut conn = db::test_db();
        setup(&mut conn);
        match_activity(&conn, "a1").unwrap();
        assert_eq!(efforts_for_activity(&conn, "a1").unwrap().len(), 1);

        // Replace the activity's track with a different road (lon shifted
        // ~700 m) — the old effort must be REMOVED by the refresh, not kept.
        conn.execute("DELETE FROM trackpoint WHERE activity_id='a1'", [])
            .unwrap();
        let tps: Vec<TrackPoint> = (0..100)
            .map(|i| TrackPoint {
                activity_id: "a1".to_string(),
                t: None,
                lat: Some(55.0 + i as f64 * STEP_DEG),
                lon: Some(37.01),
                altitude_m: None, speed_mps: None,
                hr: None, cadence: None, power_w: None, temperature_c: None,
                vertical_oscillation_mm: None, stance_time_ms: None, stance_time_percent: None,
                step_length_mm: None, grade_percent: None,
                left_right_balance: None, left_torque_effectiveness: None,
                right_torque_effectiveness: None, left_pedal_smoothness: None,
                right_pedal_smoothness: None,
            })
            .collect();
        db::trackpoints::insert_trackpoints(&conn, &tps).unwrap();

        assert_eq!(match_activity(&conn, "a1").unwrap(), 0);
        assert!(efforts_for_activity(&conn, "a1").unwrap().is_empty());
    }

    #[test]
    fn backwards_clock_yields_untimed_effort() {
        let mut conn = db::test_db();
        insert_activity(&conn, "a1", "ride");
        // Clock runs BACKWARDS (device glitch): elapsed must be None, never
        // negative.
        insert_track(&conn, "a1", 100, -10);
        insert_line_segment(&mut conn, "seg1", "a1", 20, 50);
        assert_eq!(match_activity(&conn, "a1").unwrap(), 1);
        let e = &efforts_for_activity(&conn, "a1").unwrap()[0];
        assert_eq!(e.elapsed_s, None);
        assert_eq!(e.rank, None);
    }

    #[test]
    fn efforts_cascade_with_segment_and_activity() {
        let mut conn = db::test_db();
        setup(&mut conn);
        match_activity(&conn, "a1").unwrap();

        conn.execute("DELETE FROM segment WHERE id='seg1'", []).unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM segment_effort", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
    }
}


