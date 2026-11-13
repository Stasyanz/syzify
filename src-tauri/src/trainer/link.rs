//! What every trainer and sensor link looks like to the rest of the app,
//! whatever carries it (BLE, ANT+, the fake).
//!
//! # The contract
//!
//! - **Polled, not awaited.** The session engine asks a link on its tick
//!   for everything that happened since the last poll. A real transport
//!   does its radio work in a background task and hands events over
//!   through a channel that `poll` drains; the fake simulates up to `now`.
//! - **Session time.** Every `at` is time since the session began. The
//!   session begins when the engine creates its first link (pairing on the
//!   training screen counts), and every link of the session shares that
//!   start; the ride's own clock is the engine's business. A real link is
//!   given the start `Instant` when it is created and stamps each event when
//!   it *arrives* (four readings a second keep four different stamps,
//!   however rarely the engine polls); the fake is given `now` and stays
//!   deterministic, so a test rides an hour in microseconds.
//! - **Order.** Events come out in the order they happened.
//! - **Identity comes late.** `Connected` is the first time the device is
//!   heard; controls sent before it are `Refused`. What the
//!   device says about itself arrives as `Identified`, possibly after
//!   readings (an ANT+ trainer sends its manufacturer and model every ~16 s;
//!   BLE reads Device Information after discovery), and again when it
//!   learns more. The engine then picks the profile (`profile_for`) and
//!   hands it to the trainer (`use_profile`); until then a link behaves as
//!   [`GENERIC`](super::profile::GENERIC).
//! - **One outcome per control.** `send` returns an id; exactly one
//!   `LinkEvent::Control` with that id follows, stamped when the outcome
//!   became known. Two identical targets in flight (a resend every 2 s, an
//!   answer timeout of 3 s) are told apart by id.
//! - **`Failed` is the end.** Nothing follows it but the outcomes still
//!   owed: `NotAcknowledged` for controls that had gone out, `Refused` for
//!   the rest and for anything sent after. A link that can come back
//!   reports `Lost` instead.

use std::time::Duration;

use super::profile::TrainerProfile;

/// How a device is reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    Ble,
    Ant,
    Fake,
}

/// What a device says about itself. Which fields are filled depends on the
/// transport: ANT+ gives numbers (channel id, common page 0x50), BLE an
/// address, an advertised name and the Device Information strings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub transport: Transport,
    /// ANT+ device number, 20 bits (the Avanti here: 14830). With the
    /// transport, enough to find the same unit again.
    pub ant_device_number: Option<u32>,
    /// BLE peripheral identifier as the OS reports it.
    pub ble_address: Option<String>,
    /// BLE advertised name ("AVANTI COG").
    pub name: Option<String>,
    /// ANT+ manufacturer id (86 = Elite).
    pub ant_manufacturer: Option<u16>,
    /// ANT+ model number (45 = Avanti).
    pub ant_model: Option<u16>,
    /// BLE Device Information: Manufacturer Name String.
    pub manufacturer: Option<String>,
    /// BLE Device Information: Model Number String.
    pub model: Option<String>,
    pub firmware: Option<String>,
    /// Whether the trainer says it can hold a target power (ERG): ANT+ FE-C
    /// page 0x36, BLE FTMS Target Setting Features. None until it said — a
    /// trainer that never says is taken at its word when it acknowledges.
    pub target_power: Option<bool>,
}

impl Identity {
    pub fn new(transport: Transport) -> Identity {
        Identity {
            transport,
            ant_device_number: None,
            ble_address: None,
            name: None,
            ant_manufacturer: None,
            ant_model: None,
            manufacturer: None,
            model: None,
            firmware: None,
            target_power: None,
        }
    }
}

/// One update from a device. A field is `None` when this update does not
/// carry it, never zero for "unknown".
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Reading {
    /// Session time the update arrived.
    pub at: Duration,
    pub watts: Option<u16>,
    pub cadence_rpm: Option<u8>,
    pub speed_kmh: Option<f32>,
    pub heart_rate: Option<u8>,
    /// Right pedal's share of the power, percent (pedals only).
    pub balance_right: Option<u8>,
    /// The trainer's own claim that it is holding the target (FE-C 0x19
    /// target flag).
    pub reports_on_target: Option<bool>,
    /// The target the trainer says it has (FE-C 0x31 echo).
    ///
    /// Both claims can outlive the target: the Avanti kept making them while
    /// its brake followed the wheel. Whether to believe them is
    /// `TrainerProfile::trust_target_flags`.
    pub reported_target: Option<u16>,
}

/// An order to the trainer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Control {
    /// ERG: hold this many watts.
    TargetPower(u16),
    /// Basic resistance, percent of the trainer's range (above 100 counts
    /// as 100).
    Resistance(u8),
    /// Let go: the trainer stops fighting the rider. ANT+ FE-C sends 0 %
    /// resistance; BLE FTMS sends Reset, which also hands control back, so
    /// the link takes control again before the next control.
    Release,
}

/// Names one `send`, to match its outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ControlId(pub u64);

/// What became of a control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlOutcome {
    /// The trainer confirmed it. Not proof that it acts on it: see
    /// `TrainerProfile::trust_target_flags`.
    Acknowledged,
    /// It went out but the trainer did not confirm (ANT+
    /// TRANSFER_TX_FAILED, a BLE write that failed). It may or may not have
    /// landed. Worth retrying.
    NotAcknowledged,
    /// It went out and no answer came within the profile's
    /// `control_answer_timeout`. It may still land later.
    TimedOut,
    /// It never went out: the link was lost, failed, or not yet connected.
    Refused,
}

/// Why a link gave up for good.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkFailure {
    /// The OS will not let the app use the radio (Bluetooth permission).
    PermissionDenied,
    /// The radio is off.
    RadioOff,
    /// The Bluetooth adapter went away.
    AdapterGone,
    /// No ANT+ USB stick, or it was pulled.
    StickGone,
    /// Another app holds the ANT+ stick.
    StickBusy,
    /// Anything else, in the library's own words.
    Other(String),
}

/// Everything a poll can report.
#[derive(Debug, Clone, PartialEq)]
pub enum LinkEvent {
    /// The device is heard for the first time.
    Connected { at: Duration },
    /// What the device says about itself, now or more of it than before.
    Identified { at: Duration, identity: Identity },
    /// The device went silent (BLE disconnect, ANT+ RX_FAIL_GO_TO_SEARCH).
    /// `at` is when it went silent, as closely as the link can tell (after
    /// its last packet, or when the radio stack noticed). Acknowledgements
    /// still owed then end `NotAcknowledged` at `at`.
    Lost { at: Duration },
    /// Heard again after `Lost`, `outage` later.
    Back { at: Duration, outage: Duration },
    Reading(Reading),
    Control { id: ControlId, at: Duration, control: Control, outcome: ControlOutcome },
    /// The link gave up; see the contract above.
    Failed { at: Duration, reason: LinkFailure },
}

/// A device that only reports: power pedals, a heart rate strap.
pub trait SensorLink: Send {
    /// What the device said about itself so far.
    fn identity(&self) -> Option<&Identity>;
    /// Everything that happened up to `now`, in order.
    fn poll(&mut self, now: Duration) -> Vec<LinkEvent>;
}

/// A trainer: reports like a sensor and takes controls.
pub trait TrainerLink: SensorLink {
    /// The model's quirks the link itself has to honour (how long to wait
    /// for an answer, whether a failed FTMS `start` after a reconnect
    /// matters). Called once the identity picks a profile, and again if a
    /// later identity picks another.
    fn use_profile(&mut self, profile: &'static TrainerProfile);
    /// Queue a control; never blocks. Its outcome comes back from a later
    /// `poll` under the returned id.
    fn send(&mut self, control: Control) -> ControlId;
}
