//! Trainers, power pedals and heart rate straps over Bluetooth LE.
//!
//! - [`gatt`] — the profiles' bytes: FTMS, Cycling Power, Heart Rate,
//!   Device Information.
//! - [`machine`] — one device link as a state machine without I/O: every
//!   decision (scan, connect, reconnect, the control handshake, answers and
//!   timeouts) and the btleplug traps it steers around.
//! - [`inbox`] — the session's side of a link run by a background task:
//!   commands out, events in, a failure when the task dies.
//! - [`scan`] — one scan for all links on the shared adapter.
//! - `driver` (macOS only) — runs a machine against btleplug and serves it
//!   as a [`TrainerLink`](crate::trainer::link::TrainerLink). The feature
//!   is macOS only, and so is the dependency.

pub mod gatt;
pub mod inbox;
pub mod machine;
pub mod scan;

#[cfg(target_os = "macos")]
pub mod driver;
