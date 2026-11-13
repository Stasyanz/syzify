//! A FakeTrainer behind an ANT stick, for tests: the real [`Machine`] runs
//! against a stick that answers its setup, sends the trainer's readings as
//! FE-C pages and hands its acknowledged pages to the trainer. The whole
//! ANT+ link, without a radio.

use std::collections::HashSet;
use std::time::Duration;

use super::frame::*;
use super::machine::tests::{data, event, ok};
use super::machine::{Input, Machine, Wanted};
use super::pages::KIND_FEC;
use crate::models::training::SensorRole;
use crate::trainer::fake::FakeTrainer;
use crate::trainer::link::{Control, ControlId, ControlOutcome, Identity, LinkEvent, SensorLink, TrainerLink};
use crate::trainer::profile::TrainerProfile;

const NUMBER: u16 = 14830;

/// The stick's side: what it says back, and the trainer behind it.
struct Stick {
    trainer: FakeTrainer,
    /// What the stick has to say, in order.
    out: Vec<Msg>,
    /// The trainer's control ids of the pages it took.
    sent: HashSet<ControlId>,
    events: u8,
    accumulated: u16,
    pages: u32,
}

impl Stick {
    /// The stick takes a message from the machine.
    fn take(&mut self, m: Msg) {
        match m.id {
            MSG_RESET => self.out.push(Msg::new(MSG_STARTUP, &[0x20])),
            MSG_ACKNOWLEDGED => {
                let control = match (m.data[1], m.data[8]) {
                    (0x31, _) => Control::TargetPower(u16::from_le_bytes([m.data[7], m.data[8]]) / 4),
                    (_, 0) => Control::Release,
                    (_, half) => Control::Resistance(half / 2),
                };
                let id = self.trainer.send(control);
                self.sent.insert(id);
            }
            _ => self.out.push(ok(&m)),
        }
    }

    /// The trainer's events up to `now`, as the stick would tell them.
    fn hear(&mut self, now: Duration) {
        for e in self.trainer.poll(now) {
            match e {
                LinkEvent::Reading(r) => {
                    self.pages += 1;
                    // The trainer says who it is now and then, like FE-C.
                    if self.pages % 16 == 1 {
                        self.out.push(data(0, NUMBER, KIND_FEC, [0x50, 0xFF, 0xFF, 1, 86, 0, 45, 0]));
                    }
                    let watts = r.watts.unwrap_or(0);
                    self.events = self.events.wrapping_add(1);
                    self.accumulated = self.accumulated.wrapping_add(watts);
                    let [a0, a1] = self.accumulated.to_le_bytes();
                    let on_target = if r.reports_on_target == Some(false) { 1 } else { 0 };
                    let page = [0x19, self.events, r.cadence_rpm.unwrap_or(0xFF), a0, a1, (watts & 0xFF) as u8, (watts >> 8) as u8 | 0x30, 0x30 | on_target];
                    self.out.push(data(0, NUMBER, KIND_FEC, page));
                }
                LinkEvent::Lost { .. } => self.out.push(event(0, EVENT_RX_FAIL_GO_TO_SEARCH)),
                // The machine keeps its own time for an answer: a time-out is
                // nothing the stick says.
                LinkEvent::Control { id, outcome, .. } if self.sent.remove(&id) && outcome != ControlOutcome::TimedOut => {
                    let ok = outcome == ControlOutcome::Acknowledged;
                    self.out.push(event(0, if ok { EVENT_TRANSFER_TX_COMPLETED } else { EVENT_TRANSFER_TX_FAILED }));
                }
                _ => {}
            }
        }
    }
}

/// The trainer's link over ANT+: channel 0 of a machine on a fake stick.
pub struct AntSim {
    machine: Machine,
    stick: Stick,
    now: Duration,
    identity: Option<Identity>,
    next_id: u64,
}

impl AntSim {
    pub fn new(trainer: FakeTrainer) -> AntSim {
        let (machine, reset) = Machine::new(Duration::ZERO, &[0; 8], &[Wanted { role: SensorRole::Trainer, number: 0 }]);
        let mut stick = Stick { trainer, out: Vec::new(), sent: HashSet::new(), events: 0, accumulated: 0, pages: 0 };
        stick.take(reset);
        AntSim { machine, stick, now: Duration::ZERO, identity: None, next_id: 0 }
    }

    /// Everything the stick has to say goes to the machine, and what the
    /// machine sends back to the stick, until both are quiet.
    fn settle(&mut self, now: Duration) {
        while !self.stick.out.is_empty() {
            for m in std::mem::take(&mut self.stick.out) {
                for reply in self.machine.handle(now, Input::Message(m)) {
                    self.stick.take(reply);
                }
            }
        }
    }
}

impl SensorLink for AntSim {
    fn identity(&self) -> Option<&Identity> {
        self.identity.as_ref()
    }

    fn poll(&mut self, now: Duration) -> Vec<LinkEvent> {
        self.now = now;
        self.settle(now);
        self.stick.hear(now);
        self.settle(now);
        // A control waiting behind one that timed out goes now.
        let replies = self.machine.handle(now, Input::Tick);
        replies.into_iter().for_each(|m| self.stick.take(m));
        self.settle(now);
        let events = self.machine.take_events(0);
        for e in &events {
            if let LinkEvent::Identified { identity, .. } = e {
                self.identity = Some(identity.clone());
            }
        }
        events
    }
}

impl TrainerLink for AntSim {
    fn use_profile(&mut self, profile: &'static TrainerProfile) {
        self.machine.use_profile(0, profile);
        self.stick.trainer.use_profile(profile);
    }

    fn send(&mut self, control: Control) -> ControlId {
        let id = ControlId(self.next_id);
        self.next_id += 1;
        for m in self.machine.send(self.now, 0, id, control) {
            self.stick.take(m);
        }
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trainer::erg::{Erg, ErgHandle};
    use crate::trainer::fake::FakeConfig;
    use crate::trainer::fit::SubSport;
    use crate::trainer::session::Session;

    fn secs(s: u64) -> Duration {
        Duration::from_secs(s)
    }

    fn ride(cfg: FakeConfig, target: u16, until: u64) -> (Session, ErgHandle) {
        let link = AntSim::new(FakeTrainer::new(cfg));
        let mut session = Session::new(Duration::ZERO, 0, SubSport::IndoorCycling, Box::new(link), None, None, None);
        let erg = ErgHandle::default();
        erg.set_target(Some(target));
        session.add_controller(Box::new(Erg::new(erg.clone())));
        for q in 1..=until * 4 {
            session.tick(Duration::from_millis(q * 250));
        }
        (session, erg)
    }

    fn held(session: &Session, from: usize, to: usize, target: u16) -> bool {
        let watts: Vec<u16> = session.seconds()[from..to].iter().filter_map(|s| s.trainer_watts).collect();
        !watts.is_empty() && watts.iter().all(|w| w.abs_diff(target) <= 15)
    }

    /// The Avanti over ANT+: it lets go of a target after ~18 s, so ERG's
    /// resends hold it; the stick is set up, the trainer named and its
    /// profile taken.
    #[test]
    fn erg_holds_an_avanti_through_the_stick() {
        let (session, erg) = ride(FakeConfig::elite_avanti(85), 180, 90);
        assert!(held(&session, 15, 90, 180), "{:?}", session.seconds()[15..25].iter().map(|s| s.trainer_watts).collect::<Vec<_>>());
        assert_eq!(session.identified_profile().map(|p| p.key), Some("elite-avanti"));
        assert_eq!(erg.view().map(|v| v.state), Some(crate::models::training::ErgState::Holding));
    }

    /// A drop: the stick reports it, the trainer comes back by itself, and
    /// the target goes again.
    #[test]
    fn a_drop_through_the_stick_is_lost_then_back_and_held_again() {
        let (session, _) = ride(FakeConfig::elite_avanti(85).with_drop(secs(40), secs(10)), 180, 100);
        assert!(held(&session, 15, 40, 180));
        assert!(held(&session, 60, 100, 180), "held again after the drop");
        assert!(session.seconds()[42..48].iter().all(|s| s.trainer_watts.is_none()), "nothing heard in the drop");
    }

    #[test]
    fn controls_through_the_stick_get_their_outcomes() {
        let mut cfg = FakeConfig::steady(85);
        cfg.lose_every_nth_control = Some(2);
        let mut link = AntSim::new(FakeTrainer::new(cfg));
        let mut seen = Vec::new();
        // Polls 4 a second for `secs` more.
        let mut q = 0u64;
        let mut ride = |link: &mut AntSim, secs: u64| {
            let mut out = Vec::new();
            for _ in 0..secs * 4 {
                q += 1;
                out.extend(link.poll(Duration::from_millis(q * 250)));
            }
            out
        };
        seen.extend(ride(&mut link, 3));
        assert!(seen.iter().any(|e| matches!(e, LinkEvent::Connected { .. })));
        assert!(link.identity().is_some_and(|i| i.ant_manufacturer == Some(86)));
        for control in [Control::TargetPower(200), Control::Resistance(30), Control::Release] {
            link.send(control);
            seen.extend(ride(&mut link, 2));
        }
        let outcomes: Vec<(Control, ControlOutcome)> = seen
            .iter()
            .filter_map(|e| match e {
                LinkEvent::Control { control, outcome, .. } => Some((*control, *outcome)),
                _ => None,
            })
            .collect();
        use ControlOutcome::*;
        assert_eq!(
            outcomes,
            vec![(Control::TargetPower(200), Acknowledged), (Control::Resistance(30), NotAcknowledged), (Control::Release, Acknowledged)]
        );
    }
}
