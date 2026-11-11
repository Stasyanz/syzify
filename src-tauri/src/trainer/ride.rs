//! Finishing a ride: the FIT file goes through the normal import (so the
//! ride gets every analysis, its original is kept in `raw/` — sealed when
//! activities are encrypted — and it can be uploaded elsewhere), then the
//! journal goes. Import first, journal second: a crash between the two
//! imports the journal again, which the import reports as a duplicate.
//! A journal left by a crash is finished the same way.

use std::path::Path;

use rusqlite::Connection;

use super::fit::{self, Ride};
use super::journal::{self, ReadError};
use crate::import::pipeline::{import_bytes, ImportOutcome};

#[derive(Debug)]
pub enum Finished {
    /// Through the import (imported, or a duplicate of a ride already in).
    Imported(ImportOutcome),
    /// Not one value recorded: nothing to import.
    Empty,
}

/// Why a ride could not be finished. The journal stays in every case.
#[derive(Debug)]
pub enum FinishError {
    /// The journal does not read (locked vault, another key, ruined).
    Journal(ReadError),
    /// The import refused it, or the journal could not be removed after.
    Failed(String),
}

impl std::fmt::Display for FinishError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FinishError::Journal(e) => write!(f, "{e}"),
            FinishError::Failed(e) => write!(f, "{e}"),
        }
    }
}

/// The FIT file's name in the import: what `raw_file.original_path` keeps.
pub fn file_name(ride: &Ride) -> String {
    format!("syzify-ride-{}.fit", ride.start_unix)
}

fn has_values(ride: &Ride) -> bool {
    ride.samples
        .iter()
        .any(|s| s.watts.is_some() || s.cadence_rpm.is_some() || s.heart_rate.is_some() || s.speed_mps.is_some())
}

/// Imports the ride; `key` is the activities key.
pub fn import_ride(conn: &Connection, vault: &Path, ride: &Ride, key: Option<&[u8; 32]>) -> Result<Finished, String> {
    if !has_values(ride) {
        return Ok(Finished::Empty);
    }
    let bytes = fit::encode(ride);
    import_bytes(conn, vault, &file_name(ride), &bytes, key).map(Finished::Imported)
}

/// Imports the ride, then removes its journal. A failed import keeps the
/// journal, so the ride can be finished later.
pub fn finish(
    conn: &Connection,
    vault: &Path,
    ride: &Ride,
    journal: Option<&Path>,
    key: Option<&[u8; 32]>,
) -> Result<Finished, FinishError> {
    let finished = import_ride(conn, vault, ride, key).map_err(FinishError::Failed)?;
    if let Some(path) = journal {
        journal::discard(path).map_err(|e| FinishError::Failed(format!("the ride is in, but its journal stays: {e}")))?;
    }
    Ok(finished)
}

/// Finishes a ride a crash left behind.
pub fn finish_journal(conn: &Connection, vault: &Path, path: &Path, key: Option<&[u8; 32]>) -> Result<Finished, FinishError> {
    let recovered = journal::read(path, key).map_err(FinishError::Journal)?;
    finish(conn, vault, &recovered.ride(), Some(path), key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::ScratchDir;
    use crate::trainer::fake::{FakeConfig, FakeTrainer};
    use crate::trainer::fit::SubSport;
    use crate::trainer::journal::{fixed, Journal, Second};
    use crate::trainer::link::{Control, Identity, LinkEvent, Reading, SensorLink, TrainerLink, Transport};
    use crate::trainer::session::{Controller, Session};
    use std::time::Duration;

    /// 2026-11-11 18:00:00 UTC.
    const START: i64 = 1_794_420_000;

    struct Hold(u16);
    impl Controller for Hold {
        fn tick(&mut self, _: Duration, _: &Second, _: &[LinkEvent], t: &mut dyn TrainerLink) {
            t.send(Control::TargetPower(self.0));
        }
    }

    struct Strap(u64, Identity);
    impl SensorLink for Strap {
        fn identity(&self) -> Option<&Identity> {
            Some(&self.1)
        }
        fn poll(&mut self, now: Duration) -> Vec<LinkEvent> {
            let mut out = Vec::new();
            if self.0 == 0 {
                out.push(LinkEvent::Identified { at: Duration::ZERO, identity: self.1.clone() });
            }
            while self.0 <= now.as_secs() {
                out.push(LinkEvent::Reading(Reading { at: Duration::from_secs(self.0), heart_rate: Some(130), ..Reading::default() }));
                self.0 += 1;
            }
            out
        }
    }

    /// Ten minutes in ERG at 150 W, a lap at 5 min, heart rate from a strap.
    fn ten_minutes(vault: &Path, key: Option<[u8; 32]>, sub_sport: SubSport) -> Session {
        let journal = Journal::create(vault, START, sub_sport, fixed(key)).unwrap();
        let trainer = Box::new(FakeTrainer::new(FakeConfig::elite_avanti(85)));
        let strap = Strap(0, Identity { name: Some("HRM-Pro".into()), ble_address: Some("A1".into()), ..Identity::new(Transport::Ble) });
        assert!(strap.identity().is_some());
        let mut s = Session::new(Duration::ZERO, START, sub_sport, trainer, None, Some(Box::new(strap)), Some(journal));
        s.add_controller(Box::new(Hold(150)));
        for t in 1..=600 {
            if t == 301 {
                s.lap(Duration::from_secs(300));
            }
            s.tick(Duration::from_secs(t));
        }
        s
    }

    /// sport, power, cadence, heart rate, duration, creator.
    type Row = (String, Option<f64>, Option<f64>, Option<f64>, Option<f64>, Option<String>);

    fn activity(conn: &Connection) -> Row {
        conn.query_row("SELECT sport_type, avg_power_w, avg_cadence, avg_hr, duration_s, source_device FROM activity", [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))
        })
        .unwrap()
    }

    #[test]
    fn a_ride_goes_through_the_normal_import_as_an_indoor_ride() {
        let vault = ScratchDir::new("ride_import");
        let conn = crate::db::test_db();
        let (ride, journal, _) = ten_minutes(&vault, None, SubSport::IndoorCycling).finish(Duration::from_secs(600));
        let finished = finish(&conn, &vault, &ride, journal.as_deref(), None).unwrap();
        assert!(matches!(finished, Finished::Imported(ImportOutcome::Imported)), "{finished:?}");

        let (sport, power, cadence, hr, duration, source) = activity(&conn);
        assert_eq!(sport, "indoor_ride");
        assert!(power.is_some_and(|p| (148.0..=153.0).contains(&p)), "ERG held 150: {power:?}");
        assert_eq!(cadence, Some(85.0));
        assert_eq!(hr, Some(130.0));
        assert!(duration.is_some_and(|d| (599.0..=601.0).contains(&d)), "{duration:?}");
        assert_eq!(source.as_deref(), Some(fit::PRODUCT_NAME), "the creator is Syzify, not «Development»");
        let laps: i64 = conn.query_row("SELECT COUNT(*) FROM lap", [], |r| r.get(0)).unwrap();
        assert_eq!(laps, 2);
        let (points, with_power): (i64, i64) = conn
            .query_row("SELECT COUNT(*), COUNT(power_w) FROM trackpoint", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!((points, with_power), (600, 600), "a point a second, each with power");
        // The trainer and the strap are sensors of the ride (gear rules key on them).
        let sensors: i64 = conn.query_row("SELECT COUNT(*) FROM activity_sensor", [], |r| r.get(0)).unwrap();
        assert_eq!(sensors, 2);

        assert!(journal::pending(&vault).is_empty(), "the journal is gone");
        let original: String = conn.query_row("SELECT original_path FROM raw_file", [], |r| r.get(0)).unwrap();
        assert_eq!(original, file_name(&ride));
    }

    #[test]
    fn a_virtual_ride_imports_as_one() {
        let vault = ScratchDir::new("ride_virtual");
        let conn = crate::db::test_db();
        let (ride, journal, _) = ten_minutes(&vault, None, SubSport::VirtualActivity).finish(Duration::from_secs(600));
        finish(&conn, &vault, &ride, journal.as_deref(), None).unwrap();
        assert_eq!(activity(&conn).0, "virtual_ride");
    }

    #[test]
    fn a_crashed_ride_is_finished_from_its_sealed_journal() {
        let vault = ScratchDir::new("ride_recover");
        let conn = crate::db::test_db();
        let key = [9u8; 32];
        drop(ten_minutes(&vault, Some(key), SubSport::IndoorCycling)); // the app died mid-ride
        let path = journal::pending(&vault).pop().expect("journal");
        let locked = finish_journal(&conn, &vault, &path, None).unwrap_err();
        assert!(matches!(locked, FinishError::Journal(ReadError::Locked)), "{locked}");
        assert_eq!(locked.to_string(), ReadError::Locked.to_string());
        assert!(path.exists(), "the journal waits for the key");
        let finished = finish_journal(&conn, &vault, &path, Some(&key)).unwrap();
        assert!(matches!(finished, Finished::Imported(ImportOutcome::Imported)));
        assert_eq!(activity(&conn).0, "indoor_ride");
        assert!(!path.exists());
        let stored: String = conn.query_row("SELECT path_in_vault FROM raw_file", [], |r| r.get(0)).unwrap();
        assert!(stored.ends_with(".enc"), "the original is sealed too: {stored}");
    }

    #[test]
    fn the_same_ride_twice_is_a_duplicate() {
        let vault = ScratchDir::new("ride_twice");
        let conn = crate::db::test_db();
        let (ride, _, _) = ten_minutes(&vault, None, SubSport::IndoorCycling).finish(Duration::from_secs(600));
        import_ride(&conn, &vault, &ride, None).unwrap();
        assert!(matches!(import_ride(&conn, &vault, &ride, None).unwrap(), Finished::Imported(ImportOutcome::Skipped)));
    }

    #[test]
    fn a_ride_without_a_value_imports_nothing_and_drops_its_journal() {
        let vault = ScratchDir::new("ride_empty");
        let conn = crate::db::test_db();
        let path = Journal::create(&vault, START, SubSport::IndoorCycling, fixed(None)).unwrap().path().to_path_buf();
        assert!(matches!(finish_journal(&conn, &vault, &path, None).unwrap(), Finished::Empty));
        assert!(!path.exists());
        // Ten minutes of seconds with nothing in them: a silent trainer.
        let silent = Ride {
            start_unix: START,
            sub_sport: SubSport::IndoorCycling,
            samples: (0..600).map(|t| fit::Sample { t, distance_m: Some(0.0), ..fit::Sample::default() }).collect(),
            lap_starts: vec![],
            devices: vec![],
        };
        assert!(matches!(import_ride(&conn, &vault, &silent, None).unwrap(), Finished::Empty));
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM activity", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn any_one_value_makes_a_ride() {
        let with = |sample: fit::Sample| Ride {
            start_unix: START,
            sub_sport: SubSport::IndoorCycling,
            samples: vec![fit::Sample::default(), sample],
            lap_starts: vec![],
            devices: vec![],
        };
        let one = |f: fn(&mut fit::Sample)| {
            let mut s = fit::Sample { t: 1, ..fit::Sample::default() };
            f(&mut s);
            has_values(&with(s))
        };
        assert!(one(|s| s.watts = Some(120)));
        assert!(one(|s| s.cadence_rpm = Some(80)));
        assert!(one(|s| s.heart_rate = Some(110)));
        assert!(one(|s| s.speed_mps = Some(8.0)), "a trainer that sends only speed still rode");
        assert!(!one(|s| s.distance_m = Some(0.0)));
    }

    #[test]
    fn a_failed_import_keeps_the_journal() {
        let vault = ScratchDir::new("ride_failed");
        let conn = crate::db::test_db();
        let (ride, journal, _) = ten_minutes(&vault, None, SubSport::IndoorCycling).finish(Duration::from_secs(600));
        // The vault's raw/ is a file: the original cannot be stored.
        std::fs::write(vault.join("raw"), "not a directory").unwrap();
        let e = finish(&conn, &vault, &ride, journal.as_deref(), None).unwrap_err();
        assert!(matches!(e, FinishError::Failed(_)), "{e}");
        assert!(journal.is_some_and(|p| p.exists()));
    }

    #[test]
    fn a_journal_already_gone_or_one_that_cannot_go() {
        let vault = ScratchDir::new("ride_journal_gone");
        let conn = crate::db::test_db();
        let (ride, journal, _) = ten_minutes(&vault, None, SubSport::IndoorCycling).finish(Duration::from_secs(600));
        let journal = journal.expect("journal");
        std::fs::remove_file(&journal).unwrap(); // someone removed it already
        assert!(finish(&conn, &vault, &ride, Some(&journal), None).is_ok());

        // A "journal" that is a directory cannot be removed: the ride is in, and it says so.
        let other = Ride { start_unix: START + 1, ..ride };
        let dir = vault.join("rides").join("stuck.journal");
        std::fs::create_dir_all(&dir).unwrap();
        let e = finish(&conn, &vault, &other, Some(&dir), None).unwrap_err();
        assert!(e.to_string().contains("the ride is in"), "{e}");
    }
}
