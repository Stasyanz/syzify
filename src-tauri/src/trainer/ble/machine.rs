//! One BLE device link as a state machine with no I/O of its own: the
//! driver feeds it what the radio said ([`Input`]) and carries out what it
//! asks ([`Action`]); what the session should hear comes out as
//! [`LinkEvent`]s, keeping the contract in `trainer::link`. Every decision
//! lives here, so every decision is tested without a radio.
//!
//! Three btleplug 0.11 traps on macOS, all hit by the spike, shape it:
//!
//! 1. After a disconnect btleplug forgets the peripheral; the next
//!    advertisement brings a new one. So a lost device is found again by
//!    scanning, and the driver takes a fresh handle for every connect.
//! 2. A timeout around `connect()` only stops our wait: CoreBluetooth keeps
//!    connecting, a second connect races it, and btleplug panics in its own
//!    thread, taking every sensor down. So the machine never asks for a
//!    second connect while one is pending, however long it takes.
//! 3. A disconnect in the middle of a connect leaves that connect pending
//!    for ever. Then nothing is connecting any more, and starting over
//!    (scan, connect) is safe.
//!
//! Every connect gets a number ([`Conn`]); what the driver reports from a
//! connection carries it, and anything from an older connection is ignored.
//!
//! The FTMS control point runs one procedure at a time. The machine writes
//! one control, waits for its answer, and after a timeout keeps the line
//! clear for the late answer before the next write, so an answer never
//! lands on the wrong control. Control is taken (request control, start)
//! on every connect and again — at most once per `erg_resend` — after the
//! trainer says "control not permitted" or a Reset handed it back.

use std::collections::VecDeque;
use std::time::Duration;

use super::gatt::{self, CrankCadence};
use crate::trainer::link::{Control, ControlId, ControlOutcome, Identity, LinkEvent, LinkFailure, Reading, Transport};
use crate::trainer::profile::{StartAfterReconnect, TrainerProfile, GENERIC};

/// How long the radio may stay in an unknown state before the link gives
/// up. btleplug cannot tell "not allowed" from "unknown", and on first use
/// macOS holds the state unknown while its permission dialog is open.
pub const RADIO_GRACE: Duration = Duration::from_secs(60);

/// A trainer under control or being ridden that sends nothing for this
/// long — not even the answers to the controls it keeps getting — is
/// treated as lost (a disconnect that never reached us). Only within
/// [`CONTROLLED`] of a control or a cadence above zero: whether a trainer
/// at rest keeps sending is not measured, and pedals and straps go quiet
/// at rest.
pub const TRAINER_SILENCE: Duration = Duration::from_secs(15);

/// A control or a cadence above zero within this long means the trainer
/// is in use (ERG resends every 2 s).
pub const CONTROLLED: Duration = Duration::from_secs(30);

/// Discovery is retried this many times before the link gives up.
const DISCOVERY_TRIES: u32 = 3;

/// Subscriptions failing on this many connections in a row (a device that
/// wants pairing, a refused descriptor) end the link.
const SUBSCRIBE_TRIES: u32 = 3;

/// A cadence above this from crank events is a wrapped counter, not a rider.
const MAX_RPM: f32 = 250.0;

/// Numbers one connect, to tell its results from an older connection's.
pub type Conn = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Trainer,
    Pedals,
    Heart,
}

impl Role {
    /// The service a device of this role advertises.
    pub fn service(self) -> u16 {
        match self {
            Role::Trainer => gatt::FITNESS_MACHINE,
            Role::Pedals => gatt::CYCLING_POWER,
            Role::Heart => gatt::HEART_RATE,
        }
    }

    fn measurement(self) -> u16 {
        match self {
            Role::Trainer => gatt::INDOOR_BIKE_DATA,
            Role::Pedals => gatt::CYCLING_POWER_MEASUREMENT,
            Role::Heart => gatt::HEART_RATE_MEASUREMENT,
        }
    }
}

/// Which device to link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub role: Role,
    /// This device and no other (an address the OS gave before).
    pub address: Option<String>,
    /// A device whose advertised name contains this, ignoring case. An
    /// Assioma Duo is two devices over BLE, each its own leg's power: link
    /// both, each by its name, and sum them (`trainer::pedals::PedalPair`).
    pub name_contains: Option<String>,
}

impl Target {
    pub fn any(role: Role) -> Target {
        Target { role, address: None, name_contains: None }
    }

    fn accepts(&self, ad: &Advert) -> bool {
        if let Some(address) = &self.address {
            return ad.address == *address;
        }
        let named = match (&self.name_contains, &ad.name) {
            (None, _) => true,
            (Some(want), Some(name)) => name.to_lowercase().contains(&want.to_lowercase()),
            (Some(_), None) => false,
        };
        // Trainers advertise Cycling Power too: pedals are a power source
        // that is not a fitness machine.
        let not_a_trainer = self.role != Role::Pedals || !ad.services.contains(&gatt::FITNESS_MACHINE);
        named && ad.services.contains(&self.role.service()) && not_a_trainer
    }
}

/// An advertisement as the driver saw it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Advert {
    pub address: String,
    pub name: Option<String>,
    /// 16-bit service ids it advertised.
    pub services: Vec<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RadioState {
    On,
    Off,
    Unknown,
}

/// What the radio said. Results of work on a connection carry its
/// [`Conn`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    Radio(RadioState),
    Advertised(Advert),
    Connected { conn: Conn },
    ConnectFailed { conn: Conn, error: String },
    Disconnected { address: String },
    /// Services discovered; the 16-bit characteristics found.
    Discovered { conn: Conn, characteristics: Vec<u16> },
    DiscoveryFailed { conn: Conn, error: String },
    /// A subscription did not take (or never answered).
    SubscribeFailed { conn: Conn, characteristic: u16 },
    Notified { conn: Conn, characteristic: u16, value: Vec<u8> },
    Read { conn: Conn, characteristic: u16, value: Vec<u8> },
    /// The control point write itself failed (its answer comes as a
    /// notification otherwise).
    WriteFailed { conn: Conn, error: String },
    /// Time passed.
    Tick,
}

/// What the driver should do, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    StartScan { service: u16 },
    StopScan,
    /// Connect with a fresh handle, and wait as long as it takes; what this
    /// connection reports carries `conn`.
    Connect { address: String, conn: Conn },
    /// Drop the current connection (it went silent or broke).
    Disconnect,
    Discover,
    Subscribe { characteristic: u16 },
    Read { characteristic: u16 },
    /// Off the reading loop: a write with response may never complete.
    Write { characteristic: u16, bytes: Vec<u8> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum State {
    /// The radio is not on yet.
    Waiting,
    Scanning,
    Connecting { address: String, conn: Conn },
    Discovering { address: String, conn: Conn, tries: u32 },
    Ready { address: String, conn: Conn },
    Failed,
}

/// What a control point write in flight is for.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Step {
    /// Taking control: request control, then start.
    Handshake(u8),
    User(ControlId, Control),
}

impl Step {
    fn op(self) -> u8 {
        match self {
            Step::Handshake(op) => op,
            Step::User(_, control) => gatt::control_write(control)[0],
        }
    }
}

/// How a write ended.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Answer {
    Result(u8),
    TimedOut,
    WriteFailed,
}

fn same_kind(a: Control, b: Control) -> bool {
    std::mem::discriminant(&a) == std::mem::discriminant(&b)
}

pub struct Machine {
    target: Target,
    profile: &'static TrainerProfile,
    state: State,
    started: Duration,
    next_conn: Conn,
    identity: Identity,
    identified: bool,
    connected_once: bool,
    lost_at: Option<Duration>,
    last_heard: Duration,
    /// When the session last sent a control (the silence watchdog runs
    /// only under control).
    last_control: Option<Duration>,
    /// When the trainer last reported a cadence above zero: someone rides,
    /// so a trainer that goes silent is lost, ERG or not.
    last_pedalling: Option<Duration>,
    /// Connections in a row whose subscription failed.
    subscribe_failures: u32,
    has_control_point: bool,
    handshake: VecDeque<u8>,
    /// Control must be asked for again before the next user control.
    need_control: bool,
    /// ... but not before this (a trainer held by another app would
    /// otherwise be asked in a loop).
    retake_not_before: Duration,
    queue: VecDeque<(ControlId, Control)>,
    in_flight: Option<(Step, Duration)>,
    /// A write that timed out: its late answer may still come until then,
    /// and nothing else is written meanwhile.
    stale: Option<(u8, Duration)>,
    cadence: CrankCadence,
    events: Vec<LinkEvent>,
}

impl Machine {
    pub fn new(target: Target, at: Duration) -> Machine {
        Machine {
            target,
            profile: &GENERIC,
            state: State::Waiting,
            started: at,
            next_conn: 0,
            identity: Identity::new(Transport::Ble),
            identified: false,
            connected_once: false,
            lost_at: None,
            last_heard: at,
            last_control: None,
            last_pedalling: None,
            subscribe_failures: 0,
            has_control_point: false,
            handshake: VecDeque::new(),
            need_control: false,
            retake_not_before: Duration::ZERO,
            queue: VecDeque::new(),
            in_flight: None,
            stale: None,
            cadence: CrankCadence::default(),
            events: Vec::new(),
        }
    }

    pub fn use_profile(&mut self, profile: &'static TrainerProfile) {
        self.profile = profile;
    }

    /// The identity, once announced.
    pub fn identity(&self) -> Option<&Identity> {
        self.identified.then_some(&self.identity)
    }

    /// Whether a connection is up (worth a disconnect when the link ends).
    pub fn is_connected(&self) -> bool {
        matches!(self.state, State::Discovering { .. } | State::Ready { .. })
    }

    /// Events for the session since the last call.
    pub fn take_events(&mut self) -> Vec<LinkEvent> {
        std::mem::take(&mut self.events)
    }

    fn conn(&self) -> Option<Conn> {
        match self.state {
            State::Connecting { conn, .. } | State::Discovering { conn, .. } | State::Ready { conn, .. } => Some(conn),
            _ => None,
        }
    }

    fn ready(&self) -> bool {
        matches!(self.state, State::Ready { .. })
    }

    pub fn handle(&mut self, at: Duration, input: Input) -> Vec<Action> {
        if self.state == State::Failed {
            return Vec::new();
        }
        let current = self.conn();
        let stale = |conn: Conn| current != Some(conn);
        match input {
            Input::Radio(RadioState::Off) => {
                self.fail(at, LinkFailure::RadioOff);
                Vec::new()
            }
            Input::Radio(RadioState::On) if self.state == State::Waiting => {
                self.state = State::Scanning;
                vec![self.scan()]
            }
            Input::Radio(_) => Vec::new(),
            Input::Advertised(ad) => self.advertised(ad),
            Input::Connected { conn } => {
                if let State::Connecting { address, conn: c } = &self.state {
                    if *c == conn {
                        self.state = State::Discovering { address: address.clone(), conn, tries: 1 };
                        return vec![Action::Discover];
                    }
                }
                Vec::new()
            }
            Input::ConnectFailed { conn, .. } if matches!(self.state, State::Connecting { .. }) && !stale(conn) => {
                // Wait for it to advertise again.
                self.state = State::Scanning;
                vec![self.scan()]
            }
            Input::ConnectFailed { .. } => Vec::new(),
            Input::Disconnected { address } => self.disconnected(at, &address),
            Input::Discovered { conn, .. } | Input::DiscoveryFailed { conn, .. } if stale(conn) => Vec::new(),
            Input::Discovered { characteristics, .. } => self.discovered(at, &characteristics),
            Input::DiscoveryFailed { error, .. } => {
                if let State::Discovering { address, conn, tries } = &self.state {
                    if *tries >= DISCOVERY_TRIES {
                        self.fail(at, LinkFailure::Other(format!("service discovery failed: {error}")));
                    } else {
                        self.state = State::Discovering { address: address.clone(), conn: *conn, tries: tries + 1 };
                        return vec![Action::Discover];
                    }
                }
                Vec::new()
            }
            Input::SubscribeFailed { conn, characteristic: gatt::FTMS_CONTROL_POINT } if self.ready() && !stale(conn) => {
                // Indications that want pairing while the data is open: keep
                // the readings, give up control (controls are refused).
                self.settle_all(at);
                Vec::new()
            }
            Input::SubscribeFailed { conn, characteristic } if self.ready() && !stale(conn) => {
                self.subscribe_failures += 1;
                if self.subscribe_failures >= SUBSCRIBE_TRIES {
                    let error = format!("subscribing to {characteristic:#06x} keeps failing");
                    self.fail(at, LinkFailure::Other(error));
                    vec![Action::Disconnect]
                } else {
                    self.drop_connection(at)
                }
            }
            Input::SubscribeFailed { .. } => Vec::new(),
            Input::Notified { conn, characteristic, value } if self.ready() && !stale(conn) => {
                self.notified(at, characteristic, &value)
            }
            Input::Notified { .. } => Vec::new(),
            Input::Read { conn, characteristic, value } if self.ready() && !stale(conn) => {
                self.read(at, characteristic, &value);
                Vec::new()
            }
            Input::Read { .. } => Vec::new(),
            Input::WriteFailed { conn, .. } if self.ready() && !stale(conn) && self.in_flight.is_some() => {
                self.settle(at, Answer::WriteFailed);
                self.pump(at)
            }
            Input::WriteFailed { .. } => Vec::new(),
            Input::Tick => self.tick(at),
        }
    }

    /// A control from the session. Refused at once unless the trainer is
    /// connected and has a control point. A newer control replaces a
    /// queued one of the same kind (a release, every queued one): only the
    /// latest target matters.
    pub fn send(&mut self, at: Duration, id: ControlId, control: Control) -> Vec<Action> {
        if !(self.ready() && self.has_control_point) {
            self.events.push(LinkEvent::Control { id, at, control, outcome: ControlOutcome::Refused });
            return Vec::new();
        }
        self.last_control = Some(at);
        let (superseded, kept): (Vec<_>, Vec<_>) = self
            .queue
            .drain(..)
            .partition(|&(_, queued)| control == Control::Release || same_kind(queued, control));
        self.queue = kept.into();
        for (old, queued) in superseded {
            self.events.push(LinkEvent::Control { id: old, at, control: queued, outcome: ControlOutcome::Refused });
        }
        self.queue.push_back((id, control));
        self.pump(at)
    }

    fn scan(&self) -> Action {
        Action::StartScan { service: self.target.role.service() }
    }

    fn fail(&mut self, at: Duration, reason: LinkFailure) {
        self.events.push(LinkEvent::Failed { at, reason });
        self.settle_all(at);
        self.state = State::Failed;
    }

    fn advertised(&mut self, ad: Advert) -> Vec<Action> {
        if self.state != State::Scanning || !self.target.accepts(&ad) {
            return Vec::new();
        }
        if ad.name.is_some() {
            self.identity.name = ad.name.clone();
        }
        self.identity.ble_address = Some(ad.address.clone());
        self.next_conn += 1;
        let conn = self.next_conn;
        self.state = State::Connecting { address: ad.address.clone(), conn };
        vec![Action::StopScan, Action::Connect { address: ad.address, conn }]
    }

    /// The connection is gone: Lost (once, however many tries it takes to
    /// get it back), owed controls settled, scanning again.
    fn lose(&mut self, at: Duration) {
        if self.connected_once && self.lost_at.is_none() {
            self.lost_at = Some(at);
            self.events.push(LinkEvent::Lost { at });
        }
        self.settle_all(at);
        self.state = State::Scanning;
    }

    fn disconnected(&mut self, at: Duration, address: &str) -> Vec<Action> {
        match &self.state {
            // Trap 3: that connect will never resolve; nothing is connecting.
            State::Connecting { address: a, .. } if a == address => {
                self.state = State::Scanning;
                vec![self.scan()]
            }
            State::Discovering { address: a, .. } | State::Ready { address: a, .. } if a == address => {
                self.lose(at);
                vec![self.scan()]
            }
            _ => Vec::new(),
        }
    }

    /// Connected but useless (silent, or a subscription failed): hang up
    /// and find it again.
    fn drop_connection(&mut self, at: Duration) -> Vec<Action> {
        self.lose(at);
        vec![Action::Disconnect, self.scan()]
    }

    fn discovered(&mut self, at: Duration, characteristics: &[u16]) -> Vec<Action> {
        let State::Discovering { address, conn, .. } = &self.state else { return Vec::new() };
        let (address, conn) = (address.clone(), *conn);
        let measurement = self.target.role.measurement();
        if !characteristics.contains(&measurement) {
            self.fail(at, LinkFailure::Other(format!("the device has no characteristic {measurement:#06x}")));
            return Vec::new();
        }
        self.state = State::Ready { address, conn };
        self.last_heard = at;
        self.cadence = CrankCadence::default();
        let mut actions = vec![Action::Subscribe { characteristic: measurement }];
        self.has_control_point = self.target.role == Role::Trainer && characteristics.contains(&gatt::FTMS_CONTROL_POINT);
        if self.has_control_point {
            actions.push(Action::Subscribe { characteristic: gatt::FTMS_CONTROL_POINT });
        }
        for c in [gatt::MANUFACTURER_NAME, gatt::MODEL_NUMBER, gatt::FIRMWARE_REVISION] {
            if characteristics.contains(&c) {
                actions.push(Action::Read { characteristic: c });
            }
        }
        // What the trainer can be told to do: ERG or not.
        if self.has_control_point && characteristics.contains(&gatt::FTMS_FEATURE) {
            actions.push(Action::Read { characteristic: gatt::FTMS_FEATURE });
        }
        // After the first connection every loss went through `lose`, which
        // set `lost_at`: a connect without it is the first.
        if let Some(lost) = self.lost_at.take() {
            self.events.push(LinkEvent::Back { at, outage: at.saturating_sub(lost) });
        } else if !self.connected_once {
            self.connected_once = true;
            self.events.push(LinkEvent::Connected { at });
            self.identified = true;
            self.events.push(LinkEvent::Identified { at, identity: self.identity.clone() });
        }
        if self.has_control_point {
            self.handshake = VecDeque::from([gatt::OP_REQUEST_CONTROL, gatt::OP_START]);
            actions.extend(self.pump(at));
        }
        actions
    }

    fn notified(&mut self, at: Duration, characteristic: u16, value: &[u8]) -> Vec<Action> {
        self.last_heard = at;
        if characteristic == gatt::FTMS_CONTROL_POINT && self.has_control_point {
            return self.answered(at, value);
        }
        if characteristic != self.target.role.measurement() {
            return Vec::new();
        }
        // The data flows: the subscription took.
        self.subscribe_failures = 0;
        let rpm = |c: f32| (c <= MAX_RPM).then(|| c.round() as u8);
        let reading = match self.target.role {
            Role::Trainer => gatt::parse_indoor_bike_data(value).map(|b| {
                let cadence_rpm = b.cadence_rpm.and_then(rpm);
                // Silence right after pedalling is suspect; after "0 rpm"
                // it is a rest.
                match cadence_rpm {
                    Some(0) => self.last_pedalling = None,
                    Some(_) => self.last_pedalling = Some(at),
                    None => {}
                }
                Reading { at, watts: b.watts.map(|w| w.max(0) as u16), cadence_rpm, speed_kmh: b.speed_kmh, ..Reading::default() }
            }),
            Role::Pedals => gatt::parse_cycling_power(value).map(|p| Reading {
                at,
                watts: Some(p.watts.max(0) as u16),
                cadence_rpm: p.crank.and_then(|crank| self.cadence.push(at, crank)).and_then(rpm),
                balance_right: p.balance_right.map(|b| b.round() as u8),
                ..Reading::default()
            }),
            Role::Heart => gatt::parse_heart_rate(value).map(|bpm| Reading {
                at,
                heart_rate: Some(bpm.min(255) as u8),
                ..Reading::default()
            }),
        };
        if let Some(r) = reading {
            self.events.push(LinkEvent::Reading(r));
        }
        Vec::new()
    }

    fn read(&mut self, at: Duration, characteristic: u16, value: &[u8]) {
        if characteristic == gatt::FTMS_FEATURE {
            let said = gatt::parse_target_power_supported(value);
            if said.is_some() && said != self.identity.target_power {
                self.identity.target_power = said;
                if self.identified {
                    self.events.push(LinkEvent::Identified { at, identity: self.identity.clone() });
                }
            }
            return;
        }
        let text = String::from_utf8_lossy(value).trim_end_matches('\0').trim().to_string();
        if text.is_empty() {
            return;
        }
        let field = match characteristic {
            gatt::MANUFACTURER_NAME => &mut self.identity.manufacturer,
            gatt::MODEL_NUMBER => &mut self.identity.model,
            gatt::FIRMWARE_REVISION => &mut self.identity.firmware,
            _ => return,
        };
        if field.as_deref() != Some(text.as_str()) {
            *field = Some(text);
            if self.identified {
                self.events.push(LinkEvent::Identified { at, identity: self.identity.clone() });
            }
        }
    }

    /// A control point indication.
    fn answered(&mut self, at: Duration, value: &[u8]) -> Vec<Action> {
        let Some(answer) = gatt::parse_control_response(value) else { return Vec::new() };
        if let Some((op, _)) = self.stale {
            if answer.request == op {
                // The late answer to a write that timed out: the line is clear.
                self.stale = None;
                return self.pump(at);
            }
        }
        match self.in_flight {
            Some((step, _)) if step.op() == answer.request => {
                self.settle(at, Answer::Result(answer.result));
                self.pump(at)
            }
            _ => Vec::new(),
        }
    }

    /// The write in flight is over.
    fn settle(&mut self, at: Duration, answer: Answer) {
        let Some((step, _)) = self.in_flight.take() else { return };
        if answer == Answer::TimedOut {
            self.stale = Some((step.op(), at + self.profile.control_answer_timeout));
        }
        let not_permitted = answer == Answer::Result(gatt::RESULT_CONTROL_NOT_PERMITTED);
        match step {
            Step::Handshake(_) if not_permitted => {
                // Someone else holds the trainer. What waits was never sent.
                self.handshake.clear();
                self.need_control = true;
                for (id, control) in self.queue.drain(..) {
                    self.events.push(LinkEvent::Control { id, at, control, outcome: ControlOutcome::Refused });
                }
            }
            Step::Handshake(gatt::OP_START) if matches!(answer, Answer::Result(r) if r != gatt::RESULT_SUCCESS) => {
                // The Avanti fails start after every reconnect and takes the
                // target all the same. A trainer cannot be told a reconnect
                // from a new session (or a restart of the app), so the
                // profile's word counts for every connect.
                let excused = self.profile.ftms_start_after_reconnect == StartAfterReconnect::MayFail;
                self.need_control |= !excused;
            }
            // A slow or lost handshake answer is no refusal: go on, as the
            // spike did, and let the target's own answer tell.
            Step::Handshake(_) => {}
            Step::User(id, control) => {
                let outcome = match answer {
                    Answer::Result(gatt::RESULT_SUCCESS) => ControlOutcome::Acknowledged,
                    Answer::TimedOut => ControlOutcome::TimedOut,
                    Answer::Result(_) | Answer::WriteFailed => ControlOutcome::NotAcknowledged,
                };
                // A Reset hands control back; "not permitted" says we lost it.
                if not_permitted || (control == Control::Release && outcome == ControlOutcome::Acknowledged) {
                    self.need_control = true;
                }
                self.events.push(LinkEvent::Control { id, at, control, outcome });
            }
        }
    }

    /// Every control still owed ends now: the one in flight went out,
    /// the queued ones never did.
    fn settle_all(&mut self, at: Duration) {
        if let Some((Step::User(id, control), _)) = self.in_flight.take() {
            self.events.push(LinkEvent::Control { id, at, control, outcome: ControlOutcome::NotAcknowledged });
        }
        self.in_flight = None;
        for (id, control) in self.queue.drain(..) {
            self.events.push(LinkEvent::Control { id, at, control, outcome: ControlOutcome::Refused });
        }
        self.handshake.clear();
        self.has_control_point = false;
        self.need_control = false;
        self.stale = None;
    }

    /// Start the next write when the line is clear.
    fn pump(&mut self, at: Duration) -> Vec<Action> {
        if self.in_flight.is_some() || !self.ready() {
            return Vec::new();
        }
        match self.stale {
            Some((_, until)) if at < until => return Vec::new(),
            Some(_) => self.stale = None,
            None => {}
        }
        if self.handshake.is_empty() && self.need_control && !self.queue.is_empty() {
            if at < self.retake_not_before {
                return Vec::new();
            }
            self.need_control = false;
            self.retake_not_before = at + self.profile.erg_resend;
            self.handshake = VecDeque::from([gatt::OP_REQUEST_CONTROL, gatt::OP_START]);
        }
        let (step, bytes) = if let Some(op) = self.handshake.pop_front() {
            (Step::Handshake(op), vec![op])
        } else if !self.need_control {
            match self.queue.pop_front() {
                Some((id, control)) => (Step::User(id, control), gatt::control_write(control)),
                None => return Vec::new(),
            }
        } else {
            return Vec::new();
        };
        self.in_flight = Some((step, at));
        vec![Action::Write { characteristic: gatt::FTMS_CONTROL_POINT, bytes }]
    }

    fn tick(&mut self, at: Duration) -> Vec<Action> {
        if self.state == State::Waiting && at.saturating_sub(self.started) >= RADIO_GRACE {
            self.fail(at, LinkFailure::PermissionDenied);
            return Vec::new();
        }
        let recent = |t: Option<Duration>| t.is_some_and(|t| at.saturating_sub(t) < CONTROLLED);
        let controlled = recent(self.last_control) || recent(self.last_pedalling);
        if self.ready()
            && self.target.role == Role::Trainer
            && controlled
            && at.saturating_sub(self.last_heard) >= TRAINER_SILENCE
        {
            return self.drop_connection(at);
        }
        if let Some((_, sent)) = self.in_flight {
            if at.saturating_sub(sent) >= self.profile.control_answer_timeout {
                self.settle(at, Answer::TimedOut);
            }
        }
        self.pump(at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(secs: f32) -> Duration {
        Duration::from_secs_f32(secs)
    }

    const AVANTI: &str = "6A6E19E4-6047-11EE-19BF-763AD50C7FD5";

    fn avanti_ad() -> Advert {
        Advert {
            address: AVANTI.into(),
            name: Some("AVANTI COG".into()),
            services: vec![gatt::CYCLING_POWER, gatt::FITNESS_MACHINE],
        }
    }

    const TRAINER_CHARS: [u16; 5] = [
        gatt::INDOOR_BIKE_DATA,
        gatt::FTMS_CONTROL_POINT,
        gatt::MANUFACTURER_NAME,
        gatt::MODEL_NUMBER,
        gatt::FIRMWARE_REVISION,
    ];

    /// The current connection's number, for inputs that carry it.
    fn conn(m: &Machine) -> Conn {
        m.conn().unwrap_or(u64::MAX)
    }

    fn cp(m: &Machine, op: u8, result: u8) -> Input {
        Input::Notified { conn: conn(m), characteristic: gatt::FTMS_CONTROL_POINT, value: vec![0x80, op, result] }
    }

    fn ok(m: &Machine, op: u8) -> Input {
        cp(m, op, gatt::RESULT_SUCCESS)
    }

    fn failed(m: &Machine, op: u8) -> Input {
        cp(m, op, 0x04)
    }

    fn not_permitted(m: &Machine, op: u8) -> Input {
        cp(m, op, gatt::RESULT_CONTROL_NOT_PERMITTED)
    }

    fn data(m: &Machine, characteristic: u16, value: &[u8]) -> Input {
        Input::Notified { conn: conn(m), characteristic, value: value.to_vec() }
    }

    fn write(bytes: &[u8]) -> Action {
        Action::Write { characteristic: gatt::FTMS_CONTROL_POINT, bytes: bytes.to_vec() }
    }

    const TARGET_150: [u8; 3] = [0x05, 0x96, 0x00];

    /// Connect from scanning to discovered, returning discovery's actions.
    fn connect(m: &mut Machine, at: f32, chars: &[u16]) -> Vec<Action> {
        let actions = m.handle(s(at), Input::Advertised(avanti_ad()));
        assert!(matches!(actions.as_slice(), [Action::StopScan, Action::Connect { .. }]), "{actions:?}");
        let c = conn(m);
        assert_eq!(m.handle(s(at + 0.5), Input::Connected { conn: c }), vec![Action::Discover]);
        m.handle(s(at + 1.0), Input::Discovered { conn: c, characteristics: chars.to_vec() })
    }

    /// A trainer connected and in control: scan, connect, discover, the
    /// handshake answered.
    fn ready_trainer() -> Machine {
        let mut m = Machine::new(Target::any(Role::Trainer), s(0.0));
        assert_eq!(m.handle(s(0.1), Input::Radio(RadioState::On)), vec![Action::StartScan { service: gatt::FITNESS_MACHINE }]);
        let actions = connect(&mut m, 0.5, &TRAINER_CHARS);
        assert_eq!(
            actions,
            vec![
                Action::Subscribe { characteristic: gatt::INDOOR_BIKE_DATA },
                Action::Subscribe { characteristic: gatt::FTMS_CONTROL_POINT },
                Action::Read { characteristic: gatt::MANUFACTURER_NAME },
                Action::Read { characteristic: gatt::MODEL_NUMBER },
                Action::Read { characteristic: gatt::FIRMWARE_REVISION },
                write(&[gatt::OP_REQUEST_CONTROL]),
            ]
        );
        assert_eq!(m.handle(s(1.6), ok(&m, gatt::OP_REQUEST_CONTROL)), vec![write(&[gatt::OP_START])]);
        assert_eq!(m.handle(s(1.7), ok(&m, gatt::OP_START)), vec![]);
        m
    }

    fn controls(events: &[LinkEvent]) -> Vec<(ControlId, ControlOutcome)> {
        events
            .iter()
            .filter_map(|e| match e {
                LinkEvent::Control { id, outcome, .. } => Some((*id, *outcome)),
                _ => None,
            })
            .collect()
    }

    fn reconnect(m: &mut Machine, at: f32) -> Vec<Action> {
        m.handle(s(at), Input::Disconnected { address: AVANTI.into() });
        connect(m, at + 1.0, &TRAINER_CHARS)
    }

    // --- connecting -----------------------------------------------------

    #[test]
    fn it_scans_connects_discovers_and_takes_control() {
        let mut m = ready_trainer();
        assert!(m.is_connected());
        let events = m.take_events();
        assert_eq!(events[0], LinkEvent::Connected { at: s(1.5) });
        let LinkEvent::Identified { identity, .. } = &events[1] else { panic!("{events:?}") };
        assert_eq!(identity.name.as_deref(), Some("AVANTI COG"));
        assert_eq!(identity.ble_address.as_deref(), Some(AVANTI));
        assert_eq!(identity.transport, Transport::Ble);
    }

    #[test]
    fn device_information_extends_the_identity() {
        let mut m = ready_trainer();
        m.take_events();
        let c = conn(&m);
        m.handle(s(3.0), Input::Read { conn: c, characteristic: gatt::MANUFACTURER_NAME, value: b"Elite\0".to_vec() });
        m.handle(s(3.0), Input::Read { conn: c, characteristic: gatt::MANUFACTURER_NAME, value: b"Elite".to_vec() });
        m.handle(s(3.1), Input::Read { conn: c, characteristic: gatt::FIRMWARE_REVISION, value: b"  ".to_vec() });
        m.handle(s(3.2), Input::Read { conn: c, characteristic: gatt::INDOOR_BIKE_DATA, value: b"x".to_vec() });
        m.handle(s(3.3), Input::Read { conn: c + 7, characteristic: gatt::MODEL_NUMBER, value: b"old".to_vec() });
        let events = m.take_events();
        assert_eq!(events.len(), 1, "one update, no repeats, nothing for blanks or strangers: {events:?}");
        assert_eq!(m.identity().and_then(|i| i.manufacturer.as_deref()), Some("Elite"));
        assert_eq!(m.identity().and_then(|i| i.model.as_deref()), None);
        m.handle(s(3.4), Input::Read { conn: c, characteristic: gatt::MODEL_NUMBER, value: b"Avanti".to_vec() });
        m.handle(s(3.5), Input::Read { conn: c, characteristic: gatt::FIRMWARE_REVISION, value: b"37".to_vec() });
        let id = m.identity().cloned().unwrap_or_else(|| Identity::new(Transport::Ble));
        assert_eq!((id.model.as_deref(), id.firmware.as_deref()), (Some("Avanti"), Some("37")));
        assert_eq!(m.take_events().len(), 2, "each field announced");
    }

    /// FTMS Fitness Machine Feature says whether the trainer holds a target
    /// power (#292): read with the rest, told once per change.
    #[test]
    fn the_trainer_says_whether_it_holds_watts() {
        let mut m = Machine::new(Target::any(Role::Trainer), s(0.0));
        m.handle(s(0.1), Input::Radio(RadioState::On));
        let chars = [&TRAINER_CHARS[..], &[gatt::FTMS_FEATURE]].concat();
        let actions = connect(&mut m, 1.0, &chars);
        assert!(actions.contains(&Action::Read { characteristic: gatt::FTMS_FEATURE }), "{actions:?}");
        m.take_events();
        let c = conn(&m);
        // Machine features, then target setting features (bit 3: power).
        let feature = |settings: u8| vec![0x83, 0x14, 0, 0, settings, 0x20, 0, 0];
        for (at, value) in [(3.0, vec![1, 2, 3]), (3.1, feature(0x0C)), (3.2, feature(0x0C)), (3.3, feature(0x04))] {
            m.handle(s(at), Input::Read { conn: c, characteristic: gatt::FTMS_FEATURE, value });
        }
        let said: Vec<Option<bool>> = m
            .take_events()
            .iter()
            .filter_map(|e| if let LinkEvent::Identified { identity, .. } = e { Some(identity.target_power) } else { None })
            .collect();
        assert_eq!(said, [Some(true), Some(false)], "too short says nothing, a repeat nothing new");
    }

    /// Pedals offering the same characteristic are not asked: it is a
    /// trainer's question.
    #[test]
    fn only_a_trainer_is_asked_whether_it_holds_watts() {
        let mut m = Machine::new(Target::any(Role::Pedals), s(0.0));
        m.handle(s(0.1), Input::Radio(RadioState::On));
        let assioma = Advert { address: "P".into(), name: Some("ASSIOMA55430L".into()), services: vec![gatt::CYCLING_POWER] };
        m.handle(s(1.0), Input::Advertised(assioma));
        let c = conn(&m);
        m.handle(s(1.5), Input::Connected { conn: c });
        let chars = vec![gatt::CYCLING_POWER_MEASUREMENT, gatt::FTMS_FEATURE, gatt::FTMS_CONTROL_POINT];
        let actions = m.handle(s(2.0), Input::Discovered { conn: c, characteristics: chars });
        assert!(actions.contains(&Action::Subscribe { characteristic: gatt::CYCLING_POWER_MEASUREMENT }), "discovered: {actions:?}");
        assert!(!actions.contains(&Action::Read { characteristic: gatt::FTMS_FEATURE }), "{actions:?}");
    }

    #[test]
    fn it_picks_the_device_it_was_asked_for() {
        let assioma_l = Advert { address: "L".into(), name: Some("ASSIOMA55430L".into()), services: vec![gatt::CYCLING_POWER] };
        let assioma_r = Advert { address: "R".into(), name: Some("ASSIOMA61026R".into()), services: vec![gatt::CYCLING_POWER] };
        let no_name = Advert { address: "X".into(), name: None, services: vec![gatt::CYCLING_POWER] };

        let by_name = Target { name_contains: Some("61026r".into()), ..Target::any(Role::Pedals) };
        assert!(!by_name.accepts(&assioma_l) && by_name.accepts(&assioma_r) && !by_name.accepts(&no_name));

        let by_address = Target { address: Some("L".into()), ..Target::any(Role::Pedals) };
        assert!(by_address.accepts(&assioma_l) && !by_address.accepts(&assioma_r));

        assert!(!Target::any(Role::Trainer).accepts(&assioma_l), "pedals are not a trainer");
        assert!(Target::any(Role::Trainer).accepts(&avanti_ad()));
        assert!(!Target::any(Role::Pedals).accepts(&avanti_ad()), "a trainer advertising Cycling Power is not pedals");
        assert!(Target::any(Role::Pedals).accepts(&no_name));
    }

    #[test]
    fn it_ignores_what_does_not_concern_it() {
        let mut m = Machine::new(Target::any(Role::Trainer), s(0.0));
        assert!(m.handle(s(0.0), Input::Advertised(avanti_ad())).is_empty(), "not scanning before the radio is on");
        m.handle(s(0.1), Input::Radio(RadioState::On));
        assert!(m.handle(s(0.2), Input::Radio(RadioState::On)).is_empty(), "already scanning");
        assert!(m.handle(s(0.3), Input::Connected { conn: 1 }).is_empty(), "no connect asked for");
        assert!(m.handle(s(0.4), Input::Disconnected { address: AVANTI.into() }).is_empty());
        assert!(m.handle(s(0.5), Input::Discovered { conn: 1, characteristics: TRAINER_CHARS.to_vec() }).is_empty());
        assert!(m.handle(s(0.6), Input::ConnectFailed { conn: 1, error: "?".into() }).is_empty());
        assert!(m.handle(s(0.7), Input::Notified { conn: 1, characteristic: gatt::INDOOR_BIKE_DATA, value: vec![0x44, 0, 0xB8, 0x0B, 0xB4, 0, 0xC8, 0] }).is_empty());
        assert!(m.handle(s(0.8), Input::SubscribeFailed { conn: 1, characteristic: gatt::INDOOR_BIKE_DATA }).is_empty());
        assert!(m.handle(s(0.9), Input::WriteFailed { conn: 1, error: "?".into() }).is_empty());
        assert!(m.take_events().is_empty());
        assert!(!m.is_connected());
    }

    #[test]
    fn results_from_an_older_connection_are_ignored() {
        let mut m = Machine::new(Target::any(Role::Trainer), s(0.0));
        m.handle(s(0.0), Input::Radio(RadioState::On));
        m.handle(s(0.1), Input::Advertised(avanti_ad()));
        let first = conn(&m);
        m.handle(s(1.0), Input::Disconnected { address: AVANTI.into() }); // mid-connect
        m.handle(s(1.1), Input::Advertised(avanti_ad()));
        let second = conn(&m);
        assert_ne!(first, second);
        assert!(m.handle(s(1.2), Input::Connected { conn: first }).is_empty(), "the old connect resolving late");
        assert!(m.handle(s(1.3), Input::ConnectFailed { conn: first, error: "old".into() }).is_empty());
        assert_eq!(m.handle(s(1.4), Input::Connected { conn: second }), vec![Action::Discover]);
        assert!(m.handle(s(1.5), Input::Discovered { conn: first, characteristics: TRAINER_CHARS.to_vec() }).is_empty());
        assert!(!m.handle(s(1.6), Input::Discovered { conn: second, characteristics: TRAINER_CHARS.to_vec() }).is_empty());
        // A late subscription failure from the old connection leaves the
        // new one in control.
        m.handle(s(1.7), ok(&m, gatt::OP_REQUEST_CONTROL));
        m.handle(s(1.8), ok(&m, gatt::OP_START));
        m.handle(s(1.9), Input::SubscribeFailed { conn: first, characteristic: gatt::FTMS_CONTROL_POINT });
        assert_eq!(m.send(s(2.0), ControlId(1), Control::TargetPower(150)), vec![write(&TARGET_150)]);
    }

    #[test]
    fn a_slow_connect_is_never_doubled() {
        // Trap 2: however long the connect takes, no second one.
        let mut m = Machine::new(Target::any(Role::Trainer), s(0.0));
        m.handle(s(0.0), Input::Radio(RadioState::On));
        m.handle(s(0.1), Input::Advertised(avanti_ad()));
        let c = conn(&m);
        for t in 1..600 {
            assert!(m.handle(s(t as f32), Input::Tick).is_empty());
            assert!(m.handle(s(t as f32), Input::Advertised(avanti_ad())).is_empty(), "a new advertisement starts nothing either");
        }
        assert_eq!(m.handle(s(600.0), Input::Connected { conn: c }), vec![Action::Discover]);
    }

    #[test]
    fn a_drop_while_connecting_starts_over() {
        // Trap 3: that connect will never resolve, so scan and connect anew.
        let mut m = Machine::new(Target::any(Role::Trainer), s(0.0));
        m.handle(s(0.0), Input::Radio(RadioState::On));
        m.handle(s(0.1), Input::Advertised(avanti_ad()));
        assert_eq!(
            m.handle(s(5.0), Input::Disconnected { address: AVANTI.into() }),
            vec![Action::StartScan { service: gatt::FITNESS_MACHINE }]
        );
        assert_eq!(m.handle(s(6.0), Input::Advertised(avanti_ad())).len(), 2);
        assert!(m.take_events().is_empty(), "never connected: nothing lost");
    }

    #[test]
    fn a_failed_connect_waits_for_the_next_advertisement() {
        let mut m = Machine::new(Target::any(Role::Trainer), s(0.0));
        m.handle(s(0.0), Input::Radio(RadioState::On));
        m.handle(s(0.1), Input::Advertised(avanti_ad()));
        let c = conn(&m);
        assert_eq!(
            m.handle(s(1.0), Input::ConnectFailed { conn: c, error: "busy".into() }),
            vec![Action::StartScan { service: gatt::FITNESS_MACHINE }]
        );
        assert_eq!(m.handle(s(2.0), Input::Advertised(avanti_ad())).len(), 2);
    }

    #[test]
    fn a_drop_is_lost_once_then_found_by_scanning_then_back() {
        // Trap 1: the old handle is dead; find it again by scanning.
        let mut m = ready_trainer();
        m.take_events();
        assert_eq!(
            m.handle(s(100.0), Input::Disconnected { address: AVANTI.into() }),
            vec![Action::StartScan { service: gatt::FITNESS_MACHINE }]
        );
        // The first try drops again while discovering: still one Lost.
        m.handle(s(101.0), Input::Advertised(avanti_ad()));
        m.handle(s(102.0), Input::Connected { conn: conn(&m) });
        assert!(m.is_connected());
        m.handle(s(103.0), Input::Disconnected { address: AVANTI.into() });
        let actions = connect(&mut m, 110.0, &TRAINER_CHARS);
        assert!(actions.contains(&write(&[gatt::OP_REQUEST_CONTROL])), "control is taken again");
        assert_eq!(
            m.take_events(),
            vec![LinkEvent::Lost { at: s(100.0) }, LinkEvent::Back { at: s(111.0), outage: s(111.0) - s(100.0) }]
        );
    }

    #[test]
    fn a_silent_trainer_under_control_is_hung_up_on_and_found_again() {
        let mut m = ready_trainer();
        m.take_events();
        m.send(s(9.0), ControlId(1), Control::TargetPower(150));
        m.handle(s(9.1), ok(&m, gatt::OP_SET_TARGET_POWER));
        let bike = [0x44, 0x00, 0xB8, 0x0B, 0xB4, 0x00, 0xC8, 0x00];
        m.handle(s(10.0), data(&m, gatt::INDOOR_BIKE_DATA, &bike));
        assert!(m.handle(s(24.9), Input::Tick).is_empty());
        assert_eq!(
            m.handle(s(25.0), Input::Tick),
            vec![Action::Disconnect, Action::StartScan { service: gatt::FITNESS_MACHINE }]
        );
        assert!(m.take_events().contains(&LinkEvent::Lost { at: s(25.0) }));
    }

    #[test]
    fn a_quiet_trainer_nobody_controls_is_left_alone() {
        // Whether a trainer at rest keeps sending is not measured.
        let mut m = ready_trainer();
        assert!(m.handle(s(600.0), Input::Tick).is_empty());
        m.send(s(600.0), ControlId(1), Control::TargetPower(150));
        m.handle(s(600.1), ok(&m, gatt::OP_SET_TARGET_POWER));
        assert!(m.handle(s(630.2), Input::Tick).is_empty(), "the last control is 30 s old");
    }

    #[test]
    fn a_ridden_trainer_that_goes_silent_is_lost_even_without_erg() {
        let mut m = ready_trainer();
        m.take_events();
        m.handle(s(10.0), data(&m, gatt::INDOOR_BIKE_DATA, &[0x05, 0x00, 0xA0, 0x00])); // 80 rpm
        assert!(m.handle(s(24.9), Input::Tick).is_empty());
        assert_eq!(m.handle(s(25.0), Input::Tick)[0], Action::Disconnect);

        // Pedalling, then stopped: the trainer reads 0 rpm, then may go quiet.
        let mut at_rest = ready_trainer();
        at_rest.handle(s(10.0), data(&at_rest, gatt::INDOOR_BIKE_DATA, &[0x05, 0x00, 0xA0, 0x00])); // 80 rpm
        at_rest.handle(s(12.0), data(&at_rest, gatt::INDOOR_BIKE_DATA, &[0x05, 0x00, 0x00, 0x00])); // 0 rpm
        for t in 13..60 {
            assert!(at_rest.handle(s(t as f32), Input::Tick).is_empty(), "a standing trainer may be quiet ({t} s)");
        }
    }

    #[test]
    fn a_control_point_that_will_not_subscribe_leaves_a_read_only_trainer() {
        // Indications that want pairing; the data is open.
        let mut m = ready_trainer();
        m.take_events();
        m.send(s(2.0), ControlId(1), Control::TargetPower(150));
        let bike = [0x41, 0x00, 0x96, 0x00];
        for i in 0..3 {
            let t = 3.0 + i as f32;
            m.handle(s(t), data(&m, gatt::INDOOR_BIKE_DATA, &bike));
            let c = conn(&m);
            assert!(m.handle(s(t + 0.5), Input::SubscribeFailed { conn: c, characteristic: gatt::FTMS_CONTROL_POINT }).is_empty());
        }
        m.handle(s(7.0), data(&m, gatt::INDOOR_BIKE_DATA, &bike));
        assert!(m.send(s(8.0), ControlId(2), Control::TargetPower(150)).is_empty());
        let events = m.take_events();
        assert!(!events.iter().any(|e| matches!(e, LinkEvent::Lost { .. } | LinkEvent::Failed { .. })), "{events:?}");
        assert_eq!(events.iter().filter(|e| matches!(e, LinkEvent::Reading(_))).count(), 4, "the readings go on");
        assert_eq!(controls(&events), vec![(ControlId(1), ControlOutcome::NotAcknowledged), (ControlId(2), ControlOutcome::Refused)]);
    }

    #[test]
    fn quiet_pedals_are_not_hung_up_on() {
        let mut p = Machine::new(Target::any(Role::Pedals), s(0.0));
        p.handle(s(0.0), Input::Radio(RadioState::On));
        p.handle(s(0.1), Input::Advertised(Advert { address: "P".into(), name: None, services: vec![gatt::CYCLING_POWER] }));
        let c = conn(&p);
        p.handle(s(0.2), Input::Connected { conn: c });
        p.handle(s(0.3), Input::Discovered { conn: c, characteristics: vec![gatt::CYCLING_POWER_MEASUREMENT] });
        assert!(p.handle(s(600.0), Input::Tick).is_empty(), "pedals at rest send nothing");
    }

    #[test]
    fn a_failed_subscription_hangs_up_and_starts_over() {
        let mut m = ready_trainer();
        m.take_events();
        let c = conn(&m);
        assert!(m.handle(s(3.0), Input::SubscribeFailed { conn: c + 1, characteristic: gatt::INDOOR_BIKE_DATA }).is_empty(), "not this connection");
        assert_eq!(
            m.handle(s(3.0), Input::SubscribeFailed { conn: c, characteristic: gatt::INDOOR_BIKE_DATA }),
            vec![Action::Disconnect, Action::StartScan { service: gatt::FITNESS_MACHINE }]
        );
        assert_eq!(m.take_events(), vec![LinkEvent::Lost { at: s(3.0) }]);
    }

    #[test]
    fn subscriptions_failing_connection_after_connection_end_the_link() {
        let mut m = ready_trainer();
        m.take_events();
        let fail_subscription = |m: &mut Machine, at: f32| {
            let c = conn(m);
            m.handle(s(at), Input::SubscribeFailed { conn: c, characteristic: gatt::INDOOR_BIKE_DATA })
        };
        fail_subscription(&mut m, 3.0);
        connect(&mut m, 4.0, &TRAINER_CHARS);
        // Data on this connection: the count starts over.
        m.handle(s(5.5), data(&m, gatt::INDOOR_BIKE_DATA, &[0x41, 0x00, 0x96, 0x00]));
        fail_subscription(&mut m, 6.0);
        connect(&mut m, 7.0, &TRAINER_CHARS);
        fail_subscription(&mut m, 9.0);
        connect(&mut m, 10.0, &TRAINER_CHARS);
        assert_eq!(fail_subscription(&mut m, 12.0), vec![Action::Disconnect], "the third in a row");
        assert!(matches!(m.take_events().last(), Some(LinkEvent::Failed { reason: LinkFailure::Other(_), .. })));
    }

    #[test]
    fn a_device_without_the_measurement_or_with_broken_discovery_fails() {
        let mut m = Machine::new(Target::any(Role::Trainer), s(0.0));
        m.handle(s(0.0), Input::Radio(RadioState::On));
        m.handle(s(0.1), Input::Advertised(avanti_ad()));
        let c = conn(&m);
        m.handle(s(0.2), Input::Connected { conn: c });
        m.send(s(0.25), ControlId(1), Control::TargetPower(150));
        m.handle(s(0.3), Input::Discovered { conn: c, characteristics: vec![gatt::FTMS_CONTROL_POINT] });
        let events = m.take_events();
        assert_eq!(controls(&events), vec![(ControlId(1), ControlOutcome::Refused)]);
        assert!(matches!(events.last(), Some(LinkEvent::Failed { reason: LinkFailure::Other(_), .. })), "{events:?}");

        let mut flaky = Machine::new(Target::any(Role::Trainer), s(0.0));
        flaky.handle(s(0.0), Input::Radio(RadioState::On));
        flaky.handle(s(0.1), Input::Advertised(avanti_ad()));
        let c = conn(&flaky);
        flaky.handle(s(0.2), Input::Connected { conn: c });
        let again = Input::DiscoveryFailed { conn: c, error: "timeout".into() };
        assert_eq!(flaky.handle(s(1.0), again.clone()), vec![Action::Discover]);
        assert_eq!(flaky.handle(s(2.0), again.clone()), vec![Action::Discover]);
        assert!(flaky.handle(s(3.0), again).is_empty());
        assert!(matches!(flaky.take_events().last(), Some(LinkEvent::Failed { .. })));
    }

    #[test]
    fn the_radio_decides_whether_there_is_a_link_at_all() {
        let mut off = Machine::new(Target::any(Role::Trainer), s(0.0));
        off.handle(s(1.0), Input::Radio(RadioState::Off));
        assert_eq!(off.take_events(), vec![LinkEvent::Failed { at: s(1.0), reason: LinkFailure::RadioOff }]);
        assert!(off.handle(s(2.0), Input::Radio(RadioState::On)).is_empty(), "failed is the end");

        let mut silent = Machine::new(Target::any(Role::Trainer), s(10.0));
        silent.handle(s(10.0), Input::Radio(RadioState::Unknown));
        assert!(silent.handle(s(69.9), Input::Tick).is_empty(), "time to answer the permission dialog");
        silent.handle(s(70.0), Input::Tick);
        assert_eq!(silent.take_events(), vec![LinkEvent::Failed { at: s(70.0), reason: LinkFailure::PermissionDenied }]);

        let mut late = Machine::new(Target::any(Role::Trainer), s(0.0));
        late.handle(s(1.0), Input::Radio(RadioState::Unknown));
        assert_eq!(late.handle(s(30.0), Input::Radio(RadioState::On)).len(), 1, "allowed after 30 s");
        late.handle(s(100.0), Input::Tick);
        assert!(late.take_events().is_empty());
    }

    // --- readings ---------------------------------------------------------

    #[test]
    fn trainer_readings() {
        let mut m = ready_trainer();
        m.take_events();
        m.handle(s(3.0), data(&m, gatt::INDOOR_BIKE_DATA, &[0x44, 0x00, 0xB8, 0x0B, 0xB4, 0x00, 0xC8, 0x00]));
        // More data (no speed) + power: −100 W, which reads 0.
        m.handle(s(3.1), data(&m, gatt::INDOOR_BIKE_DATA, &[0x41, 0x00, 0x9C, 0xFF]));
        m.handle(s(3.2), data(&m, gatt::INDOOR_BIKE_DATA, &[0x40])); // broken
        m.handle(s(3.3), data(&m, gatt::HEART_RATE_MEASUREMENT, &[0x00, 70])); // not ours
        // 89.5 rpm (179 half-rpm) rounds to 90; 300 rpm (600) is a wrapped counter.
        m.handle(s(3.4), data(&m, gatt::INDOOR_BIKE_DATA, &[0x45, 0x00, 0xB3, 0x00, 0x2C, 0x01]));
        m.handle(s(3.5), data(&m, gatt::INDOOR_BIKE_DATA, &[0x05, 0x00, 0x58, 0x02]));
        assert_eq!(
            m.take_events(),
            vec![
                LinkEvent::Reading(Reading { at: s(3.0), watts: Some(200), cadence_rpm: Some(90), speed_kmh: Some(30.0), ..Reading::default() }),
                LinkEvent::Reading(Reading { at: s(3.1), watts: Some(0), ..Reading::default() }),
                LinkEvent::Reading(Reading { at: s(3.4), watts: Some(300), cadence_rpm: Some(90), ..Reading::default() }),
                LinkEvent::Reading(Reading { at: s(3.5), ..Reading::default() }),
            ]
        );
    }

    fn pedals() -> Machine {
        let mut p = Machine::new(Target::any(Role::Pedals), s(0.0));
        p.handle(s(0.0), Input::Radio(RadioState::On));
        let ad = Advert { address: "P".into(), name: Some("ASSIOMA55430L".into()), services: vec![gatt::CYCLING_POWER] };
        p.handle(s(0.1), Input::Advertised(ad));
        let c = conn(&p);
        p.handle(s(0.2), Input::Connected { conn: c });
        // Even a control point listed by a power meter is not used.
        let actions = p.handle(s(0.3), Input::Discovered { conn: c, characteristics: vec![gatt::CYCLING_POWER_MEASUREMENT, gatt::FTMS_CONTROL_POINT] });
        assert_eq!(actions, vec![Action::Subscribe { characteristic: gatt::CYCLING_POWER_MEASUREMENT }], "no control point, no writes");
        p.take_events();
        p
    }

    fn crank(m: &Machine, revs: u16, time: u16) -> Input {
        let [r0, r1] = revs.to_le_bytes();
        let [t0, t1] = time.to_le_bytes();
        data(m, gatt::CYCLING_POWER_MEASUREMENT, &[0x23, 0x00, 0xFA, 0x00, 0x68, r0, r1, t0, t1])
    }

    fn readings(events: Vec<LinkEvent>) -> Vec<Reading> {
        events.into_iter().filter_map(|e| if let LinkEvent::Reading(r) = e { Some(r) } else { None }).collect()
    }

    fn pedal_readings(events: Vec<LinkEvent>) -> Vec<(Option<u16>, Option<u8>, Option<u8>)> {
        readings(events).into_iter().map(|r| (r.watts, r.cadence_rpm, r.balance_right)).collect()
    }

    #[test]
    fn pedal_readings_with_cadence_from_the_crank() {
        let mut p = pedals();
        p.handle(s(1.0), crank(&p, 10, 1024));
        p.handle(s(2.0), crank(&p, 11, 2048));
        p.handle(s(3.0), crank(&p, 12, 2048 + 341)); // 1/3 s for a turn: 180 rpm
        assert_eq!(
            pedal_readings(p.take_events()),
            vec![(Some(250), None, Some(48)), (Some(250), Some(60), Some(48)), (Some(250), Some(180), Some(48))]
        );
        assert!(p.send(s(4.0), ControlId(1), Control::Release).is_empty());
        assert_eq!(controls(&p.take_events()), vec![(ControlId(1), ControlOutcome::Refused)], "pedals take no controls");
        p.handle(s(4.1), data(&p, gatt::FTMS_CONTROL_POINT, &[0x80, 0x05, 0x01]));
        assert!(p.take_events().is_empty(), "nor read control point answers");
    }

    #[test]
    fn cadence_starts_afresh_after_a_reconnect() {
        let mut p = pedals();
        p.handle(s(1.0), crank(&p, 10, 1024));
        p.handle(s(2.0), crank(&p, 11, 2048));
        p.handle(s(3.0), Input::Disconnected { address: "P".into() });
        p.handle(s(73.0), Input::Advertised(Advert { address: "P".into(), name: None, services: vec![gatt::CYCLING_POWER] }));
        let c = conn(&p);
        p.handle(s(73.1), Input::Connected { conn: c });
        p.handle(s(73.2), Input::Discovered { conn: c, characteristics: vec![gatt::CYCLING_POWER_MEASUREMENT] });
        p.take_events();
        // 70 s at 90 rpm later: the 1/1024 s counter wrapped. The first
        // event after the reconnect is no cadence at all, not a crazy one.
        p.handle(s(74.0), crank(&p, 11 + 105, 2048 + 600));
        p.handle(s(75.0), crank(&p, 11 + 106, 2048 + 600 + 683)); // 90 rpm
        assert_eq!(pedal_readings(p.take_events()), vec![(Some(250), None, Some(48)), (Some(250), Some(90), Some(48))]);
    }

    #[test]
    fn heart_rate_readings() {
        let mut strap = Machine::new(Target::any(Role::Heart), s(0.0));
        strap.handle(s(0.0), Input::Radio(RadioState::On));
        strap.handle(s(0.1), Input::Advertised(Advert { address: "H".into(), name: None, services: vec![gatt::HEART_RATE] }));
        let c = conn(&strap);
        strap.handle(s(0.2), Input::Connected { conn: c });
        strap.handle(s(0.3), Input::Discovered { conn: c, characteristics: vec![gatt::HEART_RATE_MEASUREMENT] });
        strap.take_events();
        strap.handle(s(1.0), data(&strap, gatt::HEART_RATE_MEASUREMENT, &[0x00, 72]));
        strap.handle(s(2.0), data(&strap, gatt::HEART_RATE_MEASUREMENT, &[0b100, 72])); // no skin contact
        strap.handle(s(3.0), data(&strap, gatt::HEART_RATE_MEASUREMENT, &[0x01, 0x2C, 0x01])); // 300 in u16
        let bpm: Vec<Option<u8>> = readings(strap.take_events()).into_iter().map(|r| r.heart_rate).collect();
        assert_eq!(bpm, vec![Some(72), Some(255)], "300 does not wrap to 44");
    }

    // --- controls -----------------------------------------------------------

    #[test]
    fn controls_go_one_at_a_time_and_get_their_answers() {
        let mut m = ready_trainer();
        m.take_events();
        assert_eq!(m.send(s(3.0), ControlId(1), Control::TargetPower(150)), vec![write(&TARGET_150)]);
        assert!(m.send(s(3.1), ControlId(2), Control::TargetPower(160)).is_empty(), "one in flight");
        assert_eq!(m.handle(s(3.2), ok(&m, gatt::OP_SET_TARGET_POWER)), vec![write(&[0x05, 0xA0, 0x00])]);
        assert!(m.handle(s(3.3), failed(&m, gatt::OP_SET_TARGET_POWER)).is_empty());
        assert!(m.handle(s(3.4), ok(&m, gatt::OP_SET_TARGET_POWER)).is_empty(), "an answer to nothing");
        assert_eq!(
            controls(&m.take_events()),
            vec![(ControlId(1), ControlOutcome::Acknowledged), (ControlId(2), ControlOutcome::NotAcknowledged)]
        );
    }

    #[test]
    fn a_newer_target_replaces_the_queued_one() {
        let mut m = ready_trainer();
        m.take_events();
        m.send(s(3.0), ControlId(1), Control::TargetPower(150)); // in flight
        m.send(s(3.1), ControlId(2), Control::TargetPower(150)); // queued
        m.send(s(3.2), ControlId(3), Control::Resistance(10)); // queued, another kind
        m.send(s(3.3), ControlId(4), Control::TargetPower(250)); // replaces 2
        assert_eq!(controls(&m.take_events()), vec![(ControlId(2), ControlOutcome::Refused)]);
        assert_eq!(m.handle(s(3.4), ok(&m, gatt::OP_SET_TARGET_POWER)), vec![write(&[gatt::OP_SET_TARGET_RESISTANCE, 10])]);
        m.send(s(3.5), ControlId(5), Control::Release); // replaces 4
        assert_eq!(controls(&m.take_events()), vec![(ControlId(1), ControlOutcome::Acknowledged), (ControlId(4), ControlOutcome::Refused)]);
        assert_eq!(m.handle(s(3.6), ok(&m, gatt::OP_SET_TARGET_RESISTANCE)), vec![write(&[gatt::OP_RESET])]);
    }

    #[test]
    fn an_unanswered_control_times_out_and_its_late_answer_is_not_the_next_ones() {
        let mut m = ready_trainer();
        m.take_events();
        m.send(s(3.0), ControlId(1), Control::TargetPower(150));
        m.send(s(4.0), ControlId(2), Control::TargetPower(150));
        assert!(m.handle(s(5.9), Input::Tick).is_empty());
        // Timed out: the line is kept clear for its late answer.
        assert!(m.handle(s(6.0), Input::Tick).is_empty());
        assert_eq!(controls(&m.take_events()), vec![(ControlId(1), ControlOutcome::TimedOut)]);
        // The late answer, a failure: it is #1's, not #2's. Then #2 goes.
        assert_eq!(m.handle(s(7.0), failed(&m, gatt::OP_SET_TARGET_POWER)), vec![write(&TARGET_150)]);
        m.handle(s(7.5), ok(&m, gatt::OP_SET_TARGET_POWER));
        assert_eq!(controls(&m.take_events()), vec![(ControlId(2), ControlOutcome::Acknowledged)]);
    }

    #[test]
    fn a_late_answer_that_never_comes_holds_the_line_one_timeout_more() {
        let mut m = ready_trainer();
        m.take_events();
        m.send(s(3.0), ControlId(1), Control::TargetPower(150));
        m.send(s(4.0), ControlId(2), Control::TargetPower(150));
        m.handle(s(6.0), Input::Tick); // #1 timed out
        assert!(m.handle(s(8.9), Input::Tick).is_empty());
        assert_eq!(m.handle(s(9.0), Input::Tick), vec![write(&TARGET_150)]);
    }

    #[test]
    fn the_profile_sets_how_long_an_answer_may_take() {
        static PATIENT: TrainerProfile = TrainerProfile {
            key: "patient",
            name: "Patient",
            matches: &[],
            erg_resend: Duration::from_secs(2),
            trust_target_flags: false,
            ftms_start_after_reconnect: StartAfterReconnect::MayFail,
            control_answer_timeout: Duration::from_secs(5),
            link_drops_routine: true,
            release_budget: Duration::from_secs(15),
        };
        let mut m = ready_trainer();
        m.use_profile(&PATIENT);
        m.take_events();
        m.send(s(3.0), ControlId(1), Control::TargetPower(150));
        m.handle(s(6.0), Input::Tick); // 3 s: GENERIC would give up here
        m.handle(s(7.9), Input::Tick);
        assert!(m.take_events().is_empty());
        m.handle(s(8.0), Input::Tick);
        assert_eq!(controls(&m.take_events()), vec![(ControlId(1), ControlOutcome::TimedOut)]);
    }

    #[test]
    fn a_failed_write_is_not_acknowledged_and_the_next_one_goes() {
        let mut m = ready_trainer();
        m.take_events();
        m.send(s(3.0), ControlId(1), Control::Resistance(40));
        m.send(s(3.0), ControlId(2), Control::TargetPower(150));
        assert!(m.handle(s(3.1), Input::WriteFailed { conn: conn(&m) + 1, error: "old".into() }).is_empty());
        assert_eq!(m.handle(s(3.1), Input::WriteFailed { conn: conn(&m), error: "gatt".into() }), vec![write(&TARGET_150)]);
        assert_eq!(controls(&m.take_events()), vec![(ControlId(1), ControlOutcome::NotAcknowledged)]);
    }

    #[test]
    fn controls_before_connecting_are_refused() {
        let mut m = Machine::new(Target::any(Role::Trainer), s(0.0));
        assert!(m.send(s(0.0), ControlId(1), Control::TargetPower(150)).is_empty());
        assert_eq!(controls(&m.take_events()), vec![(ControlId(1), ControlOutcome::Refused)]);
    }

    #[test]
    fn a_drop_settles_what_is_owed() {
        let mut m = ready_trainer();
        m.take_events();
        m.send(s(3.0), ControlId(1), Control::TargetPower(150));
        m.send(s(3.0), ControlId(2), Control::Resistance(10));
        m.handle(s(3.5), Input::Disconnected { address: AVANTI.into() });
        assert_eq!(
            controls(&m.take_events()),
            vec![(ControlId(1), ControlOutcome::NotAcknowledged), (ControlId(2), ControlOutcome::Refused)]
        );
        assert!(m.handle(s(10.0), Input::Tick).is_empty());
        m.send(s(11.0), ControlId(3), Control::TargetPower(150));
        assert_eq!(controls(&m.take_events()), vec![(ControlId(3), ControlOutcome::Refused)], "while down");
    }

    #[test]
    fn failing_settles_controls_in_flight_and_queued() {
        let mut m = ready_trainer();
        m.take_events();
        m.send(s(3.0), ControlId(1), Control::TargetPower(150));
        m.send(s(3.0), ControlId(2), Control::Resistance(10));
        m.handle(s(3.5), Input::Radio(RadioState::Off));
        let events = m.take_events();
        assert!(matches!(events[0], LinkEvent::Failed { .. }));
        assert_eq!(controls(&events), vec![(ControlId(1), ControlOutcome::NotAcknowledged), (ControlId(2), ControlOutcome::Refused)]);
        assert!(!m.is_connected());
    }

    // --- taking control -------------------------------------------------------

    #[test]
    fn the_avanti_failing_start_after_a_reconnect_is_excused() {
        let mut m = ready_trainer();
        m.use_profile(&crate::trainer::profile::ELITE_AVANTI);
        reconnect(&mut m, 100.0);
        m.handle(s(102.0), ok(&m, gatt::OP_REQUEST_CONTROL));
        m.handle(s(102.1), failed(&m, gatt::OP_START));
        assert_eq!(m.send(s(104.0), ControlId(9), Control::TargetPower(150)), vec![write(&TARGET_150)], "straight to the target");
    }

    #[test]
    fn a_failed_start_on_the_first_connect_is_excused_too() {
        // To the trainer a new session or a restarted app is a reconnect.
        let mut m = Machine::new(Target::any(Role::Trainer), s(0.0));
        m.use_profile(&crate::trainer::profile::ELITE_AVANTI);
        m.handle(s(0.0), Input::Radio(RadioState::On));
        connect(&mut m, 0.5, &TRAINER_CHARS);
        m.handle(s(1.6), ok(&m, gatt::OP_REQUEST_CONTROL));
        m.handle(s(1.7), failed(&m, gatt::OP_START));
        assert_eq!(m.send(s(3.0), ControlId(1), Control::TargetPower(150)), vec![write(&TARGET_150)]);
    }

    static STRICT_START: TrainerProfile = TrainerProfile {
        key: "strict-start",
        name: "Strict start",
        matches: &[],
        erg_resend: Duration::from_secs(2),
        trust_target_flags: false,
        ftms_start_after_reconnect: StartAfterReconnect::MustSucceed,
        control_answer_timeout: Duration::from_secs(3),
        link_drops_routine: true,
        release_budget: Duration::from_secs(15),
    };

    /// Answers every control point write at once: request control ok,
    /// start `start`, anything else ok. Counts the writes of each op.
    fn answer_at_once(m: &mut Machine, at: f32, actions: Vec<Action>, start: u8, writes: &mut [u32; 256]) {
        let mut pending = actions;
        let mut guard = 0;
        while let Some(action) = pending.pop() {
            guard += 1;
            assert!(guard < 1000, "a write loop inside one moment");
            if let Action::Write { bytes, .. } = action {
                writes[bytes[0] as usize] += 1;
                let result = if bytes[0] == gatt::OP_START { start } else { gatt::RESULT_SUCCESS };
                pending.extend(m.handle(s(at), cp(m, bytes[0], result)));
            }
        }
    }

    #[test]
    fn a_start_that_always_fails_is_retried_at_most_once_per_resend() {
        // A trainer whose start must succeed and never does, answering at
        // once: the retakes are paced by erg_resend, never a loop.
        let mut m = Machine::new(Target::any(Role::Trainer), s(0.0));
        m.use_profile(&STRICT_START);
        m.handle(s(0.0), Input::Radio(RadioState::On));
        let mut writes = [0u32; 256];
        let first = connect(&mut m, 0.0, &TRAINER_CHARS);
        answer_at_once(&mut m, 1.0, first, 0x04, &mut writes);
        for i in 0..240u64 {
            let t = 1.25 + 0.25 * i as f32;
            if i % 8 == 0 {
                let actions = m.send(s(t), ControlId(i), Control::TargetPower(150));
                answer_at_once(&mut m, t, actions, 0x04, &mut writes);
            }
            let actions = m.handle(s(t), Input::Tick);
            answer_at_once(&mut m, t, actions, 0x04, &mut writes);
        }
        let requests = writes[gatt::OP_REQUEST_CONTROL as usize];
        assert!((25..=32).contains(&requests), "about one per 2 s over 60 s: {requests}");
        assert_eq!(writes[gatt::OP_SET_TARGET_POWER as usize], 0, "a target never goes while control is not ours");
    }

    #[test]
    fn a_start_answered_not_permitted_takes_control_again_even_when_start_may_fail() {
        let mut m = Machine::new(Target::any(Role::Trainer), s(0.0));
        m.use_profile(&crate::trainer::profile::ELITE_AVANTI);
        m.handle(s(0.0), Input::Radio(RadioState::On));
        connect(&mut m, 0.0, &TRAINER_CHARS);
        m.handle(s(1.1), ok(&m, gatt::OP_REQUEST_CONTROL));
        m.handle(s(1.2), not_permitted(&m, gatt::OP_START));
        assert_eq!(m.send(s(3.0), ControlId(1), Control::TargetPower(150)), vec![write(&[gatt::OP_REQUEST_CONTROL])]);
    }

    #[test]
    fn a_reconnect_starts_with_a_clean_control_line() {
        // A pending retake and a line held for a late answer belong to the
        // old connection.
        let mut m = ready_trainer();
        m.send(s(3.0), ControlId(1), Control::Release);
        m.handle(s(3.1), ok(&m, gatt::OP_RESET)); // control handed back
        m.send(s(4.0), ControlId(2), Control::TargetPower(150)); // retake: request control in flight
        m.handle(s(7.0), Input::Tick); // it timed out: the line is held until 10 s
        let actions = reconnect(&mut m, 7.0); // discovered again at 9 s
        assert!(actions.contains(&write(&[gatt::OP_REQUEST_CONTROL])), "not held by the old line: {actions:?}");
        m.handle(s(9.1), ok(&m, gatt::OP_REQUEST_CONTROL));
        m.handle(s(9.2), ok(&m, gatt::OP_START));
        assert_eq!(m.send(s(9.3), ControlId(3), Control::TargetPower(150)), vec![write(&TARGET_150)], "no second handshake");
    }

    #[test]
    fn a_retake_owed_by_the_old_connection_is_not_carried_over() {
        // A Release handed control back, nothing queued yet: the retake is
        // owed. The reconnect takes control anyway; no second handshake.
        let mut m = ready_trainer();
        m.send(s(3.0), ControlId(1), Control::Release);
        m.handle(s(3.1), ok(&m, gatt::OP_RESET));
        reconnect(&mut m, 5.0);
        m.handle(s(7.1), ok(&m, gatt::OP_REQUEST_CONTROL));
        m.handle(s(7.2), ok(&m, gatt::OP_START));
        assert_eq!(m.send(s(7.3), ControlId(2), Control::TargetPower(150)), vec![write(&TARGET_150)]);
    }

    #[test]
    fn cadence_up_to_250_rpm_is_a_rider() {
        let mut m = ready_trainer();
        m.take_events();
        m.handle(s(3.0), data(&m, gatt::INDOOR_BIKE_DATA, &[0x05, 0x00, 0xF4, 0x01])); // 500 half-rpm
        m.handle(s(3.1), data(&m, gatt::INDOOR_BIKE_DATA, &[0x05, 0x00, 0xF6, 0x01])); // 251
        let rpm: Vec<Option<u8>> = readings(m.take_events()).into_iter().map(|r| r.cadence_rpm).collect();
        assert_eq!(rpm, vec![Some(250), None]);
    }

    #[test]
    fn a_start_that_must_succeed_is_taken_again_before_the_next_control() {
        static STRICT: TrainerProfile = TrainerProfile {
            key: "strict",
            name: "Strict",
            matches: &[],
            erg_resend: Duration::from_secs(2),
            trust_target_flags: false,
            ftms_start_after_reconnect: StartAfterReconnect::MustSucceed,
            control_answer_timeout: Duration::from_secs(3),
            link_drops_routine: true,
            release_budget: Duration::from_secs(15),
        };
        let mut m = ready_trainer();
        m.use_profile(&STRICT);
        reconnect(&mut m, 100.0);
        m.handle(s(102.0), ok(&m, gatt::OP_REQUEST_CONTROL));
        m.handle(s(102.1), failed(&m, gatt::OP_START));
        assert_eq!(m.send(s(104.0), ControlId(9), Control::TargetPower(150)), vec![write(&[gatt::OP_REQUEST_CONTROL])]);
    }

    #[test]
    fn slow_handshake_answers_are_no_refusal() {
        // After a reconnect the Avanti's answers took longer than 3 s; its
        // profile waits 5 s.
        let mut m = ready_trainer();
        m.use_profile(&crate::trainer::profile::ELITE_AVANTI);
        reconnect(&mut m, 100.0); // request control written at 102 s
        assert!(m.handle(s(106.9), Input::Tick).is_empty(), "still waiting");
        assert!(m.handle(s(107.0), Input::Tick).is_empty(), "request control timed out; its answer may still come");
        assert_eq!(m.handle(s(107.5), ok(&m, gatt::OP_REQUEST_CONTROL)), vec![write(&[gatt::OP_START])], "late, then on to start");
        m.handle(s(112.5), Input::Tick); // start timed out too
        m.handle(s(117.5), Input::Tick); // and never answered
        assert_eq!(m.send(s(118.0), ControlId(1), Control::TargetPower(150)), vec![write(&TARGET_150)], "no new handshake");
    }

    #[test]
    fn a_trainer_held_by_another_app_is_asked_again_now_and_then_and_every_control_settled() {
        let mut m = Machine::new(Target::any(Role::Trainer), s(0.0));
        m.handle(s(0.0), Input::Radio(RadioState::On));
        connect(&mut m, 0.0, &TRAINER_CHARS);
        assert!(m.handle(s(1.1), not_permitted(&m, gatt::OP_REQUEST_CONTROL)).is_empty(), "no start after it");
        let mut writes = 0;
        // A minute of a target every 2 s, every answer "not permitted".
        for i in 0..30u64 {
            let t = 2.0 + 2.0 * i as f32;
            for a in m.send(s(t), ControlId(i), Control::TargetPower(150)) {
                assert_eq!(a, write(&[gatt::OP_REQUEST_CONTROL]));
                writes += 1;
                m.handle(s(t + 0.1), not_permitted(&m, gatt::OP_REQUEST_CONTROL));
            }
            for tick in 1..4 {
                assert!(m.handle(s(t + 0.5 * tick as f32), Input::Tick).is_empty(), "time alone asks nothing");
            }
        }
        assert!(writes <= 31, "at most one request per erg_resend, not a storm: {writes}");
        let outcomes = controls(&m.take_events());
        assert_eq!(outcomes.len(), 30, "every control settled");
        assert!(outcomes.iter().all(|&(_, o)| o == ControlOutcome::Refused));
    }

    #[test]
    fn release_hands_control_back_and_the_next_target_takes_it_again() {
        let mut m = ready_trainer();
        m.take_events();
        assert_eq!(m.send(s(3.0), ControlId(1), Control::Release), vec![write(&[gatt::OP_RESET])]);
        m.handle(s(3.1), ok(&m, gatt::OP_RESET));
        assert_eq!(m.send(s(10.0), ControlId(2), Control::TargetPower(150)), vec![write(&[gatt::OP_REQUEST_CONTROL])]);
        assert_eq!(m.handle(s(10.1), ok(&m, gatt::OP_REQUEST_CONTROL)), vec![write(&[gatt::OP_START])]);
        assert_eq!(m.handle(s(10.2), ok(&m, gatt::OP_START)), vec![write(&TARGET_150)]);
    }

    #[test]
    fn a_target_answered_not_permitted_takes_control_again() {
        let mut m = ready_trainer();
        m.take_events();
        m.send(s(3.0), ControlId(1), Control::TargetPower(150));
        m.handle(s(3.1), not_permitted(&m, gatt::OP_SET_TARGET_POWER));
        assert_eq!(controls(&m.take_events()), vec![(ControlId(1), ControlOutcome::NotAcknowledged)]);
        assert_eq!(m.send(s(5.0), ControlId(2), Control::TargetPower(150)), vec![write(&[gatt::OP_REQUEST_CONTROL])]);
    }

    #[test]
    fn a_control_point_answer_for_another_step_is_ignored() {
        let mut m = Machine::new(Target::any(Role::Trainer), s(0.0));
        m.handle(s(0.0), Input::Radio(RadioState::On));
        connect(&mut m, 0.0, &TRAINER_CHARS); // request control in flight
        assert!(m.handle(s(1.1), ok(&m, gatt::OP_START)).is_empty(), "not the answer awaited");
        assert!(m.handle(s(1.2), Input::Notified { conn: conn(&m), characteristic: gatt::FTMS_CONTROL_POINT, value: vec![0x05] }).is_empty());
        assert_eq!(m.handle(s(1.3), ok(&m, gatt::OP_REQUEST_CONTROL)), vec![write(&[gatt::OP_START])]);
    }

    // --- the contract, at random -------------------------------------------

    /// Random inputs and controls in any state; the link contract holds:
    /// one outcome per control in the end, time never runs back, nothing
    /// but outcomes after Failed, Lost and Back take turns.
    #[test]
    fn the_link_contract_holds_whatever_the_radio_does() {
        use rand::{rngs::StdRng, Rng, SeedableRng};
        let mut rng = StdRng::seed_from_u64(271);
        for _ in 0..2000 {
            let role = [Role::Trainer, Role::Pedals, Role::Heart][rng.gen_range(0..3)];
            let mut m = Machine::new(Target::any(role), Duration::ZERO);
            if rng.gen_bool(0.5) {
                m.use_profile(&crate::trainer::profile::ELITE_AVANTI);
            }
            let mut t = Duration::ZERO;
            let mut events = Vec::new();
            let mut sent = 0u64;
            for _ in 0..rng.gen_range(10..200) {
                t += Duration::from_millis(rng.gen_range(0..2000));
                let c = m.conn().unwrap_or(0) + rng.gen_range(0..2) * u64::from(rng.gen_bool(0.1));
                let address = if rng.gen_bool(0.9) { AVANTI.to_string() } else { "other".to_string() };
                let op = [gatt::OP_REQUEST_CONTROL, gatt::OP_START, gatt::OP_SET_TARGET_POWER, gatt::OP_RESET][rng.gen_range(0..4)];
                let result = [1, 1, 1, 4, 5][rng.gen_range(0..5)];
                let input = match rng.gen_range(0..14) {
                    0 => Input::Radio([RadioState::On, RadioState::On, RadioState::Unknown, RadioState::Off][rng.gen_range(0..4)]),
                    1 | 2 => Input::Advertised(Advert {
                        address,
                        name: Some("AVANTI COG".into()),
                        services: vec![gatt::FITNESS_MACHINE, gatt::CYCLING_POWER, gatt::HEART_RATE],
                    }),
                    3 => Input::Connected { conn: c },
                    4 => Input::ConnectFailed { conn: c, error: "x".into() },
                    5 => Input::Disconnected { address },
                    6 => Input::Discovered {
                        conn: c,
                        characteristics: vec![
                            gatt::INDOOR_BIKE_DATA,
                            gatt::FTMS_CONTROL_POINT,
                            gatt::CYCLING_POWER_MEASUREMENT,
                            gatt::HEART_RATE_MEASUREMENT,
                        ],
                    },
                    7 => Input::DiscoveryFailed { conn: c, error: "x".into() },
                    8 => Input::SubscribeFailed { conn: c, characteristic: gatt::INDOOR_BIKE_DATA },
                    9 => Input::Notified { conn: c, characteristic: gatt::FTMS_CONTROL_POINT, value: vec![0x80, op, result] },
                    10 => Input::Notified { conn: c, characteristic: gatt::INDOOR_BIKE_DATA, value: vec![0x44, 0, 0xB8, 0x0B, 0xB4, 0, 0xC8, 0] },
                    11 => Input::WriteFailed { conn: c, error: "x".into() },
                    _ => Input::Tick,
                };
                m.handle(t, input);
                if rng.gen_bool(0.3) {
                    let control = [Control::TargetPower(150), Control::Resistance(10), Control::Release][rng.gen_range(0..3)];
                    m.send(t, ControlId(sent), control);
                    sent += 1;
                }
                events.extend(m.take_events());
            }
            // The end of the link settles everything still owed.
            t += Duration::from_secs(1);
            m.handle(t, Input::Radio(RadioState::Off));
            events.extend(m.take_events());

            let mut outcomes = vec![0u32; sent as usize];
            let mut last = Duration::ZERO;
            let mut failed = false;
            let mut lost = false;
            for e in &events {
                let at = match e {
                    LinkEvent::Connected { at }
                    | LinkEvent::Identified { at, .. }
                    | LinkEvent::Lost { at }
                    | LinkEvent::Back { at, .. }
                    | LinkEvent::Control { at, .. }
                    | LinkEvent::Failed { at, .. } => *at,
                    LinkEvent::Reading(r) => r.at,
                };
                assert!(at >= last, "time ran back: {events:?}");
                last = at;
                match e {
                    LinkEvent::Control { id, .. } => outcomes[id.0 as usize] += 1,
                    LinkEvent::Failed { .. } => {
                        assert!(!failed, "two failures");
                        failed = true;
                    }
                    _ => assert!(!failed, "{e:?} after Failed"),
                }
                match e {
                    LinkEvent::Lost { .. } => {
                        assert!(!lost, "Lost twice: {events:?}");
                        lost = true;
                    }
                    LinkEvent::Back { .. } => {
                        assert!(lost, "Back without Lost");
                        lost = false;
                    }
                    _ => {}
                }
            }
            assert!(failed);
            assert!(outcomes.iter().all(|&n| n == 1), "outcomes per control: {outcomes:?}");
        }
    }
}
