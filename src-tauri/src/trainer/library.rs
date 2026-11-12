//! A ride and the library: importing a finished one, and the rides a crash
//! left behind (list, finish, discard). Free of Tauri: it takes the
//! `AppState` and, where it matters, the running ride's journal, so the
//! commands stay thin and this is tested on a plain state.
//!
//! The vault being unlocked is the caller's check (`ensure_vault_unlocked`);
//! the journal's key is strict here — the activities scope on and no key is
//! an error, never plain text into an encrypted vault.

use std::path::{Path, PathBuf};

use super::fit::Ride;
use super::journal::{self, ReadError};
use super::ride::{self, FinishError, Finished};
use super::runner::riding;
use crate::db;
use crate::error::CommandError;
use crate::import::pipeline::ImportOutcome;
use crate::models::training::{FinishKind, PendingRide, TrainingFinish};
use crate::state::AppState;

/// The activities key for a ride's journal and import.
pub fn ride_key(state: &AppState) -> Result<Option<[u8; 32]>, String> {
    state.encryption_key_strict(|s| s.activities)
}

fn finish_error(e: FinishError) -> CommandError {
    match e {
        FinishError::Journal(ReadError::Locked) => {
            CommandError::coded("training.journalLocked", "The ride is encrypted: unlock the vault first")
        }
        other => other.to_string().into(),
    }
}

/// What came of the ride that started at `start_unix`: the new activity, or
/// the one already in the library (when it came in under this ride's name).
fn outcome(conn: &rusqlite::Connection, start_unix: i64, finished: Finished) -> Result<TrainingFinish, CommandError> {
    let kind = match finished {
        Finished::Empty => return Ok(TrainingFinish { kind: FinishKind::Empty, activity_id: None }),
        Finished::Imported(ImportOutcome::Skipped) => FinishKind::Duplicate,
        Finished::Imported(_) => FinishKind::Imported,
    };
    let activity_id = db::raw_files::activity_of_original(conn, &ride::file_name_at(start_unix)).map_err(|e| e.to_string())?;
    Ok(TrainingFinish { kind, activity_id })
}

/// Imports a ride just ridden and removes its journal.
pub fn finish_ride(state: &AppState, ride: &Ride, journal: Option<&Path>) -> Result<TrainingFinish, CommandError> {
    let key = ride_key(state)?;
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let finished = ride::finish(&conn, &state.vault_path, ride, journal, key.as_ref()).map_err(finish_error)?;
    outcome(&conn, ride.start_unix, finished)
}

/// The journal of a left-over ride, if it is one: not the ride running
/// (`active`), and still there.
fn left_over(state: &AppState, active: Option<&Path>, start_unix: i64) -> Result<PathBuf, CommandError> {
    let path = journal::path_of(&state.vault_path, start_unix);
    if active == Some(path.as_path()) {
        return Err(riding());
    }
    if !path.is_file() {
        return Err(CommandError::coded("training.noSuchRide", "That ride is gone"));
    }
    Ok(path)
}

/// The rides a crash or a closed app left, but the one running (`active`).
pub fn pending(state: &AppState, active: Option<&Path>) -> Vec<PendingRide> {
    // A locked vault reads nothing sealed: those wait for the unlock. A
    // sealed journal in a vault that is not locked was sealed elsewhere.
    let (key, vault_locked) = match ride_key(state) {
        Ok(key) => (key, false),
        Err(_) => (None, true),
    };
    journal::pending(&state.vault_path)
        .into_iter()
        .filter(|p| Some(p.as_path()) != active)
        .filter_map(|path| {
            let start_unix = journal::start_of(&path)?;
            let mut ride = PendingRide { start_unix, ride_s: None, locked: false, sealed_elsewhere: false, unreadable: None };
            match journal::read(&path, key.as_ref()) {
                Ok(r) => ride.ride_s = Some(r.seconds.len() as u32),
                Err(ReadError::Locked) if vault_locked => ride.locked = true,
                Err(ReadError::Locked) => ride.sealed_elsewhere = true,
                Err(e) => ride.unreadable = Some(e.to_string()),
            }
            Some(ride)
        })
        .collect()
}

/// Finishes a left-over ride into the library. The database lock is held
/// from the check to the import: two finishes of one ride cannot cross.
pub fn finish_pending(state: &AppState, active: Option<&Path>, start_unix: i64) -> Result<TrainingFinish, CommandError> {
    let key = ride_key(state)?;
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let path = left_over(state, active, start_unix)?;
    let finished = ride::finish_journal(&conn, &state.vault_path, &path, key.as_ref()).map_err(finish_error)?;
    outcome(&conn, start_unix, finished)
}

/// Throws a left-over ride away (under the database lock, like a finish).
pub fn discard_pending(state: &AppState, active: Option<&Path>, start_unix: i64) -> Result<(), CommandError> {
    let _serial = state.db.lock().map_err(|e| e.to_string())?;
    let path = left_over(state, active, start_unix)?;
    Ok(journal::discard(&path)?)
}

/// Refuses an encryption change while a ride is left over: its journal is
/// sealed (or not) under the key that is about to change.
pub fn ensure_no_left_over_rides(state: &AppState) -> Result<(), CommandError> {
    if journal::pending(&state.vault_path).is_empty() {
        Ok(())
    } else {
        Err(CommandError::coded("training.pendingRides", "Finish or discard the unfinished indoor rides first"))
    }
}
