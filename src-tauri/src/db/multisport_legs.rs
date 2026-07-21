use rusqlite::{params, Connection, Result};

use crate::models::activity::{Activity, SportType};
use crate::models::multisport_leg::MultisportLeg;

pub fn insert_legs(conn: &Connection, legs: &[MultisportLeg]) -> Result<()> {
    let mut stmt = conn.prepare(
        "INSERT INTO multisport_leg (activity_id, leg_number, sport_type,
         is_transition, start_time, total_distance_m, total_timer_time_s,
         total_elapsed_time_s, avg_speed_mps, avg_hr, max_hr, total_ascent_m,
         total_calories)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
    )?;
    for leg in legs {
        stmt.execute(params![
            leg.activity_id,
            leg.leg_number,
            leg.sport_type,
            leg.is_transition,
            leg.start_time,
            leg.total_distance_m,
            leg.total_timer_time_s,
            leg.total_elapsed_time_s,
            leg.avg_speed_mps,
            leg.avg_hr,
            leg.max_hr,
            leg.total_ascent_m,
            leg.total_calories,
        ])?;
    }
    Ok(())
}

pub fn get_legs(conn: &Connection, activity_id: &str) -> Result<Vec<MultisportLeg>> {
    let mut stmt = conn.prepare(
        "SELECT id, activity_id, leg_number, sport_type, is_transition,
         start_time, total_distance_m, total_timer_time_s,
         total_elapsed_time_s, avg_speed_mps, avg_hr, max_hr, total_ascent_m,
         total_calories, source_activity_id
         FROM multisport_leg WHERE activity_id = ?1 ORDER BY leg_number",
    )?;
    let rows = stmt.query_map(params![activity_id], |row| {
        Ok(MultisportLeg {
            id: row.get(0)?,
            activity_id: row.get(1)?,
            leg_number: row.get(2)?,
            sport_type: row.get(3)?,
            is_transition: row.get(4)?,
            start_time: row.get(5)?,
            total_distance_m: row.get(6)?,
            total_timer_time_s: row.get(7)?,
            total_elapsed_time_s: row.get(8)?,
            avg_speed_mps: row.get(9)?,
            avg_hr: row.get(10)?,
            max_hr: row.get(11)?,
            total_ascent_m: row.get(12)?,
            total_calories: row.get(13)?,
            source_activity_id: row.get(14)?,
        })
    })?;
    rows.collect()
}

/// The du/triathlon discipline a sport belongs to, or None when the sport
/// can't be an event leg at all. Containers model real multisport EVENTS —
/// running, cycling, swimming, skiing (winter triathlon) in combinations —
/// not arbitrary same-day workouts glued together.
fn discipline(sport: &str) -> Option<&'static str> {
    match sport {
        "run" | "trail_run" | "treadmill" => Some("run"),
        "ride" | "mountain_bike" => Some("bike"),
        "swim" | "open_water" => Some("swim"),
        "ski" | "ski_xc" => Some("ski"),
        _ => None,
    }
}

/// Local wall-clock instant of a stored ISO timestamp — with or without an
/// offset (manual/legacy rows store naive local times, rfc3339 parsing
/// rejects those and used to silently skip their transitions). Transitions
/// compare same-day instants from one recording context, so the LOCAL clock
/// is the right timeline; an offset, when present, is simply dropped.
fn local_instant(s: &str) -> Option<chrono::NaiveDateTime> {
    chrono::NaiveDateTime::parse_from_str(s.get(..19)?, "%Y-%m-%dT%H:%M:%S").ok()
}

/// Wall-clock end of a merging leg: its last trackpoint's timestamp — the
/// elapsed end, pauses included. `duration_s` is TIMER time, so
/// start + duration lands BEFORE the real end for any leg with pauses and
/// would inflate the transition that follows by the paused time. Trackless
/// activities (manual race entries) fall back to start + duration, which is
/// wall-clock there — manual durations are entered as full segment times.
fn leg_wall_clock_end(
    conn: &Connection,
    a: &Activity,
) -> Result<Option<chrono::NaiveDateTime>> {
    use rusqlite::OptionalExtension;
    let last_t: Option<String> = conn
        .query_row(
            "SELECT t FROM trackpoint WHERE activity_id = ?1 AND t IS NOT NULL
             ORDER BY id DESC LIMIT 1",
            params![a.id],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(end) = last_t.as_deref().and_then(local_instant) {
        return Ok(Some(end));
    }
    Ok(match (local_instant(&a.start_time), a.duration_s) {
        (Some(start), Some(dur)) => {
            Some(start + chrono::Duration::milliseconds((dur * 1000.0) as i64))
        }
        _ => None,
    })
}

/// Merge several standalone same-day activities into one triathlon container.
/// The container carries aggregated metrics; the sources become its legs
/// (parent_id set) with computed transitions between them. Returns the new
/// container's id. Runs in a transaction — all or nothing.
///
/// `new_id` is passed in (the command generates the UUID) so this stays a
/// pure DB operation, testable without a UUID source.
pub fn merge_into_triathlon(
    conn: &mut Connection,
    new_id: &str,
    activity_ids: &[String],
) -> Result<String> {
    use rusqlite::Error::SqliteFailure;
    if activity_ids.len() < 2 {
        return Err(SqliteFailure(
            rusqlite::ffi::Error::new(1),
            Some("need at least two activities to merge".into()),
        ));
    }
    // Duathlon or triathlon — nothing longer (transitions are computed from
    // the gaps, they are never selected as activities).
    if activity_ids.len() > 3 {
        return Err(SqliteFailure(
            rusqlite::ffi::Error::new(1),
            Some("a multisport event has at most three legs".into()),
        ));
    }
    // The command is public IPC (plugins can call it) — a repeated id would
    // double the container's metrics and duplicate legs, silently.
    let unique: std::collections::HashSet<&String> = activity_ids.iter().collect();
    if unique.len() != activity_ids.len() {
        return Err(SqliteFailure(
            rusqlite::ffi::Error::new(1),
            Some("the same activity is listed twice".into()),
        ));
    }

    let tx = conn.transaction()?;

    // Load and order the sources by start time. Reject if any is already a
    // leg or a container (only standalone top-level activities can merge).
    let mut acts: Vec<Activity> = Vec::new();
    for id in activity_ids {
        let a = super::activities::get_activity_by_id(&tx, id)?.ok_or_else(|| {
            SqliteFailure(rusqlite::ffi::Error::new(1), Some(format!("activity {id} not found")))
        })?;
        if a.parent_id.is_some() {
            return Err(SqliteFailure(
                rusqlite::ffi::Error::new(1),
                Some(format!("activity {id} is already part of a multisport")),
            ));
        }
        // A container carries no parent_id itself — recognize it by its
        // adopted children or its leg rows (FIT-native multisport). Merging
        // one would nest containers and double-count its aggregate.
        let is_multisport: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM activity WHERE parent_id = ?1)
                 OR EXISTS(SELECT 1 FROM multisport_leg WHERE activity_id = ?1)",
            params![id],
            |r| r.get(0),
        )?;
        if is_multisport {
            return Err(SqliteFailure(
                rusqlite::ffi::Error::new(1),
                Some(format!("activity {id} is a multisport itself")),
            ));
        }
        if discipline(&a.sport_type).is_none() {
            return Err(SqliteFailure(
                rusqlite::ffi::Error::new(1),
                Some(format!(
                    "{} can't be a multisport leg — only running, cycling, swimming and skiing combine",
                    a.sport_type
                )),
            ));
        }
        acts.push(a);
    }
    acts.sort_by(|a, b| a.start_time.cmp(&b.start_time));

    // A triathlon's legs happen on one calendar day. start_time is stored
    // with each activity's local offset, so the first 10 chars ARE the local
    // date — same-watch recordings share the offset, no re-projection needed.
    let day = acts[0].start_time.get(..10);
    if acts.iter().any(|a| a.start_time.get(..10) != day) {
        return Err(SqliteFailure(
            rusqlite::ffi::Error::new(1),
            Some("activities span multiple days — a triathlon happens on one day".into()),
        ));
    }

    // At least two distinct disciplines make an event. Run-bike-run is a
    // classic duathlon (3 legs, 2 disciplines); run + run is just two runs.
    let disciplines: std::collections::HashSet<&str> =
        acts.iter().filter_map(|a| discipline(&a.sport_type)).collect();
    if disciplines.len() < 2 {
        return Err(SqliteFailure(
            rusqlite::ffi::Error::new(1),
            Some("legs must span at least two disciplines (run/bike/swim/ski)".into()),
        ));
    }
    let event_title = if disciplines.len() == 2 { "Duathlon" } else { "Triathlon" };

    // Aggregate the container's headline metrics.
    let sum = |f: fn(&Activity) -> Option<f64>| {
        let vals: Vec<f64> = acts.iter().filter_map(f).collect();
        if vals.is_empty() { None } else { Some(vals.iter().sum()) }
    };
    let dist = sum(|a| a.distance_m);
    let dur = sum(|a| a.duration_s);
    let elev = sum(|a| a.elev_gain_m);
    let cals = sum(|a| a.calories);
    // Avg HR weighted by each leg's duration; max HR is the peak.
    let (mut hr_num, mut hr_den) = (0.0, 0.0);
    for a in &acts {
        if let (Some(hr), Some(d)) = (a.avg_hr, a.duration_s) {
            hr_num += hr * d;
            hr_den += d;
        }
    }
    let avg_hr = if hr_den > 0.0 { Some(hr_num / hr_den) } else { None };
    let max_hr = acts
        .iter()
        .filter_map(|a| a.max_hr)
        .fold(None, |m: Option<f64>, v| Some(m.map_or(v, |x| x.max(v))));

    let container = Activity {
        id: new_id.to_string(),
        start_time: acts[0].start_time.clone(),
        timezone_offset: acts[0].timezone_offset,
        sport_type: SportType::Triathlon.as_str().to_string(),
        title: Some(event_title.to_string()),
        distance_m: dist,
        duration_s: dur,
        elev_gain_m: elev,
        calories: cals,
        avg_hr,
        max_hr,
        avg_speed_mps: match (dist, dur) {
            (Some(d), Some(t)) if t > 0.0 => Some(d / t),
            _ => None,
        },
        // Everything else defaults; a container has no track of its own.
        ..Activity::empty(new_id, &acts[0].start_time)
    };
    super::activities::insert_activity(&tx, &container)?;

    // Build the legs (sport + transitions) and adopt the sources.
    let mut legs: Vec<MultisportLeg> = Vec::new();
    let mut leg_no = 0;
    for (i, a) in acts.iter().enumerate() {
        // Transition before this leg = gap since the previous leg ended.
        if i > 0 {
            if let (Some(prev_end), Some(cur_start)) = (
                leg_wall_clock_end(&tx, &acts[i - 1])?,
                local_instant(&a.start_time),
            ) {
                let gap = (cur_start - prev_end).num_seconds() as f64;
                if gap > 0.0 {
                    leg_no += 1;
                    legs.push(MultisportLeg {
                        id: None,
                        activity_id: new_id.to_string(),
                        leg_number: leg_no,
                        sport_type: "transition".to_string(),
                        is_transition: true,
                        start_time: None,
                        total_distance_m: None,
                        total_timer_time_s: Some(gap),
                        total_elapsed_time_s: Some(gap),
                        avg_speed_mps: None,
                        avg_hr: None,
                        max_hr: None,
                        total_ascent_m: None,
                        total_calories: None,
                        source_activity_id: None,
                    });
                }
            }
        }
        leg_no += 1;
        legs.push(MultisportLeg {
            id: None,
            activity_id: new_id.to_string(),
            leg_number: leg_no,
            sport_type: a.sport_type.clone(),
            is_transition: false,
            start_time: Some(a.start_time.clone()),
            total_distance_m: a.distance_m,
            total_timer_time_s: a.duration_s,
            total_elapsed_time_s: a.duration_s,
            avg_speed_mps: a.avg_speed_mps,
            avg_hr: a.avg_hr,
            max_hr: a.max_hr,
            total_ascent_m: a.elev_gain_m,
            total_calories: a.calories,
            source_activity_id: Some(a.id.clone()),
        });
        tx.execute(
            "UPDATE activity SET parent_id = ?1 WHERE id = ?2",
            params![new_id, a.id],
        )?;
    }

    // Insert legs (can't call insert_legs — it takes &Connection, we hold a tx).
    {
        let mut stmt = tx.prepare(
            "INSERT INTO multisport_leg (activity_id, leg_number, sport_type,
             is_transition, start_time, total_distance_m, total_timer_time_s,
             total_elapsed_time_s, avg_speed_mps, avg_hr, max_hr, total_ascent_m,
             total_calories, source_activity_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        )?;
        for leg in &legs {
            stmt.execute(params![
                leg.activity_id, leg.leg_number, leg.sport_type, leg.is_transition,
                leg.start_time, leg.total_distance_m, leg.total_timer_time_s,
                leg.total_elapsed_time_s, leg.avg_speed_mps, leg.avg_hr, leg.max_hr,
                leg.total_ascent_m, leg.total_calories, leg.source_activity_id,
            ])?;
        }
    }

    tx.commit()?;
    Ok(new_id.to_string())
}

/// Reverse a merge: free the container's legs (parent_id → NULL) and delete
/// the container. The standalone activities reappear in every list intact.
pub fn unmerge(conn: &mut Connection, container_id: &str) -> Result<()> {
    let tx = conn.transaction()?;
    tx.execute(
        "UPDATE activity SET parent_id = NULL WHERE parent_id = ?1",
        params![container_id],
    )?;
    // The container's leg rows go with it via ON DELETE CASCADE.
    tx.execute("DELETE FROM activity WHERE id = ?1", params![container_id])?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn leg(n: i32, sport: &str, transition: bool) -> MultisportLeg {
        MultisportLeg {
            id: None,
            activity_id: "tri-1".into(),
            leg_number: n,
            sport_type: sport.into(),
            is_transition: transition,
            start_time: None,
            total_distance_m: Some(1000.0 * n as f64),
            total_timer_time_s: Some(600.0),
            total_elapsed_time_s: Some(610.0),
            avg_speed_mps: Some(2.5),
            avg_hr: Some(150.0),
            max_hr: Some(170.0),
            total_ascent_m: None,
            total_calories: None,
            source_activity_id: None,
        }
    }

    /// Round-trip in leg order, transitions flagged; deleting the parent
    /// activity cascades the legs away.
    #[test]
    fn legs_roundtrip_and_cascade() {
        let conn = db::test_db();
        conn.execute(
            "INSERT INTO activity (id, start_time) VALUES ('tri-1', '2026-07-01T08:00:00+00:00')",
            [],
        )
        .unwrap();

        insert_legs(
            &conn,
            &[leg(1, "swim", false), leg(2, "transition", true), leg(3, "ride", false)],
        )
        .unwrap();

        let legs = get_legs(&conn, "tri-1").unwrap();
        assert_eq!(legs.len(), 3);
        assert_eq!(legs[0].sport_type, "swim");
        assert!(legs[1].is_transition);
        assert_eq!(legs[2].leg_number, 3);
        assert_eq!(legs[0].total_distance_m, Some(1000.0));

        conn.execute("DELETE FROM activity WHERE id = 'tri-1'", []).unwrap();
        assert!(get_legs(&conn, "tri-1").unwrap().is_empty());
    }

    fn insert_source(conn: &Connection, id: &str, sport: &str, start: &str, dist: f64, dur: f64) {
        let mut a = Activity::empty(id, start);
        a.sport_type = sport.to_string();
        a.distance_m = Some(dist);
        a.duration_s = Some(dur);
        a.avg_hr = Some(150.0);
        a.max_hr = Some(170.0);
        super::super::activities::insert_activity(conn, &a).unwrap();
    }

    /// Merge builds a triathlon container with summed metrics, ordered legs,
    /// a transition computed from the gap between legs, and adopts the
    /// sources (parent_id set → hidden from lists). Unmerge reverses it.
    #[test]
    fn merge_builds_container_with_legs_and_transitions() {
        let mut conn = db::test_db();
        // Swim 08:00 (10 min), then ride at 08:15 → a 5-min T1.
        insert_source(&conn, "swim", "swim", "2021-07-17T08:00:00+03:00", 1500.0, 600.0);
        insert_source(&conn, "ride", "ride", "2021-07-17T08:15:00+03:00", 40000.0, 3600.0);
        insert_source(&conn, "run", "run", "2021-07-17T09:20:00+03:00", 10000.0, 2400.0);

        let container = merge_into_triathlon(
            &mut conn,
            "tri-new",
            &["run".into(), "swim".into(), "ride".into()], // order shouldn't matter
        )
        .unwrap();
        assert_eq!(container, "tri-new");

        // Container: triathlon, summed distance/duration, weighted HR.
        let c = super::super::activities::get_activity_by_id(&conn, "tri-new")
            .unwrap()
            .unwrap();
        assert_eq!(c.sport_type, "triathlon");
        assert_eq!(c.parent_id, None);
        assert_eq!(c.distance_m, Some(51500.0));
        assert_eq!(c.duration_s, Some(6600.0));

        // Legs in time order, transitions interleaved.
        let legs = get_legs(&conn, "tri-new").unwrap();
        let sports: Vec<&str> = legs.iter().map(|l| l.sport_type.as_str()).collect();
        assert_eq!(sports, ["swim", "transition", "ride", "transition", "run"]);
        // T1 = 08:15 − (08:00 + 10min) = 5 min.
        assert_eq!(legs[1].total_timer_time_s, Some(300.0));
        // Sport legs link back to their source activities.
        assert_eq!(legs[0].source_activity_id.as_deref(), Some("swim"));
        assert_eq!(legs[1].source_activity_id, None);

        // Sources are adopted → hidden from the library list.
        let listed = super::super::activities::get_activities(
            &conn,
            &crate::models::activity::ActivityFilters::default(),
        )
        .unwrap();
        let ids: Vec<&str> = listed.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, ["tri-new"]); // only the container shows

        // Unmerge frees the sources and removes the container.
        unmerge(&mut conn, "tri-new").unwrap();
        assert!(super::super::activities::get_activity_by_id(&conn, "tri-new")
            .unwrap()
            .is_none());
        let after = super::super::activities::get_activities(
            &conn,
            &crate::models::activity::ActivityFilters::default(),
        )
        .unwrap();
        assert_eq!(after.len(), 3); // swim, ride, run back in the list
        assert!(get_legs(&conn, "tri-new").unwrap().is_empty());
    }

    /// An activity already part of a multisport can't be merged again.
    #[test]
    fn merge_rejects_already_merged_and_too_few() {
        let mut conn = db::test_db();
        insert_source(&conn, "a1", "swim", "2021-07-17T08:00:00+03:00", 1500.0, 600.0);
        insert_source(&conn, "a2", "run", "2021-07-17T08:20:00+03:00", 5000.0, 1500.0);
        merge_into_triathlon(&mut conn, "tri", &["a1".into(), "a2".into()]).unwrap();

        insert_source(&conn, "a3", "ride", "2021-07-17T09:00:00+03:00", 20000.0, 1800.0);
        let err = merge_into_triathlon(&mut conn, "tri2", &["a1".into(), "a3".into()]);
        assert!(err.is_err(), "a1 is already a leg");

        let too_few = merge_into_triathlon(&mut conn, "tri3", &["a3".into()]);
        assert!(too_few.is_err());
    }

    /// A container (merged or FIT-native multisport) can't be merged again —
    /// that would nest containers and double-count its aggregate.
    #[test]
    fn merge_rejects_containers() {
        let mut conn = db::test_db();
        insert_source(&conn, "a1", "swim", "2021-07-17T08:00:00+03:00", 1500.0, 600.0);
        insert_source(&conn, "a2", "run", "2021-07-17T08:20:00+03:00", 5000.0, 1500.0);
        merge_into_triathlon(&mut conn, "tri", &["a1".into(), "a2".into()]).unwrap();

        // Merged container (has adopted children).
        insert_source(&conn, "a3", "ride", "2021-07-17T10:00:00+03:00", 20000.0, 1800.0);
        assert!(merge_into_triathlon(&mut conn, "tri2", &["tri".into(), "a3".into()]).is_err());

        // FIT-native multisport (has leg rows, no children).
        insert_source(&conn, "fit-tri", "triathlon", "2021-07-18T08:00:00+03:00", 8000.0, 4300.0);
        insert_legs(
            &conn,
            &[MultisportLeg { activity_id: "fit-tri".into(), ..leg(1, "run", false) }],
        )
        .unwrap();
        assert!(
            merge_into_triathlon(&mut conn, "tri3", &["fit-tri".into(), "a3".into()]).is_err()
        );
    }

    /// Transitions measure from the previous leg's WALL-CLOCK end (its last
    /// trackpoint), not start + timer time — a paused leg must not leak its
    /// pauses into T1. Trackless legs keep the start + duration fallback
    /// (covered by the merge round-trip test's 5-minute T1).
    #[test]
    fn transition_measured_from_last_trackpoint_not_timer_end() {
        let mut conn = db::test_db();
        // Swim: starts 08:00, timer 10 min, but the track runs to 08:20
        // (10 minutes of pauses). Run starts 08:25.
        insert_source(&conn, "swim", "swim", "2021-07-17T08:00:00+03:00", 1500.0, 600.0);
        insert_source(&conn, "run", "run", "2021-07-17T08:25:00+03:00", 5000.0, 1500.0);
        conn.execute(
            "INSERT INTO trackpoint (activity_id, t) VALUES
             ('swim', '2021-07-17T08:05:00+03:00'),
             ('swim', '2021-07-17T08:20:00+03:00')",
            [],
        )
        .unwrap();

        merge_into_triathlon(&mut conn, "tri", &["swim".into(), "run".into()]).unwrap();
        let legs = get_legs(&conn, "tri").unwrap();
        assert!(legs[1].is_transition);
        // T1 = 08:25 − 08:20 = 5 min; start + timer would have said 15.
        assert_eq!(legs[1].total_timer_time_s, Some(300.0));
    }

    /// Legacy manual entries store NAIVE local times (no offset) — rfc3339
    /// parsing rejected those and their transitions silently vanished
    /// (the 2014 Nikolov Perevoz rebuild surfaced this). Local wall-clock
    /// comparison must work with and without offsets alike.
    #[test]
    fn transitions_computed_for_offsetless_timestamps() {
        let mut conn = db::test_db();
        // The real Nikolov Perevoz 2014 numbers: swim 11:56:29 + 24:35 ends
        // 12:21:04; ride starts 12:40:43 → T1 = 19:39 (1179 s).
        insert_source(&conn, "swim", "swim", "2014-06-21T11:56:29", 750.0, 1475.0);
        insert_source(&conn, "ride", "ride", "2014-06-21T12:40:43", 20000.0, 2380.0);

        merge_into_triathlon(&mut conn, "np", &["swim".into(), "ride".into()]).unwrap();
        let legs = get_legs(&conn, "np").unwrap();
        let sports: Vec<&str> = legs.iter().map(|l| l.sport_type.as_str()).collect();
        assert_eq!(sports, ["swim", "transition", "ride"]);
        assert_eq!(legs[1].total_timer_time_s, Some(1179.0));
    }

    /// Legs of one triathlon share a calendar day — cross-day selections
    /// (a mis-click across years) must not fuse into a "Triathlon" with a
    /// months-long transition.
    #[test]
    fn merge_rejects_cross_day_activities() {
        let mut conn = db::test_db();
        insert_source(&conn, "a1", "swim", "2021-07-17T23:50:00+03:00", 1500.0, 600.0);
        insert_source(&conn, "a2", "run", "2021-07-18T00:20:00+03:00", 5000.0, 1500.0);
        let err = merge_into_triathlon(&mut conn, "tri", &["a1".into(), "a2".into()]);
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("one day"));
    }

    /// Only running/cycling/swimming/skiing combine into an event — a
    /// Strength + Paddling merge produced a nonsense "Triathlon" (0-distance
    /// leg, 1 km/h average) before this gate.
    #[test]
    fn merge_rejects_non_event_sports() {
        let mut conn = db::test_db();
        insert_source(&conn, "st", "strength", "2026-07-15T18:43:00+03:00", 0.0, 3052.0);
        insert_source(&conn, "pd", "paddle", "2026-07-15T19:57:00+03:00", 1060.0, 868.0);
        let err = merge_into_triathlon(&mut conn, "tri", &["st".into(), "pd".into()]);
        assert!(err.unwrap_err().to_string().contains("can't be a multisport leg"));
    }

    /// Two same-discipline workouts are just two workouts, not an event;
    /// and an event has at most three legs.
    #[test]
    fn merge_rejects_single_discipline_and_too_many() {
        let mut conn = db::test_db();
        insert_source(&conn, "r1", "run", "2021-07-17T08:00:00+03:00", 5000.0, 1500.0);
        insert_source(&conn, "r2", "trail_run", "2021-07-17T10:00:00+03:00", 8000.0, 3000.0);
        let err = merge_into_triathlon(&mut conn, "tri", &["r1".into(), "r2".into()]);
        assert!(err.unwrap_err().to_string().contains("two disciplines"));

        let ids: Vec<String> = ["a", "b", "c", "d"].iter().map(|s| s.to_string()).collect();
        let err = merge_into_triathlon(&mut conn, "tri2", &ids);
        assert!(err.unwrap_err().to_string().contains("at most three"));
    }

    /// Run-bike-run is a classic duathlon: 3 legs, 2 disciplines — allowed,
    /// and the container says so in its title.
    #[test]
    fn merge_titles_duathlon_for_two_disciplines() {
        let mut conn = db::test_db();
        insert_source(&conn, "r1", "run", "2021-07-17T08:00:00+03:00", 5000.0, 1500.0);
        insert_source(&conn, "b1", "ride", "2021-07-17T08:35:00+03:00", 20000.0, 2400.0);
        insert_source(&conn, "r2", "run", "2021-07-17T09:20:00+03:00", 2500.0, 800.0);
        merge_into_triathlon(&mut conn, "du", &["r1".into(), "b1".into(), "r2".into()]).unwrap();

        let c = super::super::activities::get_activity_by_id(&conn, "du").unwrap().unwrap();
        assert_eq!(c.title.as_deref(), Some("Duathlon"));
        // Three disciplines still title as Triathlon (covered by the merge
        // round-trip test's swim/ride/run fixture).
    }

    /// Unmerge is validation-free by design: a container created BEFORE the
    /// discipline gate existed (e.g. Strength + Paddling) must still
    /// dissolve cleanly back into its standalone activities.
    #[test]
    fn unmerge_frees_legacy_containers_with_now_invalid_sports() {
        let mut conn = db::test_db();
        insert_source(&conn, "st", "strength", "2026-07-15T18:43:00+03:00", 0.0, 3052.0);
        insert_source(&conn, "pd", "paddle", "2026-07-15T19:57:00+03:00", 1060.0, 868.0);
        // Build the container manually — the merge gate would refuse it now.
        let mut c = Activity::empty("old-tri", "2026-07-15T18:43:00+03:00");
        c.sport_type = "triathlon".to_string();
        super::super::activities::insert_activity(&conn, &c).unwrap();
        conn.execute("UPDATE activity SET parent_id = 'old-tri' WHERE id IN ('st','pd')", [])
            .unwrap();
        insert_legs(
            &conn,
            &[MultisportLeg { activity_id: "old-tri".into(), ..leg(1, "strength", false) }],
        )
        .unwrap();

        unmerge(&mut conn, "old-tri").unwrap();
        assert!(super::super::activities::get_activity_by_id(&conn, "old-tri")
            .unwrap()
            .is_none());
        for id in ["st", "pd"] {
            let a = super::super::activities::get_activity_by_id(&conn, id).unwrap().unwrap();
            assert_eq!(a.parent_id, None, "{id} must be standalone again");
        }
        assert!(get_legs(&conn, "old-tri").unwrap().is_empty());
    }

    /// The same id listed twice must be rejected, not double-counted.
    #[test]
    fn merge_rejects_duplicate_ids() {
        let mut conn = db::test_db();
        insert_source(&conn, "a1", "swim", "2021-07-17T08:00:00+03:00", 1500.0, 600.0);
        assert!(merge_into_triathlon(&mut conn, "tri", &["a1".into(), "a1".into()]).is_err());
    }

    /// A merged leg can't be deleted out from under its container — the leg
    /// row's source link would dangle and the aggregate would go stale.
    #[test]
    fn merged_leg_cannot_be_deleted_directly() {
        let mut conn = db::test_db();
        insert_source(&conn, "a1", "swim", "2021-07-17T08:00:00+03:00", 1500.0, 600.0);
        insert_source(&conn, "a2", "run", "2021-07-17T08:20:00+03:00", 5000.0, 1500.0);
        merge_into_triathlon(&mut conn, "tri", &["a1".into(), "a2".into()]).unwrap();

        assert!(super::super::activities::delete_activity(&conn, "a1").is_err());
        // Unmerge frees it; then deletion works.
        unmerge(&mut conn, "tri").unwrap();
        super::super::activities::delete_activity(&conn, "a1").unwrap();
    }
}
