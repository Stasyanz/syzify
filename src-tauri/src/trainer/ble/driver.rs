//! Runs a [`Machine`] against btleplug and serves it as a
//! [`TrainerLink`] (macOS only, like the feature).
//!
//! Thin on purpose: it turns radio events into [`Input`]s and carries out
//! [`Action`]s, and decides nothing; what the session sees goes through
//! [`Inbox`]. It never wraps `connect()` in a timeout and takes a fresh
//! peripheral handle for every connect (see the traps in
//! [`super::machine`]); everything else that may hang — a subscription, a
//! disconnect — is bounded, and a write runs in a task of its own, since
//! one with response may never complete (btleplug 0.11.8 drops ATT errors
//! on writes and leaves the future pending).
//!
//! All links share one adapter. So **one device, one link**: two links
//! that could pick the same device (two `Target::any(Trainer)`) would
//! connect it twice in the same CoreBluetooth manager — trap 2 — and one's
//! disconnect would cut the other. The session engine names each device
//! once.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use btleplug::api::{Central, CentralEvent, CentralState, Manager as _, Peripheral as _, ScanFilter, WriteType};
use btleplug::platform::{Adapter, Manager, Peripheral, PeripheralId};
use futures::StreamExt;
use tokio::sync::{mpsc, Mutex, OnceCell};
use uuid::Uuid;

use super::discover::{Discovery, Found, SERVICES};
use super::gatt;
use super::inbox::{forward, refuse_remaining, Command, Inbox};
use super::machine::{Action, Advert, Conn, Input, Machine, RadioState, Target};
use super::scan::{ScanOrder, Scans};
use crate::trainer::link::{Control, ControlId, Identity, LinkEvent, LinkFailure, SensorLink, TrainerLink};
use crate::trainer::profile::TrainerProfile;

/// How often the machine is told that time passed (timeouts, silence).
const TICK: Duration = Duration::from_millis(250);
/// A subscription or a disconnect that takes longer has failed. (Never a
/// connect: see trap 2.)
const BOUNDED: Duration = Duration::from_secs(5);

/// A trainer, pedals or a strap over BLE. Dropping it ends the link.
pub struct BleLink {
    inbox: Inbox,
    commands: mpsc::UnboundedSender<Command>,
    next_id: u64,
    epoch: Instant,
}

impl BleLink {
    /// Starts linking `target` on the app's async runtime; `epoch` is the
    /// session's start.
    pub fn start(target: Target, epoch: Instant) -> BleLink {
        let (events_tx, events) = mpsc::unbounded_channel();
        let (commands, commands_rx) = mpsc::unbounded_channel();
        tauri::async_runtime::spawn(run(target, epoch, commands_rx, events_tx));
        BleLink { inbox: Inbox::new(events), commands, next_id: 0, epoch }
    }
}

impl SensorLink for BleLink {
    fn identity(&self) -> Option<&Identity> {
        self.inbox.identity()
    }

    /// Events are stamped when they arrive; `now` stamps only a failure
    /// the task could not report itself.
    fn poll(&mut self, now: Duration) -> Vec<LinkEvent> {
        self.inbox.poll(now)
    }
}

impl TrainerLink for BleLink {
    fn use_profile(&mut self, profile: &'static TrainerProfile) {
        let _ = self.commands.send(Command::UseProfile(profile));
    }

    fn send(&mut self, control: Control) -> ControlId {
        let id = ControlId(self.next_id);
        self.next_id += 1;
        if self.commands.send(Command::Send(id, control)).is_err() {
            self.inbox.refused(id, control, self.epoch.elapsed());
        }
        id
    }
}

fn short(uuid: Uuid) -> Option<u16> {
    gatt::short_uuid(uuid.as_u128())
}

fn radio_state(state: CentralState) -> RadioState {
    match state {
        CentralState::PoweredOn => RadioState::On,
        CentralState::PoweredOff => RadioState::Off,
        CentralState::Unknown => RadioState::Unknown,
    }
}

/// One adapter for the process: each one is a CoreBluetooth manager with a
/// thread of its own that never ends.
static ADAPTER: OnceCell<Adapter> = OnceCell::const_new();

/// The adapter's single scan, shared by every link. Held until the order
/// reached CoreBluetooth, so one link's stop cannot land after another's
/// start.
static SCANS: Mutex<Scans> = Mutex::const_new(Scans::new());

/// Numbers the links, for the scan coordinator.
static NEXT_LINK: AtomicU64 = AtomicU64::new(1);

async fn adapter() -> Result<Adapter, LinkFailure> {
    ADAPTER
        .get_or_try_init(|| async {
            let manager = Manager::new().await.map_err(failure)?;
            let adapters = manager.adapters().await.map_err(failure)?;
            adapters.into_iter().next().ok_or(LinkFailure::AdapterGone)
        })
        .await
        .cloned()
}

fn failure(e: btleplug::Error) -> LinkFailure {
    match e {
        btleplug::Error::PermissionDenied => LinkFailure::PermissionDenied,
        other => LinkFailure::Other(other.to_string()),
    }
}

async fn run(
    target: Target,
    epoch: Instant,
    mut commands: mpsc::UnboundedReceiver<Command>,
    events: mpsc::UnboundedSender<LinkEvent>,
) {
    let at = || epoch.elapsed();
    let mut machine = Machine::new(target, at());
    let central = match adapter().await {
        Ok(c) => c,
        Err(reason) => {
            let _ = events.send(LinkEvent::Failed { at: at(), reason });
            refuse_remaining(&mut commands, &events, at());
            return;
        }
    };
    let mut central_events = match central.events().await {
        Ok(s) => s,
        Err(e) => {
            let _ = events.send(LinkEvent::Failed { at: at(), reason: failure(e) });
            refuse_remaining(&mut commands, &events, at());
            return;
        }
    };
    let (inputs_tx, mut inputs) = mpsc::unbounded_channel();
    let link = NEXT_LINK.fetch_add(1, Ordering::Relaxed);
    let mut radio = Radio { central, link, ids: HashMap::new(), peripheral: None, conn: 0, inputs: inputs_tx };
    let state = radio.central.adapter_state().await.map(radio_state).unwrap_or(RadioState::Unknown);
    let first = machine.handle(at(), Input::Radio(state));
    radio.run(first).await;
    let mut tick = tokio::time::interval(TICK);
    loop {
        if !forward(&mut machine, &events) {
            break;
        }
        let actions = tokio::select! {
            Some(event) = central_events.next() => match radio.input_from(event).await {
                Some(input) => machine.handle(at(), input),
                None => Vec::new(),
            },
            Some(input) = inputs.recv() => machine.handle(at(), input),
            command = commands.recv() => match command {
                Some(Command::Send(id, control)) => machine.send(at(), id, control),
                Some(Command::UseProfile(profile)) => {
                    machine.use_profile(profile);
                    Vec::new()
                }
                None => break,
            },
            _ = tick.tick() => machine.handle(at(), Input::Tick),
        };
        radio.run(actions).await;
    }
    refuse_remaining(&mut commands, &events, at());
    let link = radio.link;
    radio.scan(|s| s.stop(link)).await;
    // Whatever state the machine ended in — a connection up after a
    // failure, a connect still pending — the device must be free for the
    // next session (cancelling a pending connect is not a second connect).
    radio.disconnect().await;
}

struct Radio {
    central: Adapter,
    /// This link's number for the shared scan.
    link: u64,
    /// Addresses seen in advertisements, for a fresh handle on connect.
    ids: HashMap<String, PeripheralId>,
    /// The handle of the current connection, and its number.
    peripheral: Option<Peripheral>,
    conn: Conn,
    inputs: mpsc::UnboundedSender<Input>,
}

/// The peripheral an advertising event is about.
fn advertiser(event: &CentralEvent) -> Option<&PeripheralId> {
    match event {
        CentralEvent::DeviceDiscovered(id)
        | CentralEvent::DeviceUpdated(id)
        | CentralEvent::ServicesAdvertisement { id, .. }
        | CentralEvent::ServiceDataAdvertisement { id, .. }
        | CentralEvent::ManufacturerDataAdvertisement { id, .. } => Some(id),
        _ => None,
    }
}

/// What a peripheral advertises, as the machine reads it.
async fn advert(central: &Adapter, id: &PeripheralId) -> Option<Advert> {
    let p = central.peripheral(id).await.ok()?;
    let props = p.properties().await.ok().flatten()?;
    let services = props.services.into_iter().filter_map(short).collect();
    Some(Advert { address: id.to_string(), name: props.local_name, services })
}

/// Changes what one link looks for and tells the radio, all under the
/// coordinator's lock.
async fn scan_for(central: &Adapter, change: impl FnOnce(&mut Scans) -> ScanOrder) {
    let mut scans = SCANS.lock().await;
    match change(&mut scans) {
        ScanOrder::Scan(services) => {
            let services = services.into_iter().map(|s| Uuid::from_u128(gatt::full_uuid(s))).collect();
            if let Err(e) = central.start_scan(ScanFilter { services }).await {
                eprintln!("[ble] scan failed: {e}");
            }
        }
        ScanOrder::Stop => {
            let _ = central.stop_scan().await;
        }
        ScanOrder::Keep => {}
    }
}

/// Scans for trainers, power meters and straps for `window` and lists what
/// advertised. Shares the scan with any running link. A device already
/// linked does not advertise and is not listed.
pub async fn find_devices(window: Duration) -> Result<Vec<Found>, LinkFailure> {
    let central = adapter().await?;
    let mut events = central.events().await.map_err(failure)?;
    // One scan coordinator entry per service: they share the union scan.
    let links: Vec<(u64, u16)> = SERVICES.iter().map(|&s| (NEXT_LINK.fetch_add(1, Ordering::Relaxed), s)).collect();
    for &(link, service) in &links {
        scan_for(&central, |s| s.start(link, service)).await;
    }
    let mut found = Discovery::default();
    let deadline = tokio::time::Instant::now() + window;
    while let Ok(Some(event)) = tokio::time::timeout_at(deadline, events.next()).await {
        if let Some(id) = advertiser(&event) {
            if let Some(ad) = advert(&central, id).await {
                found.saw(&ad);
            }
        }
    }
    for &(link, _) in &links {
        scan_for(&central, |s| s.stop(link)).await;
    }
    Ok(found.into_found())
}

impl Radio {
    async fn input_from(&mut self, event: CentralEvent) -> Option<Input> {
        if let Some(id) = advertiser(&event) {
            let ad = advert(&self.central, id).await?;
            self.ids.insert(ad.address.clone(), id.clone());
            return Some(Input::Advertised(ad));
        }
        match event {
            CentralEvent::StateUpdate(state) => Some(Input::Radio(radio_state(state))),
            CentralEvent::DeviceDisconnected(id) => Some(Input::Disconnected { address: id.to_string() }),
            _ => None,
        }
    }

    async fn run(&mut self, actions: Vec<Action>) {
        let mut on_peripheral = Vec::new();
        for action in actions {
            match action {
                Action::StartScan { service } => {
                    let link = self.link;
                    self.scan(|s| s.start(link, service)).await;
                }
                Action::StopScan => {
                    let link = self.link;
                    self.scan(|s| s.stop(link)).await;
                }
                Action::Connect { address, conn } => self.connect(address, conn).await,
                Action::Disconnect => self.disconnect().await,
                other => on_peripheral.push(other),
            }
        }
        if on_peripheral.is_empty() {
            return;
        }
        let Some(p) = self.peripheral.clone() else { return };
        let (conn, inputs) = (self.conn, self.inputs.clone());
        // In order, so the control point is subscribed before the first
        // write; a read or a write never holds up what follows, and a
        // subscription that does not answer is a failure, not a hang.
        tokio::spawn(async move {
            // After a failed subscription the machine has gone read-only or
            // is hanging up: what this batch would still write is no longer
            // its wish.
            let mut subscribed = true;
            for action in on_peripheral {
                match action {
                    Action::Discover => discover(&p, conn, &inputs).await,
                    Action::Subscribe { characteristic } => {
                        let done = match find(&p, characteristic) {
                            Some(c) => matches!(tokio::time::timeout(BOUNDED, p.subscribe(&c)).await, Ok(Ok(()))),
                            None => false,
                        };
                        // The machine decides what a failure means (the
                        // control point: read-only; the data: start over);
                        // the reads that follow still run.
                        if !done {
                            subscribed = false;
                            let _ = inputs.send(Input::SubscribeFailed { conn, characteristic });
                        }
                    }
                    Action::Read { characteristic } => {
                        if let Some(c) = find(&p, characteristic) {
                            let (p, inputs) = (p.clone(), inputs.clone());
                            tokio::spawn(async move {
                                if let Ok(Ok(value)) = tokio::time::timeout(BOUNDED, p.read(&c)).await {
                                    let _ = inputs.send(Input::Read { conn, characteristic, value });
                                }
                            });
                        }
                    }
                    Action::Write { .. } if !subscribed => {}
                    Action::Write { characteristic, bytes } => {
                        let (p, inputs) = (p.clone(), inputs.clone());
                        tokio::spawn(async move {
                            let result = match find(&p, characteristic) {
                                Some(c) => p.write(&c, &bytes, WriteType::WithResponse).await.map_err(|e| e.to_string()),
                                None => Err(format!("no characteristic {characteristic:#06x}")),
                            };
                            if let Err(error) = result {
                                let _ = inputs.send(Input::WriteFailed { conn, error });
                            }
                        });
                    }
                    Action::StartScan { .. } | Action::StopScan | Action::Connect { .. } | Action::Disconnect => {}
                }
            }
        });
    }

    /// A fresh handle every time, and no timeout around the connect.
    async fn connect(&mut self, address: String, conn: Conn) {
        self.conn = conn;
        let peripheral = match self.ids.get(&address) {
            Some(id) => self.central.peripheral(id).await.map_err(|e| e.to_string()),
            None => Err("never advertised".to_string()),
        };
        match peripheral {
            Ok(p) => {
                self.peripheral = Some(p.clone());
                let inputs = self.inputs.clone();
                tokio::spawn(async move {
                    let input = match p.connect().await {
                        Ok(()) => Input::Connected { conn },
                        Err(e) => Input::ConnectFailed { conn, error: e.to_string() },
                    };
                    let _ = inputs.send(input);
                });
            }
            Err(error) => {
                let _ = self.inputs.send(Input::ConnectFailed { conn, error });
            }
        }
    }

    /// Changes what this link looks for.
    async fn scan(&self, change: impl FnOnce(&mut Scans) -> ScanOrder) {
        scan_for(&self.central, change).await;
    }

    /// Bounded: on a handle btleplug already forgot, disconnect never
    /// returns.
    async fn disconnect(&mut self) {
        if let Some(p) = self.peripheral.take() {
            let _ = tokio::time::timeout(BOUNDED, p.disconnect()).await;
        }
    }
}

/// Discovers the services, then forwards the notifications of this
/// connection (a reconnect brings a new stream with the new handle).
async fn discover(p: &Peripheral, conn: Conn, inputs: &mpsc::UnboundedSender<Input>) {
    if let Err(e) = p.discover_services().await {
        let _ = inputs.send(Input::DiscoveryFailed { conn, error: e.to_string() });
        return;
    }
    match p.notifications().await {
        Ok(mut stream) => {
            let inputs = inputs.clone();
            tokio::spawn(async move {
                while let Some(n) = stream.next().await {
                    if let Some(characteristic) = short(n.uuid) {
                        if inputs.send(Input::Notified { conn, characteristic, value: n.value }).is_err() {
                            return;
                        }
                    }
                }
            });
        }
        Err(e) => {
            let _ = inputs.send(Input::DiscoveryFailed { conn, error: e.to_string() });
            return;
        }
    }
    let characteristics = p.characteristics().into_iter().filter_map(|c| short(c.uuid)).collect();
    let _ = inputs.send(Input::Discovered { conn, characteristics });
}

fn find(p: &Peripheral, characteristic: u16) -> Option<btleplug::api::Characteristic> {
    let uuid = Uuid::from_u128(gatt::full_uuid(characteristic));
    p.characteristics().into_iter().find(|c| c.uuid == uuid)
}
