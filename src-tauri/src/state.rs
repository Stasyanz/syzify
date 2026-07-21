use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use notify::RecommendedWatcher;
use rusqlite::Connection;

/// Shared DB handle. `Arc` so the plugin host can hold a clone without depending
/// on the Tauri `AppHandle` (keeps `plugins/` decoupled from Tauri & testable).
pub type Db = Arc<Mutex<Connection>>;

/// One-at-a-time gate for an operation that must not run concurrently with
/// itself (a geocoding pass, a backup). `try_begin` either claims the slot —
/// released when the returned guard drops, panics included — or returns None
/// while another holder is alive.
#[derive(Default)]
pub struct SingleFlight(AtomicBool);

impl SingleFlight {
    pub fn try_begin(&self) -> Option<SingleFlightGuard<'_>> {
        self.0
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()?;
        Some(SingleFlightGuard(&self.0))
    }
}

pub struct SingleFlightGuard<'a>(&'a AtomicBool);

impl Drop for SingleFlightGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

pub struct AppState {
    pub db: Db,
    pub vault_path: PathBuf,
    pub encryption_key: Mutex<Option<[u8; 32]>>,
    pub watcher_handle: Mutex<Option<RecommendedWatcher>>,
    /// True at boot when the `database` scope is encrypted: `db` holds an
    /// in-memory placeholder and background services are deferred until
    /// `unlock_vault` opens the real keyed database.
    pub db_locked: Mutex<bool>,
    /// Set when the vault couldn't be opened at boot (e.g. it lives in a
    /// macOS-protected folder and the app lacks Full Disk Access). `db` holds
    /// an in-memory placeholder; the frontend shows a recoverable error screen
    /// instead of the app crashing at startup.
    pub vault_error: Mutex<Option<String>>,
    /// Guards one-time startup of the DB-dependent background services. They
    /// start at boot for a plaintext vault, or after `unlock_vault` for an
    /// encrypted one — never before the key is loaded, and never twice.
    pub services_started: Mutex<bool>,
    /// Only one geocoding pass at a time: boot, post-import and the Settings
    /// toggle can all start one, and overlapping passes would multiply the
    /// request rate at Nominatim (their policy is 1 req/s) and re-send the
    /// same coordinates.
    pub geocoding_flight: SingleFlight,
    /// Only one vault-mutating operation at a time — backup, restore or
    /// relocation. Concurrent backups would share (and clobber) the same
    /// `.backup-snapshot` directory; a restore or relocation moves raw/ and
    /// photos/ out from under a running backup's directory walk.
    pub vault_flight: SingleFlight,
}

impl AppState {
    /// The in-memory vault key, but only when `scope` selects an enabled scope
    /// in vault.lock. Writers of new vault files must gate on this rather than
    /// on the raw key: encrypting a file outside its scope would strand it as
    /// ciphertext — disable_encryption discards the key forever, and nothing
    /// would ever decrypt an out-of-scope `.enc` back.
    pub fn encryption_key_for(
        &self,
        scope: impl Fn(&crate::crypto::EncryptionScopes) -> bool,
    ) -> Result<Option<[u8; 32]>, String> {
        let enabled = crate::crypto::read_vault_lock(&self.vault_path)?
            .map(|lock| scope(&lock.scopes))
            .unwrap_or(false);
        if !enabled {
            return Ok(None);
        }
        Ok(*self.encryption_key.lock().map_err(|e| e.to_string())?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn state_with(vault: &Path, key: Option<[u8; 32]>) -> AppState {
        AppState {
            db: Arc::new(Mutex::new(crate::db::test_db())),
            vault_path: vault.to_path_buf(),
            encryption_key: Mutex::new(key),
            watcher_handle: Mutex::new(None),
            db_locked: Mutex::new(false),
            vault_error: Mutex::new(None),
            services_started: Mutex::new(false),
            geocoding_flight: crate::state::SingleFlight::default(),
            vault_flight: crate::state::SingleFlight::default(),
        }
    }

    fn lock_with(activities: bool, photos: bool) -> crate::crypto::VaultLock {
        crate::crypto::VaultLock {
            salt: String::new(),
            verifier: String::new(),
            nonce: String::new(),
            created_at: String::new(),
            scopes: crate::crypto::EncryptionScopes { activities, database: false, photos },
        }
    }

    fn unique_dir(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("syz_state_{}_{}", tag, uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    /// The key gate: writers get the in-memory key only when their scope is
    /// enabled in vault.lock. Encrypting outside the scope would strand
    /// ciphertext — disable discards the key without decrypting such files.
    #[test]
    fn encryption_key_for_requires_matching_scope() {
        let key = [6u8; 32];

        // No vault.lock at all: no encryption, even with a key in memory.
        let vault = unique_dir("nolock");
        let state = state_with(&vault, Some(key));
        assert_eq!(state.encryption_key_for(|s| s.photos).unwrap(), None);
        assert_eq!(state.encryption_key_for(|s| s.activities).unwrap(), None);

        // Lock present: each scope gates its own writers independently.
        let vault = unique_dir("photos_only");
        crate::crypto::write_vault_lock(&vault, &lock_with(false, true)).unwrap();
        let state = state_with(&vault, Some(key));
        assert_eq!(state.encryption_key_for(|s| s.photos).unwrap(), Some(key));
        assert_eq!(state.encryption_key_for(|s| s.activities).unwrap(), None);

        let vault = unique_dir("activities_only");
        crate::crypto::write_vault_lock(&vault, &lock_with(true, false)).unwrap();
        let state = state_with(&vault, Some(key));
        assert_eq!(state.encryption_key_for(|s| s.activities).unwrap(), Some(key));
        assert_eq!(state.encryption_key_for(|s| s.photos).unwrap(), None);

        // Scope on but vault locked (no key yet): nothing to encrypt with.
        let state = state_with(&vault, None);
        assert_eq!(state.encryption_key_for(|s| s.activities).unwrap(), None);
    }

    /// The slot is exclusive while a guard lives and frees on drop — panics
    /// included (Drop runs during unwind), so a crashed pass can't wedge the
    /// gate for the rest of the session.
    #[test]
    fn single_flight_is_exclusive_and_frees_on_drop() {
        let flight = SingleFlight::default();

        let guard = flight.try_begin().expect("free slot must be claimable");
        assert!(flight.try_begin().is_none(), "second claim must fail");
        drop(guard);
        assert!(flight.try_begin().is_some(), "slot must free on drop");

        let result = std::panic::catch_unwind(|| {
            let _guard = flight.try_begin().unwrap();
            panic!("boom");
        });
        assert!(result.is_err());
        assert!(
            flight.try_begin().is_some(),
            "slot must free when the holder panics"
        );
    }
}
