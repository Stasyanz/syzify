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

use crate::error::CommandError;

/// The app's words when `vault_flight` is held: one vault mutation (backup,
/// restore, relocation, encryption) at a time.
pub(crate) fn vault_busy() -> CommandError {
    CommandError::coded("vault.busy", "Another vault operation is already in progress")
}
