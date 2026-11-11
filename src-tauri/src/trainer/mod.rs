//! Smart trainers: one interface over every transport, per-model quirks as
//! data, and a trainer in software for tests.
//!
//! - [`link`] — what a trainer or sensor looks like to the session engine,
//!   and the contract every transport keeps.
//! - [`profile`] — per-model quirks, matched by identity, with a cautious
//!   generic profile for unknown models.
//! - `fake` (`cfg(test)`, so for tests inside this crate) — a
//!   deterministic trainer with opt-in quirks.

#[cfg(test)]
pub mod fake;
pub mod link;
pub mod profile;
