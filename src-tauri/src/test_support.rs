//! Scratch directories for tests.
//!
//! A test that touches the filesystem gets a directory of its own under the
//! system temp dir, named with a fresh UUID, and the guard removes it when
//! it goes out of scope — on a panic too, so an interrupted run leaves
//! nothing behind. Fixed names were the trap behind #132: a `.enc` from an
//! earlier run survived a silently ignored `remove_dir_all`, and the next
//! run's disable sweep tried to open it under a fresh key.

use std::ops::Deref;
use std::path::{Path, PathBuf};

/// An empty directory that exists for the life of the guard.
pub struct ScratchDir(PathBuf);

impl ScratchDir {
    /// A fresh directory `syz_<tag>_<uuid>` under the temp dir.
    pub fn new(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!("syz_{tag}_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&path).expect("create scratch dir");
        Self(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Deref for ScratchDir {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Path> for ScratchDir {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        match std::fs::remove_dir_all(&self.0) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            // A directory that will not go is a test leaving state behind:
            // fail loudly — unless the test is already failing, when a
            // second panic would abort the whole run.
            Err(e) if !std::thread::panicking() => {
                panic!("scratch dir {} was not removed: {e}", self.0.display())
            }
            Err(_) => {}
        }
    }
}

/// An unlocked plaintext-vault `AppState` over the given connection, for a
/// test that needs the state but neither a vault on disk nor an app.
pub fn app_state_on(conn: rusqlite::Connection) -> crate::state::AppState {
    use std::sync::{Arc, Mutex};
    crate::state::AppState {
        db: Arc::new(Mutex::new(conn)),
        vault_path: PathBuf::new(),
        encryption_key: Mutex::new(None),
        watcher_handle: Mutex::new(None),
        db_locked: Mutex::new(false),
        vault_error: Mutex::new(None),
        services_started: Mutex::new(false),
        geocoding_flight: crate::state::SingleFlight::default(),
        vault_flight: crate::state::SingleFlight::default(),
    }
}

/// A mock app that manages an unlocked plaintext-vault state on a fresh
/// migrated database with its vault at `vault`, for a test that calls a
/// command through its `State` argument: `list_gear(app.state())`.
pub fn mock_app_at(vault: &Path) -> tauri::App<tauri::test::MockRuntime> {
    use tauri::Manager;
    let mut state = app_state_on(crate::db::test_db());
    state.vault_path = vault.to_path_buf();
    let app = tauri::test::mock_builder()
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("mock app");
    app.manage(state);
    app
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_guard_is_its_own_fresh_directory() {
        let a = ScratchDir::new("t");
        let b = ScratchDir::new("t");
        assert_ne!(a.path(), b.path());
        assert!(a.is_dir() && b.is_dir());
        assert!(std::fs::read_dir(&a).unwrap().next().is_none(), "starts empty");
        assert!(a.file_name().unwrap().to_str().unwrap().starts_with("syz_t_"));
    }

    #[test]
    fn the_directory_goes_with_the_guard() {
        let dir = ScratchDir::new("drop");
        let path = dir.path().to_path_buf();
        std::fs::write(dir.join("nested.txt"), b"x").unwrap();
        drop(dir);
        assert!(!path.exists());
    }

    #[test]
    fn the_directory_goes_when_the_test_panics() {
        let path = std::sync::Mutex::new(PathBuf::new());
        let outcome = std::panic::catch_unwind(|| {
            let dir = ScratchDir::new("panic");
            *path.lock().unwrap() = dir.path().to_path_buf();
            std::fs::write(dir.join("left.enc"), b"ciphertext").unwrap();
            panic!("the test body fails");
        });
        assert!(outcome.is_err());
        let path = path.into_inner().unwrap();
        assert!(!path.exists(), "{} survived the panic", path.display());
    }

    #[test]
    fn an_already_removed_directory_is_fine() {
        let dir = ScratchDir::new("gone");
        std::fs::remove_dir_all(dir.path()).unwrap();
        drop(dir);
    }

    /// A test that is already failing keeps its own panic: a stuck
    /// directory must not raise a second one, which would abort the run.
    #[cfg(unix)]
    #[test]
    fn a_stuck_directory_stays_quiet_while_the_test_panics() {
        use std::os::unix::fs::PermissionsExt;
        let paths = std::sync::Mutex::new((PathBuf::new(), PathBuf::new()));
        let outcome = std::panic::catch_unwind(|| {
            let dir = ScratchDir::new("stuck_panic");
            let locked = dir.join("locked");
            std::fs::create_dir(&locked).unwrap();
            std::fs::write(locked.join("f"), b"x").unwrap();
            std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
            *paths.lock().unwrap() = (dir.path().to_path_buf(), locked);
            panic!("the test body fails first");
        });
        let (path, locked) = paths.into_inner().unwrap();
        let _ = std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755));
        let _ = std::fs::remove_dir_all(&path);
        let msg = outcome.unwrap_err();
        let msg = msg.downcast_ref::<&str>().copied().unwrap_or_default();
        assert_eq!(msg, "the test body fails first");
    }

    /// A directory the guard cannot remove fails the test instead of
    /// quietly piling up in the temp dir.
    #[cfg(unix)]
    #[test]
    fn a_directory_that_will_not_go_fails_loudly() {
        use std::os::unix::fs::PermissionsExt;
        let dir = ScratchDir::new("stuck");
        let locked = dir.join("locked");
        std::fs::create_dir(&locked).unwrap();
        std::fs::write(locked.join("f"), b"x").unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
        let path = dir.path().to_path_buf();
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(dir)));
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
        let _ = std::fs::remove_dir_all(&path);
        // root can delete anything: then there is nothing to be loud about.
        if outcome.is_ok() {
            assert!(!path.exists());
            return;
        }
        let msg = outcome.unwrap_err();
        let msg = msg.downcast_ref::<String>().cloned().unwrap_or_default();
        assert!(msg.contains("was not removed"), "{msg}");
    }
}
