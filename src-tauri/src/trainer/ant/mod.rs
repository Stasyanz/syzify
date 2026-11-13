//! Trainers, power pedals and heart rate straps over ANT+, through a USB
//! stick (Dynastream/Garmin `0fcf:1008`/`1009` and clones). No Bluetooth
//! permission is involved.
//!
//! - [`frame`] — the stick's serial messages and their framing.
//! - [`pages`] — ANT+ data pages: FE-C, Bicycle Power, Heart Rate, the
//!   common pages, the trainer's control pages.
//! - [`machine`] — the stick and its channels as a state machine without
//!   I/O: setup, a channel per device, drops, one acknowledged page in
//!   flight per channel.
//! - [`discover`] — what a scan heard, for the device picker.
//! - [`key`] — the ANT+ network key, given at build time (it is not in
//!   these sources); without it a build has no ANT+.
//! - `driver` (macOS only) — runs a machine against the stick (nusb) and
//!   serves each channel as a
//!   [`TrainerLink`](crate::trainer::link::TrainerLink).

pub mod discover;
pub mod frame;
pub mod machine;
pub mod pages;
#[cfg(test)]
pub mod sim;

pub mod key;

#[cfg(target_os = "macos")]
pub mod driver;
