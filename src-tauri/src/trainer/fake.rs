//! A trainer in software, for tests.
//!
//! Deterministic: it simulates up to the `now` it is polled with, one
//! reading slot at a time; a control sent between polls goes out in the
//! next slot, as over ANT+. The physics is the least that reproduces what
//! the spike measured on a real trainer:
//!
//! - the brake is a torque that follows its goal with a first-order lag,
//!   so power = torque × cadence overshoots when the cadence steps up and
//!   settles a few seconds later;
//! - riding free, the goal torque grows with cadence (power ∝ cadence²);
//! - in ERG the goal is target ÷ cadence.
//!
//! Quirks are opt-in through [`FakeConfig`]: a late identity, a control
//! timeout after which ERG lets go, flags that keep claiming the target
//! after that, outages (a drop between two slots goes unnoticed, as it
//! would on air), a target forgotten in an outage, slow, lost or missing
//! acknowledgements, and the link failing for good.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::link::{
    Control, ControlId, ControlOutcome, Identity, LinkEvent, LinkFailure, Reading, SensorLink, TrainerLink, Transport,
};
use super::profile::{TrainerProfile, GENERIC};

#[derive(Debug, Clone)]
pub struct FakeConfig {
    pub identity: Identity,
    /// A reading every this long (ANT+ FE-C: 8192/32768 s = 250 ms). Must
    /// be above zero.
    pub period: Duration,
    /// The rider: from each time on, this cadence; 0 rpm before the first
    /// entry. Any order.
    pub cadence: Vec<(Duration, u8)>,
    /// Time constant of the brake following its goal.
    pub brake_lag: Duration,
    /// Riding free, power = `free_ride_k` × cadence².
    pub free_ride_k: f32,
    /// The identity is told this long after first contact (an ANT+ trainer
    /// sends its 0x50 page every ~16 s; readings come before it).
    pub identify_after: Duration,
    /// ERG lets go this long after the last control that landed.
    pub control_timeout: Option<Duration>,
    /// After letting go, keep claiming the old target (flag and echo).
    pub flags_lie: bool,
    /// Outages: (start, length). Nothing is heard and controls are refused
    /// meanwhile; the control timeout keeps running.
    pub drops: Vec<(Duration, Duration)>,
    /// An outage makes the trainer drop ERG.
    pub forgets_target_in_drop: bool,
    /// How long after a control lands its acknowledgement comes. Longer
    /// than the profile's `control_answer_timeout` reports `TimedOut`.
    pub ack_delay: Duration,
    /// Of the controls that go out, every n-th is lost on the way: not
    /// applied, `NotAcknowledged`.
    pub lose_every_nth_control: Option<u32>,
    /// Of the controls that go out, every n-th lands but its
    /// acknowledgement is lost: applied, `NotAcknowledged`.
    pub lose_every_nth_ack: Option<u32>,
    /// The link fails for good at this time.
    pub fail_at: Option<(Duration, LinkFailure)>,
    /// The trainer reads this many watts below what the legs put in (what
    /// power pedals read), by cadence; in ERG it holds its own reading.
    pub reads_low: fn(u8) -> f32,
    /// The pedals read up to this many watts either way of the legs, at
    /// random (seeded: the same every run).
    pub pedal_noise: f32,
}

fn reads_true(_: u8) -> f32 {
    0.0
}

impl FakeConfig {
    /// A trainer without quirks, ridden at a steady cadence.
    pub fn steady(cadence_rpm: u8) -> FakeConfig {
        FakeConfig {
            identity: Identity { name: Some("FAKE".into()), ..Identity::new(Transport::Fake) },
            period: Duration::from_millis(250),
            cadence: vec![(Duration::ZERO, cadence_rpm)],
            brake_lag: Duration::from_millis(1500),
            // 80 rpm → 224 W, 97 rpm → 329 W: the Avanti riding free.
            free_ride_k: 0.035,
            identify_after: Duration::ZERO,
            control_timeout: None,
            flags_lie: false,
            drops: Vec::new(),
            forgets_target_in_drop: false,
            ack_delay: Duration::ZERO,
            lose_every_nth_control: None,
            lose_every_nth_ack: None,
            fail_at: None,
            reads_low: reads_true,
            pedal_noise: 0.0,
        }
    }

    /// The Elite Avanti as the spike measured it over ANT+: lets go of ERG
    /// ~18 s after the last target while still claiming it, and tells who
    /// it is with its 0x50 page.
    pub fn elite_avanti(cadence_rpm: u8) -> FakeConfig {
        FakeConfig {
            identity: Identity {
                ant_device_number: Some(14830),
                ant_manufacturer: Some(86),
                ant_model: Some(45),
                firmware: Some("3.7".into()),
                // Its 0x36: basic resistance, target power, simulation.
                target_power: Some(true),
                ..Identity::new(Transport::Ant)
            },
            control_timeout: Some(Duration::from_secs(18)),
            flags_lie: true,
            ..FakeConfig::steady(cadence_rpm)
        }
    }

    /// The rider changes cadence at `at`.
    pub fn then_cadence(mut self, at: Duration, cadence_rpm: u8) -> FakeConfig {
        self.cadence.push((at, cadence_rpm));
        self
    }

    pub fn with_drop(mut self, at: Duration, length: Duration) -> FakeConfig {
        self.drops.push((at, length));
        self
    }

    fn cadence_at(&self, t: Duration) -> u8 {
        // Sorted by `FakeTrainer::new`.
        self.cadence.iter().take_while(|&&(from, _)| from <= t).last().map_or(0, |&(_, c)| c)
    }

    fn in_drop(&self, t: Duration) -> Option<Duration> {
        self.drops
            .iter()
            .find(|&&(start, len)| t >= start && start.checked_add(len).is_none_or(|end| t < end))
            .map(|&(start, _)| start)
    }
}

fn nth(every: Option<u32>, count: u32) -> bool {
    every.is_some_and(|n| n > 0 && count.is_multiple_of(n))
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Mode {
    Free,
    Erg(u16),
    Resistance(u8),
}

/// An outcome to report at `due`; `went_out` tells what a failure makes of it.
struct Owed {
    due: Duration,
    id: ControlId,
    control: Control,
    outcome: ControlOutcome,
    went_out: bool,
}

pub struct FakeTrainer {
    cfg: FakeConfig,
    profile: &'static TrainerProfile,
    connected: bool,
    identified: bool,
    failed: bool,
    /// The next reading slot.
    next_slot: Duration,
    /// Simulated up to here.
    simulated: Duration,
    mode: Mode,
    last_control_at: Option<Duration>,
    /// The target it claims after letting go, when its flags lie.
    claimed_target: Option<u16>,
    /// Brake torque, watts per rpm.
    torque: f32,
    next_id: u64,
    queued: Vec<(ControlId, Control)>,
    owed: Vec<Owed>,
    controls_out: u32,
    lost_at: Option<Duration>,
    /// What the pedals read, slot by slot, for [`FakePedals`].
    pedals: Option<Arc<Mutex<Vec<Reading>>>>,
    /// The pedals' noise generator.
    seed: u64,
}

impl FakeTrainer {
    /// Power pedals on this trainer: they read what the legs put in, every
    /// slot, through the trainer's drops (they are another radio), not
    /// through `drops` of their own (Lost, then Back), nor `mutes` (silent,
    /// never said lost).
    pub fn pedals(&mut self, drops: Vec<(Duration, Duration)>, mutes: Vec<(Duration, Duration)>) -> FakePedals {
        let tap = Arc::new(Mutex::new(Vec::new()));
        self.pedals = Some(tap.clone());
        FakePedals { tap, drops, mutes, connected: false, lost_at: None }
    }

    pub fn new(mut cfg: FakeConfig) -> FakeTrainer {
        assert!(cfg.period > Duration::ZERO, "a zero period never reaches `now`");
        cfg.cadence.sort_by_key(|&(t, _)| t);
        // The wheel is already turning when the session starts.
        let torque = cfg.free_ride_k * cfg.cadence_at(Duration::ZERO) as f32;
        FakeTrainer {
            cfg,
            profile: &GENERIC,
            connected: false,
            identified: false,
            failed: false,
            next_slot: Duration::ZERO,
            simulated: Duration::ZERO,
            mode: Mode::Free,
            last_control_at: None,
            claimed_target: None,
            torque,
            next_id: 0,
            queued: Vec::new(),
            owed: Vec::new(),
            controls_out: 0,
            lost_at: None,
            pedals: None,
            seed: 0x2545_F491_4F6C_DD1D,
        }
    }

    /// Uniform in −1…1, from a linear congruential generator.
    fn noise(&mut self) -> f32 {
        self.seed = self.seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        (self.seed >> 40) as f32 / (1u64 << 23) as f32 - 1.0
    }

    fn slot(&mut self, t: Duration, events: &mut Vec<LinkEvent>) {
        let dt = t.saturating_sub(self.simulated).as_secs_f32();
        self.simulated = t;

        if let Some((at, reason)) = self.cfg.fail_at.clone().filter(|(at, _)| *at <= t) {
            self.fail(at, reason, events);
            return;
        }

        let down = self.cfg.in_drop(t);
        if !self.connected {
            // First contact needs the device to be heard.
            if down.is_none() {
                self.connected = true;
                events.push(LinkEvent::Connected { at: t });
            }
        } else {
            match (down, self.lost_at) {
                (Some(start), None) => {
                    // What was due before it went silent was heard; what is
                    // due in the silence never will be.
                    self.report_owed(|due| due < start, events);
                    self.lost_at = Some(start);
                    events.push(LinkEvent::Lost { at: start });
                    for o in std::mem::take(&mut self.owed) {
                        let outcome = ControlOutcome::NotAcknowledged;
                        events.push(LinkEvent::Control { id: o.id, at: start, control: o.control, outcome });
                    }
                }
                (None, Some(lost)) => {
                    self.lost_at = None;
                    events.push(LinkEvent::Back { at: t, outage: t - lost });
                    if self.cfg.forgets_target_in_drop {
                        self.let_go(false);
                    }
                }
                _ => {}
            }
        }
        // Anything due between the previous slot and this one happened
        // before what this slot announces.
        self.report_owed(|due| due < t, events);
        let heard = self.connected && down.is_none();
        if heard && !self.identified && t >= self.cfg.identify_after {
            self.identified = true;
            events.push(LinkEvent::Identified { at: t, identity: self.cfg.identity.clone() });
        }

        for (id, control) in std::mem::take(&mut self.queued) {
            self.deliver(id, control, t, !heard);
        }

        if let (Mode::Erg(_), Some(timeout), Some(last)) = (self.mode, self.cfg.control_timeout, self.last_control_at) {
            if t.saturating_sub(last) > timeout {
                self.let_go(self.cfg.flags_lie);
            }
        }

        let rpm = self.cfg.cadence_at(t);
        let cadence = rpm as f32;
        let low = (self.cfg.reads_low)(rpm);
        let goal = match self.mode {
            Mode::Free => self.cfg.free_ride_k * cadence,
            // It holds its own reading: the legs put in what it reads low.
            Mode::Erg(target) if cadence > 0.0 => (target as f32 + low).max(0.0) / cadence,
            Mode::Erg(_) => self.torque,
            // 50 % ≈ riding free; 0 % (a release) is lighter.
            Mode::Resistance(pct) => self.cfg.free_ride_k * cadence * (0.4 + 1.2 * pct as f32 / 100.0),
        };
        let lag = self.cfg.brake_lag.as_secs_f32();
        let follow = if lag > 0.0 { 1.0 - (-dt / lag).exp() } else { 1.0 };
        self.torque += (goal - self.torque) * follow;

        self.report_owed(|due| due <= t, events);

        let legs = (self.torque * cadence).max(0.0);
        if let Some(tap) = self.pedals.clone() {
            let read = legs + self.cfg.pedal_noise * self.noise();
            let watts = Some(read.max(0.0).round().min(u16::MAX as f32) as u16);
            tap.lock().unwrap().push(Reading { at: t, watts, cadence_rpm: Some(rpm), ..Reading::default() });
        }
        if heard {
            let watts = (legs - low).max(0.0).round().min(u16::MAX as f32) as u16;
            let (reports_on_target, reported_target) = match self.mode {
                Mode::Erg(target) => {
                    let near = (watts as f32 - target as f32).abs() <= target as f32 * 0.05;
                    (Some(cadence > 0.0 && near), Some(target))
                }
                _ => (self.claimed_target.map(|_| true), self.claimed_target),
            };
            events.push(LinkEvent::Reading(Reading {
                at: t,
                watts: Some(watts),
                cadence_rpm: Some(cadence as u8),
                speed_kmh: Some(cadence * 0.36),
                reports_on_target,
                reported_target,
                ..Reading::default()
            }));
        }
    }

    /// Back to riding free; a lying trainer keeps claiming the old target.
    fn let_go(&mut self, keep_claiming: bool) {
        if let Mode::Erg(target) = self.mode {
            self.claimed_target = keep_claiming.then_some(target);
        }
        self.mode = Mode::Free;
    }

    /// A control goes out in slot `t`; `unreachable` while not connected or
    /// in an outage.
    fn deliver(&mut self, id: ControlId, control: Control, t: Duration, unreachable: bool) {
        let owe = |due, outcome, went_out| Owed { due, id, control, outcome, went_out };
        if unreachable {
            self.owed.push(owe(t, ControlOutcome::Refused, false));
            return;
        }
        self.controls_out += 1;
        if nth(self.cfg.lose_every_nth_control, self.controls_out) {
            self.owed.push(owe(t, ControlOutcome::NotAcknowledged, true));
            return;
        }
        self.last_control_at = Some(t);
        self.claimed_target = None;
        self.mode = match control {
            Control::TargetPower(w) => Mode::Erg(w),
            Control::Resistance(pct) => Mode::Resistance(pct.min(100)),
            Control::Release => Mode::Resistance(0),
        };
        let outcome = if nth(self.cfg.lose_every_nth_ack, self.controls_out) {
            owe(t, ControlOutcome::NotAcknowledged, true)
        } else if self.cfg.ack_delay > self.profile.control_answer_timeout {
            owe(t + self.profile.control_answer_timeout, ControlOutcome::TimedOut, true)
        } else {
            owe(t + self.cfg.ack_delay, ControlOutcome::Acknowledged, true)
        };
        self.owed.push(outcome);
    }

    /// Reports, in time order, the outcomes whose time `now_due` accepts.
    fn report_owed(&mut self, now_due: impl Fn(Duration) -> bool, events: &mut Vec<LinkEvent>) {
        let (mut due, later): (Vec<Owed>, Vec<Owed>) =
            std::mem::take(&mut self.owed).into_iter().partition(|o| now_due(o.due));
        self.owed = later;
        due.sort_by_key(|o| (o.due, o.id));
        events.extend(due.into_iter().map(|o| LinkEvent::Control { id: o.id, at: o.due, control: o.control, outcome: o.outcome }));
    }

    fn fail(&mut self, at: Duration, reason: LinkFailure, events: &mut Vec<LinkEvent>) {
        self.failed = true;
        self.report_owed(|due| due <= at, events);
        events.push(LinkEvent::Failed { at, reason });
        for o in std::mem::take(&mut self.owed) {
            let outcome = if o.went_out { ControlOutcome::NotAcknowledged } else { ControlOutcome::Refused };
            events.push(LinkEvent::Control { id: o.id, at, control: o.control, outcome });
        }
        for (id, control) in std::mem::take(&mut self.queued) {
            events.push(LinkEvent::Control { id, at, control, outcome: ControlOutcome::Refused });
        }
    }
}

impl SensorLink for FakeTrainer {
    fn identity(&self) -> Option<&Identity> {
        self.identified.then_some(&self.cfg.identity)
    }

    fn poll(&mut self, now: Duration) -> Vec<LinkEvent> {
        let mut events = Vec::new();
        while !self.failed && self.next_slot <= now {
            let t = self.next_slot;
            self.slot(t, &mut events);
            self.next_slot += self.cfg.period;
        }
        if self.failed {
            for (id, control) in std::mem::take(&mut self.queued) {
                events.push(LinkEvent::Control { id, at: now, control, outcome: ControlOutcome::Refused });
            }
        }
        events
    }
}

impl TrainerLink for FakeTrainer {
    fn use_profile(&mut self, profile: &'static TrainerProfile) {
        self.profile = profile;
    }

    fn send(&mut self, control: Control) -> ControlId {
        let id = ControlId(self.next_id);
        self.next_id += 1;
        self.queued.push((id, control));
        id
    }
}

/// Power pedals on a [`FakeTrainer`] ([`FakeTrainer::pedals`]). Poll the
/// trainer first: they read what it simulated.
pub struct FakePedals {
    tap: Arc<Mutex<Vec<Reading>>>,
    drops: Vec<(Duration, Duration)>,
    mutes: Vec<(Duration, Duration)>,
    connected: bool,
    lost_at: Option<Duration>,
}

impl SensorLink for FakePedals {
    fn identity(&self) -> Option<&Identity> {
        None
    }

    fn poll(&mut self, now: Duration) -> Vec<LinkEvent> {
        let mut events = Vec::new();
        if !std::mem::replace(&mut self.connected, true) {
            events.push(LinkEvent::Connected { at: now });
        }
        for r in std::mem::take(&mut *self.tap.lock().unwrap()) {
            let within = |&(start, len): &(Duration, Duration)| r.at >= start && r.at < start + len;
            let down = self.drops.iter().find(|span| within(span)).map(|&(start, _)| start);
            match (down, self.lost_at) {
                (Some(start), None) => {
                    self.lost_at = Some(start);
                    events.push(LinkEvent::Lost { at: start });
                }
                (None, Some(lost)) => {
                    self.lost_at = None;
                    events.push(LinkEvent::Back { at: r.at, outage: r.at - lost });
                }
                _ => {}
            }
            if down.is_none() && !self.mutes.iter().any(within) {
                // (within: the span holds this reading's time)
                events.push(LinkEvent::Reading(r));
            }
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trainer::profile::{profile_for, ELITE_AVANTI};

    fn secs(s: f32) -> Duration {
        Duration::from_secs_f32(s)
    }

    fn readings(events: &[LinkEvent]) -> Vec<&Reading> {
        events
            .iter()
            .filter_map(|e| match e {
                LinkEvent::Reading(r) => Some(r),
                _ => None,
            })
            .collect()
    }

    fn reading_at(events: &[LinkEvent], at: f32) -> Reading {
        readings(events).into_iter().find(|r| r.at == secs(at)).cloned().unwrap_or_default()
    }

    fn watts_between(events: &[LinkEvent], from: f32, to: f32) -> Vec<f32> {
        readings(events)
            .into_iter()
            .filter(|r| r.at >= secs(from) && r.at < secs(to))
            .filter_map(|r| r.watts.map(f32::from))
            .collect()
    }

    fn mean(v: &[f32]) -> f32 {
        assert!(!v.is_empty(), "no readings in the window");
        v.iter().sum::<f32>() / v.len() as f32
    }

    fn controls(events: &[LinkEvent]) -> Vec<(ControlId, Duration, ControlOutcome)> {
        events
            .iter()
            .filter_map(|e| match e {
                LinkEvent::Control { id, at, outcome, .. } => Some((*id, *at, *outcome)),
                _ => None,
            })
            .collect()
    }

    /// Ride until `end` the way the engine will: each second poll, then
    /// decide; the target goes out every `resend` (once at the start when
    /// None) and lands in the next slot, 250 ms later.
    fn ride_erg(trainer: &mut FakeTrainer, target: u16, resend: Option<f32>, end: f32) -> Vec<LinkEvent> {
        let mut events = Vec::new();
        let mut next_send = 0.0;
        let mut t = 0.0;
        while t <= end {
            events.extend(trainer.poll(secs(t)));
            if t >= next_send {
                trainer.send(Control::TargetPower(target));
                next_send = resend.map_or(f32::INFINITY, |r| t + r);
            }
            t += 1.0;
        }
        events
    }

    #[test]
    fn it_connects_identifies_then_reads_four_times_a_second() {
        let mut fake = FakeTrainer::new(FakeConfig::steady(80));
        let events = fake.poll(secs(10.0));
        assert_eq!(events[0], LinkEvent::Connected { at: Duration::ZERO });
        assert!(matches!(&events[1], LinkEvent::Identified { at, identity } if *at == Duration::ZERO && identity.transport == Transport::Fake));
        assert_eq!(readings(&events).len(), 41, "slots 0, 0.25 … 10.0");
        assert!(fake.poll(secs(5.0)).is_empty(), "the clock never runs backwards");
    }

    #[test]
    fn an_ant_trainer_tells_who_it_is_after_its_first_readings() {
        let mut cfg = FakeConfig::elite_avanti(80);
        cfg.identify_after = secs(16.0);
        let mut fake = FakeTrainer::new(cfg);
        let early = fake.poll(secs(10.0));
        assert!(!readings(&early).is_empty());
        assert!(!early.iter().any(|e| matches!(e, LinkEvent::Identified { .. })));
        assert!(fake.identity().is_none());
        let later = fake.poll(secs(20.0));
        let id = later.iter().find_map(|e| match e {
            LinkEvent::Identified { at, identity } if *at == secs(16.0) => Some(identity.clone()),
            _ => None,
        });
        assert_eq!(id.as_ref().map(|i| profile_for(i).key), Some("elite-avanti"));
        assert_eq!(fake.identity(), id.as_ref());
    }

    #[test]
    fn riding_free_power_grows_with_the_square_of_cadence() {
        // Given out of order on purpose.
        let mut cfg = FakeConfig::steady(80);
        cfg.cadence = vec![(secs(30.0), 97), (Duration::ZERO, 80)];
        let mut fake = FakeTrainer::new(cfg);
        let events = fake.poll(secs(60.0));
        assert!((reading_at(&events, 0.0).watts.unwrap_or(0) as f32 - 224.0).abs() < 1.0, "already turning at 0 s");
        assert!((mean(&watts_between(&events, 20.0, 30.0)) - 224.0).abs() < 3.0);
        assert!((mean(&watts_between(&events, 50.0, 60.0)) - 329.0).abs() < 3.0);
        assert!(readings(&events).iter().all(|r| r.reports_on_target.is_none() && r.reported_target.is_none()));
    }

    #[test]
    fn erg_holds_the_target_across_cadences() {
        let cfg = FakeConfig::steady(60).then_cadence(secs(40.0), 90);
        let mut fake = FakeTrainer::new(cfg);
        let events = ride_erg(&mut fake, 150, Some(2.0), 80.0);
        assert!((mean(&watts_between(&events, 10.0, 40.0)) - 150.0).abs() < 2.0, "at 60 rpm");
        assert!((mean(&watts_between(&events, 50.0, 80.0)) - 150.0).abs() < 2.0, "at 90 rpm");
    }

    #[test]
    fn a_cadence_step_overshoots_then_settles_and_an_honest_flag_says_so() {
        let cfg = FakeConfig::steady(60).then_cadence(secs(30.0), 90);
        let mut fake = FakeTrainer::new(cfg);
        let events = ride_erg(&mut fake, 150, Some(2.0), 45.0);
        let step = reading_at(&events, 30.0);
        assert!(step.watts.unwrap_or(0) > 190, "the brake lags the step: {step:?}");
        assert_eq!(step.reports_on_target, Some(false), "an honest trainer is off target in the overshoot");
        let settled: Vec<&Reading> = readings(&events).into_iter().filter(|r| r.at >= secs(38.0)).collect();
        assert!(settled.iter().all(|r| r.watts.is_some_and(|w| (w as f32 - 150.0).abs() <= 7.5)));
        assert!(settled.iter().all(|r| r.reports_on_target == Some(true) && r.reported_target == Some(150)));
    }

    #[test]
    fn the_avanti_lets_go_of_a_single_target_about_18_s_later_but_still_claims_it() {
        let mut fake = FakeTrainer::new(FakeConfig::elite_avanti(80));
        let events = ride_erg(&mut fake, 150, None, 40.0);
        // The target lands at 0.25 s and is let go once 18 s have passed.
        assert!((mean(&watts_between(&events, 16.0, 18.5)) - 150.0).abs() < 3.0, "still held at 18 s");
        assert!(reading_at(&events, 20.0).watts.unwrap_or(0) > 180, "letting go by 20 s");
        assert!(mean(&watts_between(&events, 30.0, 40.0)) > 210.0, "back to riding free");
        let last = reading_at(&events, 40.0);
        assert_eq!((last.reports_on_target, last.reported_target), (Some(true), Some(150)), "flag and echo lie");
    }

    #[test]
    fn an_honest_trainer_that_lets_go_says_so() {
        let mut cfg = FakeConfig::elite_avanti(80);
        cfg.flags_lie = false;
        let mut fake = FakeTrainer::new(cfg);
        let events = ride_erg(&mut fake, 150, None, 40.0);
        let last = reading_at(&events, 40.0);
        assert_eq!((last.reports_on_target, last.reported_target), (None, None));
    }

    #[test]
    fn a_new_control_ends_the_lie() {
        let mut fake = FakeTrainer::new(FakeConfig::elite_avanti(80));
        let mut events = ride_erg(&mut fake, 150, None, 30.0);
        fake.send(Control::Release);
        events.extend(fake.poll(secs(35.0)));
        let last = reading_at(&events, 35.0);
        assert_eq!((last.reports_on_target, last.reported_target), (None, None));
    }

    #[test]
    fn the_avanti_preset_and_its_profile_tell_the_same_story() {
        let cfg = FakeConfig::elite_avanti(80);
        let profile = profile_for(&cfg.identity);
        assert_eq!(profile.key, ELITE_AVANTI.key, "the preset is recognised as the Avanti");
        assert_eq!(cfg.flags_lie, !profile.trust_target_flags, "lying flags ⇔ not trusted");
        let timeout = cfg.control_timeout.unwrap_or(Duration::MAX);
        assert!(profile.erg_resend * 3 < timeout, "resend comfortably inside the control timeout");
    }

    #[test]
    fn the_avanti_holds_when_the_target_is_resent_as_its_profile_says() {
        let mut fake = FakeTrainer::new(FakeConfig::elite_avanti(80));
        let events = ride_erg(&mut fake, 150, Some(ELITE_AVANTI.erg_resend.as_secs_f32()), 120.0);
        assert!((mean(&watts_between(&events, 10.0, 120.0)) - 150.0).abs() < 2.0);
    }

    #[test]
    fn a_lost_control_does_not_restart_the_control_timeout() {
        let mut cfg = FakeConfig::elite_avanti(80);
        cfg.lose_every_nth_control = Some(2);
        let mut fake = FakeTrainer::new(cfg);
        let mut events = fake.poll(secs(10.0));
        fake.send(Control::TargetPower(150)); // lands at 10.25
        events.extend(fake.poll(secs(15.0)));
        fake.send(Control::TargetPower(150)); // lost
        events.extend(fake.poll(secs(40.0)));
        let outcomes: Vec<ControlOutcome> = controls(&events).into_iter().map(|c| c.2).collect();
        assert_eq!(outcomes, vec![ControlOutcome::Acknowledged, ControlOutcome::NotAcknowledged]);
        assert!((mean(&watts_between(&events, 25.0, 28.0)) - 150.0).abs() < 3.0, "held until 28.25");
        assert!(reading_at(&events, 31.0).watts.unwrap_or(0) > 180, "let go 18 s after the one that landed");
    }

    #[test]
    fn a_drop_hides_the_trainer_refuses_controls_and_keeps_the_target() {
        // Starts between two slots: Lost tells when it began.
        let cfg = FakeConfig::steady(80).with_drop(secs(20.1), secs(5.0));
        let mut fake = FakeTrainer::new(cfg);
        let events = ride_erg(&mut fake, 150, Some(2.0), 30.0);
        assert!(events.contains(&LinkEvent::Lost { at: secs(20.1) }));
        let back = events.iter().find_map(|e| match e {
            LinkEvent::Back { at, outage } => Some((*at, *outage)),
            _ => None,
        });
        assert_eq!(back, Some((secs(25.25), secs(25.25) - secs(20.1))));
        assert!(watts_between(&events, 20.1, 25.25).is_empty(), "nothing heard meanwhile");
        let during: Vec<ControlOutcome> =
            controls(&events).into_iter().filter(|c| c.1 > secs(20.1) && c.1 < secs(25.25)).map(|c| c.2).collect();
        assert_eq!(during, vec![ControlOutcome::Refused; 3], "targets sent at 20, 22 and 24 s");
        assert!((mean(&watts_between(&events, 25.25, 30.0)) - 150.0).abs() < 3.0, "still in ERG after it");
    }

    #[test]
    fn a_drop_longer_than_the_control_timeout_ends_erg() {
        let cfg = FakeConfig::elite_avanti(80).with_drop(secs(10.0), secs(30.0));
        let mut fake = FakeTrainer::new(cfg);
        let events = ride_erg(&mut fake, 150, Some(2.0), 40.0);
        assert!(reading_at(&events, 40.0).watts.unwrap_or(0) > 200, "it let go while nobody could reach it");
    }

    #[test]
    fn a_trainer_that_forgets_in_a_drop_needs_the_target_again() {
        let mut cfg = FakeConfig::steady(80).with_drop(secs(10.0), secs(2.0));
        cfg.forgets_target_in_drop = true;
        let mut fake = FakeTrainer::new(cfg);
        let events = ride_erg(&mut fake, 150, None, 20.0);
        assert!(mean(&watts_between(&events, 16.0, 20.0)) > 200.0, "riding free after Back");
    }

    #[test]
    fn acknowledgements_can_be_slow_lost_or_never_come() {
        let mut cfg = FakeConfig::steady(80);
        cfg.ack_delay = secs(1.0);
        cfg.lose_every_nth_ack = Some(2);
        let mut fake = FakeTrainer::new(cfg);
        let first = fake.send(Control::TargetPower(150)); // lands at 0, ack at 1 s
        let mut events = fake.poll(secs(5.0));
        let second = fake.send(Control::TargetPower(180)); // lands, ack lost
        events.extend(fake.poll(secs(15.0)));
        assert_eq!(
            controls(&events),
            vec![
                (first, secs(1.0), ControlOutcome::Acknowledged),
                (second, secs(5.25), ControlOutcome::NotAcknowledged),
            ]
        );
        assert!((mean(&watts_between(&events, 12.0, 15.0)) - 180.0).abs() < 3.0, "the unacknowledged one landed");

        let mut cfg = FakeConfig::steady(80);
        cfg.ack_delay = secs(4.0);
        let mut fake = FakeTrainer::new(cfg);
        fake.use_profile(&GENERIC); // answer timeout 3 s
        let id = fake.send(Control::TargetPower(150));
        let events = fake.poll(secs(10.0));
        assert_eq!(controls(&events), vec![(id, secs(3.0), ControlOutcome::TimedOut)]);
        assert!((mean(&watts_between(&events, 8.0, 10.0)) - 150.0).abs() < 3.0, "timed out, applied all the same");
    }

    fn profile_with_answer_timeout(timeout: Duration) -> &'static TrainerProfile {
        Box::leak(Box::new(TrainerProfile {
            key: "test",
            name: "Test",
            matches: &[],
            erg_resend: GENERIC.erg_resend,
            trust_target_flags: GENERIC.trust_target_flags,
            ftms_start_after_reconnect: GENERIC.ftms_start_after_reconnect,
            control_answer_timeout: timeout,
            link_drops_routine: GENERIC.link_drops_routine,
            release_budget: GENERIC.release_budget,
        }))
    }

    #[test]
    fn the_profile_in_use_sets_the_answer_timeout() {
        let mut cfg = FakeConfig::steady(80);
        cfg.ack_delay = secs(4.0);
        let mut patient = FakeTrainer::new(cfg.clone());
        patient.use_profile(profile_with_answer_timeout(secs(5.0)));
        let id = patient.send(Control::TargetPower(150));
        assert_eq!(controls(&patient.poll(secs(10.0))), vec![(id, secs(4.0), ControlOutcome::Acknowledged)]);
        // Until told otherwise a link behaves as GENERIC: 3 s.
        let mut hasty = FakeTrainer::new(cfg);
        let id = hasty.send(Control::TargetPower(150));
        assert_eq!(controls(&hasty.poll(secs(10.0))), vec![(id, secs(3.0), ControlOutcome::TimedOut)]);
    }

    #[test]
    fn an_acknowledgement_due_in_an_outage_never_arrives() {
        let mut cfg = FakeConfig::steady(80).with_drop(secs(10.5), secs(5.0));
        cfg.ack_delay = secs(1.0);
        let mut fake = FakeTrainer::new(cfg);
        let mut events = fake.poll(secs(10.0));
        let id = fake.send(Control::TargetPower(150)); // lands at 10.25, ack due 11.25: in the silence
        events.extend(fake.poll(secs(20.0)));
        assert_eq!(controls(&events), vec![(id, secs(10.5), ControlOutcome::NotAcknowledged)]);
        let lost = events.iter().position(|e| matches!(e, LinkEvent::Lost { .. }));
        let outcome = events.iter().position(|e| matches!(e, LinkEvent::Control { .. }));
        assert!(lost < outcome, "settled when the link noticed, not before");
    }

    #[test]
    fn what_was_due_before_an_outage_comes_before_lost() {
        let mut cfg = FakeConfig::steady(80).with_drop(secs(10.1), secs(2.0));
        cfg.ack_delay = Duration::from_millis(50);
        let mut fake = FakeTrainer::new(cfg);
        let mut events = fake.poll(secs(9.75));
        let id = fake.send(Control::TargetPower(150)); // lands at 10.0, acknowledged at 10.05
        events.extend(fake.poll(secs(15.0)));
        let outcome = events.iter().position(|e| matches!(e, LinkEvent::Control { id: i, outcome: ControlOutcome::Acknowledged, .. } if *i == id));
        let lost = events.iter().position(|e| matches!(e, LinkEvent::Lost { .. }));
        assert!(outcome.is_some() && outcome < lost, "events in the order they happened");
    }

    #[test]
    fn an_acknowledgement_due_after_the_outage_began_is_lost_too() {
        // Due at 10.2 s: after the outage began (10.1), before the slot that
        // notices it (10.25).
        let mut cfg = FakeConfig::steady(80).with_drop(secs(10.1), secs(2.0));
        cfg.ack_delay = Duration::from_millis(200);
        let mut fake = FakeTrainer::new(cfg);
        let mut events = fake.poll(secs(9.75));
        let id = fake.send(Control::TargetPower(150)); // lands at 10.0
        events.extend(fake.poll(secs(15.0)));
        assert_eq!(controls(&events), vec![(id, secs(10.1), ControlOutcome::NotAcknowledged)]);
    }

    #[test]
    fn an_outcome_due_just_before_the_identity_comes_first() {
        let mut cfg = FakeConfig::elite_avanti(80);
        cfg.identify_after = secs(16.0);
        cfg.ack_delay = Duration::from_millis(100);
        let mut fake = FakeTrainer::new(cfg);
        let mut events = fake.poll(secs(15.5));
        let id = fake.send(Control::TargetPower(150)); // lands at 15.75, acknowledged at 15.85
        events.extend(fake.poll(secs(17.0)));
        let outcome = events.iter().position(|e| matches!(e, LinkEvent::Control { id: i, .. } if *i == id));
        let identified = events.iter().position(|e| matches!(e, LinkEvent::Identified { .. }));
        assert!(outcome.is_some() && outcome < identified, "events in the order they happened");
    }

    #[test]
    fn an_identity_due_in_an_outage_waits_until_it_is_heard() {
        let mut cfg = FakeConfig::steady(80).with_drop(secs(10.0), secs(10.0));
        cfg.identify_after = secs(15.0);
        let mut fake = FakeTrainer::new(cfg);
        let events = fake.poll(secs(25.0));
        let at = events.iter().find_map(|e| match e {
            LinkEvent::Identified { at, .. } => Some(*at),
            _ => None,
        });
        assert_eq!(at, Some(secs(20.0)));
    }

    #[test]
    fn a_device_not_yet_heard_neither_connects_nor_takes_controls() {
        let mut cfg = FakeConfig::steady(80).with_drop(Duration::ZERO, secs(30.0));
        cfg.identify_after = secs(16.0);
        let mut fake = FakeTrainer::new(cfg);
        let early = fake.send(Control::TargetPower(150));
        let events = fake.poll(secs(29.0));
        assert_eq!(controls(&events), vec![(early, Duration::ZERO, ControlOutcome::Refused)]);
        assert!(events.iter().all(|e| matches!(e, LinkEvent::Control { .. })), "no Connected, Lost or Identified yet");
        let later = fake.poll(secs(31.0));
        assert_eq!(later[0], LinkEvent::Connected { at: secs(30.0) });
        assert!(matches!(&later[1], LinkEvent::Identified { at, .. } if *at == secs(30.0)));
    }

    #[test]
    fn an_honest_flag_allows_five_percent() {
        // 150 W at 80 rpm; right after a cadence step the power follows the
        // cadence before the brake catches up.
        for (rpm, on_target) in [(83, true), (86, false)] {
            let mut fake = FakeTrainer::new(FakeConfig::steady(80).then_cadence(secs(20.0), rpm));
            let events = ride_erg(&mut fake, 150, Some(2.0), 21.0);
            let step = reading_at(&events, 20.0);
            assert_eq!(step.reports_on_target, Some(on_target), "{rpm} rpm: {step:?}");
        }
    }

    #[test]
    fn a_trainer_that_forgets_in_a_drop_stops_claiming_too() {
        let mut cfg = FakeConfig::elite_avanti(80).with_drop(secs(10.0), secs(2.0));
        cfg.forgets_target_in_drop = true;
        let mut fake = FakeTrainer::new(cfg);
        let events = ride_erg(&mut fake, 150, None, 14.0);
        let after = reading_at(&events, 14.0);
        assert_eq!((after.reports_on_target, after.reported_target), (None, None));
    }

    #[test]
    fn outcomes_come_out_in_time_order() {
        let mut cfg = FakeConfig::steady(80);
        cfg.ack_delay = Duration::from_millis(3100);
        let mut fake = FakeTrainer::new(cfg);
        fake.use_profile(profile_with_answer_timeout(secs(5.0)));
        let a = fake.send(Control::TargetPower(150)); // lands at 0, acknowledged at 3.1 s
        let mut events = fake.poll(Duration::ZERO);
        fake.use_profile(profile_with_answer_timeout(Duration::from_millis(2800)));
        let b = fake.send(Control::TargetPower(150)); // lands at 0.25, times out at 3.05 s
        events.extend(fake.poll(secs(5.0)));
        assert_eq!(
            controls(&events),
            vec![
                (b, Duration::from_millis(3050), ControlOutcome::TimedOut),
                (a, Duration::from_millis(3100), ControlOutcome::Acknowledged),
            ]
        );
    }

    #[test]
    fn every_send_gets_its_own_id_and_one_outcome() {
        let mut fake = FakeTrainer::new(FakeConfig::steady(80));
        let a = fake.send(Control::TargetPower(150));
        let b = fake.send(Control::TargetPower(150));
        assert_ne!(a, b);
        let ids: Vec<ControlId> = controls(&fake.poll(secs(5.0))).into_iter().map(|c| c.0).collect();
        assert_eq!(ids, vec![a, b]);
    }

    #[test]
    fn a_failed_link_goes_silent_and_settles_what_it_owes() {
        let mut cfg = FakeConfig::steady(80);
        cfg.ack_delay = secs(2.0);
        cfg.fail_at = Some((secs(10.0), LinkFailure::AdapterGone));
        let mut fake = FakeTrainer::new(cfg);
        let mut events = fake.poll(secs(9.0));
        let in_flight = fake.send(Control::TargetPower(150)); // lands at 9.25, ack due 11.25
        events.extend(fake.poll(secs(20.0)));
        let failed: Vec<&LinkEvent> = events.iter().filter(|e| matches!(e, LinkEvent::Failed { .. })).collect();
        assert_eq!(failed, vec![&LinkEvent::Failed { at: secs(10.0), reason: LinkFailure::AdapterGone }]);
        assert!(readings(&events).iter().all(|r| r.at < secs(10.0)), "silent from then on");
        assert_eq!(controls(&events), vec![(in_flight, secs(10.0), ControlOutcome::NotAcknowledged)]);
        let late = fake.send(Control::Release);
        assert_eq!(controls(&fake.poll(secs(21.0))), vec![(late, secs(21.0), ControlOutcome::Refused)]);
    }

    #[test]
    fn a_control_still_queued_when_the_link_fails_is_refused() {
        let mut cfg = FakeConfig::steady(80);
        cfg.fail_at = Some((secs(10.0), LinkFailure::RadioOff));
        let mut fake = FakeTrainer::new(cfg);
        fake.poll(secs(9.75));
        let queued = fake.send(Control::Release); // would go out in the 10.0 slot
        let events = fake.poll(secs(11.0));
        let failed = events.iter().position(|e| matches!(e, LinkEvent::Failed { .. }));
        let refused = events.iter().position(|e| matches!(e, LinkEvent::Control { id, .. } if *id == queued));
        assert!(failed.is_some() && failed < refused, "settled after Failed");
        assert_eq!(controls(&events), vec![(queued, secs(10.0), ControlOutcome::Refused)]);
    }

    #[test]
    fn release_and_resistance_set_the_brake_from_the_rider() {
        let mut fake = FakeTrainer::new(FakeConfig::steady(80));
        fake.send(Control::TargetPower(300));
        let mut events = fake.poll(secs(10.0));
        fake.send(Control::Release);
        events.extend(fake.poll(secs(20.0)));
        assert!(mean(&watts_between(&events, 15.0, 20.0)) < 224.0 * 0.5, "lighter than riding free");
        fake.send(Control::Resistance(50));
        events.extend(fake.poll(secs(30.0)));
        assert!((mean(&watts_between(&events, 25.0, 30.0)) - 224.0).abs() < 5.0, "50 % ≈ riding free");
        fake.send(Control::Resistance(255));
        events.extend(fake.poll(secs(40.0)));
        let mut full = FakeTrainer::new(FakeConfig::steady(80));
        full.send(Control::Resistance(100));
        // Both settled by 38 s: 255 % lands at 30.25 and acts as 100 %.
        let reference = mean(&watts_between(&full.poll(secs(40.0)), 38.0, 40.0));
        assert!((mean(&watts_between(&events, 38.0, 40.0)) - reference).abs() < 1.5, "above 100 % is 100 %");
    }

    #[test]
    fn standing_still_reads_zero_and_off_target() {
        let cfg = FakeConfig::steady(80).then_cadence(secs(10.0), 0);
        let mut fake = FakeTrainer::new(cfg);
        let events = ride_erg(&mut fake, 150, Some(2.0), 20.0);
        let still: Vec<&Reading> = readings(&events).into_iter().filter(|r| r.at >= secs(11.0)).collect();
        assert!(still.iter().all(|r| r.watts == Some(0) && r.reports_on_target == Some(false)));
    }

    #[test]
    fn a_drop_for_ever_never_ends_and_never_panics() {
        let cfg = FakeConfig::steady(80).with_drop(secs(5.0), Duration::MAX);
        let mut fake = FakeTrainer::new(cfg);
        let events = fake.poll(secs(60.0));
        assert!(events.contains(&LinkEvent::Lost { at: secs(5.0) }));
        assert!(!events.iter().any(|e| matches!(e, LinkEvent::Back { .. })));
    }

    #[test]
    #[should_panic(expected = "zero period")]
    fn a_zero_period_is_refused_up_front() {
        let mut cfg = FakeConfig::steady(80);
        cfg.period = Duration::ZERO;
        FakeTrainer::new(cfg);
    }
}
