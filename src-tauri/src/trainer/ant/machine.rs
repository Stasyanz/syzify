//! The ANT stick as a state machine without I/O: it is given what the
//! stick said and the time, and answers with the messages to send. One
//! stick serves every device of the session, a channel each.
//!
//! What the spike taught (docs/trainers/elite-avanti.md):
//!
//! - **Low-priority search only.** A high-priority search takes the radio
//!   and blinds the other channels; each channel searches at low priority,
//!   for ever (a device that goes away comes back by itself).
//! - **A drop is the stick's to report**: `RX_FAIL_GO_TO_SEARCH` is Lost,
//!   the next page heard is Back — nothing to reconnect.
//! - **One acknowledged page in flight per channel**: the stick refuses a
//!   second (`TRANSFER_IN_PROGRESS`), so one more waits here behind it (a
//!   release at the end must not meet a target's slot and be "refused").
//!   Its result is `TRANSFER_TX_COMPLETED` or `_FAILED`; a refusal of the
//!   message is a page that never went out.
//! - **A drop starts the counters over**: what comes after a silence is
//!   not compared with what went before (a wrapped counter would read as
//!   thousands of watts).
//! - Extended messages name the sender, so "any trainer" finds its number.

use std::collections::VecDeque;
use std::time::Duration;

use super::frame::*;
use super::pages::{self, Decoder, KIND_FEC, KIND_HEART, KIND_POWER};
use crate::models::training::SensorRole;
use crate::trainer::link::{Control, ControlId, ControlOutcome, Identity, LinkEvent, LinkFailure};
use crate::trainer::profile::{TrainerProfile, GENERIC};

/// 2400 + 57 = 2457 MHz, the ANT+ frequency.
pub const ANT_PLUS_FREQ: u8 = 57;
/// After a reset some sticks say nothing: go on after this.
const RESET_WAIT: Duration = Duration::from_millis(1000);
/// A configuration message unanswered this long: the stick is not there.
const ANSWER_WAIT: Duration = Duration::from_secs(2);

/// The device type of a role.
pub fn kind_of(role: SensorRole) -> u8 {
    match role {
        SensorRole::Trainer => KIND_FEC,
        SensorRole::Pedals => KIND_POWER,
        SensorRole::Strap => KIND_HEART,
    }
}

/// Messages that configure the stick, sent one at a time, each after the
/// last was answered; first a reset.
#[derive(Debug)]
pub struct Setup {
    queue: VecDeque<Msg>,
    /// Waiting for the stick to start since this, after the reset.
    reset_at: Option<Duration>,
    /// The message awaiting its answer, and since when.
    waiting: Option<(u8, Duration)>,
}

/// Where a setup is.
#[derive(Debug, PartialEq, Eq)]
pub enum Progress {
    Send(Msg),
    Wait,
    Done,
    Failed(String),
}

impl Setup {
    /// A setup sending `queue`; the reset goes first.
    pub fn new(now: Duration, queue: Vec<Msg>) -> (Setup, Msg) {
        (Setup { queue: queue.into(), reset_at: Some(now), waiting: None }, Msg::new(MSG_RESET, &[0]))
    }

    /// The common start: the network key, and the sender's id on data.
    pub fn network(key: &[u8; 8]) -> Vec<Msg> {
        vec![Msg::new(MSG_SET_KEY, &[&[0u8][..], key].concat()), Msg::new(MSG_LIB_CONFIG, &[0, LIB_CHANNEL_ID])]
    }

    fn next(&mut self, now: Duration) -> Progress {
        match self.queue.pop_front() {
            Some(m) => {
                self.waiting = Some((m.id, now));
                Progress::Send(m)
            }
            None => {
                self.waiting = None;
                Progress::Done
            }
        }
    }

    pub fn on_message(&mut self, now: Duration, m: &Msg) -> Progress {
        if self.reset_at.is_some() {
            if m.id != MSG_STARTUP {
                return Progress::Wait;
            }
            self.reset_at = None;
            return self.next(now);
        }
        match (self.waiting, m.id, m.data.get(1), m.data.get(2)) {
            (Some((id, _)), MSG_CHANNEL_EVENT, Some(&answered), Some(&code)) if answered == id => match code {
                0 => self.next(now),
                code => Progress::Failed(format!("the ANT stick refused message 0x{id:02X} (code 0x{code:02X})")),
            },
            _ => Progress::Wait,
        }
    }

    pub fn on_tick(&mut self, now: Duration) -> Progress {
        match (self.reset_at, self.waiting) {
            (Some(at), _) if now.saturating_sub(at) >= RESET_WAIT => {
                self.reset_at = None;
                self.next(now)
            }
            (None, Some((id, at))) if now.saturating_sub(at) >= ANSWER_WAIT => {
                Progress::Failed(format!("the ANT stick did not answer message 0x{id:02X}"))
            }
            _ => Progress::Wait,
        }
    }
}

/// A device to link: its role, and its number (0: the first of its kind).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Wanted {
    pub role: SensorRole,
    pub number: u32,
}

/// What the machine is told.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    Message(Msg),
    /// Time passed.
    Tick,
    /// The USB link failed: the stick was pulled.
    Gone,
}

#[derive(Debug)]
struct Channel {
    kind: u8,
    number: u32,
    heard: bool,
    lost_at: Option<Duration>,
    identity: Option<Identity>,
    decoder: Decoder,
    in_flight: Option<(ControlId, Control, Duration)>,
    /// The control waiting behind the one in flight.
    queued: Option<(ControlId, Control)>,
    profile: &'static TrainerProfile,
    events: Vec<LinkEvent>,
}

impl Channel {
    fn answer(&mut self, now: Duration, outcome: ControlOutcome) {
        if let Some((id, control, _)) = self.in_flight.take() {
            self.events.push(LinkEvent::Control { id, at: now, control, outcome });
        }
    }

    /// The one in flight went out unconfirmed; the one waiting never went.
    fn settle(&mut self, now: Duration) {
        self.answer(now, ControlOutcome::NotAcknowledged);
        if let Some((id, control)) = self.queued.take() {
            self.events.push(LinkEvent::Control { id, at: now, control, outcome: ControlOutcome::Refused });
        }
    }

    /// The device is not heard any more: lost, once, until heard again.
    /// What follows its counters is not to be compared with what went before.
    fn lose(&mut self, now: Duration) {
        if self.heard && self.lost_at.is_none() {
            self.lost_at = Some(now);
            self.decoder = Decoder::default();
            self.settle(now);
            self.events.push(LinkEvent::Lost { at: now });
        }
    }

    /// The page for `control` goes on channel `ch`, in flight from `now`.
    fn go(&mut self, now: Duration, ch: usize, id: ControlId, control: Control) -> Msg {
        self.in_flight = Some((id, control, now));
        Msg::new(MSG_ACKNOWLEDGED, &[&[ch as u8][..], &pages::control_page(control)].concat())
    }
}

#[derive(Debug)]
enum Phase {
    Setting(Setup),
    Running,
    Failed,
}

/// The stick and its channels; channel `i` is the `i`-th device wanted.
#[derive(Debug)]
pub struct Machine {
    phase: Phase,
    channels: Vec<Channel>,
}

impl Machine {
    /// Opens a channel per device on network `key`: returns the machine and
    /// the first message (a reset).
    pub fn new(now: Duration, key: &[u8; 8], wanted: &[Wanted]) -> (Machine, Msg) {
        let mut queue = Setup::network(key);
        for (i, w) in wanted.iter().enumerate() {
            let ch = i as u8;
            let kind = kind_of(w.role);
            let [lo, hi, ..] = w.number.to_le_bytes();
            let [plo, phi] = pages::period(kind).to_le_bytes();
            queue.extend([
                // Bidirectional slave on network 0.
                Msg::new(MSG_ASSIGN, &[ch, 0x00, 0]),
                // Transmission type 0: any (the number's top nibble rides in
                // it, so the low 16 bits are what is matched).
                Msg::new(MSG_CHANNEL_ID, &[ch, lo, hi, kind, 0]),
                Msg::new(MSG_PERIOD, &[ch, plo, phi]),
                // No high-priority search; low priority for ever.
                Msg::new(MSG_SEARCH_TIMEOUT, &[ch, 0]),
                Msg::new(MSG_LOW_PRIORITY_SEARCH, &[ch, 0xFF]),
                Msg::new(MSG_RF_FREQ, &[ch, ANT_PLUS_FREQ]),
                Msg::new(MSG_OPEN, &[ch]),
            ]);
        }
        let (setup, reset) = Setup::new(now, queue);
        let channels = wanted
            .iter()
            .map(|w| Channel {
                kind: kind_of(w.role),
                number: w.number,
                heard: false,
                lost_at: None,
                identity: None,
                decoder: Decoder::default(),
                in_flight: None,
                queued: None,
                profile: &GENERIC,
                events: Vec::new(),
            })
            .collect();
        (Machine { phase: Phase::Setting(setup), channels }, reset)
    }

    /// What channel `ch` reported since the last call.
    pub fn take_events(&mut self, ch: usize) -> Vec<LinkEvent> {
        self.channels.get_mut(ch).map(|c| std::mem::take(&mut c.events)).unwrap_or_default()
    }

    pub fn use_profile(&mut self, ch: usize, profile: &'static TrainerProfile) {
        if let Some(c) = self.channels.get_mut(ch) {
            c.profile = profile;
        }
    }

    /// Every channel fails (the stick is gone or refused its setup); what
    /// was in flight went out unconfirmed.
    fn fail(&mut self, now: Duration, reason: LinkFailure) {
        self.phase = Phase::Failed;
        for c in &mut self.channels {
            c.settle(now);
            c.events.push(LinkEvent::Failed { at: now, reason: reason.clone() });
        }
    }

    fn progress(&mut self, now: Duration, p: Progress) -> Vec<Msg> {
        match p {
            Progress::Send(m) => vec![m],
            Progress::Wait => Vec::new(),
            Progress::Done => {
                self.phase = Phase::Running;
                Vec::new()
            }
            Progress::Failed(why) => {
                self.fail(now, LinkFailure::Other(why));
                Vec::new()
            }
        }
    }

    pub fn handle(&mut self, now: Duration, input: Input) -> Vec<Msg> {
        if matches!(input, Input::Gone) && !matches!(self.phase, Phase::Failed) {
            self.fail(now, LinkFailure::StickGone);
            return Vec::new();
        }
        match (&mut self.phase, input) {
            (Phase::Setting(setup), Input::Message(m)) => {
                let p = setup.on_message(now, &m);
                self.progress(now, p)
            }
            (Phase::Setting(setup), Input::Tick) => {
                let p = setup.on_tick(now);
                self.progress(now, p)
            }
            (Phase::Running, Input::Message(m)) => {
                let mut out = self.message(now, &m);
                out.extend(self.flush(now));
                out
            }
            (Phase::Running, Input::Tick) => {
                for c in &mut self.channels {
                    if c.in_flight.is_some_and(|(_, _, at)| now.saturating_sub(at) >= c.profile.control_answer_timeout) {
                        c.answer(now, ControlOutcome::TimedOut);
                    }
                }
                self.flush(now)
            }
            _ => Vec::new(),
        }
    }

    /// A control waiting behind one in flight goes once the line is free.
    fn flush(&mut self, now: Duration) -> Vec<Msg> {
        let mut out = Vec::new();
        for (ch, c) in self.channels.iter_mut().enumerate() {
            if c.in_flight.is_none() {
                if let Some((id, control)) = c.queued.take() {
                    out.push(c.go(now, ch, id, control));
                }
            }
        }
        out
    }

    fn message(&mut self, now: Duration, m: &Msg) -> Vec<Msg> {
        match m.id {
            MSG_BROADCAST | MSG_ACKNOWLEDGED | MSG_BURST => {
                let Some(p) = pages::packet(&m.data) else { return Vec::new() };
                let Some(c) = self.channels.get_mut(usize::from(p.channel)) else { return Vec::new() };
                if !c.heard {
                    c.heard = true;
                    c.events.push(LinkEvent::Connected { at: now });
                }
                if let Some(lost) = c.lost_at.take() {
                    c.events.push(LinkEvent::Back { at: now, outage: now.saturating_sub(lost) });
                }
                let identity = c.identity.get_or_insert_with(|| {
                    let sender = p.from.unwrap_or(pages::Sender { number: c.number, kind: c.kind });
                    pages::identity_of(sender)
                });
                // Known, and something new: said (a firmware heard later too).
                if pages::absorb(identity, c.kind, &p.page) {
                    let maker = identity.manufacturer.clone().unwrap_or_else(|| "ANT+".into());
                    identity.name = identity.ant_device_number.map(|n| format!("{maker} {n}"));
                    c.events.push(LinkEvent::Identified { at: now, identity: identity.clone() });
                }
                if let Some(r) = c.decoder.reading(c.kind, now, &p.page) {
                    c.events.push(LinkEvent::Reading(r));
                }
                Vec::new()
            }
            MSG_CHANNEL_EVENT => {
                let (Some(&ch), Some(&what), Some(&code)) = (m.data.first(), m.data.get(1), m.data.get(2)) else { return Vec::new() };
                let Some(c) = self.channels.get_mut(usize::from(ch)) else { return Vec::new() };
                match (what, code) {
                    (CHANNEL_EVENT, EVENT_TRANSFER_TX_COMPLETED) => c.answer(now, ControlOutcome::Acknowledged),
                    (CHANNEL_EVENT, EVENT_TRANSFER_TX_FAILED) => c.answer(now, ControlOutcome::NotAcknowledged),
                    (CHANNEL_EVENT, EVENT_RX_FAIL_GO_TO_SEARCH) => c.lose(now),
                    // Closed by itself (it is never asked to): open it again,
                    // and the device is lost meanwhile.
                    (CHANNEL_EVENT, EVENT_CHANNEL_CLOSED) => {
                        c.lose(now);
                        return vec![Msg::new(MSG_OPEN, &[ch])];
                    }
                    // The stick turned the page down: it never went out.
                    (MSG_ACKNOWLEDGED, code) if code != 0 => c.answer(now, ControlOutcome::Refused),
                    _ => {}
                }
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// Sends `control` on channel `ch` as an acknowledged page; with one in
    /// flight it waits behind it (a newer one takes its place, the older
    /// one never goes out). Refused at once: not heard yet, lost, the stick
    /// not up.
    pub fn send(&mut self, now: Duration, ch: usize, id: ControlId, control: Control) -> Vec<Msg> {
        let running = matches!(self.phase, Phase::Running);
        let Some(c) = self.channels.get_mut(ch) else { return Vec::new() };
        if !running || !c.heard || c.lost_at.is_some() {
            c.events.push(LinkEvent::Control { id, at: now, control, outcome: ControlOutcome::Refused });
            return Vec::new();
        }
        if c.in_flight.is_some() {
            if let Some((old, was)) = c.queued.replace((id, control)) {
                c.events.push(LinkEvent::Control { id: old, at: now, control: was, outcome: ControlOutcome::Refused });
            }
            return Vec::new();
        }
        vec![c.go(now, ch, id, control)]
    }

    /// What closes every channel at the end: a reset.
    pub fn close() -> Msg {
        Msg::new(MSG_RESET, &[0])
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::trainer::link::{Reading, Transport};

    pub(crate) const KEY: [u8; 8] = [1, 2, 3, 4, 5, 6, 7, 8];

    fn ms(m: u64) -> Duration {
        Duration::from_millis(m)
    }

    pub(crate) fn ok(m: &Msg) -> Msg {
        Msg::new(MSG_CHANNEL_EVENT, &[m.data.first().copied().unwrap_or(0), m.id, 0])
    }

    /// Answers every setup message until the machine runs; returns what it
    /// sent.
    fn set_up(m: &mut Machine, first: Msg) -> Vec<Msg> {
        let mut sent = vec![first];
        let mut out = m.handle(ms(10), Input::Message(Msg::new(MSG_STARTUP, &[0x20])));
        while let Some(next) = out.pop() {
            sent.push(next.clone());
            out = m.handle(ms(20), Input::Message(ok(&next)));
        }
        assert!(matches!(m.phase, Phase::Running));
        sent
    }

    fn trainer_and_strap() -> (Machine, Msg) {
        Machine::new(ms(0), &KEY, &[Wanted { role: SensorRole::Trainer, number: 0 }, Wanted { role: SensorRole::Strap, number: 7 }])
    }

    /// A data message on `ch` from device `number` of `kind`.
    pub(crate) fn data(ch: u8, number: u16, kind: u8, page: [u8; 8]) -> Msg {
        let [lo, hi] = number.to_le_bytes();
        Msg::new(MSG_BROADCAST, &[&[ch][..], &page, &[0x80, lo, hi, kind, 0x05]].concat())
    }

    pub(crate) fn event(ch: u8, code: u8) -> Msg {
        Msg::new(MSG_CHANNEL_EVENT, &[ch, CHANNEL_EVENT, code])
    }

    const FEC: [u8; 8] = [0x19, 1, 85, 0, 0, 150, 0x30, 0x30];

    #[test]
    fn the_stick_is_reset_keyed_and_given_a_low_priority_channel_each() {
        let (mut m, first) = trainer_and_strap();
        let sent = set_up(&mut m, first);
        let ids: Vec<u8> = sent.iter().map(|s| s.id).collect();
        let channel = [MSG_ASSIGN, MSG_CHANNEL_ID, MSG_PERIOD, MSG_SEARCH_TIMEOUT, MSG_LOW_PRIORITY_SEARCH, MSG_RF_FREQ, MSG_OPEN];
        assert_eq!(ids, [&[MSG_RESET, MSG_SET_KEY, MSG_LIB_CONFIG][..], &channel, &channel].concat());
        assert_eq!(sent[1].data, vec![0, 1, 2, 3, 4, 5, 6, 7, 8], "the key on network 0");
        assert_eq!(sent[2].data, vec![0, LIB_CHANNEL_ID]);
        assert_eq!(sent[4].data, vec![0, 0, 0, KIND_FEC, 0], "any trainer");
        assert_eq!(sent[11].data, vec![1, 7, 0, KIND_HEART, 0], "strap #7 on channel 1");
        assert_eq!(sent[5].data, vec![0, 0x00, 0x20], "period 8192");
        assert_eq!(sent[6].data, vec![0, 0], "no high-priority search");
        assert_eq!(sent[7].data, vec![0, 0xFF], "low priority, for ever");
        assert_eq!(sent[8].data, vec![0, ANT_PLUS_FREQ]);
    }

    #[test]
    fn a_stick_that_says_nothing_after_a_reset_is_set_up_anyway_but_one_that_refuses_fails() {
        let (mut m, _) = trainer_and_strap();
        assert!(m.handle(ms(500), Input::Tick).is_empty(), "still waiting for the start");
        assert!(m.handle(ms(600), Input::Message(Msg::new(MSG_BROADCAST, &[0; 9]))).is_empty(), "not the start");
        let key = m.handle(ms(1000), Input::Tick);
        assert_eq!(key[0].id, MSG_SET_KEY);
        assert!(m.handle(ms(1500), Input::Message(Msg::new(MSG_CHANNEL_EVENT, &[0, MSG_ASSIGN, 0]))).is_empty(), "another answer");
        let refused = Msg::new(MSG_CHANNEL_EVENT, &[0, MSG_SET_KEY, 0x28]);
        assert!(m.handle(ms(1600), Input::Message(refused)).is_empty());
        for ch in 0..2 {
            assert!(matches!(&m.take_events(ch)[..], [LinkEvent::Failed { reason: LinkFailure::Other(why), .. }] if why.contains("0x46")));
        }
        assert!(m.handle(ms(1700), Input::Tick).is_empty(), "failed: nothing more");
        assert!(m.handle(ms(1800), Input::Gone).is_empty());
        assert!(m.take_events(0).is_empty(), "failed once");

        let (mut quiet, _) = trainer_and_strap();
        quiet.handle(ms(10), Input::Message(Msg::new(MSG_STARTUP, &[0])));
        quiet.handle(ms(2009), Input::Tick);
        assert!(quiet.take_events(1).is_empty(), "within the answer's wait");
        quiet.handle(ms(2010), Input::Tick);
        assert!(matches!(&quiet.take_events(1)[..], [LinkEvent::Failed { reason: LinkFailure::Other(why), .. }] if why.contains("did not answer")));
    }

    #[test]
    fn a_device_heard_is_connected_read_and_said_who_it_is() {
        let (mut m, first) = trainer_and_strap();
        set_up(&mut m, first);
        m.handle(ms(100), Input::Message(data(0, 14830, KIND_FEC, FEC)));
        m.handle(ms(350), Input::Message(data(0, 14830, KIND_FEC, [0x50, 0xFF, 0xFF, 1, 86, 0, 45, 0])));
        let events = m.take_events(0);
        assert_eq!(events[0], LinkEvent::Connected { at: ms(100) });
        assert!(matches!(&events[1], LinkEvent::Reading(Reading { watts: Some(150), cadence_rpm: Some(85), .. })));
        let LinkEvent::Identified { identity, .. } = &events[2] else { panic!("{events:?}") };
        assert_eq!(identity.transport, Transport::Ant);
        assert_eq!((identity.ant_device_number, identity.ant_manufacturer, identity.ant_model), (Some(14830), Some(86), Some(45)));
        assert_eq!(identity.name.as_deref(), Some("Elite 14830"));
        assert_eq!(events.len(), 3, "an identity page is no reading");
        // Its firmware, heard later: said again. Pages may come acknowledged
        // or in a burst too.
        m.handle(ms(400), Input::Message(Msg::new(MSG_ACKNOWLEDGED, &data(0, 14830, KIND_FEC, [0x51, 0xFF, 0xFF, 37, 1, 2, 3, 4]).data)));
        m.handle(ms(450), Input::Message(Msg::new(MSG_BURST, &data(0, 14830, KIND_FEC, FEC).data)));
        let later = m.take_events(0);
        assert!(matches!(&later[0], LinkEvent::Identified { identity, .. } if identity.firmware.as_deref() == Some("3.7")), "{later:?}");
        assert!(matches!(&later[1], LinkEvent::Reading(_)));
        // Without the extended part: the number asked for; a maker not named.
        m.handle(ms(400), Input::Message(Msg::new(MSG_BROADCAST, &[1, 0x83, 2, 9, 0x7A, 0, 0, 0, 120])));
        m.handle(ms(500), Input::Message(Msg::new(MSG_BROADCAST, &[1, 0x82, 99, 0, 0, 0, 0, 0, 121])));
        let strap = m.take_events(1);
        let LinkEvent::Identified { identity, .. } = &strap[2] else { panic!("{strap:?}") };
        assert_eq!((identity.ant_device_number, identity.name.as_deref()), (Some(7), Some("ANT+ 7")));
        // Messages for no channel, or that are no page, change nothing.
        m.handle(ms(600), Input::Message(data(5, 1, KIND_FEC, FEC)));
        m.handle(ms(600), Input::Message(Msg::new(MSG_BROADCAST, &[0, 1, 2])));
        m.handle(ms(600), Input::Message(event(9, EVENT_RX_FAIL)));
        m.handle(ms(600), Input::Message(Msg::new(MSG_CHANNEL_EVENT, &[0])));
        m.handle(ms(600), Input::Message(Msg::new(0x99, &[0])));
        assert!(m.take_events(0).is_empty() && m.take_events(9).is_empty());
    }

    #[test]
    fn a_drop_is_lost_then_back_by_itself() {
        let (mut m, first) = trainer_and_strap();
        set_up(&mut m, first);
        m.handle(ms(100), Input::Message(event(0, EVENT_RX_FAIL_GO_TO_SEARCH)));
        assert!(m.take_events(0).is_empty(), "never heard: not lost");
        m.handle(ms(200), Input::Message(data(0, 1, KIND_FEC, FEC)));
        m.handle(ms(300), Input::Message(event(0, EVENT_RX_FAIL)));
        m.handle(ms(1000), Input::Message(event(0, EVENT_RX_FAIL_GO_TO_SEARCH)));
        m.handle(ms(1100), Input::Message(event(0, EVENT_RX_FAIL_GO_TO_SEARCH)));
        m.take_events(0);
        let id = ControlId(1);
        m.send(ms(1200), 0, id, Control::TargetPower(150));
        assert_eq!(m.take_events(0), vec![LinkEvent::Control { id, at: ms(1200), control: Control::TargetPower(150), outcome: ControlOutcome::Refused }], "lost: never went out");
        // Back within 20 s, its counters restarted in between (2 events,
        // 400 more: a plausible 200 W): not compared with those before the
        // drop — the page's own 150 W.
        let after = [0x19, 3, 85, 0x90, 0x01, 150, 0x30, 0x30];
        m.handle(ms(9000), Input::Message(data(0, 1, KIND_FEC, after)));
        let back = m.take_events(0);
        assert_eq!(back[0], LinkEvent::Back { at: ms(9000), outage: ms(8000) });
        assert!(matches!(&back[1], LinkEvent::Reading(Reading { watts: Some(150), .. })), "the page's own watts: {back:?}");
        // A channel that closes by itself is opened again; the device is
        // lost meanwhile.
        assert_eq!(m.handle(ms(9500), Input::Message(event(0, EVENT_CHANNEL_CLOSED))), vec![Msg::new(MSG_OPEN, &[0])]);
        assert_eq!(m.take_events(0), vec![LinkEvent::Lost { at: ms(9500) }]);
    }

    #[test]
    fn one_acknowledged_page_at_a_time_and_each_gets_its_outcome() {
        let (mut m, first) = trainer_and_strap();
        let control = |m: &mut Machine, now, id| m.send(ms(now), 0, ControlId(id), Control::TargetPower(150));
        assert!(control(&mut m, 0, 1).is_empty(), "the stick is not up");
        set_up(&mut m, first);
        assert!(control(&mut m, 50, 2).is_empty(), "not heard yet");
        m.handle(ms(100), Input::Message(data(0, 1, KIND_FEC, FEC)));
        let page = Msg::new(MSG_ACKNOWLEDGED, &[0, 0x31, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x58, 0x02]);
        assert_eq!(control(&mut m, 200, 3), vec![page.clone()]);
        // One in flight: the next waits behind it, a newer one in its place.
        assert!(control(&mut m, 210, 4).is_empty());
        assert!(control(&mut m, 220, 5).is_empty());
        let next = m.handle(ms(400), Input::Message(event(0, EVENT_TRANSFER_TX_COMPLETED)));
        assert_eq!(next, vec![page], "the waiting one goes once the line is free");
        m.handle(ms(700), Input::Message(event(0, EVENT_TRANSFER_TX_FAILED)));
        control(&mut m, 800, 6);
        // TRANSFER_IN_PROGRESS: the stick did not take it.
        m.handle(ms(810), Input::Message(Msg::new(MSG_CHANNEL_EVENT, &[0, MSG_ACKNOWLEDGED, 0x1F])));
        control(&mut m, 900, 7);
        // The stick's answer to the message itself, fine: not the outcome.
        m.handle(ms(910), Input::Message(Msg::new(MSG_CHANNEL_EVENT, &[0, MSG_ACKNOWLEDGED, 0])));
        m.handle(ms(900 + 2999), Input::Tick);
        m.handle(ms(900 + 3000), Input::Tick);
        control(&mut m, 4000, 8);
        control(&mut m, 4010, 9);
        m.handle(ms(4100), Input::Message(event(0, EVENT_RX_FAIL_GO_TO_SEARCH)));
        let outcomes: Vec<(u64, ControlOutcome)> = m
            .take_events(0)
            .into_iter()
            .filter_map(|e| match e {
                LinkEvent::Control { id, outcome, .. } => Some((id.0, outcome)),
                _ => None,
            })
            .collect();
        use ControlOutcome::*;
        assert_eq!(
            outcomes,
            vec![(1, Refused), (2, Refused), (4, Refused), (3, Acknowledged), (5, NotAcknowledged), (6, Refused), (7, TimedOut), (8, NotAcknowledged), (9, Refused)],
            "lost: the one in flight went out unconfirmed, the one waiting never went"
        );
        // A profile with a longer answer: waited for.
        m.use_profile(0, &crate::trainer::profile::ELITE_AVANTI);
        m.handle(ms(5000), Input::Message(data(0, 1, KIND_FEC, FEC)));
        control(&mut m, 5100, 10);
        m.handle(ms(5100 + 4000), Input::Tick);
        m.handle(ms(5100 + 5000), Input::Tick);
        let late: Vec<LinkEvent> = m.take_events(0).into_iter().filter(|e| matches!(e, LinkEvent::Control { .. })).collect();
        assert_eq!(late, vec![LinkEvent::Control { id: ControlId(10), at: ms(10100), control: Control::TargetPower(150), outcome: TimedOut }]);
        m.use_profile(7, &GENERIC);
        assert!(m.send(ms(1), 7, ControlId(0), Control::Release).is_empty(), "no such channel");
    }

    /// A control waiting behind one in flight goes only once that one has
    /// its answer — a page heard meanwhile sends nothing — or timed out.
    #[test]
    fn the_one_waiting_goes_only_when_the_line_is_free() {
        let (mut m, first) = trainer_and_strap();
        set_up(&mut m, first);
        m.handle(ms(100), Input::Message(data(0, 1, KIND_FEC, FEC)));
        let target = |w| Msg::new(MSG_ACKNOWLEDGED, &[&[0u8][..], &pages::control_page(Control::TargetPower(w))].concat());
        assert_eq!(m.send(ms(200), 0, ControlId(1), Control::TargetPower(150)), vec![target(150)]);
        assert!(m.send(ms(210), 0, ControlId(2), Control::TargetPower(160)).is_empty());
        assert!(m.handle(ms(300), Input::Message(data(0, 1, KIND_FEC, FEC))).is_empty(), "still in flight: nothing goes");
        assert!(m.handle(ms(310), Input::Tick).is_empty());
        assert_eq!(m.handle(ms(400), Input::Message(event(0, EVENT_TRANSFER_TX_COMPLETED))), vec![target(160)]);
        // Timed out: the one waiting goes on the tick that says so.
        assert!(m.send(ms(500), 0, ControlId(3), Control::TargetPower(170)).is_empty(), "behind 2");
        assert_eq!(m.handle(ms(400 + 3000), Input::Tick), vec![target(170)]);
        let outcomes: Vec<(u64, ControlOutcome)> = m
            .take_events(0)
            .into_iter()
            .filter_map(|e| match e {
                LinkEvent::Control { id, outcome, .. } => Some((id.0, outcome)),
                _ => None,
            })
            .collect();
        assert_eq!(outcomes, vec![(1, ControlOutcome::Acknowledged), (2, ControlOutcome::TimedOut)], "one each, 3 in flight");
    }

    #[test]
    fn a_stick_pulled_fails_every_channel_after_settling_what_was_in_flight() {
        let (mut m, first) = trainer_and_strap();
        set_up(&mut m, first);
        m.handle(ms(100), Input::Message(data(0, 1, KIND_FEC, FEC)));
        m.send(ms(200), 0, ControlId(1), Control::Release);
        m.take_events(0);
        m.handle(ms(300), Input::Gone);
        assert_eq!(
            m.take_events(0),
            vec![
                LinkEvent::Control { id: ControlId(1), at: ms(300), control: Control::Release, outcome: ControlOutcome::NotAcknowledged },
                LinkEvent::Failed { at: ms(300), reason: LinkFailure::StickGone },
            ]
        );
        assert_eq!(m.take_events(1), vec![LinkEvent::Failed { at: ms(300), reason: LinkFailure::StickGone }]);
        // Failed is the end: a channel heard before refuses what comes.
        m.send(ms(400), 0, ControlId(2), Control::Release);
        assert_eq!(m.take_events(0), vec![LinkEvent::Control { id: ControlId(2), at: ms(400), control: Control::Release, outcome: ControlOutcome::Refused }]);
        assert_eq!(Machine::close(), Msg::new(MSG_RESET, &[0]));
        assert_eq!(kind_of(SensorRole::Pedals), KIND_POWER);
    }
}
