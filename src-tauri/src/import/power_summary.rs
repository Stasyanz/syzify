//! The power summary a device leaves out (#224). A Garmin writes NP, FTP,
//! IF and TSS into the session; a simulator such as Zwift writes average
//! power only, a GPX with power writes nothing. What the file carried
//! stays as recorded — this only fills the gaps: NP from the track, an FTP
//! borrowed from the rider's newest earlier ride that had one, and IF, TSS
//! and the power zones that follow from the two.

use crate::db;
use crate::models::time_in_zone::TimeInZone;

/// Coggan power zone ceilings as fractions of FTP, Z1..Z6; Z7 is open. The
/// same ladder the charts fall back to (chartZones.ts COGGAN_FTP_FACTORS).
pub(crate) const COGGAN_FTP_FACTORS: [f64; 6] = [0.55, 0.75, 0.9, 1.05, 1.2, 1.5];

/// The sports whose FTP means something for `sport`: its family (a trainer
/// ride and a road ride share one), or the sport alone when it has none.
/// A run's running-power FTP is never a ride's, nor the other way round.
pub(crate) fn ftp_peers(sport: &str) -> Vec<&str> {
    let family = crate::models::activity::SportType::family(sport);
    if family.is_empty() {
        vec![sport]
    } else {
        family.to_vec()
    }
}

/// IF and TSS from NP, FTP and the duration — the device's own formula:
/// IF = NP / FTP, TSS = duration × NP² / (FTP² × 3600) × 100. No duration,
/// no TSS.
pub(crate) fn intensity_and_stress(np_w: f64, ftp_w: f64, duration_s: Option<f64>) -> (f64, Option<f64>) {
    let intensity_factor = np_w / ftp_w;
    let training_stress_score = duration_s.map(|d| d * np_w * np_w / (ftp_w * ftp_w * 3600.0) * 100.0);
    (intensity_factor, training_stress_score)
}

/// Fill what the file left out of the activity's power summary. Only NULLs
/// (and zeros) are written, so a device's own numbers are never replaced.
///
/// - NP from the stored track when the file has none; a TSS then counts
///   the same timer seconds the NP was worked out over, so a GPX's coffee
///   stops do not swell it. With the device's NP, its stored duration.
/// - The FTP of the newest earlier activity of the same family
///   (`SportType::family` — a trainer ride borrows from a road ride, never
///   from a run with running power; a sport with no family from its own
///   kind only): an FTP holds still between rides, and none is better than
///   a guess — no earlier activity with one, no FTP.
/// - Power zones from the track only when the FTP was borrowed here and
///   the file counted no power zones of its own.
///
/// A multisport file is left alone: one NP over a swim, a ride and a run
/// means nothing. One transaction: a failure leaves the activity as it was.
pub(crate) fn fill_missing(conn: &rusqlite::Connection, id: &str, sport: &str) -> rusqlite::Result<()> {
    let Some(stored) = db::activities::power_summary(conn, id)? else {
        return Ok(());
    };
    if db::multisport_legs::is_multisport(conn, id)? {
        return Ok(());
    }
    let positive = |v: Option<f64>| v.filter(|v| v.is_finite() && *v > 0.0);
    let (np, duration_s) = match positive(stored.normalized_power_w) {
        Some(np) => (np, positive(stored.duration_s)),
        None => {
            if !db::power_curve::has_power(conn, id)? {
                return Ok(());
            }
            let (t, power) = db::power_curve::power_track(conn, id)?;
            let Some((np, timer_s)) = crate::import::power_curve::normalized_power(&t, &power) else {
                return Ok(());
            };
            (np, Some(timer_s))
        }
    };
    let (ftp, borrowed) = match positive(stored.threshold_power_w) {
        Some(ftp) => (Some(ftp), false),
        None => (db::activities::ftp_before(conn, id, &ftp_peers(sport))?, true),
    };
    let (intensity_factor, training_stress_score) = match ftp {
        Some(ftp) => {
            let (i, s) = intensity_and_stress(np, ftp, duration_s);
            (Some(i), s)
        }
        None => (None, None),
    };

    let tx = conn.unchecked_transaction()?;
    db::activities::fill_power_summary(
        &tx,
        id,
        &crate::models::activity::PowerSummary {
            normalized_power_w: Some(np),
            threshold_power_w: ftp,
            intensity_factor,
            training_stress_score,
            duration_s: None,
        },
    )?;
    if let (Some(ftp), true) = (ftp, borrowed) {
        let has_power_zones = db::time_in_zones::get_time_in_zones(&tx, id)?
            .iter()
            .any(|z| z.zone_type == "power");
        if !has_power_zones {
            recompute_power_zones(&tx, id, ftp)?;
        }
    }
    tx.commit()
}

/// Lend an FTP to every activity that has NP but none (#224): run at the
/// end of an import batch, so an older ride that comes in after the
/// Zwift rides it should have lent to — a batch is walked in path order,
/// a sync may fetch the newest first — still reaches them. Cheap: the
/// candidates are read from the activity table alone, and one with no
/// earlier FTP of its kind (a run with a pod and no running FTP yet) costs
/// one lookup and no write — it stays a candidate batch after batch. One
/// activity's failure is logged and the rest are still lent to.
pub(crate) fn lend_ftp(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    for (id, sport) in db::activities::ftp_gaps(conn)? {
        if db::activities::ftp_before(conn, &id, &ftp_peers(&sport))?.is_none() {
            continue;
        }
        if let Err(e) = fill_missing(conn, &id, &sport) {
            eprintln!("Could not lend an FTP to {id}: {e}");
        }
    }
    Ok(())
}


/// Rebuild the activity's power time-in-zone rows from its trackpoints on
/// the Coggan ladder of `ftp_w`, bucketed by zone ceiling; Z7 is open.
/// Each sample stands for the road up to the next one when that gap is
/// short (Garmin smart recording spaces samples up to
/// [`crate::import::power_curve::MAX_HOLD_S`] apart at steady effort) and
/// for one second when the gap is long — an auto-pause, a dropped
/// recording. The device tallies zones on timer time and a stop is not on
/// the timer, so a stop must land in no zone at all: counting it as 0 W
/// would swell Z1 by the length of every coffee break and push the total
/// past the timer, and even the power curve's ten-second hold would leak
/// ten seconds into every stop. No trackpoints with power → the rows are
/// left alone.
pub(crate) fn recompute_power_zones(conn: &rusqlite::Connection, id: &str, ftp_w: f64) -> rusqlite::Result<()> {
    let cols = db::trackpoints::get_trackpoints_columnar(conn, id)?;
    let mut samples: Vec<(f64, f64)> = cols
        .t
        .iter()
        .zip(cols.power_w.iter())
        .filter_map(|(t, p)| match (t, p) {
            (Some(t), Some(p)) if t.is_finite() && *p >= 0 => Some((*t, *p as f64)),
            _ => None,
        })
        .collect();
    if samples.is_empty() {
        return Ok(());
    }
    samples.sort_by(|a, b| a.0.total_cmp(&b.0));
    let max_hold = crate::import::power_curve::MAX_HOLD_S as f64;
    let ceilings: Vec<f64> = COGGAN_FTP_FACTORS.iter().map(|f| (f * ftp_w).round()).collect();
    let mut seconds = vec![0.0_f64; ceilings.len() + 1];
    for (k, (t, p)) in samples.iter().enumerate() {
        let gap = samples.get(k + 1).map(|(next, _)| next - t);
        let span = match gap {
            Some(g) if g > 0.0 && g <= max_hold => g,
            _ => 1.0,
        };
        let zone = ceilings.iter().position(|c| *p <= *c).unwrap_or(ceilings.len());
        seconds[zone] += span;
    }
    let rows: Vec<TimeInZone> = seconds
        .iter()
        .enumerate()
        .map(|(i, s)| TimeInZone {
            id: None,
            activity_id: id.to_string(),
            zone_type: "power".to_string(),
            zone_index: i as i32,
            time_s: *s,
            zone_high_boundary: ceilings.get(i).copied(),
        })
        .collect();
    db::time_in_zones::delete_time_in_zones_of_type(conn, id, "power")?;
    db::time_in_zones::insert_time_in_zones(conn, &rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    /// An activity of `sport` starting `day` at 18:00 with `set` applied
    /// (an SQL SET list, or empty), and — when `watts` is given — two
    /// minutes of power at 1 Hz.
    fn activity(conn: &Connection, id: &str, sport: &str, day: &str, set: &str, watts: Option<i32>) {
        conn.execute(
            "INSERT INTO activity (id, start_time, sport_type) VALUES (?1, ?2, ?3)",
            rusqlite::params![id, format!("2026-10-{day}T18:00:00+00:00"), sport],
        )
        .unwrap();
        if !set.is_empty() {
            conn.execute(&format!("UPDATE activity SET {set} WHERE id = ?1"), [id]).unwrap();
        }
        if let Some(w) = watts {
            for i in 0..120 {
                conn.execute(
                    "INSERT INTO trackpoint (activity_id, t, power_w) VALUES (?1, ?2, ?3)",
                    rusqlite::params![id, format!("2026-10-{day}T18:{:02}:{:02}+00:00", i / 60, i % 60), w],
                )
                .unwrap();
            }
        }
    }

    fn fill(conn: &Connection, id: &str) {
        let sport: String = conn
            .query_row("SELECT sport_type FROM activity WHERE id = ?1", [id], |r| r.get(0))
            .unwrap();
        fill_missing(conn, id, &sport).unwrap();
    }

    fn summary(conn: &Connection, id: &str) -> crate::models::activity::PowerSummary {
        db::activities::power_summary(conn, id).unwrap().unwrap()
    }

    fn power_zone_seconds(conn: &Connection, id: &str) -> Vec<f64> {
        let mut zones: Vec<_> = db::time_in_zones::get_time_in_zones(conn, id)
            .unwrap()
            .into_iter()
            .filter(|z| z.zone_type == "power")
            .collect();
        zones.sort_by_key(|z| z.zone_index);
        zones.iter().map(|z| z.time_s).collect()
    }

    /// A Zwift ride — average power only — gets NP from its track, the FTP
    /// of the newest earlier ride, IF and TSS from the two, and power zones.
    /// A later ride lends nothing, and neither does a run's running power.
    /// The TSS counts the two minutes the NP was worked out over, not the
    /// hour the stored duration says: one time basis for both.
    #[test]
    fn a_ride_without_a_summary_borrows_the_newest_earlier_ride_ftp() {
        let conn = db::test_db();
        activity(&conn, "old", "ride", "01", "threshold_power_w = 238", None);
        activity(&conn, "recent", "indoor_ride", "10", "threshold_power_w = 250", None);
        activity(&conn, "stryd", "run", "15", "threshold_power_w = 400", None);
        activity(&conn, "later", "ride", "20", "threshold_power_w = 300", None);
        activity(&conn, "zwift", "virtual_ride", "17", "duration_s = 3600, avg_power_w = 200", Some(200));

        fill(&conn, "zwift");
        let s = summary(&conn, "zwift");
        assert!((s.normalized_power_w.unwrap() - 200.0).abs() < 1e-9, "steady 200 W");
        assert_eq!(s.threshold_power_w, Some(250.0), "the indoor ride's, not the run's or the later one's");
        assert!((s.intensity_factor.unwrap() - 0.8).abs() < 1e-9);
        assert!(
            (s.training_stress_score.unwrap() - 64.0 / 30.0).abs() < 1e-9,
            "120 s × 200² / (250² × 3600) × 100"
        );
        // 200 W on FTP 250 is Z3 (188 < 200 ≤ 225), all two minutes of it.
        assert_eq!(power_zone_seconds(&conn, "zwift"), vec![0.0, 0.0, 120.0, 0.0, 0.0, 0.0, 0.0]);
    }

    /// What the device wrote stays, every field of it, and no zones appear
    /// that it did not count.
    #[test]
    fn a_device_summary_is_left_as_recorded() {
        let conn = db::test_db();
        activity(&conn, "earlier", "ride", "01", "threshold_power_w = 300", None);
        activity(
            &conn,
            "edge",
            "ride",
            "17",
            "duration_s = 3600, normalized_power_w = 180, threshold_power_w = 254,
             intensity_factor = 0.71, training_stress_score = 50",
            Some(200),
        );
        fill(&conn, "edge");
        assert_eq!(
            summary(&conn, "edge"),
            crate::models::activity::PowerSummary {
                normalized_power_w: Some(180.0),
                threshold_power_w: Some(254.0),
                intensity_factor: Some(0.71),
                training_stress_score: Some(50.0),
                duration_s: Some(3600.0),
            }
        );
        assert!(power_zone_seconds(&conn, "edge").is_empty());
    }

    /// A 0 the file wrote is no value either: it is filled like a NULL.
    #[test]
    fn a_stored_zero_is_filled_like_a_gap() {
        let conn = db::test_db();
        activity(&conn, "earlier", "ride", "01", "threshold_power_w = 250", None);
        activity(
            &conn,
            "zeros",
            "ride",
            "17",
            "normalized_power_w = 0, threshold_power_w = 0, intensity_factor = 0, training_stress_score = 0",
            Some(200),
        );
        fill(&conn, "zeros");
        let s = summary(&conn, "zeros");
        assert!((s.normalized_power_w.unwrap() - 200.0).abs() < 1e-9);
        assert_eq!(s.threshold_power_w, Some(250.0));
        assert!(s.intensity_factor.unwrap() > 0.0 && s.training_stress_score.unwrap() > 0.0);
    }

    /// No earlier activity of the family with an FTP: NP alone — a guessed
    /// FTP would put numbers on the page nobody can trace.
    #[test]
    fn without_an_earlier_ftp_only_np_is_filled() {
        let conn = db::test_db();
        activity(&conn, "ride", "ride", "01", "threshold_power_w = 250", None);
        // A sport with no family borrows from its own kind only.
        activity(&conn, "kayak", "paddle", "17", "duration_s = 3600", Some(120));
        fill(&conn, "kayak");
        let s = summary(&conn, "kayak");
        assert!((s.normalized_power_w.unwrap() - 120.0).abs() < 1e-9);
        assert_eq!((s.threshold_power_w, s.intensity_factor, s.training_stress_score), (None, None, None));
        assert!(power_zone_seconds(&conn, "kayak").is_empty());

        activity(&conn, "earlier-kayak", "paddle", "10", "threshold_power_w = 150", None);
        fill(&conn, "kayak");
        assert_eq!(summary(&conn, "kayak").threshold_power_w, Some(150.0));
    }

    /// Zones the file counted stay; the borrowed FTP still fills the rest.
    /// With the device's NP and no duration there is an IF but no TSS.
    #[test]
    fn device_zones_stay_and_no_duration_means_no_tss() {
        let conn = db::test_db();
        activity(&conn, "earlier", "ride", "01", "threshold_power_w = 250", None);
        activity(&conn, "counted", "ride", "17", "normalized_power_w = 200", Some(200));
        db::time_in_zones::insert_time_in_zones(
            &conn,
            &[TimeInZone {
                id: None,
                activity_id: "counted".into(),
                zone_type: "power".into(),
                zone_index: 1,
                time_s: 99.0,
                zone_high_boundary: None,
            }],
        )
        .unwrap();
        fill(&conn, "counted");
        let s = summary(&conn, "counted");
        assert_eq!(s.threshold_power_w, Some(250.0));
        assert!((s.intensity_factor.unwrap() - 0.8).abs() < 1e-9);
        assert_eq!(s.training_stress_score, None, "no duration to figure TSS over");
        assert_eq!(power_zone_seconds(&conn, "counted"), vec![99.0], "the device's own count");
    }

    /// A multisport file is left alone: one NP over a swim, a ride and a
    /// run would mean nothing.
    #[test]
    fn a_multisport_activity_is_left_alone() {
        let conn = db::test_db();
        activity(&conn, "earlier", "triathlon", "01", "threshold_power_w = 250", None);
        activity(&conn, "tri", "triathlon", "17", "duration_s = 120", Some(200));
        conn.execute(
            "INSERT INTO multisport_leg (activity_id, leg_number, sport_type) VALUES ('tri', 1, 'ride')",
            [],
        )
        .unwrap();
        fill(&conn, "tri");
        assert_eq!(summary(&conn, "tri").normalized_power_w, None);
    }

    /// A track too short for one 30 s window gives no NP, and with no NP
    /// nothing else follows; a missing activity is no error.
    #[test]
    fn no_np_means_nothing_is_written() {
        let conn = db::test_db();
        activity(&conn, "earlier", "ride", "01", "threshold_power_w = 250", None);
        activity(&conn, "blip", "ride", "17", "duration_s = 10", None);
        for i in 0..10 {
            conn.execute(
                "INSERT INTO trackpoint (activity_id, t, power_w) VALUES ('blip', ?1, 300)",
                [format!("2026-10-17T18:00:{i:02}+00:00")],
            )
            .unwrap();
        }
        fill(&conn, "blip");
        assert_eq!(summary(&conn, "blip").threshold_power_w, None);
        fill_missing(&conn, "nowhere", "ride").unwrap();
    }

    /// The end of a batch: an older ride that came in after a Zwift ride
    /// lends it the FTP the Zwift ride could not borrow on its way in.
    #[test]
    fn lend_ftp_reaches_rides_imported_before_their_lender() {
        let conn = db::test_db();
        activity(&conn, "zwift", "virtual_ride", "17", "", Some(200));
        fill(&conn, "zwift");
        assert_eq!(summary(&conn, "zwift").threshold_power_w, None, "nothing earlier yet");

        activity(&conn, "edge", "ride", "10", "threshold_power_w = 250", None);
        lend_ftp(&conn).unwrap();
        assert_eq!(summary(&conn, "zwift").threshold_power_w, Some(250.0));
        assert_eq!(power_zone_seconds(&conn, "zwift").len(), 7);
    }

    /// One activity that cannot be lent to is logged and left as it was;
    /// one with no earlier FTP of its kind is not written at all — it stays
    /// a candidate batch after batch.
    #[test]
    fn lend_ftp_skips_what_it_cannot_lend_to() {
        let conn = db::test_db();
        activity(&conn, "edge", "ride", "01", "threshold_power_w = 250", None);
        activity(&conn, "zwift", "virtual_ride", "17", "normalized_power_w = 200", Some(200));
        activity(&conn, "pod", "run", "17", "normalized_power_w = 300", None);
        conn.execute_batch(
            "CREATE TRIGGER fail_power_zones BEFORE INSERT ON time_in_zone
             BEGIN SELECT RAISE(ABORT, 'zone insert refused'); END;",
        )
        .unwrap();
        lend_ftp(&conn).unwrap();
        assert_eq!(summary(&conn, "zwift").threshold_power_w, None, "rolled back, logged");

        conn.execute_batch("DROP TRIGGER fail_power_zones; DELETE FROM activity WHERE id = 'zwift';").unwrap();
        let writes = || -> i64 { conn.query_row("SELECT total_changes()", [], |r| r.get(0)).unwrap() };
        let before = writes();
        lend_ftp(&conn).unwrap();
        assert_eq!(writes(), before, "no running FTP to lend: no write");
    }

    /// The fill is one transaction: a failure in the zone rebuild leaves
    /// the activity as it was imported.
    #[test]
    fn a_failed_zone_rebuild_rolls_the_fill_back() {
        let conn = db::test_db();
        activity(&conn, "earlier", "ride", "01", "threshold_power_w = 250", None);
        activity(&conn, "zwift", "virtual_ride", "17", "duration_s = 3600", Some(200));
        conn.execute_batch(
            "CREATE TRIGGER fail_power_zones BEFORE INSERT ON time_in_zone
             BEGIN SELECT RAISE(ABORT, 'zone insert refused'); END;",
        )
        .unwrap();
        assert!(fill_missing(&conn, "zwift", "virtual_ride").is_err());
        assert!(lend_ftp(&conn).is_ok(), "nothing to lend to: no NP was kept");
        let s = summary(&conn, "zwift");
        assert_eq!((s.normalized_power_w, s.threshold_power_w), (None, None));
    }

    #[test]
    fn ftp_peers_are_the_family_or_the_sport_alone() {
        assert_eq!(ftp_peers("virtual_ride"), ["ride", "mountain_bike", "indoor_ride", "virtual_ride"]);
        assert_eq!(ftp_peers("paddle"), ["paddle"]);
    }
}

