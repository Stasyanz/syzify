//! The session's side of a link run by a background task: controls go out
//! as [`Command`]s through one channel, events come back through another.
//! Free of any radio, so the part of the contract it keeps (order, one
//! outcome per control, a `Failed` when the task dies) is tested in CI; the
//! BLE driver uses it on macOS.

use std::time::Duration;

use tokio::sync::mpsc::{self, error::TryRecvError};

use super::machine::Machine;
use crate::trainer::link::{Control, ControlId, ControlOutcome, Identity, LinkEvent, LinkFailure};
use crate::trainer::profile::TrainerProfile;

/// What the session asks of the task.
pub enum Command {
    Send(ControlId, Control),
    UseProfile(&'static TrainerProfile),
}

/// Collects what the task reports, for `poll`.
pub struct Inbox {
    events: mpsc::UnboundedReceiver<LinkEvent>,
    identity: Option<Identity>,
    /// Outcomes of controls that never reached the task. They are reported
    /// after everything the task said: it had ended before they were sent.
    late: Vec<LinkEvent>,
    failed: bool,
}

impl Inbox {
    pub fn new(events: mpsc::UnboundedReceiver<LinkEvent>) -> Inbox {
        Inbox { events, identity: None, late: Vec::new(), failed: false }
    }

    pub fn identity(&self) -> Option<&Identity> {
        self.identity.as_ref()
    }

    /// A control the task could not take any more.
    pub fn refused(&mut self, id: ControlId, control: Control, at: Duration) {
        self.late.push(LinkEvent::Control { id, at, control, outcome: ControlOutcome::Refused });
    }

    /// Everything the task reported, then the late refusals. A task that
    /// ended without a `Failed` (a panic in the radio library) gets one, so
    /// the session never waits on a dead link.
    pub fn poll(&mut self, now: Duration) -> Vec<LinkEvent> {
        let mut out = Vec::new();
        loop {
            match self.events.try_recv() {
                Ok(event) => {
                    match &event {
                        LinkEvent::Identified { identity, .. } => self.identity = Some(identity.clone()),
                        LinkEvent::Failed { .. } => self.failed = true,
                        _ => {}
                    }
                    out.push(event);
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    if !self.failed {
                        self.failed = true;
                        // Before the late refusals: the task died first.
                        let at = self.late.iter().filter_map(event_at).min().unwrap_or(now).min(now);
                        out.push(LinkEvent::Failed { at, reason: LinkFailure::Other("the link's task ended".into()) });
                    }
                    break;
                }
            }
        }
        out.append(&mut self.late);
        out
    }
}

fn event_at(e: &LinkEvent) -> Option<Duration> {
    match e {
        LinkEvent::Control { at, .. } => Some(*at),
        _ => None,
    }
}

/// Hands the machine's events to the session; false once the link is over
/// (it failed, or nobody listens any more).
pub fn forward(machine: &mut Machine, events: &mpsc::UnboundedSender<LinkEvent>) -> bool {
    let mut alive = true;
    for event in machine.take_events() {
        alive &= !matches!(event, LinkEvent::Failed { .. });
        alive &= events.send(event).is_ok();
    }
    alive
}

/// The task is ending: no more commands, and every control still waiting
/// in the channel is refused (it never went out).
pub fn refuse_remaining(
    commands: &mut mpsc::UnboundedReceiver<Command>,
    events: &mpsc::UnboundedSender<LinkEvent>,
    at: Duration,
) {
    commands.close();
    while let Ok(command) = commands.try_recv() {
        if let Command::Send(id, control) = command {
            let _ = events.send(LinkEvent::Control { id, at, control, outcome: ControlOutcome::Refused });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trainer::ble::machine::{Input, RadioState, Role, Target};
    use crate::trainer::link::Transport;

    fn s(secs: u64) -> Duration {
        Duration::from_secs(secs)
    }

    #[test]
    fn poll_keeps_the_order_and_learns_the_identity() {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut inbox = Inbox::new(rx);
        let identity = Identity { name: Some("AVANTI COG".into()), ..Identity::new(Transport::Ble) };
        tx.send(LinkEvent::Connected { at: s(1) }).unwrap();
        tx.send(LinkEvent::Identified { at: s(1), identity: identity.clone() }).unwrap();
        tx.send(LinkEvent::Lost { at: s(2) }).unwrap();
        assert!(inbox.identity().is_none());
        assert_eq!(
            inbox.poll(s(3)),
            vec![LinkEvent::Connected { at: s(1) }, LinkEvent::Identified { at: s(1), identity: identity.clone() }, LinkEvent::Lost { at: s(2) }]
        );
        assert_eq!(inbox.identity(), Some(&identity));
        assert!(inbox.poll(s(4)).is_empty(), "nothing new, nothing ended");
    }

    #[test]
    fn a_task_that_failed_properly_gets_no_second_failure() {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut inbox = Inbox::new(rx);
        tx.send(LinkEvent::Failed { at: s(5), reason: LinkFailure::RadioOff }).unwrap();
        drop(tx);
        inbox.refused(ControlId(1), Control::Release, s(6));
        assert_eq!(
            inbox.poll(s(7)),
            vec![
                LinkEvent::Failed { at: s(5), reason: LinkFailure::RadioOff },
                LinkEvent::Control { id: ControlId(1), at: s(6), control: Control::Release, outcome: ControlOutcome::Refused },
            ]
        );
        assert!(inbox.poll(s(8)).is_empty());
    }

    #[test]
    fn a_task_that_died_silently_is_reported_failed_before_the_late_refusals() {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut inbox = Inbox::new(rx);
        tx.send(LinkEvent::Connected { at: s(1) }).unwrap();
        drop(tx); // a panic in the task
        inbox.refused(ControlId(1), Control::TargetPower(150), s(4));
        let events = inbox.poll(s(9));
        assert_eq!(events[0], LinkEvent::Connected { at: s(1) });
        assert!(matches!(&events[1], LinkEvent::Failed { at, reason: LinkFailure::Other(_) } if *at == s(4)), "{events:?}");
        assert!(matches!(events[2], LinkEvent::Control { outcome: ControlOutcome::Refused, .. }));
        assert!(inbox.poll(s(10)).is_empty(), "failed once");

        let (tx, rx) = mpsc::unbounded_channel::<LinkEvent>();
        let mut quiet = Inbox::new(rx);
        drop(tx);
        assert!(matches!(quiet.poll(s(3)).as_slice(), [LinkEvent::Failed { at, .. }] if *at == s(3)), "now, with nothing late");
    }

    #[test]
    fn forward_says_when_the_link_is_over() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut m = Machine::new(Target::any(Role::Trainer), s(0));
        m.handle(s(1), Input::Radio(RadioState::On));
        assert!(forward(&mut m, &tx), "nothing to say, still alive");
        m.send(s(2), ControlId(1), Control::Release); // refused: not connected
        assert!(forward(&mut m, &tx));
        assert!(matches!(rx.try_recv(), Ok(LinkEvent::Control { outcome: ControlOutcome::Refused, .. })));
        m.handle(s(3), Input::Radio(RadioState::Off));
        assert!(!forward(&mut m, &tx), "failed");
        assert!(matches!(rx.try_recv(), Ok(LinkEvent::Failed { .. })));

        let mut m = Machine::new(Target::any(Role::Trainer), s(0));
        m.send(s(1), ControlId(2), Control::Release);
        drop(rx);
        assert!(!forward(&mut m, &tx), "nobody listens");
    }

    #[test]
    fn controls_left_in_the_channel_are_refused_when_the_task_ends() {
        let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel();
        let (tx, mut rx) = mpsc::unbounded_channel();
        cmd_tx.send(Command::UseProfile(&crate::trainer::profile::GENERIC)).ok();
        cmd_tx.send(Command::Send(ControlId(7), Control::TargetPower(150))).ok();
        refuse_remaining(&mut cmd_rx, &tx, s(9));
        assert_eq!(
            rx.try_recv().ok(),
            Some(LinkEvent::Control { id: ControlId(7), at: s(9), control: Control::TargetPower(150), outcome: ControlOutcome::Refused })
        );
        assert!(rx.try_recv().is_err());
        assert!(cmd_tx.send(Command::Send(ControlId(8), Control::Release)).is_err(), "closed: the sender learns it at once");
    }
}
