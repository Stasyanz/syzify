//! Smart trainers: one interface over every transport, per-model quirks as
//! data, and a trainer in software for tests.
//!
//! - [`ble`] — trainers, pedals and straps over Bluetooth LE.
//! - [`link`] — what a trainer or sensor looks like to the session engine,
//!   and the contract every transport keeps.
//! - [`profile`] — per-model quirks, matched by identity, with a cautious
//!   generic profile for unknown models.
//! - `fake` (`cfg(test)`, so for tests inside this crate) — a
//!   deterministic trainer with opt-in quirks.

pub mod awake;
pub mod ble;
#[cfg(test)]
pub mod fake;
pub mod fit;
pub mod journal;
pub mod link;
pub mod profile;
pub mod ride;
pub mod runner;
pub mod session;
