pub mod activities;
pub mod dashboard;
pub mod export;
pub mod gear;
pub mod import;
pub mod monitoring;
pub mod photos;
pub mod plugins;
pub mod recovery;
pub mod segments;
pub mod settings;
pub mod tiles;
pub mod training;

use crate::error::CommandError;

/// The app's words when `vault_flight` is held: one vault mutation (backup,
/// restore, relocation, encryption) at a time.
pub(crate) fn vault_busy() -> CommandError {
    CommandError::coded("vault.busy", "Another vault operation is already in progress")
}

/// Why `flight` refused: a ride says so (it holds the vault for an hour),
/// anything else is the general refusal.
pub(crate) fn vault_busy_in(flight: &crate::state::SingleFlight) -> CommandError {
    if flight.held_by_ride() {
        crate::trainer::runner::ride_under_way()
    } else {
        vault_busy()
    }
}
