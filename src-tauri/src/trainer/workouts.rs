//! The workouts folder: `.zwo` files the user keeps (Zwift's own folder
//! works: `Documents/Zwift/Workouts/<id>`). Tauri-free.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use super::zwo::{self, MAX_BYTES};
use crate::error::CommandError;
use crate::models::training::{WorkoutFile, WorkoutProblem as Problem};

/// The setting that holds the folder.
pub const DIR_SETTING: &str = "training.workouts_dir";
/// Folders this deep below it are looked in (Zwift keeps one per user).
const MAX_DEPTH: usize = 3;
/// More files than this are not listed.
const MAX_FILES: usize = 2000;

fn is_zwo(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("zwo"))
}

/// The `.zwo` files under `dir`, by path; hidden entries are left out.
fn files(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut folders = vec![(dir.to_path_buf(), 0)];
    while let Some((folder, depth)) = folders.pop() {
        let Ok(entries) = fs::read_dir(&folder) else { continue };
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            // Links are not followed: a folder may not lead out of itself.
            let Ok(kind) = entry.file_type() else { continue };
            let path = entry.path();
            if kind.is_dir() && depth < MAX_DEPTH {
                folders.push((path, depth + 1));
            } else if kind.is_file() && is_zwo(&path) {
                found.push(path);
            }
        }
    }
    found.sort();
    found.truncate(MAX_FILES);
    found
}

/// Reads one file, at most a byte more than `MAX_BYTES` of it (to tell a
/// file too big).
fn read(path: &Path) -> Result<zwo::Plan, Problem> {
    let unreadable = |e: std::io::Error| Problem::Unreadable { detail: e.to_string() };
    let mut bytes = Vec::new();
    let file = fs::File::open(path).map_err(unreadable)?;
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes).map_err(unreadable)?;
    zwo::parse_bytes(&bytes)
}

/// The file's name without `.zwo`.
fn stem(path: &Path) -> String {
    path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Every workout in `dir`, each read (a broken one says why).
pub fn list(dir: &Path) -> Vec<WorkoutFile> {
    files(dir)
        .into_iter()
        .map(|path| {
            let relative = path.strip_prefix(dir).unwrap_or(&path).components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
            match read(&path) {
                Ok(plan) => WorkoutFile {
                    path: relative,
                    name: plan.name.clone().unwrap_or_else(|| stem(&path)),
                    duration_s: plan.duration(),
                    profile: zwo::profile(&plan.segments),
                    notes: plan.notes,
                    problem: None,
                },
                Err(problem) => WorkoutFile {
                    path: relative,
                    name: stem(&path),
                    duration_s: 0,
                    profile: Vec::new(),
                    notes: Vec::new(),
                    problem: Some(problem),
                },
            }
        })
        .collect()
}

/// The workout at `relative` in `dir`, and the name it goes by. It must
/// be a `.zwo` file inside the folder (no `..`, no link out of it).
pub fn load(dir: &Path, relative: &str) -> Result<(zwo::Plan, String), CommandError> {
    let gone = || CommandError::coded("training.noSuchWorkout", "That workout is not in the folder");
    let root = dir.canonicalize().map_err(|_| gone())?;
    let path = root.join(relative).canonicalize().map_err(|_| gone())?;
    if !path.starts_with(&root) || !path.is_file() || !is_zwo(&path) {
        return Err(gone());
    }
    let plan = read(&path).map_err(|_| CommandError::coded("training.workoutBroken", "That workout cannot be ridden any more"))?;
    let name = plan.name.clone().unwrap_or_else(|| stem(&path));
    Ok((plan, name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::ScratchDir;

    const STEADY: &str = r#"<workout_file><name>Steady</name><workout><SteadyState Duration="600" Power="0.7"/></workout></workout_file>"#;

    fn write(dir: &Path, relative: &str, text: &str) {
        let path = dir.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    #[test]
    fn the_folder_lists_its_workouts_broken_ones_too() {
        let dir = ScratchDir::new("workouts_list");
        write(dir.path(), "b.zwo", STEADY);
        write(dir.path(), "user/a.ZWO", "<workout_file><workout><FreeRide Duration='60'/></workout></workout_file>");
        write(dir.path(), "broken.zwo", "<workout_file>");
        write(dir.path(), "notes.txt", STEADY);
        write(dir.path(), ".hidden.zwo", STEADY);
        write(dir.path(), "1/2/3/deep.zwo", STEADY);
        write(dir.path(), "1/2/3/4/too-deep.zwo", STEADY);
        let list = list(dir.path());
        let paths: Vec<&str> = list.iter().map(|w| w.path.as_str()).collect();
        assert_eq!(paths, vec!["1/2/3/deep.zwo", "b.zwo", "broken.zwo", "user/a.ZWO"]);
        assert_eq!((list[1].name.as_str(), list[1].duration_s, list[1].profile.len()), ("Steady", 600, 1));
        assert_eq!(list[3].name, "a", "no <name>: the file's");
        assert!(matches!(list[2].problem, Some(Problem::Unreadable { .. })));
        assert_eq!(list[2].name, "broken");
        assert!(super::list(&dir.path().join("missing")).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn a_file_that_cannot_be_opened_says_why() {
        use std::os::unix::fs::PermissionsExt;
        let dir = ScratchDir::new("workouts_closed");
        write(dir.path(), "closed.zwo", STEADY);
        fs::set_permissions(dir.path().join("closed.zwo"), fs::Permissions::from_mode(0o000)).unwrap();
        let listed = list(dir.path());
        fs::set_permissions(dir.path().join("closed.zwo"), fs::Permissions::from_mode(0o644)).unwrap();
        assert!(matches!(&listed[0].problem, Some(Problem::Unreadable { detail }) if !detail.is_empty()), "{:?}", listed[0].problem);
    }

    #[test]
    fn a_long_workout_is_listed_with_a_profile_to_draw_not_its_intervals() {
        let dir = ScratchDir::new("workouts_profile");
        let intervals = r#"<SteadyState Duration="9" Power="0.5"/>"#.repeat(130);
        write(dir.path(), "long.zwo", &format!("<workout_file><workout>{intervals}</workout></workout_file>"));
        let listed = list(dir.path());
        assert_eq!((listed[0].duration_s, listed[0].profile.len()), (1170, zwo::PROFILE_POINTS));
    }

    #[test]
    fn a_file_too_big_is_said_so_and_a_folder_lists_a_few_thousand_at_most() {
        let dir = ScratchDir::new("workouts_big");
        // A workout, then a megabyte of anything: not read whole.
        let big = format!("{STEADY}{}", " ".repeat(MAX_BYTES as usize));
        write(dir.path(), "big.zwo", &big);
        assert_eq!(list(dir.path())[0].problem, Some(Problem::TooBig));
        let many = ScratchDir::new("workouts_many");
        for i in 0..=MAX_FILES {
            write(many.path(), &format!("{i:05}.zwo"), "");
        }
        let listed = list(many.path());
        assert_eq!(listed.len(), MAX_FILES);
        assert_eq!(listed.last().map(|w| w.path.as_str()), Some("01999.zwo"));
    }

    #[test]
    fn a_workout_is_loaded_only_from_inside_the_folder() {
        let dir = ScratchDir::new("workouts_load");
        let inside = dir.path().join("inside");
        write(&inside, "user/steady.zwo", STEADY);
        write(&inside, "nameless.zwo", "<workout_file><workout><FreeRide Duration='60'/></workout></workout_file>");
        write(&inside, "broken.zwo", "<workout_file>");
        write(&inside, "plain.txt", STEADY);
        write(dir.path(), "outside.zwo", STEADY);
        let (plan, name) = load(&inside, "user/steady.zwo").unwrap();
        assert_eq!((plan.duration(), name.as_str()), (600, "Steady"));
        assert_eq!(load(&inside, "nameless.zwo").unwrap().1, "nameless");
        let code = |r: Result<(zwo::Plan, String), CommandError>| r.unwrap_err().code;
        assert_eq!(code(load(&inside, "../outside.zwo")), Some("training.noSuchWorkout"));
        assert_eq!(code(load(&inside, "missing.zwo")), Some("training.noSuchWorkout"));
        assert_eq!(code(load(&inside, "plain.txt")), Some("training.noSuchWorkout"));
        assert_eq!(code(load(&inside, "user")), Some("training.noSuchWorkout"));
        fs::create_dir_all(inside.join("folder.zwo")).unwrap();
        assert_eq!(code(load(&inside, "folder.zwo")), Some("training.noSuchWorkout"), "a folder, whatever its name");
        assert_eq!(code(load(&dir.path().join("nowhere"), "a.zwo")), Some("training.noSuchWorkout"));
        assert_eq!(code(load(&inside, "broken.zwo")), Some("training.workoutBroken"));
    }
}
