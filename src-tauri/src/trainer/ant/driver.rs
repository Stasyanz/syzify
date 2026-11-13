//! Runs the ANT [`Machine`] against a USB stick (nusb, macOS only like the
//! feature) and serves each channel as a [`TrainerLink`].
//!
//! One stick for the whole session: the devices picked over ANT+ get a
//! channel each on it, and their links share one task. Thin on purpose: it
//! turns USB reads into [`Input`]s and writes what the machine says;
//! everything is decided there.

use std::time::{Duration, Instant};

use nusb::transfer::RequestBuffer;
use tokio::sync::mpsc;

use super::discover::Scan;
use super::frame::{Deframer, Msg};
use super::machine::{Input, Machine, Wanted};
use crate::models::training::{FoundDevice, StickProblem};
use crate::trainer::ble::inbox::{Command, Inbox};
use crate::trainer::link::{Control, ControlId, ControlOutcome, Identity, LinkEvent, LinkFailure, SensorLink, TrainerLink};
use crate::trainer::profile::TrainerProfile;

const VENDOR: u16 = 0x0FCF;
const PRODUCTS: [u16; 2] = [0x1008, 0x1009];
const EP_OUT: u8 = 0x01;
const EP_IN: u8 = 0x81;
/// How often the machine is told that time passed (setup, answers).
const TICK: Duration = Duration::from_millis(250);

/// The stick, if one is plugged in; claimed for this process.
fn open() -> Result<Option<nusb::Interface>, StickProblem> {
    let other = |e: std::io::Error| StickProblem::Other { detail: e.to_string() };
    let Some(info) = nusb::list_devices()
        .map_err(other)?
        .find(|d| d.vendor_id() == VENDOR && PRODUCTS.contains(&d.product_id()))
    else {
        return Ok(None);
    };
    let device = info.open().map_err(other)?;
    // Claimed by another process: an app that rides with it.
    let iface = device.claim_interface(0).map_err(|_| StickProblem::InUse)?;
    Ok(Some(iface))
}

/// A stick that takes longer than this to take a message is hung.
const WRITE_WAIT: Duration = Duration::from_secs(2);

/// Reads the stick until it fails: messages, then `Gone`.
fn read(iface: nusb::Interface, inputs: mpsc::UnboundedSender<Input>) -> tauri::async_runtime::JoinHandle<()> {
    tauri::async_runtime::spawn(async move {
        let mut deframer = Deframer::default();
        loop {
            match iface.bulk_in(EP_IN, RequestBuffer::new(64)).await.into_result() {
                Ok(bytes) => {
                    for m in deframer.push(&bytes) {
                        if inputs.send(Input::Message(m)).is_err() {
                            return;
                        }
                    }
                }
                Err(_) => {
                    let _ = inputs.send(Input::Gone);
                    return;
                }
            }
        }
    })
}

/// False when the stick did not take a message (pulled, or hung).
async fn write(iface: &nusb::Interface, messages: Vec<Msg>) -> bool {
    for m in messages {
        let taken = tokio::time::timeout(WRITE_WAIT, iface.bulk_out(EP_OUT, m.frame())).await;
        if !taken.is_ok_and(|done| done.into_result().is_ok()) {
            return false;
        }
    }
    true
}

/// A trainer, pedals or a strap on the session's ANT stick. Dropping every
/// link of the stick ends it.
pub struct AntLink {
    channel: usize,
    inbox: Inbox,
    commands: mpsc::UnboundedSender<(usize, Command)>,
    next_id: u64,
    epoch: Instant,
}

impl SensorLink for AntLink {
    fn identity(&self) -> Option<&Identity> {
        self.inbox.identity()
    }

    fn poll(&mut self, now: Duration) -> Vec<LinkEvent> {
        self.inbox.poll(now)
    }
}

impl TrainerLink for AntLink {
    fn use_profile(&mut self, profile: &'static TrainerProfile) {
        let _ = self.commands.send((self.channel, Command::UseProfile(profile)));
    }

    fn send(&mut self, control: Control) -> ControlId {
        let id = ControlId(self.next_id);
        self.next_id += 1;
        if self.commands.send((self.channel, Command::Send(id, control))).is_err() {
            self.inbox.refused(id, control, self.epoch.elapsed());
        }
        id
    }
}

/// Opens the stick on network `key` with a channel per device in `wanted`
/// (in order) on the app's async runtime; `epoch` is the session's start.
pub fn start(wanted: Vec<Wanted>, key: [u8; 8], epoch: Instant) -> Vec<AntLink> {
    let (commands, commands_rx) = mpsc::unbounded_channel();
    let mut events = Vec::new();
    let links = (0..wanted.len())
        .map(|channel| {
            let (tx, rx) = mpsc::unbounded_channel();
            events.push(tx);
            AntLink { channel, inbox: Inbox::new(rx), commands: commands.clone(), next_id: 0, epoch }
        })
        .collect();
    tauri::async_runtime::spawn(run(wanted, key, epoch, commands_rx, events));
    links
}

/// Every control left unanswered is refused: the stick is gone.
fn refuse_left(commands: &mut mpsc::UnboundedReceiver<(usize, Command)>, events: &[mpsc::UnboundedSender<LinkEvent>], at: Duration) {
    commands.close();
    while let Ok((ch, command)) = commands.try_recv() {
        if let (Command::Send(id, control), Some(tx)) = (command, events.get(ch)) {
            let _ = tx.send(LinkEvent::Control { id, at, control, outcome: ControlOutcome::Refused });
        }
    }
}

/// Opens the stick and rides; a stick that cannot be had fails every link.
async fn run(
    wanted: Vec<Wanted>,
    key: [u8; 8],
    epoch: Instant,
    mut commands: mpsc::UnboundedReceiver<(usize, Command)>,
    events: Vec<mpsc::UnboundedSender<LinkEvent>>,
) {
    let at = || epoch.elapsed();
    let reason = match open() {
        Ok(Some(iface)) => return ride(wanted, key, epoch, commands, events, iface).await,
        Ok(None) => LinkFailure::StickGone,
        Err(StickProblem::InUse) => LinkFailure::StickBusy,
        Err(StickProblem::Other { detail }) => LinkFailure::Other(detail),
    };
    for tx in &events {
        let _ = tx.send(LinkEvent::Failed { at: at(), reason: reason.clone() });
    }
    refuse_left(&mut commands, &events, at());
}

/// The session on the stick, until every link is dropped or it fails.
async fn ride(
    wanted: Vec<Wanted>,
    key: [u8; 8],
    epoch: Instant,
    mut commands: mpsc::UnboundedReceiver<(usize, Command)>,
    events: Vec<mpsc::UnboundedSender<LinkEvent>>,
    iface: nusb::Interface,
) {
    let at = || epoch.elapsed();
    let (mut machine, reset) = Machine::new(at(), &key, &wanted);
    let (inputs_tx, mut inputs) = mpsc::unbounded_channel();
    let reader = read(iface.clone(), inputs_tx.clone());
    let mut ok = write(&iface, vec![reset]).await;
    let mut tick = tokio::time::interval(TICK);
    while ok {
        let out = tokio::select! {
            Some(input) = inputs.recv() => machine.handle(at(), input),
            command = commands.recv() => match command {
                Some((ch, Command::Send(id, control))) => machine.send(at(), ch, id, control),
                Some((ch, Command::UseProfile(profile))) => {
                    machine.use_profile(ch, profile);
                    Vec::new()
                }
                // Every link dropped: the session is over.
                None => break,
            },
            _ = tick.tick() => machine.handle(at(), Input::Tick),
        };
        for (ch, tx) in events.iter().enumerate() {
            for e in machine.take_events(ch) {
                let _ = tx.send(e);
            }
        }
        ok = write(&iface, out).await;
        if !ok {
            machine.handle(at(), Input::Gone);
            for (ch, tx) in events.iter().enumerate() {
                for e in machine.take_events(ch) {
                    let _ = tx.send(e);
                }
            }
        }
    }
    refuse_left(&mut commands, &events, at());
    // Closes every channel; the stick is free for the next session.
    write(&iface, vec![Machine::close()]).await;
    reader.abort();
}

/// ANT+ trainers, power meters and straps heard for `window` on network
/// `key`. No stick, or no key, is no device, not a problem.
pub async fn find_devices(window: Duration, key: Option<[u8; 8]>) -> Result<Vec<FoundDevice>, StickProblem> {
    let Some(key) = key else { return Ok(Vec::new()) };
    let Some(iface) = open()? else { return Ok(Vec::new()) };
    let epoch = Instant::now();
    let at = || epoch.elapsed();
    let (mut scan, reset) = Scan::new(at(), &key);
    let (inputs_tx, mut inputs) = mpsc::unbounded_channel();
    let reader = read(iface.clone(), inputs_tx);
    let mut ok = write(&iface, vec![reset]).await;
    let mut tick = tokio::time::interval(TICK);
    let deadline = tokio::time::Instant::now() + window;
    while ok && scan.failed().is_none() {
        let out = tokio::select! {
            Some(input) = inputs.recv() => scan.handle(at(), input),
            _ = tick.tick() => scan.handle(at(), Input::Tick),
            _ = tokio::time::sleep_until(deadline) => break,
        };
        ok = write(&iface, out).await;
    }
    write(&iface, vec![Machine::close()]).await;
    reader.abort();
    match (scan.failed(), ok) {
        (Some(why), _) => Err(StickProblem::Other { detail: why.to_string() }),
        (None, false) => Err(StickProblem::Other { detail: "the ANT stick stopped answering".into() }),
        (None, true) => Ok(scan.found()),
    }
}
