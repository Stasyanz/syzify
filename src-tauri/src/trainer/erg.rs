//! ERG: the trainer holds a target power whatever the cadence.
//!
//! What the spike taught (docs/trainers/elite-avanti.md) shapes it:
//!
//! - **Resend.** A target sent once is let go after a while (the Avanti:
//!   ~18 s), so it goes again every `erg_resend` of the profile, not only
//!   when it changes.
//! - **Watts, not flags.** "At target" and the target echo can lie. The
//!   trainer's own watts decide: away from the target for longer than a
//!   cadence step overshoots (2–8 s), the target goes again at once and the
//!   screen says so. A second without the trainer's watts says nothing.
//! - **Back, failures.** After a drop the target goes again at once; a
//!   control the trainer did not confirm goes again at once, then less and
//!   less often (up to `erg_resend`): one control in flight at a time, and
//!   no flood of a radio that is failing.
//! - **Death spiral.** Below a cadence the brake would only grow heavier:
//!   it is let go, and the target comes back with the cadence. A ride — and
//!   a ride after a pause — starts that way: the target waits for the legs.
//! - **No cadence.** A trainer that sends none (and no pedals that do) is
//!   held on its own watts rather than never: the brake goes once they
//!   stop, comes back once they turn. Slowing down is not seen — the
//!   screen says so.
//! - **Letting go** (a pause, low cadence, the target dropped) is asked
//!   until the trainer confirms it, through drops, never given up: a brake
//!   left on is what must not happen. A confirmed release outlives a short
//!   drop; after a long one it is asked again. The end of the ride is the
//!   runner's (`let_go`).
//! - **The screen says what is confirmed**, not what was wished: until the
//!   trainer answers, it waits.
//! - **Power match.** With pedals heard, the trainer is sent the target
//!   corrected by how far its watts read off the pedals'
//!   ([`super::power_match`]); the trainer's watts are then judged against
//!   what it was sent.
//!
//! The target comes from the screen ([`ErgHandle::set_target`]) or a
//! workout ([`ErgHandle::set_workout`]).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::journal::{Second, Source as Role};
use super::link::{Control, ControlId, ControlOutcome, LinkEvent, TrainerLink};
use super::power_match::{Heard, PowerMatch};
use super::session::{Controller, Moment};
use crate::models::training::{ErgLive, ErgState, MatchLive, MatchState};

/// Below this cadence for `LOW_FOR` seconds the brake is let go.
pub const LOW_CADENCE: u8 = 40;
pub const LOW_FOR: u32 = 2;
/// At or above this cadence for `BACK_FOR` seconds the target comes back.
pub const BACK_CADENCE: u8 = 50;
pub const BACK_FOR: u32 = 2;
/// Away from the target this many seconds in a row is not an overshoot.
pub const OFF_TARGET_FOR: u32 = 8;
/// Turning this many seconds with no cadence from anything: the trainer
/// sends none, and ERG holds on its watts instead.
pub const BLIND_AFTER: u32 = 5;
/// The same while the ride's pedals were never heard: they wake up with
/// the pedalling and connect a few seconds later, with the cadence.
pub const BLIND_AWAITING_PEDALS: u32 = 30;
/// A drop longer than this may have cost a release too (a trainer left
/// alone falls back to a resistance of its own): it is asked again.
const LONG_OUTAGE: Duration = Duration::from_secs(30);
/// The highest target the screen may ask for.
pub const MAX_TARGET: u16 = 2000;
/// The first retry after an unconfirmed control is at once; each next one
/// waits twice as long from this, up to `erg_resend`.
const BACKOFF: Duration = Duration::from_millis(250);

/// How far off the target still holds: 10 %, at least 15 W.
fn tolerance(target: u16) -> u16 {
    (target / 10).max(15)
}

/// How long the `fails`-th unconfirmed control in a row waits to go again.
fn backoff(fails: u32, cap: Duration) -> Duration {
    match fails {
        0 | 1 => Duration::ZERO,
        n => BACKOFF.saturating_mul(1 << (n - 1).min(16)).min(cap),
    }
}

/// A workout step: `secs` long, the target going from `from` to `to` watts
/// (steady when they are equal).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Step {
    pub secs: u32,
    pub from: u16,
    pub to: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workout {
    pub steps: Vec<Step>,
}

impl Workout {
    /// The target `t` seconds into the workout; None once it is over (and
    /// for a 0 W stretch: nothing to hold).
    pub fn target_at(&self, t: u32) -> Option<u16> {
        let mut start: u32 = 0;
        for s in &self.steps {
            let end = start.saturating_add(s.secs);
            if t < end {
                let share = f64::from(t - start) / f64::from(s.secs);
                let w = f64::from(s.from) + (f64::from(s.to) - f64::from(s.from)) * share;
                return Some((w.round() as u16).min(MAX_TARGET)).filter(|&w| w > 0);
            }
            start = end;
        }
        None
    }
}

/// Where the target comes from.
#[derive(Debug, Clone, Default)]
enum Source {
    #[default]
    Free,
    Manual(u16),
    /// `from`: the ride second it began at, set on the next tick.
    Workout {
        plan: Workout,
        from: Option<u32>,
    },
}

#[derive(Debug, Default)]
struct Shared {
    source: Source,
    view: Option<ErgLive>,
    matching: Option<MatchLive>,
}

/// The ERG controller's side the screen sees: set the target, read the
/// state. Cheap to clone.
#[derive(Debug, Clone, Default)]
pub struct ErgHandle(Arc<Mutex<Shared>>);

impl ErgHandle {
    /// Hold `watts` (capped at `MAX_TARGET`); None rides free.
    pub fn set_target(&self, watts: Option<u16>) {
        let source = match watts {
            Some(w) => Source::Manual(w.min(MAX_TARGET)),
            None => Source::Free,
        };
        if let Ok(mut s) = self.0.lock() {
            s.source = source;
        }
    }

    /// Follow `plan` from the next second; when it is over, ride free.
    pub fn set_workout(&self, plan: Workout) {
        if let Ok(mut s) = self.0.lock() {
            s.source = Source::Workout { plan, from: None };
        }
    }

    /// What ERG is doing; None riding free.
    pub fn view(&self) -> Option<ErgLive> {
        self.0.lock().ok().and_then(|s| s.view)
    }

    /// Power match, while a target is held on a ride with pedals.
    pub fn matching(&self) -> Option<MatchLive> {
        self.0.lock().ok().and_then(|s| s.matching)
    }

    /// Before the ride: the target it will start with.
    pub fn ready(&self) -> Option<ErgLive> {
        let s = self.0.lock().ok()?;
        let target_w = match &s.source {
            Source::Free => None,
            Source::Manual(w) => Some(*w),
            Source::Workout { plan, .. } => plan.target_at(0),
        }?;
        Some(ErgLive { target_w, state: ErgState::Ready })
    }

    /// The target at ride second `ride_s` (a workout starts here).
    fn target(&self, ride_s: u32) -> Option<u16> {
        let mut s = self.0.lock().ok()?;
        match &mut s.source {
            Source::Free => None,
            Source::Manual(w) => Some(*w),
            Source::Workout { plan, from } => {
                let start = *from.get_or_insert(ride_s);
                plan.target_at(ride_s.saturating_sub(start))
            }
        }
    }

    fn show(&self, view: Option<ErgLive>, matching: Option<MatchLive>) {
        // A panic elsewhere while holding it leaves plain data: write on.
        let mut s = self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        s.view = view;
        s.matching = matching;
    }
}

/// What a control does, whatever its watts: a ramp's steps are one kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Hold,
    Release,
    Other,
}

fn kind(c: Control) -> Kind {
    match c {
        Control::TargetPower(_) => Kind::Hold,
        Control::Release => Kind::Release,
        Control::Resistance(_) => Kind::Other,
    }
}

/// The controller; add it to the session and keep its handle.
pub struct Erg {
    handle: ErgHandle,
    /// The control the trainer has not answered yet, and when it went.
    in_flight: Option<(ControlId, Duration)>,
    /// The last control that went out, and when.
    sent: Option<(Control, Duration)>,
    /// What the trainer last confirmed, as long as nothing of another kind
    /// went out since (and no drop came between).
    acked: Option<Control>,
    /// Send the wish at the next chance, whatever the interval.
    force: bool,
    /// Unconfirmed controls in a row, and when the next may go.
    fails: u32,
    not_before: Duration,
    /// A target went out at some point: letting go means something.
    held: bool,
    last_wish: Option<Control>,
    low: u32,
    back: u32,
    /// The brake is let go for the legs; also how a ride starts.
    released_low: bool,
    /// Let go for a stall (not for the start or a pause); read only while
    /// `released_low`.
    stalled: bool,
    /// A cadence was ever heard this ride.
    cadence_seen: bool,
    /// Seconds turning with no cadence, before any was heard.
    no_cadence: u32,
    /// Nothing sends a cadence: the legs are told by the trainer's watts.
    blind: bool,
    off: u32,
    off_target: bool,
    /// The trainer's target from the pedals' watts.
    matcher: PowerMatch,
}

impl Erg {
    pub fn new(handle: ErgHandle) -> Erg {
        Erg {
            handle,
            in_flight: None,
            sent: None,
            acked: None,
            force: false,
            fails: 0,
            not_before: Duration::ZERO,
            held: false,
            last_wish: None,
            low: 0,
            back: 0,
            released_low: true,
            stalled: false,
            cadence_seen: false,
            no_cadence: 0,
            blind: false,
            off: 0,
            off_target: false,
            matcher: PowerMatch::default(),
        }
    }

    fn forget_target(&mut self) {
        if self.acked != Some(Control::Release) {
            self.acked = None;
        }
    }

    fn heard(&mut self, events: &[LinkEvent], moment: &Moment) {
        for event in events {
            match event {
                LinkEvent::Control { id, control, outcome, .. } if self.in_flight.is_some_and(|(f, _)| f == *id) => {
                    self.in_flight = None;
                    match outcome {
                        ControlOutcome::Acknowledged => {
                            self.acked = Some(*control);
                            self.fails = 0;
                        }
                        // It never went out (the link is down): the usual
                        // interval sends it again, Back at once.
                        ControlOutcome::Refused => {}
                        ControlOutcome::NotAcknowledged | ControlOutcome::TimedOut => {
                            self.fails += 1;
                            self.not_before = moment.now + backoff(self.fails, moment.profile.erg_resend);
                            self.force = true;
                        }
                    }
                }
                // A drop may have cost a target (it is sent again), not a
                // release: a trainer let go stays let go — unless it was
                // alone long enough to take a resistance of its own.
                LinkEvent::Back { .. } | LinkEvent::Connected { .. } => {
                    if matches!(event, LinkEvent::Back { outage, .. } if *outage > LONG_OUTAGE) {
                        self.acked = None;
                    } else {
                        self.forget_target();
                    }
                    self.fails = 0;
                    self.not_before = Duration::ZERO;
                    self.force = true;
                }
                LinkEvent::Lost { .. } => self.forget_target(),
                _ => {}
            }
        }
    }

    /// The death-spiral guard, one second of the legs: `turning` brings the
    /// target back, `stalling` lets the brake go (each so many in a row).
    fn legs(&mut self, turning: bool, stalling: bool) {
        if self.released_low {
            self.back = if turning { self.back + 1 } else { 0 };
            if self.back >= BACK_FOR {
                self.released_low = false;
                self.low = 0;
            }
        } else {
            self.low = if stalling { self.low + 1 } else { 0 };
            if self.low >= LOW_FOR {
                self.released_low = true;
                self.stalled = true;
                self.back = 0;
            }
        }
    }

    /// One closed second: the legs for the death-spiral guard, the
    /// trainer's watts against the target. `pedals_awaited`: the ride's
    /// pedals were never heard yet.
    fn judge(&mut self, s: &Second, target: Option<u16>, pedals_awaited: bool) {
        match (s.sample.cadence_rpm, s.trainer_watts) {
            (Some(cadence), _) => {
                self.cadence_seen = true;
                self.blind = false;
                self.legs(cadence >= BACK_CADENCE, cadence < LOW_CADENCE);
            }
            // Never a cadence from anything: the trainer's watts tell the
            // legs (a coasting flywheel keeps its speed, not its watts).
            (None, Some(watts)) if !self.cadence_seen => {
                let blind_after = if pedals_awaited { BLIND_AWAITING_PEDALS } else { BLIND_AFTER };
                if watts > 0 {
                    self.no_cadence += 1;
                    self.blind |= self.no_cadence >= blind_after;
                }
                if self.blind {
                    self.legs(watts > 0, watts == 0);
                }
            }
            // Held on its watts, and none came: a trainer may say nothing
            // standing still. Taken as stopped — in a drop the release
            // goes once it is back, the target only with watts again.
            (None, None) if self.blind => self.legs(false, true),
            // Otherwise a second with nothing heard (a drop) says nothing
            // about the legs: a stalling rider still sends 0 rpm.
            _ => {}
        }
        let confirmed = match self.acked {
            Some(Control::TargetPower(a)) if !self.released_low => Some(a),
            _ => None,
        };
        match (confirmed, target, s.trainer_watts) {
            // The trainer's own watts (the pedals read their own way):
            // near the target, or the last one confirmed (a ramp moves on
            // before the trainer does).
            (Some(last), Some(t), Some(w)) => {
                if w.abs_diff(t) <= tolerance(t) || w.abs_diff(last) <= tolerance(last) {
                    self.off = 0;
                    self.off_target = false;
                } else {
                    self.off += 1;
                    if self.off >= OFF_TARGET_FOR {
                        self.off_target = true;
                        self.force = true;
                        self.off = 0;
                    }
                }
            }
            // Holding, but the trainer's watts were not heard: nothing to
            // judge by.
            (Some(_), Some(_), None) => {}
            _ => {
                self.off = 0;
                self.off_target = false;
            }
        }
    }

    fn wish(&self, paused: bool, target: Option<u16>) -> Option<Control> {
        match target {
            Some(t) if !paused && !self.released_low => Some(Control::TargetPower(t)),
            _ => self.held.then_some(Control::Release),
        }
    }

    /// Whether `wish` goes out now.
    fn due(&self, wish: Control, moment: &Moment) -> bool {
        if moment.now < self.not_before || (wish == Control::Release && self.acked == Some(Control::Release)) {
            return false;
        }
        self.force
            || self.sent.is_none_or(|(c, at)| c != wish || moment.now.saturating_sub(at) >= moment.profile.erg_resend)
    }

    fn send(&mut self, wish: Control, moment: &Moment, trainer: &mut dyn TrainerLink) {
        let id = trainer.send(wish);
        // A confirmation of another kind no longer says what the trainer
        // does: this may land even unconfirmed.
        if self.acked.map(kind) != Some(kind(wish)) {
            self.acked = None;
        }
        self.in_flight = Some((id, moment.now));
        self.sent = Some((wish, moment.now));
        self.force = false;
        self.held |= kind(wish) == Kind::Hold;
    }

    fn state(&self, paused: bool, wish: Option<Control>) -> ErgState {
        let confirmed = match wish {
            Some(Control::TargetPower(_)) => self.acked.map(kind) == Some(Kind::Hold),
            Some(c) => self.acked == Some(c),
            None => true,
        };
        if !confirmed {
            ErgState::Waiting
        } else if paused {
            ErgState::Paused
        } else if self.released_low && self.stalled {
            ErgState::LowCadence
        } else if self.released_low {
            // The start, or after a pause: the target waits for the legs.
            ErgState::Ready
        } else if self.blind {
            // Before off target: what the rider must know to stop safely.
            ErgState::NoCadence
        } else if self.off_target {
            ErgState::OffTarget
        } else {
            ErgState::Holding
        }
    }
}

impl Controller for Erg {
    fn tick(&mut self, moment: &Moment, events: &[LinkEvent], trainer: &mut dyn TrainerLink) {
        self.heard(events, moment);
        let target = self.handle.target(moment.ride_s);
        if moment.paused {
            // After the pause the target waits for the legs again.
            self.released_low = true;
            self.stalled = false;
            self.low = 0;
            self.back = 0;
        }
        for s in moment.closed {
            // The trainer holds its own watts at what it is sent.
            let to_trainer = target.map(|t| self.matcher.trainer_target(t));
            self.judge(s, to_trainer, moment.pedals_awaited);
            // Close on what it confirmed: tighter than off-target's. (Let
            // go, nothing confirmed is a target: sending the release
            // forgets it.)
            let settled = match self.acked {
                Some(Control::TargetPower(a)) => s.trainer_watts.is_some_and(|w| w.abs_diff(a) <= (a / 20).max(8)),
                _ => false,
            };
            self.matcher.second(Heard {
                pedals: s.sample.watts.filter(|_| s.power_from == Some(Role::Pedals)),
                fresh: moment.pedals_heard_in == Some(s.sample.t),
                trainer: s.trainer_watts,
                cadence: s.sample.cadence_rpm,
                settled,
            });
        }
        let to_trainer = target.map(|t| self.matcher.trainer_target(t));
        let wish = self.wish(moment.paused, to_trainer);
        if wish.map(kind) != self.last_wish.map(kind) {
            self.off = 0;
            self.off_target = false;
            self.fails = 0;
            self.not_before = Duration::ZERO;
            self.force = true;
        } else if wish != self.last_wish {
            // A ramp's next step goes at once.
            self.force = true;
        }
        self.last_wish = wish;
        // A control that never got an answer (a link that broke its
        // promise) frees the line after twice the answer timeout.
        if let Some((_, at)) = self.in_flight {
            if moment.now.saturating_sub(at) > moment.profile.control_answer_timeout * 2 {
                self.in_flight = None;
                self.force = true;
            }
        }
        if let (None, Some(wish)) = (self.in_flight, wish) {
            if self.due(wish, moment) {
                self.send(wish, moment, trainer);
            }
        }
        let state = self.state(moment.paused, wish);
        let view = match target {
            Some(target_w) => Some(ErgLive { target_w, state }),
            // Riding free, the brake not confirmed let go yet: say so.
            None if wish == Some(Control::Release) && self.acked != Some(Control::Release) => {
                Some(ErgLive { target_w: 0, state: ErgState::Releasing })
            }
            None => None,
        };
        // Only while a target is held and confirmed, and once the pedals
        // were heard (or should have been: they wake with the legs).
        let holding = matches!(state, ErgState::Holding | ErgState::OffTarget | ErgState::NoCadence);
        let waking = moment.pedals_awaited && moment.ride_s < BLIND_AWAITING_PEDALS;
        let matching = match wish {
            Some(Control::TargetPower(trainer_target_w)) if holding && moment.has_pedals && !waking => {
                let state = if self.matcher.heard() { MatchState::Matching } else { MatchState::NoPedals };
                Some(MatchLive { state, trainer_target_w })
            }
            _ => None,
        };
        self.handle.show(view, matching);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trainer::fake::{FakeConfig, FakeTrainer};
    use crate::trainer::fit::SubSport;
    use crate::trainer::link::{Identity, Reading, SensorLink};
    use crate::trainer::profile::{TrainerProfile, GENERIC};
    use crate::trainer::session::Session;

    fn secs(s: u64) -> Duration {
        Duration::from_secs(s)
    }

    fn ms(m: u64) -> Duration {
        Duration::from_millis(m)
    }

    /// What the trainer was asked and answered.
    #[derive(Default)]
    struct Log {
        sent: Vec<(Duration, Control)>,
        answered: Vec<(Duration, Control, ControlOutcome)>,
        back: Vec<Duration>,
        in_flight: usize,
        max_in_flight: usize,
        states: Vec<(Duration, Option<ErgLive>)>,
    }

    /// Any trainer link, its controls and their answers logged.
    struct Spy<T> {
        inner: T,
        now: Duration,
        log: Arc<Mutex<Log>>,
    }

    impl<T: TrainerLink> SensorLink for Spy<T> {
        fn identity(&self) -> Option<&Identity> {
            self.inner.identity()
        }
        fn poll(&mut self, now: Duration) -> Vec<LinkEvent> {
            self.now = now;
            let events = self.inner.poll(now);
            let mut log = self.log.lock().unwrap();
            for e in &events {
                match e {
                    LinkEvent::Control { control, outcome, .. } => {
                        log.answered.push((now, *control, *outcome));
                        log.in_flight = log.in_flight.saturating_sub(1);
                    }
                    LinkEvent::Back { .. } => log.back.push(now),
                    _ => {}
                }
            }
            events
        }
    }

    impl<T: TrainerLink> TrainerLink for Spy<T> {
        fn use_profile(&mut self, profile: &'static TrainerProfile) {
            self.inner.use_profile(profile);
        }
        fn send(&mut self, control: Control) -> ControlId {
            let mut log = self.log.lock().unwrap();
            log.sent.push((self.now, control));
            log.in_flight += 1;
            log.max_in_flight = log.max_in_flight.max(log.in_flight);
            drop(log);
            self.inner.send(control)
        }
    }

    /// A trainer in a few lines: readings 4 a second from `ride` (cadence,
    /// watts) at each time, every control answered on the next poll as
    /// `answer` says (None: never).
    struct Stub {
        ride: fn(Duration) -> (u8, Option<u16>),
        answer: fn(Control) -> Option<ControlOutcome>,
        owed: Vec<(ControlId, Control)>,
        next: u64,
        reading: u64,
        /// Sends a cadence only within this.
        cadence: std::ops::Range<Duration>,
    }

    impl Stub {
        fn new(ride: fn(Duration) -> (u8, Option<u16>), answer: fn(Control) -> Option<ControlOutcome>) -> Stub {
            Stub { ride, answer, owed: Vec::new(), next: 0, reading: 0, cadence: Duration::ZERO..Duration::MAX }
        }

        /// No cadence at all, its watts from `ride`.
        fn blind(ride: fn(Duration) -> (u8, Option<u16>)) -> Stub {
            Stub { cadence: Duration::MAX..Duration::MAX, ..Stub::new(ride, ack) }
        }
    }

    impl SensorLink for Stub {
        fn identity(&self) -> Option<&Identity> {
            None
        }
        fn poll(&mut self, now: Duration) -> Vec<LinkEvent> {
            let answer = self.answer;
            let mut out: Vec<LinkEvent> = self
                .owed
                .drain(..)
                .filter_map(|(id, control)| answer(control).map(|outcome| LinkEvent::Control { id, at: now, control, outcome }))
                .collect();
            while self.reading * 250 <= now.as_millis() as u64 {
                let at = ms(self.reading * 250);
                let (cadence, watts) = (self.ride)(at);
                let cadence_rpm = self.cadence.contains(&at).then_some(cadence);
                out.push(LinkEvent::Reading(Reading { at, watts, cadence_rpm, ..Reading::default() }));
                self.reading += 1;
            }
            out
        }
    }

    impl TrainerLink for Stub {
        fn use_profile(&mut self, _: &'static TrainerProfile) {}
        fn send(&mut self, control: Control) -> ControlId {
            self.next += 1;
            let id = ControlId(self.next);
            self.owed.push((id, control));
            id
        }
    }

    fn ack(_: Control) -> Option<ControlOutcome> {
        Some(ControlOutcome::Acknowledged)
    }

    struct Ride {
        session: Session,
        erg: ErgHandle,
        log: Arc<Mutex<Log>>,
        /// Ticks so far, 4 a second.
        q: u64,
    }

    impl Ride {
        fn on(trainer: impl TrainerLink + 'static, target: Option<u16>) -> Ride {
            Ride::with_pedals(trainer, None, target)
        }

        fn with_pedals(trainer: impl TrainerLink + 'static, pedals: Option<Box<dyn SensorLink>>, target: Option<u16>) -> Ride {
            let log = Arc::new(Mutex::new(Log::default()));
            let mut spy = Spy { inner: trainer, now: Duration::ZERO, log: log.clone() };
            let _ = spy.identity(); // whatever the trainer is: generic here
            spy.use_profile(&GENERIC);
            let mut session = Session::new(Duration::ZERO, 0, SubSport::IndoorCycling, Box::new(spy), pedals, None, None);
            let erg = ErgHandle::default();
            erg.set_target(target);
            session.add_controller(Box::new(Erg::new(erg.clone())));
            Ride { session, erg, log, q: 0 }
        }

        fn fake(cfg: FakeConfig, target: Option<u16>) -> Ride {
            Ride::on(FakeTrainer::new(cfg), target)
        }

        /// Rides on to `until` at 4 ticks a second.
        fn to(&mut self, until: Duration) -> &mut Ride {
            while ms(self.q * 250) < until {
                self.q += 1;
                let now = ms(self.q * 250);
                self.session.tick(now);
                self.log.lock().unwrap().states.push((now, self.erg.view()));
            }
            self
        }

        fn now(&self) -> Duration {
            ms(self.q * 250)
        }

        /// A fake trainer with power pedals on it (`drops`: the pedals').
        fn matched(cfg: FakeConfig, drops: Vec<(Duration, Duration)>, target: Option<u16>) -> Ride {
            Ride::muted(cfg, drops, Vec::new(), target)
        }

        /// …and pedals silent at times without being said lost.
        fn muted(cfg: FakeConfig, drops: Vec<(Duration, Duration)>, mutes: Vec<(Duration, Duration)>, target: Option<u16>) -> Ride {
            let mut trainer = FakeTrainer::new(cfg);
            let pedals = trainer.pedals(drops, mutes);
            assert!(pedals.identity().is_none(), "nobody in particular");
            Ride::with_pedals(trainer, Some(Box::new(pedals)), target)
        }

        fn pedal_watts(&self, from: usize, to: usize) -> Vec<u16> {
            self.session.seconds()[from..to]
                .iter()
                .filter(|s| s.power_from == Some(Role::Pedals))
                .filter_map(|s| s.sample.watts)
                .collect()
        }

        fn targets_between(&self, from: Duration, to: Duration) -> Vec<u16> {
            self.sent_between(from, to).iter().filter_map(|c| if let Control::TargetPower(w) = c { Some(*w) } else { None }).collect()
        }

        fn trainer_watts(&self, from: usize, to: usize) -> Vec<u16> {
            self.session.seconds()[from..to].iter().filter_map(|s| s.trainer_watts).collect()
        }

        fn sent_between(&self, from: Duration, to: Duration) -> Vec<Control> {
            self.log.lock().unwrap().sent.iter().filter(|(at, _)| *at >= from && *at < to).map(|(_, c)| *c).collect()
        }

        fn states_between(&self, from: Duration, to: Duration) -> Vec<Option<ErgState>> {
            let log = self.log.lock().unwrap();
            log.states.iter().filter(|(at, _)| *at >= from && *at < to).map(|(_, v)| v.map(|v| v.state)).collect()
        }
    }

    fn within(watts: &[u16], target: u16) -> bool {
        !watts.is_empty() && watts.iter().all(|w| w.abs_diff(target) <= tolerance(target))
    }

    #[test]
    fn a_trainer_that_lets_go_is_held_by_the_resends() {
        let mut r = Ride::fake(FakeConfig::elite_avanti(85), Some(150));
        r.to(secs(120));
        let held = r.trainer_watts(10, 120);
        assert!(within(&held, 150), "{held:?}");
        let resends = r.sent_between(secs(0), secs(120)).len();
        assert!((55..=65).contains(&resends), "every 2 s: {resends}");
        assert_eq!(r.log.lock().unwrap().max_in_flight, 1);
        assert_eq!(r.erg.view(), Some(ErgLive { target_w: 150, state: ErgState::Holding }));

        // Sent once, the Avanti lets go after 18 s and follows the wheel.
        struct Once(bool);
        impl Controller for Once {
            fn tick(&mut self, _: &Moment, _: &[LinkEvent], trainer: &mut dyn TrainerLink) {
                if !std::mem::replace(&mut self.0, true) {
                    trainer.send(Control::TargetPower(150));
                }
            }
        }
        let mut s = Session::new(Duration::ZERO, 0, SubSport::IndoorCycling, Box::new(FakeTrainer::new(FakeConfig::elite_avanti(85))), None, None, None);
        s.add_controller(Box::new(Once(false)));
        for q in 1..=480 {
            s.tick(ms(q * 250));
        }
        let free: Vec<u16> = s.seconds()[40..].iter().filter_map(|x| x.trainer_watts).collect();
        assert!(free.iter().all(|&w| w > 200), "without resends it rides free: {free:?}");
    }

    #[test]
    fn the_target_waits_for_the_legs_at_the_start() {
        let mut r = Ride::fake(FakeConfig::elite_avanti(0).then_cadence(secs(10), 85), Some(150));
        r.to(secs(20));
        assert!(r.sent_between(secs(0), secs(10)).is_empty(), "standing still: nothing held, nothing to let go");
        assert!(r.states_between(secs(1), secs(10)).iter().all(|s| *s == Some(ErgState::Ready)), "waiting for the legs, not a fault");
        let first = r.log.lock().unwrap().sent.first().copied().expect("sent");
        assert!((secs(11)..secs(13)).contains(&first.0), "after 2 s of pedalling: {first:?}");
        assert_eq!(first.1, Control::TargetPower(150));
    }

    #[test]
    fn a_cadence_step_overshoots_without_being_off_target() {
        let mut r = Ride::fake(FakeConfig::elite_avanti(66).then_cadence(secs(30), 96), Some(150));
        r.to(secs(60));
        assert!(!r.states_between(secs(0), secs(60)).contains(&Some(ErgState::OffTarget)));
        assert!(within(&r.trainer_watts(40, 60), 150), "settled after the step");
    }

    /// Confirms everything and ignores it: 250 W, its watts heard 2
    /// seconds in 3.
    fn deaf(at: Duration) -> (u8, Option<u16>) {
        (85, (!at.as_secs().is_multiple_of(3)).then_some(250))
    }

    #[test]
    fn watts_away_from_the_target_are_off_target_whatever_the_trainer_confirms() {
        let mut r = Ride::on(Stub::new(deaf, ack), Some(150));
        r.to(secs(30));
        let states = r.states_between(secs(0), secs(30));
        let first_off = states.iter().position(|s| *s == Some(ErgState::OffTarget)).expect("off target");
        let at = ms((first_off as u64 + 1) * 250);
        assert!((secs(10)..=secs(16)).contains(&at), "after 8 heard seconds away, not at once: {at:?}");
        assert_eq!(r.sent_between(at, at + ms(250)), vec![Control::TargetPower(150)], "and the target goes at once");
    }

    #[test]
    fn a_ramp_on_a_deaf_trainer_is_off_target_too() {
        let mut r = Ride::on(Stub::new(|at| (85, (!at.as_secs().is_multiple_of(3)).then_some(400)), ack), None);
        r.erg.set_workout(Workout { steps: vec![Step { secs: 120, from: 100, to: 220 }] });
        r.to(secs(40));
        assert!(r.states_between(secs(0), secs(40)).contains(&Some(ErgState::OffTarget)));
    }

    #[test]
    fn after_a_drop_the_target_goes_again_at_once() {
        let mut r = Ride::fake(FakeConfig::elite_avanti(85).with_drop(secs(30), secs(6)), Some(150));
        r.to(secs(60));
        let back = *r.log.lock().unwrap().back.first().expect("back");
        assert_eq!(r.sent_between(back, back + ms(1)), vec![Control::TargetPower(150)], "the tick that heard Back sends");
        assert!(within(&r.trainer_watts(45, 60), 150));
    }

    #[test]
    fn unconfirmed_controls_go_again_one_at_a_time() {
        let mut cfg = FakeConfig::steady(85);
        cfg.lose_every_nth_control = Some(3);
        cfg.ack_delay = ms(600);
        let mut r = Ride::fake(cfg, Some(180));
        r.to(secs(90));
        let log = r.log.lock().unwrap();
        assert_eq!(log.max_in_flight, 1, "never two in flight");
        let failed: Vec<Duration> =
            log.answered.iter().filter(|(_, _, o)| *o == ControlOutcome::NotAcknowledged).map(|(at, _, _)| *at).collect();
        assert!(failed.len() >= 5, "{}", failed.len());
        for at in failed {
            assert!(log.sent.iter().any(|(s, _)| *s == at), "the first retry is at once: {at:?}");
        }
        drop(log);
        assert!(within(&r.trainer_watts(20, 90), 180));
    }

    #[test]
    fn a_trainer_that_never_confirms_is_asked_less_and_less_often() {
        let mut r = Ride::on(Stub::new(|_| (85, Some(150)), |_| Some(ControlOutcome::NotAcknowledged)), Some(150));
        r.to(secs(60));
        let sent: Vec<Duration> = r.log.lock().unwrap().sent.iter().map(|(at, _)| *at).collect();
        assert!(sent.len() <= 35, "not 4 a second: {}", sent.len());
        let late: Vec<Duration> = sent.iter().copied().filter(|&at| at > secs(30)).collect();
        assert!(late.windows(2).all(|w| w[1] - w[0] >= secs(2)), "at most every erg_resend: {late:?}");
        assert!(r.states_between(secs(5), secs(60)).iter().all(|s| *s == Some(ErgState::Waiting)), "never said to hold");
    }

    #[test]
    fn a_trainer_out_of_reach_is_waited_for_not_said_to_hold() {
        let mut r = Ride::on(Stub::new(|_| (85, Some(150)), |_| Some(ControlOutcome::Refused)), Some(150));
        r.to(secs(30));
        assert!(r.states_between(secs(3), secs(30)).iter().all(|s| *s == Some(ErgState::Waiting)));
        let sent = r.sent_between(secs(0), secs(30)).len();
        assert!((10..=16).contains(&sent), "every erg_resend, no flood: {sent}");
    }

    #[test]
    fn low_cadence_lets_go_and_the_target_comes_back_with_it() {
        let cfg = FakeConfig::elite_avanti(85).then_cadence(secs(30), 20).then_cadence(secs(60), 85);
        let mut r = Ride::fake(cfg, Some(200));
        r.to(secs(90));
        let around_low = r.sent_between(secs(30), secs(34));
        assert!(around_low.contains(&Control::Release), "let go within LOW_FOR: {around_low:?}");
        let low = r.sent_between(secs(34), secs(60));
        assert!(!low.contains(&Control::TargetPower(200)), "no target while the legs stall: {low:?}");
        assert!(r.states_between(secs(35), secs(60)).iter().all(|s| *s == Some(ErgState::LowCadence)));
        let back = r.sent_between(secs(60), secs(64));
        assert!(back.contains(&Control::TargetPower(200)), "back with the cadence: {back:?}");
        assert!(within(&r.trainer_watts(70, 90), 200));
    }

    #[test]
    fn a_pause_lets_go_and_the_target_waits_for_the_legs_after_it() {
        let mut r = Ride::fake(FakeConfig::elite_avanti(85), Some(150));
        r.to(secs(30));
        r.session.pause(r.now());
        let paused_at = r.now();
        r.to(secs(50));
        assert_eq!(r.sent_between(paused_at + ms(1), paused_at + ms(251)), vec![Control::Release]);
        assert_eq!(r.sent_between(paused_at + secs(1), secs(50)), vec![], "confirmed: nothing more");
        assert!(r.states_between(paused_at + secs(1), secs(50)).iter().all(|s| *s == Some(ErgState::Paused)));
        r.session.resume(r.now());
        let resumed_at = r.now();
        r.to(secs(60));
        let back = r.sent_between(resumed_at + ms(1), resumed_at + secs(4));
        assert_eq!(back.first(), Some(&Control::TargetPower(150)), "once the legs turn: {back:?}");
    }

    /// Confirms targets, never a release.
    fn no_release(c: Control) -> Option<ControlOutcome> {
        Some(if c == Control::Release { ControlOutcome::NotAcknowledged } else { ControlOutcome::Acknowledged })
    }

    #[test]
    fn a_release_is_asked_until_it_is_confirmed_never_given_up() {
        let mut r = Ride::on(Stub::new(|_| (85, Some(150)), no_release), Some(150));
        r.to(secs(10));
        r.session.pause(r.now());
        r.to(secs(120));
        let late = r.sent_between(secs(60), secs(120));
        assert!(!late.is_empty() && late.iter().all(|c| *c == Control::Release), "still asked a minute on: {late:?}");
        assert!(late.len() <= 31, "every erg_resend at most: {}", late.len());
        assert!(r.states_between(secs(11), secs(120)).iter().all(|s| *s == Some(ErgState::Waiting)), "never said to be off");
        // Riding free from here asks the same: the brake must go.
        r.erg.set_target(None);
        r.to(secs(130));
        assert!(r.sent_between(secs(120), secs(130)).contains(&Control::Release));
    }

    #[test]
    fn a_pause_in_a_long_drop_lets_go_once_the_trainer_is_back() {
        let mut r = Ride::fake(FakeConfig::elite_avanti(85).with_drop(secs(29), secs(20)), Some(200));
        r.to(secs(30));
        r.session.pause(r.now());
        r.to(secs(70));
        let back = *r.log.lock().unwrap().back.first().expect("back");
        assert_eq!(r.sent_between(back, back + ms(1)), vec![Control::Release], "the tick that heard Back lets go");
        assert_eq!(r.states_between(back + secs(2), secs(70)).last(), Some(&Some(ErgState::Paused)));
    }

    /// Releases confirmed, targets that time out (they may still land).
    fn targets_time_out(c: Control) -> Option<ControlOutcome> {
        Some(if c == Control::Release { ControlOutcome::Acknowledged } else { ControlOutcome::TimedOut })
    }

    #[test]
    fn a_target_sent_after_a_release_makes_the_next_release_count() {
        let stall = |at: Duration| (if at < secs(20) { 85 } else { 20 }, Some(150));
        let mut r = Ride::on(Stub::new(stall, targets_time_out), Some(200));
        r.to(secs(5));
        r.session.pause(r.now());
        r.to(secs(8)); // the release is confirmed
        r.session.resume(r.now());
        r.to(secs(30));
        let stalled = r.sent_between(secs(20), secs(25));
        assert!(stalled.contains(&Control::Release), "the target may have landed: let go again: {stalled:?}");
    }

    #[test]
    fn riding_free_sends_nothing_and_a_cleared_target_lets_go_once() {
        let mut r = Ride::fake(FakeConfig::elite_avanti(85), None);
        r.to(secs(20));
        assert!(r.sent_between(secs(0), secs(20)).is_empty());
        assert_eq!(r.erg.view(), None);
        assert_eq!(r.erg.ready(), None);
        r.erg.set_target(Some(150));
        assert_eq!(r.erg.ready(), Some(ErgLive { target_w: 150, state: ErgState::Ready }));
        r.to(secs(40));
        assert_eq!(r.erg.view(), Some(ErgLive { target_w: 150, state: ErgState::Holding }));
        r.erg.set_target(None);
        r.to(secs(60));
        assert_eq!(r.sent_between(secs(40), secs(60)), vec![Control::Release]);
        assert_eq!(r.erg.view(), None);
        // A pause riding free asks nothing of the trainer either.
        let mut free = Ride::fake(FakeConfig::steady(85), None);
        free.to(secs(5));
        free.session.pause(free.now());
        free.to(secs(15));
        assert!(free.sent_between(secs(0), secs(15)).is_empty());
    }

    #[test]
    fn a_workout_drives_the_target_and_ends_free() {
        let plan = Workout { steps: vec![Step { secs: 30, from: 100, to: 100 }, Step { secs: 30, from: 100, to: 200 }] };
        assert_eq!(plan.target_at(0), Some(100));
        assert_eq!(plan.target_at(29), Some(100));
        assert_eq!(plan.target_at(45), Some(150), "half way up the ramp");
        assert_eq!(plan.target_at(59), Some(197));
        assert_eq!(plan.target_at(60), None, "over");
        let odd = Workout { steps: vec![Step { secs: 10, from: 0, to: 0 }, Step { secs: u32::MAX, from: 3000, to: 3000 }] };
        assert_eq!(odd.target_at(5), None, "0 W: nothing to hold");
        assert_eq!(odd.target_at(20), Some(MAX_TARGET), "capped, and no overflow");
        let mut r = Ride::fake(FakeConfig::elite_avanti(85), None);
        r.to(secs(10));
        r.erg.set_workout(plan.clone());
        assert_eq!(r.erg.ready(), Some(ErgLive { target_w: 100, state: ErgState::Ready }));
        r.to(secs(85));
        let targets: Vec<u16> =
            r.sent_between(secs(10), secs(85)).iter().filter_map(|c| if let Control::TargetPower(w) = c { Some(*w) } else { None }).collect();
        assert_eq!(targets.first(), Some(&100));
        assert!(targets.windows(2).all(|w| w[1] >= w[0]), "only up: {targets:?}");
        assert!(targets.iter().any(|&w| w >= 190), "up the ramp: {targets:?}");
        assert_eq!(r.sent_between(secs(70), secs(85)).last(), Some(&Control::Release), "ends free");
        assert_eq!(r.erg.view(), None);
    }

    /// No cadence: still to 10 s, turning to 30, stopped to 40, turning on.
    fn blind_ride(at: Duration) -> (u8, Option<u16>) {
        let still = at < secs(10) || (secs(30)..secs(40)).contains(&at);
        (0, Some(if still { 0 } else { 150 }))
    }

    /// A trainer that sends watts and no cadence: held on its watts (rather
    /// than never), let go once they stop, and the screen says so.
    #[test]
    fn a_trainer_without_cadence_is_held_on_its_watts() {
        let mut r = Ride::on(Stub::blind(blind_ride), Some(150));
        r.to(secs(30));
        let first = r.log.lock().unwrap().sent.first().copied().expect("sent");
        assert_eq!(first.1, Control::TargetPower(150));
        assert!((secs(16)..secs(17)).contains(&first.0), "5 s turning with no cadence, then 2 s: {first:?}");
        assert_eq!(r.erg.view(), Some(ErgLive { target_w: 150, state: ErgState::NoCadence }));
        // The legs stop: the brake goes, and comes back with the watts.
        r.to(secs(50));
        let stop = r.sent_between(secs(30), secs(33));
        assert!(stop.contains(&Control::Release), "{stop:?}");
        assert!(!r.sent_between(secs(33), secs(40)).contains(&Control::TargetPower(150)));
        assert!(r.states_between(secs(34), secs(40)).iter().all(|s| *s == Some(ErgState::LowCadence)));
        assert!(r.sent_between(secs(40), secs(43)).contains(&Control::TargetPower(150)));
        // Away from the target too: what the rider cannot see comes first.
        let mut off = Ride::on(Stub::blind(|_| (0, Some(250))), Some(150));
        off.to(secs(40));
        assert_eq!(off.erg.view(), Some(ErgLive { target_w: 150, state: ErgState::NoCadence }));
    }

    /// Without cadence, a trainer that falls silent standing still is let go.
    #[test]
    fn without_cadence_a_silent_trainer_is_taken_as_stopped() {
        let mut r = Ride::on(Stub::blind(|at| (0, (at < secs(30) || at >= secs(45)).then_some(150))), Some(150));
        r.to(secs(50));
        let stop = r.sent_between(secs(30), secs(34));
        assert!(stop.contains(&Control::Release), "{stop:?}");
        assert!(!r.sent_between(secs(34), secs(45)).contains(&Control::TargetPower(150)));
        assert!(r.sent_between(secs(45), secs(48)).contains(&Control::TargetPower(150)), "back with the watts");
    }

    #[test]
    fn without_cadence_a_resumed_ride_waits_for_the_watts() {
        let mut r = Ride::on(Stub::blind(|at| (0, Some(if (secs(25)..secs(40)).contains(&at) { 0 } else { 150 }))), Some(150));
        r.to(secs(22));
        r.session.pause(r.now());
        r.to(secs(30));
        r.session.resume(r.now());
        r.to(secs(45));
        assert!(!r.sent_between(secs(30), secs(40)).contains(&Control::TargetPower(150)), "resumed with the legs still");
        assert!(r.states_between(secs(31), secs(40)).iter().all(|s| *s == Some(ErgState::Ready)));
        assert!(r.sent_between(secs(40), secs(43)).contains(&Control::TargetPower(150)));
    }

    /// A cadence heard once is the guard's: a trainer that sends one late
    /// is no longer blind, one that stops sending it does not become so.
    #[test]
    fn a_cadence_heard_late_brings_the_guard_back() {
        let late = Stub { cadence: secs(20)..Duration::MAX, ..Stub::new(|_| (85, Some(150)), ack) };
        let mut r = Ride::on(late, Some(150));
        r.to(secs(18));
        assert_eq!(r.erg.view(), Some(ErgLive { target_w: 150, state: ErgState::NoCadence }));
        r.to(secs(30));
        assert_eq!(r.erg.view(), Some(ErgLive { target_w: 150, state: ErgState::Holding }));
        let gone = Stub { cadence: Duration::ZERO..secs(10), ..Stub::new(|_| (85, Some(150)), ack) };
        let mut r = Ride::on(gone, Some(150));
        r.to(secs(30));
        assert_eq!(r.erg.view(), Some(ErgLive { target_w: 150, state: ErgState::Holding }));
    }

    /// Pedals that never answer.
    struct Silent;

    impl SensorLink for Silent {
        fn identity(&self) -> Option<&Identity> {
            None
        }
        fn poll(&mut self, _: Duration) -> Vec<LinkEvent> {
            Vec::new()
        }
    }

    #[test]
    fn pedals_not_heard_yet_are_waited_for_before_holding_without_cadence() {
        let mut r = Ride::with_pedals(Stub::blind(|_| (0, Some(150))), Some(Box::new(Silent)), Some(150));
        r.to(secs(25));
        assert!(r.sent_between(secs(0), secs(25)).is_empty(), "the pedals may bring the cadence");
        r.to(secs(40));
        assert!(r.sent_between(secs(25), secs(40)).contains(&Control::TargetPower(150)), "they never did");
    }

    #[test]
    fn after_a_stall_and_a_pause_the_target_waits_for_the_legs_not_a_fault() {
        let mut r = Ride::fake(FakeConfig::elite_avanti(85).then_cadence(secs(20), 20), Some(150));
        r.to(secs(30));
        assert_eq!(r.erg.view(), Some(ErgLive { target_w: 150, state: ErgState::LowCadence }));
        r.session.pause(r.now());
        r.to(secs(35));
        r.session.resume(r.now());
        r.to(secs(40));
        assert_eq!(r.erg.view(), Some(ErgLive { target_w: 150, state: ErgState::Ready }));
    }

    /// Let go and riding free: a long drop may have cost the release (the
    /// trainer takes a resistance of its own), so it goes again once.
    #[test]
    fn a_long_drop_asks_for_the_release_again() {
        let mut r = Ride::fake(FakeConfig::elite_avanti(85).with_drop(secs(30), secs(40)), Some(150));
        r.to(secs(20));
        r.erg.set_target(None);
        r.to(secs(100));
        assert_eq!(r.sent_between(secs(20) + ms(1), secs(100)), vec![Control::Release, Control::Release]);
    }

    #[test]
    fn riding_free_shows_the_brake_being_let_go_until_it_is() {
        let mut r = Ride::on(Stub::new(|_| (85, Some(150)), no_release), Some(150));
        r.to(secs(10));
        r.erg.set_target(None);
        r.to(secs(15));
        assert_eq!(r.erg.view(), Some(ErgLive { target_w: 0, state: ErgState::Releasing }), "not confirmed: shown");
        let mut confirmed = Ride::fake(FakeConfig::elite_avanti(85), Some(150));
        confirmed.to(secs(10));
        confirmed.erg.set_target(None);
        confirmed.to(secs(15));
        assert_eq!(confirmed.erg.view(), None, "confirmed: riding free");
    }

    /// Let go and riding free: a drop does not cost the release, so no
    /// release goes again after it.
    #[test]
    fn a_drop_after_letting_go_sends_nothing_more() {
        let mut r = Ride::fake(FakeConfig::elite_avanti(85).with_drop(secs(30), secs(6)), Some(150));
        r.to(secs(20));
        r.erg.set_target(None);
        r.to(secs(60));
        assert_eq!(r.sent_between(secs(20) + ms(1), secs(60)), vec![Control::Release], "once, not after Back");
    }

    /// How far the Avanti reads below the pedals, by cadence (the spike:
    /// −4 W at ≤ 66 rpm, +6…+24 W at 79–85, +6…+13 above).
    fn avanti_gap(rpm: u8) -> f32 {
        // Up to each cadence, this gap.
        const GAP: [(u8, f32); 4] = [(66, -4.0), (78, 6.0), (86, 18.0), (u8::MAX, 10.0)];
        GAP.iter().find(|&&(upto, _)| rpm <= upto).map_or(10.0, |&(_, gap)| gap)
    }

    fn avanti_reading_low(cadence: u8) -> FakeConfig {
        FakeConfig { reads_low: avanti_gap, ..FakeConfig::elite_avanti(cadence) }
    }

    fn near(watts: &[u16], target: u16, by: u16) -> bool {
        !watts.is_empty() && watts.iter().all(|w| w.abs_diff(target) <= by)
    }

    #[test]
    fn the_pedals_watts_are_brought_onto_the_target() {
        let mut r = Ride::matched(avanti_reading_low(85), vec![], Some(200));
        r.to(secs(180));
        let early = r.pedal_watts(4, 6);
        assert!(early.iter().any(|&w| w >= 215), "before it is learnt, the trainer's own 200 W are 218 on the pedals: {early:?}");
        assert!(near(&r.pedal_watts(120, 180), 200, 2), "{:?}", r.pedal_watts(120, 180));
        let late = r.targets_between(secs(120), secs(180));
        assert!(late.iter().all(|&w| w == 182), "steady, no pumping: {late:?}");
        assert_eq!(r.erg.matching(), Some(MatchLive { state: MatchState::Matching, trainer_target_w: 182 }));
        assert_eq!(r.erg.view(), Some(ErgLive { target_w: 200, state: ErgState::Holding }), "judged by what the trainer was sent");
        // Without pedals there is nothing to match.
        let mut plain = Ride::fake(avanti_reading_low(85), Some(200));
        plain.to(secs(30));
        assert_eq!(plain.erg.matching(), None);
        assert!(plain.targets_between(secs(0), secs(30)).iter().all(|&w| w == 200));
    }

    #[test]
    fn a_cadence_ridden_before_is_matched_at_once() {
        let cfg = avanti_reading_low(64).then_cadence(secs(150), 85).then_cadence(secs(330), 64);
        let mut r = Ride::matched(cfg, vec![], Some(200));
        r.to(secs(360));
        assert!(near(&r.pedal_watts(120, 150), 200, 2), "64 rpm: {:?}", r.pedal_watts(120, 150));
        assert!(near(&r.pedal_watts(300, 330), 200, 2), "85 rpm: {:?}", r.pedal_watts(300, 330));
        let back = r.pedal_watts(340, 360);
        assert!(near(&back, 200, 3), "back at 64 rpm, matched without learning again: {back:?}");
    }

    #[test]
    fn pedals_lost_mid_interval_fall_back_to_plain_erg() {
        let mut r = Ride::matched(avanti_reading_low(85), vec![(secs(150), secs(30))], Some(200));
        r.to(secs(148));
        assert_eq!(r.erg.matching().map(|m| m.trainer_target_w), Some(182));
        r.to(secs(170));
        assert!(r.targets_between(secs(150), secs(154)).contains(&200), "the plain target at once");
        assert!(near(&r.trainer_watts(156, 170), 200, 3), "held by the trainer's watts");
        assert_eq!(r.erg.matching(), Some(MatchLive { state: MatchState::NoPedals, trainer_target_w: 200 }));
        r.to(secs(190));
        assert!(r.targets_between(secs(180), secs(184)).contains(&182), "back, with what was learnt");
        assert_eq!(r.erg.matching().map(|m| m.state), Some(MatchState::Matching));
    }

    /// Pedals that read ±20 W at random, a cadence across a band edge (and
    /// the Avanti's gap step) every 10 s: the trainer's target moves
    /// seldom, and the pedals hold the target on average.
    #[test]
    fn riding_across_a_band_edge_with_noisy_pedals_does_not_pump() {
        let mut cfg = FakeConfig { pedal_noise: 20.0, ..avanti_reading_low(84) };
        for t in (10..600).step_by(10) {
            cfg = cfg.then_cadence(secs(t), if t % 20 == 0 { 84 } else { 89 });
        }
        let mut r = Ride::matched(cfg, vec![], Some(200));
        r.to(secs(600));
        let sent: Vec<(Duration, u16)> = r
            .log
            .lock()
            .unwrap()
            .sent
            .iter()
            .filter_map(|&(at, c)| match c {
                Control::TargetPower(w) if at >= secs(180) => Some((at, w)),
                _ => None,
            })
            .collect();
        let moves: Vec<Duration> = sent.windows(2).filter(|w| w[0].1 != w[1].1).map(|w| w[1].0).collect();
        // Following the cadence (a change every 10 s) is the point, nudges
        // come at most every SETTLE_FOR; pumping is moves on each other's
        // heels, faster than the trainer settles.
        assert!(moves.len() <= 42 + 28, "{} moves in 7 minutes", moves.len());
        // A nudge may fall just before a cadence change: rarely.
        let quick = moves.windows(2).filter(|w| w[1] - w[0] < secs(4)).count();
        assert!(quick <= 3, "{quick} moves on each other's heels: {moves:?}");
        let pedals = r.pedal_watts(180, 600);
        let mean = pedals.iter().map(|&w| f32::from(w)).sum::<f32>() / pedals.len() as f32;
        assert!((mean - 200.0).abs() <= 3.0, "{mean}");
    }

    #[test]
    fn power_match_is_shown_only_while_a_target_is_held() {
        let mut r = Ride::matched(avanti_reading_low(30).then_cadence(secs(20), 85), vec![], Some(200));
        r.to(secs(15));
        assert_eq!(r.erg.view().map(|v| v.state), Some(ErgState::Ready));
        assert_eq!(r.erg.matching(), None, "nothing asked of the trainer yet");
        r.to(secs(40));
        assert_eq!(r.erg.matching().map(|m| m.state), Some(MatchState::Matching));
        r.session.pause(r.now());
        r.to(secs(45));
        assert_eq!(r.erg.matching(), None, "paused: the brake is let go");
        // Not confirmed: nothing is said to be held.
        let mut cfg = avanti_reading_low(85);
        cfg.lose_every_nth_control = Some(1);
        let mut deaf = Ride::matched(cfg, vec![], Some(200));
        deaf.to(secs(20));
        assert_eq!(deaf.erg.view().map(|v| v.state), Some(ErgState::Waiting));
        assert_eq!(deaf.erg.matching(), None);
        // Pedals never heard: no warning while they may be waking up, then
        // one.
        let mut asleep = Ride::with_pedals(FakeTrainer::new(avanti_reading_low(85)), Some(Box::new(Silent)), Some(200));
        asleep.to(secs(20));
        assert_eq!(asleep.erg.view().map(|v| v.state), Some(ErgState::Holding));
        assert_eq!(asleep.erg.matching(), None);
        asleep.to(secs(40));
        assert_eq!(asleep.erg.matching(), Some(MatchLive { state: MatchState::NoPedals, trainer_target_w: 200 }));
    }

    /// Pedals at a steady `.0` watts and 85 rpm, four times a second.
    struct Steady(u16);

    impl SensorLink for Steady {
        fn identity(&self) -> Option<&Identity> {
            None
        }
        fn poll(&mut self, now: Duration) -> Vec<LinkEvent> {
            // A reading in every quarter: enough for every second.
            let at = ms(now.as_millis() as u64 / 250 * 250);
            vec![LinkEvent::Reading(Reading { at, watts: Some(self.0), cadence_rpm: Some(85), ..Reading::default() })]
        }
    }

    /// Where the trainer's own watts sit on a confirmed `target`, with the
    /// pedals 18 W above them: what is the trainer sent after two minutes?
    fn matched_on(target: u16, trainer: fn(Duration) -> (u8, Option<u16>), pedals: u16) -> u16 {
        let pedals = Steady(pedals);
        assert!(pedals.identity().is_none());
        let mut r = Ride::with_pedals(Stub::new(trainer, ack), Some(Box::new(pedals)), Some(target));
        r.to(secs(120));
        assert_eq!(r.erg.view().map(|v| v.state), Some(ErgState::Holding), "on target for ERG");
        r.erg.matching().expect("matching").trainer_target_w
    }

    /// Only a trainer close on what it confirmed (5 %, at least 8 W) is
    /// learnt from: ERG's 10 % is too wide for a gap of a few watts.
    #[test]
    fn a_trainer_not_close_on_its_target_teaches_nothing() {
        assert_eq!(matched_on(200, |_| (85, Some(212)), 230), 200, "12 W off 200: not close");
        assert!(matched_on(200, |_| (85, Some(210)), 228) < 200, "10 W off 200: close, just");
        assert!(matched_on(100, |_| (85, Some(107)), 125) < 100, "7 W off 100: close (the 8 W floor)");
    }

    /// Pedals heard every other second: the seconds between hold their
    /// last watts over, and those are never learnt from.
    #[test]
    fn watts_held_over_from_the_pedals_teach_nothing() {
        let mutes: Vec<(Duration, Duration)> = (1..120).step_by(2).map(|s| (secs(s), secs(1))).collect();
        let mut r = Ride::muted(avanti_reading_low(85), vec![], mutes, Some(200));
        r.to(secs(120));
        assert_eq!(r.erg.matching(), Some(MatchLive { state: MatchState::Matching, trainer_target_w: 200 }), "nothing learnt");
        r.to(secs(240));
        assert_eq!(r.erg.matching().map(|m| m.trainer_target_w), Some(182), "heard every second: learnt");
    }

    #[test]
    fn a_control_never_answered_frees_the_line() {
        let mut r = Ride::on(Stub::new(|_| (85, Some(150)), |_| None), Some(150));
        r.to(secs(20));
        let sent: Vec<Duration> = r.log.lock().unwrap().sent.iter().map(|(at, _)| *at).collect();
        // Generic: twice 3 s without an answer, then again.
        assert!(sent.len() >= 2, "{sent:?}");
        assert!(sent.windows(2).all(|w| w[1] - w[0] > secs(6)), "{sent:?}");
    }

    #[test]
    fn targets_are_capped_tolerance_has_a_floor_and_retries_back_off() {
        let erg = ErgHandle::default();
        erg.set_target(Some(5000));
        assert_eq!(erg.target(0), Some(MAX_TARGET));
        assert_eq!(tolerance(100), 15);
        assert_eq!(tolerance(300), 30);
        let cap = secs(2);
        let waits: Vec<Duration> = (1..=6).map(|n| backoff(n, cap)).collect();
        assert_eq!(waits, vec![Duration::ZERO, ms(500), secs(1), secs(2), secs(2), secs(2)]);
        assert_eq!(backoff(1000, cap), cap);
        assert_eq!(kind(Control::Resistance(5)), Kind::Other);
    }
}
