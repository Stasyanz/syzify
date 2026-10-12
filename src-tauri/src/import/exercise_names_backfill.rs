//! One-time repair of the exercise a strength set names (#213).
//!
//! The parser used to store `category` and `category_subtype` as the debug
//! dump of the FIT arrays they arrive in (`[UInt16(14), UInt16(14),
//! UInt16(65534)]`), and — for a file that carries single values — the
//! subtype as a bare number and padding as the category "unknown". Every
//! activity whose sets say anything about an exercise has its FIT file
//! decoded again with `parser::fit::exercise_sets_of_bytes`, and the sets
//! whose exercise now reads differently are rewritten by set number.
//! Nothing else about a set changes, and no set is added or removed.
//!
//! Sets are matched by position, so a file is trusted only when it decodes
//! to the very sets the activity holds — the same count, and each set with
//! the same type, duration, repetitions and weight. An activity can own a
//! second FIT file (a near-duplicate attached at import) whose sets are not
//! these.
//!
//! Same shape as the source-device backfill (#144): files are read and
//! decoded WITHOUT the DB lock, only the writes take it, per chunk. Unlike
//! it, the pass is done only when no dump is left in the vault: a dump whose
//! file could not be read this time — missing, or encrypted with the key not
//! loaded — is tried again at the next launch instead of staying for good.
//! (The single-value shape leaves no such trace; a file of that shape that
//! cannot be read keeps its old reading once the dumps are gone.)

use std::collections::HashMap;

use rusqlite::Connection;

use crate::db;
use crate::models::exercise_set::ExerciseSet;
use crate::models::raw_file::ActivityFitFile;
use crate::state::AppState;

pub const FLAG: &str = "exercise_names_backfilled_v1";
const CHUNK: usize = 50;

/// The exercise of one set: its number, category and subtype.
pub type SetExercise = (i32, Option<String>, Option<String>);
/// One activity → the sets to rename.
pub type NamesUpdate = (String, Vec<SetExercise>);

/// One activity to look at: the sets it holds, in set order, and its FIT
/// files, earliest import first.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub activity_id: String,
    pub stored: Vec<ExerciseSet>,
    pub files: Vec<String>,
}

/// Pair each activity's stored sets with its FIT files, keeping the order
/// of the file listing. An activity with no FIT file keeps an empty list:
/// there is nothing to read its sets from.
pub fn candidates(
    stored: Vec<(String, Vec<ExerciseSet>)>,
    files: &[ActivityFitFile],
) -> Vec<Candidate> {
    let mut by_activity: HashMap<&str, Vec<String>> = HashMap::new();
    for f in files {
        by_activity.entry(&f.activity_id).or_default().push(f.path_in_vault.clone());
    }
    stored
        .into_iter()
        .map(|(activity_id, stored)| Candidate {
            files: by_activity.remove(activity_id.as_str()).unwrap_or_default(),
            activity_id,
            stored,
        })
        .collect()
}

/// Whether a decoded set is the stored one, by everything a rename leaves
/// alone. Not `start_time`: it is formatted in the zone the machine was in
/// when the file was decoded, so the same set reads differently after a
/// trip.
fn same_set(decoded: &ExerciseSet, stored: &ExerciseSet) -> bool {
    decoded.set_number == stored.set_number
        && decoded.set_type == stored.set_type
        && decoded.duration_s == stored.duration_s
        && decoded.repetitions == stored.repetitions
        && decoded.weight_kg == stored.weight_kg
        && decoded.wkt_step_index == stored.wkt_step_index
}

/// Whether a file's sets are the activity's. Renaming by position is only
/// safe then.
fn same_sets(decoded: &[ExerciseSet], stored: &[ExerciseSet]) -> bool {
    decoded.len() == stored.len() && decoded.iter().zip(stored).all(|(d, s)| same_set(d, s))
}

/// Decide the updates: for each activity take the first of its files that
/// decodes to its very sets, and keep the sets whose exercise reads
/// differently from what is stored. A file that cannot be read or decoded,
/// or whose sets are not the activity's, changes nothing; an activity that
/// already reads right is not rewritten.
pub fn plan_updates(
    candidates: &[Candidate],
    read: impl Fn(&str) -> Option<Vec<u8>>,
    derive: impl Fn(&[u8], &str) -> Option<Vec<ExerciseSet>>,
) -> Vec<NamesUpdate> {
    candidates
        .iter()
        .filter_map(|c| {
            let decoded = c.files.iter().find_map(|path| {
                let sets = derive(&read(path)?, &c.activity_id)?;
                same_sets(&sets, &c.stored).then_some(sets)
            })?;
            let changed: Vec<SetExercise> = decoded
                .into_iter()
                .zip(&c.stored)
                .filter(|(new, old)| {
                    new.category != old.category || new.category_subtype != old.category_subtype
                })
                .map(|(new, _)| (new.set_number, new.category, new.category_subtype))
                .collect();
            (!changed.is_empty()).then(|| (c.activity_id.clone(), changed))
        })
        .collect()
}

/// Write one chunk of updates atomically. An activity deleted while its file
/// was being decoded has no sets left to rename: the UPDATE matches nothing.
pub fn apply(conn: &Connection, updates: &[NamesUpdate]) -> rusqlite::Result<()> {
    let tx = conn.unchecked_transaction()?;
    for (id, exercises) in updates {
        for (set_number, category, subtype) in exercises {
            db::exercise_sets::set_exercise(
                &tx,
                id,
                *set_number,
                category.as_deref(),
                subtype.as_deref(),
            )?;
        }
    }
    tx.commit()
}

/// What the vault holds to look at, read under the lock.
fn list_candidates(conn: &Connection) -> rusqlite::Result<Vec<Candidate>> {
    let stored = db::exercise_sets::activities_naming_exercises(conn)?
        .into_iter()
        .map(|id| {
            let sets = db::exercise_sets::get_exercise_sets(conn, &id)?;
            Ok((id, sets))
        })
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if stored.is_empty() {
        return Ok(Vec::new());
    }
    Ok(candidates(stored, &db::raw_files::activity_fit_files(conn)?))
}

/// The startup entry point: list under the lock, read and decode without it,
/// write per chunk, mark done once no dump is left.
pub fn run(state: &AppState) {
    let candidates = {
        let conn = match state.db.lock() {
            Ok(c) => c,
            Err(_) => return,
        };
        match db::settings::get_setting(&conn, FLAG) {
            Ok(None) => {}
            Ok(Some(_)) => return,
            Err(e) => {
                eprintln!("Failed to read settings for exercise-names backfill: {}", e);
                return;
            }
        }
        match list_candidates(&conn) {
            Ok(candidates) => candidates,
            Err(e) => {
                eprintln!("Exercise-names backfill failed to list activities: {}", e);
                return;
            }
        }
    };

    if !candidates.is_empty() {
        let key = match state.encryption_key_for(|s| s.activities) {
            Ok(key) => key,
            Err(e) => {
                eprintln!("Exercise-names backfill could not read the vault lock: {}", e);
                return;
            }
        };
        // An encrypted file without its key is simply unreadable for now:
        // its dump stays, so the pass is not marked done and comes back.
        let read = |path: &str| -> Option<Vec<u8>> {
            let full = state.vault_path.join(path);
            if path.ends_with(".enc") {
                key.as_ref()
                    .and_then(|k| crate::crypto::decrypt_file_to_memory(k, &full).ok())
            } else {
                std::fs::read(&full).ok()
            }
        };
        let updates = plan_updates(&candidates, read, crate::parser::fit::exercise_sets_of_bytes);

        for chunk in updates.chunks(CHUNK) {
            let conn = match state.db.lock() {
                Ok(c) => c,
                Err(_) => return,
            };
            if let Err(e) = apply(&conn, chunk) {
                eprintln!("Exercise-names backfill failed (will retry next launch): {}", e);
                return;
            }
        }
    }

    let conn = match state.db.lock() {
        Ok(c) => c,
        Err(_) => return,
    };
    // Done means repaired, not attempted: a dump still in the vault — its
    // file missing, unreadable this time, or holding other sets — keeps
    // the flag unset so the next launch looks again.
    match db::exercise_sets::activities_with_undecoded_exercises(&conn) {
        Ok(left) if left.is_empty() => {
            if let Err(e) = db::settings::set_setting(&conn, FLAG, "1") {
                eprintln!("Failed to mark exercise-names backfill done: {}", e);
            }
        }
        Ok(left) => eprintln!(
            "Exercise-names backfill: {} activities still hold an undecoded exercise (will retry next launch)",
            left.len()
        ),
        Err(e) => eprintln!("Exercise-names backfill failed to verify its result: {}", e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::fit_builder::strength_fixture;

    const DUMP: &str = "[UInt16(14), UInt16(14), UInt16(65534)]";

    fn file(id: &str, path: &str) -> ActivityFitFile {
        ActivityFitFile { activity_id: id.into(), path_in_vault: path.into(), source_device: None }
    }

    fn ex(number: i32, category: Option<&str>, subtype: Option<&str>) -> SetExercise {
        (number, category.map(str::to_string), subtype.map(str::to_string))
    }

    /// A set as `strength_fixture` writes its n-th: 9 + n repetitions and
    /// nothing else — so a stored set built here is the one the fixture
    /// decodes to, whatever exercise either names.
    fn set(id: &str, number: i32, category: Option<&str>, subtype: Option<&str>) -> ExerciseSet {
        ExerciseSet {
            id: None,
            activity_id: id.into(),
            set_number: number,
            start_time: None,
            category: category.map(str::to_string),
            category_subtype: subtype.map(str::to_string),
            set_type: None,
            duration_s: None,
            repetitions: Some(9 + number),
            weight_kg: None,
            wkt_step_index: None,
        }
    }

    fn candidate(id: &str, stored: Vec<ExerciseSet>, files: &[&str]) -> Candidate {
        Candidate {
            activity_id: id.into(),
            stored,
            files: files.iter().map(|f| f.to_string()).collect(),
        }
    }

    #[test]
    fn candidates_pair_the_stored_sets_with_the_files_in_listing_order() {
        let files = vec![
            file("a", "raw/a-early.fit"),
            file("a", "raw/a-late.fit"),
            file("other", "raw/other.fit"),
        ];
        let stored = vec![
            ("a".to_string(), vec![set("a", 1, Some(DUMP), None)]),
            ("no-file".to_string(), vec![set("no-file", 1, Some(DUMP), None)]),
        ];
        assert_eq!(
            candidates(stored, &files),
            vec![
                candidate("a", vec![set("a", 1, Some(DUMP), None)], &["raw/a-early.fit", "raw/a-late.fit"]),
                candidate("no-file", vec![set("no-file", 1, Some(DUMP), None)], &[]),
            ]
        );
    }

    /// The planner renames what reads differently and nothing else: sets
    /// that already read right are left out, an activity that reads right
    /// throughout is not planned at all, and an unreadable or undecodable
    /// file changes nothing.
    #[test]
    fn plans_only_the_sets_that_read_differently() {
        let cands = vec![
            // The array dump, and beside it a set that names nothing.
            candidate("dump", vec![set("dump", 1, Some(DUMP), None), set("dump", 2, None, None)], &["raw/dump.fit"]),
            // The single-value shape: a bare number for the subtype, and
            // padding stored as the category "unknown".
            candidate(
                "scalar",
                vec![set("scalar", 1, Some("lateral_raise"), Some("24")), set("scalar", 2, Some("unknown"), Some("5"))],
                &["raw/scalar.fit"],
            ),
            candidate("right", vec![set("right", 1, Some("curl"), None)], &["raw/right.fit"]),
            candidate("unreadable", vec![set("unreadable", 1, Some(DUMP), None)], &["raw/gone.fit.enc"]),
            candidate("undecodable", vec![set("undecodable", 1, Some(DUMP), None)], &["raw/bad.fit"]),
            candidate("no-file", vec![set("no-file", 1, Some(DUMP), None)], &[]),
        ];
        let read = |p: &str| (p != "raw/gone.fit.enc").then(|| p.as_bytes().to_vec());
        let derive = |bytes: &[u8], id: &str| match bytes {
            b"raw/dump.fit" => Some(vec![set(id, 1, Some("lateral_raise"), None), set(id, 2, None, None)]),
            b"raw/scalar.fit" => Some(vec![
                set(id, 1, Some("lateral_raise"), Some("seated_lateral_raise")),
                set(id, 2, None, None),
            ]),
            b"raw/right.fit" => Some(vec![set(id, 1, Some("curl"), None)]),
            b"raw/bad.fit" => None,
            other => panic!("unexpected read {:?}", std::str::from_utf8(other)),
        };
        assert_eq!(
            plan_updates(&cands, read, derive),
            vec![
                ("dump".to_string(), vec![ex(1, Some("lateral_raise"), None)]),
                (
                    "scalar".to_string(),
                    vec![ex(1, Some("lateral_raise"), Some("seated_lateral_raise")), ex(2, None, None)]
                ),
            ]
        );
    }

    /// Sets are renamed by position, so a file is trusted only when it
    /// decodes to the activity's very sets. A near-duplicate FIT file
    /// attached to the same activity may be listed first and hold other
    /// sets — fewer, more, none, or the same number of different ones: it
    /// is passed over for the file that lines up, and with no such file
    /// nothing is written.
    #[test]
    fn a_file_whose_sets_are_not_the_activitys_is_passed_over() {
        let stored = vec![set("a", 1, Some(DUMP), None), set("a", 2, Some(DUMP), None)];
        let other = |id: &str, number: i32, change: fn(&mut ExerciseSet)| {
            let mut s = set(id, number, Some("squat"), None);
            change(&mut s);
            s
        };
        let derive = move |bytes: &[u8], id: &str| match bytes {
            b"raw/one-set.fit" => Some(vec![set(id, 1, Some("squat"), None)]),
            b"raw/three-sets.fit" => Some(vec![
                set(id, 1, Some("squat"), None),
                set(id, 2, Some("squat"), None),
                set(id, 3, Some("squat"), None),
            ]),
            b"raw/no-sets.fit" => Some(vec![]),
            // The same number of sets, each differing in one thing a rename
            // never touches.
            b"raw/other-reps.fit" => Some(vec![set(id, 1, Some("squat"), None), other(id, 2, |s| s.repetitions = Some(99))]),
            b"raw/other-weight.fit" => Some(vec![other(id, 1, |s| s.weight_kg = Some(40.0)), set(id, 2, Some("squat"), None)]),
            b"raw/other-duration.fit" => Some(vec![other(id, 1, |s| s.duration_s = Some(45.0)), set(id, 2, Some("squat"), None)]),
            b"raw/other-type.fit" => Some(vec![other(id, 1, |s| s.set_type = Some("rest".into())), set(id, 2, Some("squat"), None)]),
            b"raw/other-step.fit" => Some(vec![other(id, 1, |s| s.wkt_step_index = Some(3)), set(id, 2, Some("squat"), None)]),
            b"raw/other-numbers.fit" => Some(vec![set(id, 1, Some("squat"), None), other(id, 2, |s| s.set_number = 3)]),
            b"raw/its-own.fit" => Some(vec![set(id, 1, Some("lateral_raise"), None), set(id, 2, Some("row"), None)]),
            unexpected => panic!("unexpected read {:?}", std::str::from_utf8(unexpected)),
        };
        let read = |p: &str| Some(p.as_bytes().to_vec());
        let strangers = [
            "raw/one-set.fit",
            "raw/three-sets.fit",
            "raw/no-sets.fit",
            "raw/other-reps.fit",
            "raw/other-weight.fit",
            "raw/other-duration.fit",
            "raw/other-type.fit",
            "raw/other-step.fit",
            "raw/other-numbers.fit",
        ];

        let mut with_its_own = strangers.to_vec();
        with_its_own.push("raw/its-own.fit");
        assert_eq!(
            plan_updates(&[candidate("a", stored.clone(), &with_its_own)], read, derive),
            vec![("a".to_string(), vec![ex(1, Some("lateral_raise"), None), ex(2, Some("row"), None)])]
        );
        // Each stranger alone: nothing is written.
        for stranger in strangers {
            assert_eq!(
                plan_updates(&[candidate("a", stored.clone(), &[stranger])], read, derive),
                vec![],
                "{stranger} must not rename the activity's sets"
            );
        }
        // A decode on another day reads the same set under another time
        // zone: the start time is not what a set is recognised by.
        let mut moved = stored.clone();
        moved[0].start_time = Some("2025-06-01T11:00:00+03:00".into());
        assert_eq!(plan_updates(&[candidate("a", moved, &["raw/its-own.fit"])], read, derive).len(), 1);
    }

    #[test]
    fn apply_renames_by_set_number_and_keeps_the_rest_of_the_set() {
        let conn = db::test_db();
        for id in ["x", "y"] {
            db::activities::insert_activity(
                &conn,
                &crate::models::activity::Activity {
                    id: id.into(),
                    start_time: "2025-06-01T08:00:00+00:00".into(),
                    sport_type: "strength".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        }
        db::exercise_sets::insert_exercise_sets(
            &conn,
            &[set("x", 1, Some(DUMP), None), set("x", 2, Some(DUMP), None), set("y", 1, Some(DUMP), None)],
        )
        .unwrap();

        apply(
            &conn,
            &[
                ("x".into(), vec![ex(1, Some("lateral_raise"), None), ex(2, None, None)]),
                // Deleted while its file was being decoded: nothing to rename.
                ("gone".into(), vec![ex(1, Some("curl"), None)]),
            ],
        )
        .unwrap();

        let x = db::exercise_sets::get_exercise_sets(&conn, "x").unwrap();
        assert_eq!(x.len(), 2);
        assert_eq!(x[0].category.as_deref(), Some("lateral_raise"));
        assert_eq!(x[1].category, None);
        assert_eq!((x[0].repetitions, x[1].repetitions), (Some(10), Some(11)));
        // An activity outside the update keeps what it had.
        let y = db::exercise_sets::get_exercise_sets(&conn, "y").unwrap();
        assert_eq!(y[0].category.as_deref(), Some(DUMP));
        assert_eq!(
            db::exercise_sets::activities_with_undecoded_exercises(&conn).unwrap(),
            vec!["y".to_string()]
        );
    }

    fn test_state(vault: &std::path::Path) -> AppState {
        use std::sync::{Arc, Mutex};
        AppState {
            db: Arc::new(Mutex::new(crate::db::test_db())),
            vault_path: vault.to_path_buf(),
            encryption_key: Mutex::new(None),
            watcher_handle: Mutex::new(None),
            db_locked: Mutex::new(false),
            vault_error: Mutex::new(None),
            services_started: Mutex::new(false),
            geocoding_flight: crate::state::SingleFlight::default(),
            vault_flight: crate::state::SingleFlight::default(),
        }
    }

    fn flag_of(state: &AppState) -> Option<String> {
        let conn = state.db.lock().unwrap();
        db::settings::get_setting(&conn, FLAG).unwrap()
    }

    /// A strength activity as an import before #213 left it: the sets as
    /// given, and one FIT file row per path, in the order given.
    fn stored_activity(state: &AppState, id: &str, sets: &[ExerciseSet], paths: &[&str]) {
        let conn = state.db.lock().unwrap();
        conn.execute(
            "INSERT INTO activity (id, start_time, sport_type) VALUES (?1, '2026-01-01T08:00:00+03:00', 'strength')",
            [id],
        )
        .unwrap();
        db::exercise_sets::insert_exercise_sets(&conn, sets).unwrap();
        for (i, path) in paths.iter().enumerate() {
            conn.execute(
                "INSERT INTO raw_file (id, activity_id, path_in_vault, original_path, format, hash_sha256, imported_at) \
                 VALUES (?1, ?2, ?3, ?3, 'fit', ?1, ?4)",
                rusqlite::params![format!("raw-{id}-{i}"), id, path, format!("2026-01-01 08:00:0{i}")],
            )
            .unwrap();
        }
    }

    fn write(state: &AppState, path: &str, bytes: &[u8]) {
        let full = state.vault_path.join(path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, bytes).unwrap();
    }

    fn exercises(state: &AppState, id: &str) -> Vec<SetExercise> {
        let conn = state.db.lock().unwrap();
        db::exercise_sets::get_exercise_sets(&conn, id)
            .unwrap()
            .into_iter()
            .map(|s| (s.set_number, s.category, s.category_subtype))
            .collect()
    }

    /// `run` on a vault, through the real decoder: the dumps of a fenix
    /// file become names, a set that names nothing loses its dump, the rest
    /// of every set stays, and the pass marks itself done and does not run
    /// again.
    #[test]
    fn run_renames_the_dumps_from_the_stored_files_once() {
        let vault = crate::test_support::ScratchDir::new("exnames");
        let state = test_state(&vault);
        let padding = "[UInt16(65534), UInt16(65534), UInt16(65534)]";
        stored_activity(
            &state,
            "a",
            &[
                set("a", 1, Some(DUMP), None),
                set("a", 2, Some(padding), None),
                set("a", 3, Some("[UInt16(0), UInt16(65534), UInt16(65534)]"), Some("[UInt16(1), UInt16(65534), UInt16(65534)]")),
            ],
            &["raw/a.fit"],
        );
        write(
            &state,
            "raw/a.fit",
            &strength_fixture(&[
                ([14, 14, 65534], None),
                ([65534, 65534, 65534], None),
                ([0, 65534, 65534], Some([1, 65534, 65534])),
            ]),
        );
        // A strength activity that already reads right is not rewritten —
        // its file does not even have to be there.
        stored_activity(&state, "fine", &[set("fine", 1, Some("curl"), None)], &["raw/missing.fit"]);

        run(&state);

        assert_eq!(
            exercises(&state, "a"),
            vec![
                ex(1, Some("lateral_raise"), None),
                ex(2, None, None),
                ex(3, Some("bench_press"), Some("barbell_bench_press")),
            ]
        );
        assert_eq!(exercises(&state, "fine"), vec![ex(1, Some("curl"), None)]);
        {
            let conn = state.db.lock().unwrap();
            let sets = db::exercise_sets::get_exercise_sets(&conn, "a").unwrap();
            assert_eq!(
                sets.iter().map(|s| s.repetitions).collect::<Vec<_>>(),
                vec![Some(10), Some(11), Some(12)],
                "nothing but the exercise is rewritten"
            );
        }
        assert_eq!(flag_of(&state).as_deref(), Some("1"));

        // Done is done: a dump that appears later is not this pass's.
        {
            let conn = state.db.lock().unwrap();
            db::exercise_sets::set_exercise(&conn, "a", 1, Some(DUMP), None).unwrap();
        }
        run(&state);
        assert_eq!(exercises(&state, "a")[0], ex(1, Some(DUMP), None));
    }

    /// Done means repaired: while a dump is left the flag stays unset and
    /// the next launch tries again — here the right file turns up last.
    #[test]
    fn run_is_not_done_while_a_dump_is_left() {
        let vault = crate::test_support::ScratchDir::new("exnames-left");
        let state = test_state(&vault);
        stored_activity(&state, "a", &[set("a", 1, Some(DUMP), None)], &["raw/a.fit"]);

        run(&state);
        assert_eq!(flag_of(&state), None, "the file is missing: not done");
        assert_eq!(exercises(&state, "a"), vec![ex(1, Some(DUMP), None)]);

        // A file that decodes but holds other sets is no better.
        write(&state, "raw/a.fit", &strength_fixture(&[([0, 65534, 65534], None), ([7, 65534, 65534], None)]));
        run(&state);
        assert_eq!(flag_of(&state), None);
        assert_eq!(exercises(&state, "a"), vec![ex(1, Some(DUMP), None)], "not renamed from a stranger's sets");

        write(&state, "raw/a.fit", &strength_fixture(&[([14, 14, 65534], None)]));
        run(&state);
        assert_eq!(exercises(&state, "a"), vec![ex(1, Some("lateral_raise"), None)]);
        assert_eq!(flag_of(&state).as_deref(), Some("1"));
    }

    /// One set against one set: the count alone would let a stranger's file
    /// through. The activity's set has 12 repetitions, the file's has 10.
    #[test]
    fn run_does_not_rename_from_a_file_with_as_many_but_different_sets() {
        let vault = crate::test_support::ScratchDir::new("exnames-same-count");
        let state = test_state(&vault);
        let mut stored = set("a", 1, Some(DUMP), None);
        stored.repetitions = Some(12);
        stored_activity(&state, "a", &[stored], &["raw/a.fit"]);
        write(&state, "raw/a.fit", &strength_fixture(&[([0, 65534, 65534], None)]));

        run(&state);
        assert_eq!(exercises(&state, "a"), vec![ex(1, Some(DUMP), None)]);
        assert_eq!(flag_of(&state), None);
    }

    /// The second FIT file of an activity is read when the first is not the
    /// one its sets came from.
    #[test]
    fn run_reads_the_file_the_sets_came_from() {
        let vault = crate::test_support::ScratchDir::new("exnames-two");
        let state = test_state(&vault);
        stored_activity(&state, "a", &[set("a", 1, Some(DUMP), None)], &["raw/dup.fit", "raw/own.fit"]);
        write(&state, "raw/dup.fit", &strength_fixture(&[]));
        write(&state, "raw/own.fit", &strength_fixture(&[([14, 14, 65534], None)]));

        run(&state);
        assert_eq!(exercises(&state, "a"), vec![ex(1, Some("lateral_raise"), None)]);
        assert_eq!(flag_of(&state).as_deref(), Some("1"));
    }

    /// Nothing to look at marks the pass done at once.
    #[test]
    fn run_is_done_at_once_on_a_vault_without_exercises() {
        let vault = crate::test_support::ScratchDir::new("exnames-none");
        let state = test_state(&vault);
        // Sets that say nothing about an exercise are not candidates.
        stored_activity(&state, "plain", &[set("plain", 1, None, None)], &["raw/plain.fit"]);
        run(&state);
        assert_eq!(flag_of(&state).as_deref(), Some("1"));
    }

    /// Encrypted activities: without the key the encrypted file is merely
    /// unreadable — its dump stays and the pass is not done, while a
    /// readable file beside it is repaired meanwhile. With the key loaded
    /// the file is decrypted in memory and its sets renamed.
    #[test]
    fn run_reads_encrypted_files_once_the_key_is_there() {
        let vault = crate::test_support::ScratchDir::new("exnames-enc");
        let state = test_state(&vault);
        let key = [7u8; 32];
        let (verifier, nonce) = crate::crypto::create_verifier(&key).unwrap();
        crate::crypto::write_vault_lock(
            &vault,
            &crate::crypto::VaultLock {
                salt: hex::encode([1u8; 32]),
                verifier: hex::encode(verifier),
                nonce: hex::encode(nonce),
                created_at: "2026-01-01T00:00:00Z".into(),
                scopes: crate::crypto::EncryptionScopes { activities: true, database: false, photos: false },
            },
        )
        .unwrap();
        stored_activity(&state, "e", &[set("e", 1, Some(DUMP), None)], &["raw/e.fit.enc"]);
        write(&state, "raw/e.fit", &strength_fixture(&[([14, 14, 65534], None)]));
        crate::crypto::encrypt_file(&key, &vault.join("raw/e.fit")).unwrap();
        stored_activity(&state, "plain", &[set("plain", 1, Some(DUMP), None)], &["raw/plain.fit"]);
        write(&state, "raw/plain.fit", &strength_fixture(&[([7, 65534, 65534], None)]));

        run(&state);
        assert_eq!(exercises(&state, "e"), vec![ex(1, Some(DUMP), None)], "locked: left as it is");
        assert_eq!(exercises(&state, "plain"), vec![ex(1, Some("curl"), None)]);
        assert_eq!(flag_of(&state), None, "not done while the encrypted one waits");

        *state.encryption_key.lock().unwrap() = Some(key);
        run(&state);
        assert_eq!(exercises(&state, "e"), vec![ex(1, Some("lateral_raise"), None)]);
        assert_eq!(flag_of(&state).as_deref(), Some("1"));
    }
}
