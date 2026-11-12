//! The training screen's commands (#273): find devices, pair, ride,
//! finish — and the rides a crash left behind.
//!
//! The ride runs on a thread of its own (`trainer::runner`). A command
//! hands it an order; one that has an answer waits for it off the async
//! runtime. The thread holds `vault_flight` from the start of the ride to
//! its end, so encryption, restore and relocation cannot change the vault
//! under an open journal, and keeps the Mac awake. Live values reach the
//! screen as `training:live` events (null once the session is over).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager, Runtime, State};

use super::import::ensure_vault_unlocked;
use super::vault_busy_in;
use crate::error::CommandError;
use crate::models::training::{FinishKind, FoundDevice, PendingRide, TrainingFinish, TrainingLive, TrainingPhase};
use crate::state::{AppState, SingleFlightGuard};
use crate::trainer::awake::KeepAwake;
use crate::trainer::erg::MAX_TARGET;
use crate::trainer::fit::{Ride, SubSport};
use crate::trainer::journal::{self, Journal, KeyFn};
use crate::trainer::library::{self, ride_key};
use crate::trainer::link::LinkFailure;
use crate::trainer::runner::{self, Clock, Host, Order, Reply};
use crate::trainer::session::Session;

/// The live values, a few times a second; null when the session is over.
pub const LIVE_EVENT: &str = "training:live";

/// The session's side the commands see.
#[derive(Default)]
pub struct TrainingState {
    /// Orders to the ride's thread; None when no session runs.
    orders: Mutex<Option<Sender<Order>>>,
    /// What the screen was last shown, for a screen that comes back.
    live: Mutex<Option<TrainingLive>>,
    /// The running ride's journal: not offered as a left-over one.
    active: Mutex<Option<PathBuf>>,
    /// A quit is waiting for the ride to finish.
    quitting: AtomicBool,
}

#[cfg_attr(target_os = "macos", allow(dead_code))]
fn unsupported() -> CommandError {
    CommandError::coded("training.unsupported", "Indoor training needs macOS")
}

fn no_session() -> CommandError {
    CommandError::coded("training.none", "No trainer is connected")
}

fn busy() -> CommandError {
    CommandError::coded("training.busy", "A trainer is connected already")
}

/// Why Bluetooth could not be used, in words the user can act on.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn link_failure(failure: LinkFailure) -> CommandError {
    match failure {
        LinkFailure::PermissionDenied => CommandError::coded(
            "training.bluetoothDenied",
            "Syzify may not use Bluetooth: allow it in System Settings → Privacy & Security → Bluetooth",
        ),
        LinkFailure::RadioOff => CommandError::coded("training.bluetoothOff", "Bluetooth is off"),
        LinkFailure::AdapterGone => CommandError::coded("training.noBluetooth", "No Bluetooth adapter"),
        LinkFailure::Other(why) => why.into(),
    }
}

/// The devices to link: a trainer always (an empty address would search
/// for ever), pedals and a strap when named.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn targets(trainer: String, pedals: Vec<String>, strap: Option<String>) -> Result<(String, Vec<String>, Option<String>), CommandError> {
    if trainer.trim().is_empty() {
        return Err(CommandError::coded("training.noTrainer", "Pick a trainer to ride with"));
    }
    let pedals: Vec<String> = pedals.into_iter().filter(|a| !a.trim().is_empty()).collect();
    // One pedal device, or the two of a pair (an Assioma Duo over BLE),
    // left first: the order is the caller's word.
    if pedals.len() > 2 {
        return Err(CommandError::coded("training.tooManyPedals", "Pick one pair of pedals"));
    }
    let strap = strap.filter(|a| !a.trim().is_empty());
    // One device, one link: two links on one device would fight over it
    // (and its power would count twice).
    let mut all: Vec<&String> = std::iter::once(&trainer).chain(&pedals).chain(&strap).collect();
    all.sort();
    if all.windows(2).any(|w| w[0] == w[1]) {
        return Err(CommandError::coded("training.sameDevice", "One device was picked twice"));
    }
    Ok((trainer, pedals, strap))
}

fn active_journal(training: &TrainingState) -> Result<Option<PathBuf>, CommandError> {
    Ok(training.active.lock().map_err(|e| e.to_string())?.clone())
}

/// Whether a ride is under way (begun and not over): pairing is not one.
fn ride_running(training: &TrainingState) -> bool {
    training
        .live
        .lock()
        .is_ok_and(|live| live.as_ref().is_some_and(|l| l.phase != TrainingPhase::Pairing))
}

/// Refuses what would end the process under a ride (a restart, an
/// update): the trainer would not be let go and the ride not saved.
pub(crate) fn ensure_no_ride<R: Runtime>(app: &AppHandle<R>) -> Result<(), CommandError> {
    match app.try_state::<TrainingState>() {
        Some(training) if ride_running(&training) => Err(runner::ride_under_way()),
        _ => Ok(()),
    }
}

/// The app is asked to quit (Cmd+Q, the last window closed). A ride under
/// way is finished first — the trainer let go, the ride imported — and
/// `then` quits; returns true when the quit must wait for that. Asked
/// again while it finishes, the app quits at once (the journal stays for
/// next time).
pub(crate) fn finish_before_exit<R: Runtime>(app: &AppHandle<R>, then: impl FnOnce() + Send + 'static) -> bool {
    let Some(training) = app.try_state::<TrainingState>() else { return false };
    if !ride_running(&training) || training.quitting.swap(true, Ordering::SeqCst) {
        return false;
    }
    let Ok(Some(orders)) = training.orders.lock().map(|o| o.clone()) else { return false };
    let (tx, rx) = channel();
    if orders.send(Order::Finish(tx)).is_err() {
        return false;
    }
    std::thread::spawn(move || {
        // The answer comes once the session is over, whatever it was.
        let _ = rx.recv();
        then();
    });
    true
}

/// The app is asked to exit with `code`: whether to hold the exit for the
/// ride (`finish_before_exit`). A restart cannot be held — Tauri ignores
/// `prevent_exit` for it — and `restart_app` refuses under a ride anyway.
pub(crate) fn hold_exit<R: Runtime>(app: &AppHandle<R>, code: Option<i32>, then: impl FnOnce() + Send + 'static) -> bool {
    code != Some(tauri::RESTART_EXIT_CODE) && finish_before_exit(app, then)
}

/// The app's side of a ride.
struct AppHost<'a, R: Runtime> {
    app: &'a AppHandle<R>,
    state: &'a AppState,
    training: &'a TrainingState,
    flight: Option<SingleFlightGuard<'a>>,
    awake: Option<KeepAwake>,
    /// `done` ran: the session's slots are free.
    over: bool,
}

/// A ride thread that panicked never said `done`: the host says it on its
/// way out, or the app would stay "connected" until restarted.
impl<R: Runtime> Drop for AppHost<'_, R> {
    fn drop(&mut self) {
        if !self.over {
            self.done();
        }
    }
}

impl<R: Runtime> Host for AppHost<'_, R> {
    fn begin(&mut self, start_unix: i64, sub_sport: SubSport) -> Result<Option<Journal>, CommandError> {
        let flight = self.state.vault_flight.try_begin_ride().ok_or_else(|| vault_busy_in(&self.state.vault_flight))?;
        ensure_vault_unlocked(self.state)?;
        let app = self.app.clone();
        let key: KeyFn = Box::new(move || ride_key(&app.state::<AppState>()));
        let journal = Journal::create(&self.state.vault_path, start_unix, sub_sport, key)?;
        *self.training.active.lock().map_err(|e| e.to_string())? = Some(journal.path().to_path_buf());
        self.flight = Some(flight);
        self.awake = KeepAwake::start();
        Ok(Some(journal))
    }

    fn finish(&mut self, ride: &Ride, journal: Option<&Path>) -> Result<TrainingFinish, CommandError> {
        let finished = ensure_vault_unlocked(self.state).and_then(|()| library::finish_ride(self.state, ride, journal));
        if matches!(finished, Ok(TrainingFinish { kind: FinishKind::Imported, .. })) {
            let _ = self.app.emit("activities:updated", ());
        }
        finished
    }

    fn discard(&mut self, journal: Option<&Path>) -> Result<(), CommandError> {
        match journal {
            Some(path) => Ok(journal::discard(path)?),
            None => Ok(()),
        }
    }

    fn show(&mut self, live: &TrainingLive) {
        if let Ok(mut last) = self.training.live.lock() {
            *last = Some(live.clone());
        }
        let _ = self.app.emit(LIVE_EVENT, live);
    }

    fn done(&mut self) {
        self.over = true;
        self.flight = None;
        self.awake = None;
        if let Ok(mut active) = self.training.active.lock() {
            *active = None;
        }
        if let Ok(mut live) = self.training.live.lock() {
            *live = None;
        }
        if let Ok(mut orders) = self.training.orders.lock() {
            *orders = None;
        }
        self.training.quitting.store(false, Ordering::SeqCst);
        let _ = self.app.emit(LIVE_EVENT, None::<TrainingLive>);
    }
}

fn ensure_free<R: Runtime>(app: &AppHandle<R>) -> Result<(), CommandError> {
    match app.state::<TrainingState>().orders.lock().map_err(|e| e.to_string())?.is_some() {
        true => Err(busy()),
        false => Ok(()),
    }
}

/// Runs `session` on its own thread until it ends.
pub(crate) fn start_session<R: Runtime>(
    app: &AppHandle<R>,
    session: Session,
    clock: impl Clock + Send + 'static,
) -> Result<(), CommandError> {
    let training = app.state::<TrainingState>();
    let mut orders = training.orders.lock().map_err(|e| e.to_string())?;
    if orders.is_some() {
        return Err(busy());
    }
    let (tx, rx) = channel();
    let app = app.clone();
    std::thread::Builder::new()
        .name("syzify-ride".into())
        .spawn(move || {
            let state = app.state::<AppState>();
            let training = app.state::<TrainingState>();
            let mut host =
                AppHost { app: &app, state: state.inner(), training: training.inner(), flight: None, awake: None, over: false };
            runner::run(session, rx, &clock, &mut host);
        })
        .map_err(|e| e.to_string())?;
    *orders = Some(tx);
    Ok(())
}

fn tell<R: Runtime>(app: &AppHandle<R>, order: Order) -> Result<(), CommandError> {
    let sender = app.state::<TrainingState>().orders.lock().map_err(|e| e.to_string())?.clone().ok_or_else(no_session)?;
    sender.send(order).map_err(|_| no_session())
}

/// Sends an order with an answer and waits for it off the async runtime.
async fn ask<R: Runtime, T: Send + 'static>(app: &AppHandle<R>, order: fn(Reply<T>) -> Order) -> Result<T, CommandError> {
    let (tx, rx) = channel();
    tell(app, order(tx))?;
    tauri::async_runtime::spawn_blocking(move || rx.recv())
        .await
        .map_err(|e| e.to_string())?
        .map_err(|_| no_session())?
}

/// Trainers, power meters and straps advertising now (a few seconds' scan).
#[tauri::command]
pub async fn training_scan(seconds: Option<u32>) -> Result<Vec<FoundDevice>, CommandError> {
    #[cfg(target_os = "macos")]
    {
        let window = std::time::Duration::from_secs(u64::from(seconds.unwrap_or(6).clamp(2, 20)));
        crate::trainer::ble::driver::find_devices(window).await.map_err(link_failure)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = seconds;
        Err(unsupported())
    }
}

/// Links the picked devices (BLE addresses from `training_scan`) and starts
/// pairing. `pedals`: none, one device, or a pair's two (left, right) —
/// summed into one power source.
#[tauri::command]
pub fn training_connect<R: Runtime>(
    trainer: String,
    pedals: Vec<String>,
    strap: Option<String>,
    app: AppHandle<R>,
) -> Result<(), CommandError> {
    #[cfg(target_os = "macos")]
    {
        use crate::trainer::ble::driver::BleLink;
        use crate::trainer::ble::machine::{Role, Target};
        use crate::trainer::link::SensorLink;
        use crate::trainer::pedals::PedalPair;
        let (trainer, pedals, strap) = targets(trainer, pedals, strap)?;
        ensure_free(&app)?;
        let epoch = std::time::Instant::now();
        let link = |role, address: String| BleLink::start(Target { role, address: Some(address), name_contains: None }, epoch);
        let mut sides = pedals.into_iter().map(|a| Box::new(link(Role::Pedals, a)) as Box<dyn SensorLink>);
        let pedals = match (sides.next(), sides.next()) {
            (Some(left), Some(right)) => Some(Box::new(PedalPair::new(left, right)) as Box<dyn SensorLink>),
            (one, _) => one,
        };
        let strap = strap.map(|a| Box::new(link(Role::Heart, a)) as Box<dyn SensorLink>);
        let session = Session::pairing(Box::new(link(Role::Trainer, trainer)), pedals, strap);
        start_session(&app, session, runner::SystemClock::new(epoch))
    }
    #[cfg(not(target_os = "macos"))]
    {
        targets(trainer, pedals, strap)?;
        ensure_free(&app)?;
        Err(unsupported())
    }
}

#[tauri::command]
pub async fn training_begin<R: Runtime>(app: AppHandle<R>) -> Result<(), CommandError> {
    ask(&app, Order::Begin).await
}

#[tauri::command]
pub fn training_lap<R: Runtime>(app: AppHandle<R>) -> Result<(), CommandError> {
    tell(&app, Order::Lap)
}

#[tauri::command]
pub fn training_pause<R: Runtime>(app: AppHandle<R>) -> Result<(), CommandError> {
    tell(&app, Order::Pause)
}

#[tauri::command]
pub fn training_resume<R: Runtime>(app: AppHandle<R>) -> Result<(), CommandError> {
    tell(&app, Order::Resume)
}

/// ERG: hold `watts` (1 to 2000); None rides free.
#[tauri::command]
pub fn training_set_target<R: Runtime>(watts: Option<u32>, app: AppHandle<R>) -> Result<(), CommandError> {
    let target = match watts {
        None => None,
        Some(w) if (1..=u32::from(MAX_TARGET)).contains(&w) => Some(w as u16),
        Some(w) => {
            return Err(CommandError::coded("training.badTarget", format!("A target of {w} W is out of range")).param("watts", w));
        }
    };
    tell(&app, Order::SetTarget(target))
}

/// Ends the ride: lets the trainer go, imports the ride.
#[tauri::command]
pub async fn training_finish<R: Runtime>(app: AppHandle<R>) -> Result<TrainingFinish, CommandError> {
    ask(&app, Order::Finish).await
}

/// Ends the ride and throws it away.
#[tauri::command]
pub async fn training_discard<R: Runtime>(app: AppHandle<R>) -> Result<(), CommandError> {
    ask(&app, Order::Discard).await
}

/// Drops the links before a ride began.
#[tauri::command]
pub async fn training_leave<R: Runtime>(app: AppHandle<R>) -> Result<(), CommandError> {
    ask(&app, Order::Leave).await
}

/// What the screen was last shown; None when no session runs.
#[tauri::command]
pub fn training_live(training: State<'_, TrainingState>) -> Result<Option<TrainingLive>, CommandError> {
    Ok(training.live.lock().map_err(|e| e.to_string())?.clone())
}

/// The rider's current FTP (the newest ride's), for power zones.
#[tauri::command]
pub fn training_ftp(state: State<'_, AppState>) -> Result<Option<f64>, CommandError> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let rides = crate::models::activity::SportType::family("ride");
    Ok(crate::db::activities::latest_ftp(&conn, rides).map_err(|e| e.to_string())?)
}

/// Rides a crash or a closed app left unfinished. Each journal is read
/// whole (and opened, when sealed): off the main thread.
#[tauri::command]
pub async fn training_pending<R: Runtime>(app: AppHandle<R>) -> Result<Vec<PendingRide>, CommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        let active = active_journal(&app.state::<TrainingState>())?;
        Ok(library::pending(&app.state::<AppState>(), active.as_deref()))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn training_finish_pending<R: Runtime>(start_unix: i64, app: AppHandle<R>) -> Result<TrainingFinish, CommandError> {
    let handle = app.clone();
    let finished = tauri::async_runtime::spawn_blocking(move || {
        let state = handle.state::<AppState>();
        ensure_vault_unlocked(&state)?;
        let active = active_journal(&handle.state::<TrainingState>())?;
        library::finish_pending(&state, active.as_deref(), start_unix)
    })
    .await
    .map_err(|e| e.to_string())??;
    if finished.kind == FinishKind::Imported {
        let _ = app.emit("activities:updated", ());
    }
    Ok(finished)
}

#[tauri::command]
pub fn training_discard_pending(
    start_unix: i64,
    state: State<'_, AppState>,
    training: State<'_, TrainingState>,
) -> Result<(), CommandError> {
    library::discard_pending(&state, active_journal(&training)?.as_deref(), start_unix)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::{EncryptionScopes, VaultLock};
    use crate::test_support::{mock_app_at, ScratchDir};
    use crate::trainer::fake::{FakeConfig, FakeTrainer};
    use crate::trainer::journal::{fixed, Second};
    use crate::trainer::runner::test_clock::StepClock;
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    use tauri::async_runtime::block_on;

    /// 2026-11-11 18:00:00 UTC.
    const START: i64 = 1_794_420_000;

    fn app_at(vault: &Path) -> tauri::App<tauri::test::MockRuntime> {
        let app = mock_app_at(vault);
        app.manage(TrainingState::default());
        app
    }

    fn lock(activities: bool) -> VaultLock {
        VaultLock {
            salt: String::new(),
            verifier: String::new(),
            nonce: String::new(),
            created_at: String::new(),
            scopes: EncryptionScopes { activities, database: false, photos: !activities },
        }
    }

    /// Waits (a real 20 s at most) until the screen was shown `what`.
    fn wait_for(app: &tauri::App<tauri::test::MockRuntime>, what: impl Fn(&TrainingLive) -> bool) -> TrainingLive {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(live) = training_live(app.state()).unwrap() {
                if what(&live) {
                    return live;
                }
            }
            assert!(Instant::now() < deadline, "never shown");
            std::thread::yield_now();
        }
    }

    fn avanti() -> Box<FakeTrainer> {
        Box::new(FakeTrainer::new(FakeConfig::elite_avanti(85)))
    }

    /// A left-over journal of `seconds` seconds at 180 W.
    fn left_over_ride(vault: &Path, start: i64, key: Option<[u8; 32]>, seconds: u32) {
        let mut j = Journal::create(vault, start, SubSport::IndoorCycling, fixed(key)).unwrap();
        for t in 0..seconds {
            let sample = crate::trainer::fit::Sample { t, watts: Some(180), ..Default::default() };
            j.append(&journal::Entry::Second(Second { sample, ..Second::default() })).unwrap();
        }
    }

    #[test]
    fn the_rides_key_is_none_off_scope_and_an_error_while_locked() {
        let vault = ScratchDir::new("training_key");
        let app = app_at(&vault);
        let state = app.state::<AppState>();
        assert_eq!(ride_key(&state), Ok(None), "no encryption");
        crate::crypto::write_vault_lock(&vault, &lock(false)).unwrap();
        assert_eq!(ride_key(&state), Ok(None), "photos only: rides stay plain");
        crate::crypto::write_vault_lock(&vault, &lock(true)).unwrap();
        assert!(ride_key(&state).is_err(), "activities encrypted, vault locked");
        *state.encryption_key.lock().unwrap() = Some([3; 32]);
        assert_eq!(ride_key(&state), Ok(Some([3; 32])));
    }

    #[test]
    fn a_ride_through_the_commands() {
        let vault = ScratchDir::new("training_ride");
        let app = app_at(&vault);
        let handle = app.handle().clone();
        assert_eq!(training_lap(handle.clone()).unwrap_err().code, Some("training.none"));
        assert_eq!(training_live(app.state()).unwrap(), None);

        let clock = StepClock::new();
        clock.stop_at.store(5_000, Ordering::SeqCst);
        start_session(&handle, Session::pairing(avanti(), None, None), clock.clone()).unwrap();
        let again = start_session(&handle, Session::pairing(avanti(), None, None), clock.clone());
        assert_eq!(again.unwrap_err().code, Some("training.busy"));
        assert_eq!(ensure_free(&handle).unwrap_err().code, Some("training.busy"));
        wait_for(&app, |l| l.phase == crate::models::training::TrainingPhase::Pairing);

        block_on(training_begin(handle.clone())).unwrap();
        let state = app.state::<AppState>();
        assert!(state.vault_flight.try_begin().is_none(), "the ride holds the vault");
        // The running ride is not a left-over one.
        assert!(block_on(training_pending(handle.clone())).unwrap().is_empty());
        assert_eq!(journal::pending(&vault).len(), 1);
        let active = journal::start_of(&journal::pending(&vault)[0]).unwrap();
        assert_eq!(
            training_discard_pending(active, app.state(), app.state()).unwrap_err().code,
            Some("training.riding")
        );
        let finish_running = block_on(training_finish_pending(active, handle.clone()));
        assert_eq!(finish_running.unwrap_err().code, Some("training.riding"));
        assert_eq!(block_on(training_leave(handle.clone())).unwrap_err().code, Some("training.riding"));

        clock.stop_at.store(65_000, Ordering::SeqCst);
        wait_for(&app, |l| l.ride_s >= 50);
        training_lap(handle.clone()).unwrap();
        training_pause(handle.clone()).unwrap();
        training_resume(handle.clone()).unwrap();
        // Free: letting the trainer go takes time of its own.
        clock.stop_at.store(u64::MAX, Ordering::SeqCst);
        let finished = block_on(training_finish(handle.clone())).unwrap();
        assert_eq!(finished.kind, FinishKind::Imported);
        let id = finished.activity_id.expect("the new activity");
        let conn = state.db.lock().unwrap();
        let sport: String = conn.query_row("SELECT sport_type FROM activity WHERE id = ?1", [&id], |r| r.get(0)).unwrap();
        assert_eq!(sport, "indoor_ride");
        drop(conn);
        assert_eq!(training_ftp(app.state()).unwrap(), None, "no FTP known yet");
        let set_ftp = "UPDATE activity SET threshold_power_w = 254 WHERE id = ?1";
        state.db.lock().unwrap().execute(set_ftp, [&id]).unwrap();
        assert_eq!(training_ftp(app.state()).unwrap(), Some(254.0));

        // Over: nothing runs, the vault is free, no journal is left.
        assert_eq!(training_live(app.state()).unwrap(), None);
        assert_eq!(training_pause(handle.clone()).unwrap_err().code, Some("training.none"));
        assert!(state.vault_flight.try_begin().is_some());
        assert!(journal::pending(&vault).is_empty());
        ensure_free(&handle).unwrap();
    }

    #[test]
    fn a_ride_discarded_or_left_through_the_commands() {
        let vault = ScratchDir::new("training_discard");
        let app = app_at(&vault);
        let handle = app.handle().clone();
        let clock = StepClock::new();
        clock.stop_at.store(3_000, Ordering::SeqCst);
        start_session(&handle, Session::pairing(avanti(), None, None), clock.clone()).unwrap();
        wait_for(&app, |_| true);
        block_on(training_leave(handle.clone())).unwrap();
        assert_eq!(training_live(app.state()).unwrap(), None);

        // Discarded before it began: no journal to remove.
        let clock = StepClock::new();
        clock.stop_at.store(3_000, Ordering::SeqCst);
        start_session(&handle, Session::pairing(avanti(), None, None), clock.clone()).unwrap();
        wait_for(&app, |_| true);
        clock.stop_at.store(u64::MAX, Ordering::SeqCst);
        block_on(training_discard(handle.clone())).unwrap();
        assert!(journal::pending(&vault).is_empty());

        let clock = StepClock::new();
        clock.stop_at.store(3_000, Ordering::SeqCst);
        start_session(&handle, Session::pairing(avanti(), None, None), clock.clone()).unwrap();
        block_on(training_begin(handle.clone())).unwrap();
        clock.stop_at.store(u64::MAX, Ordering::SeqCst);
        block_on(training_discard(handle.clone())).unwrap();
        assert!(journal::pending(&vault).is_empty());
        let n: i64 = app.state::<AppState>().db.lock().unwrap().query_row("SELECT COUNT(*) FROM activity", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn a_begin_on_a_busy_vault_is_refused_and_the_session_pairs_on() {
        let vault = ScratchDir::new("training_busy");
        let app = app_at(&vault);
        let handle = app.handle().clone();
        let clock = StepClock::new();
        clock.stop_at.store(3_000, Ordering::SeqCst);
        start_session(&handle, Session::pairing(avanti(), None, None), clock.clone()).unwrap();
        {
            let state = app.state::<AppState>();
            let _backup = state.vault_flight.try_begin().unwrap();
            assert_eq!(block_on(training_begin(handle.clone())).unwrap_err(), crate::commands::vault_busy());
        }
        // Locked: the activities scope on, no key.
        crate::crypto::write_vault_lock(&vault, &lock(true)).unwrap();
        assert_eq!(block_on(training_begin(handle.clone())).unwrap_err().code, Some("vault.locked"));
        assert!(app.state::<AppState>().vault_flight.try_begin().is_some(), "a refused begin keeps nothing");
        block_on(training_leave(handle)).unwrap();
    }

    #[test]
    fn left_over_rides_are_listed_finished_and_discarded() {
        let vault = ScratchDir::new("training_left_over");
        let app = app_at(&vault);
        let handle = app.handle().clone();
        left_over_ride(&vault, START, None, 120);
        left_over_ride(&vault, START + 1000, Some([5; 32]), 60);
        left_over_ride(&vault, START + 2000, None, 0);
        std::fs::write(journal::path_of(&vault, START + 3000), b"not a journal").unwrap();

        let pending = block_on(training_pending(handle.clone())).unwrap();
        // (start, seconds, sealed elsewhere — the vault is not encrypted, unreadable)
        let summary: Vec<(i64, Option<u32>, bool, bool)> =
            pending.iter().map(|p| (p.start_unix - START, p.ride_s, p.sealed_elsewhere, p.unreadable.is_some())).collect();
        assert_eq!(summary, vec![(0, Some(120), false, false), (1000, None, true, false), (2000, Some(0), false, false), (3000, None, false, true)]);
        assert!(pending.iter().all(|p| !p.locked));

        // Encryption waits for them.
        let scopes = EncryptionScopes { activities: true, database: false, photos: false };
        let enable = block_on(crate::commands::settings::enable_encryption("pw".into(), scopes, handle.clone()));
        assert_eq!(enable.unwrap_err().code, Some("training.pendingRides"));
        let disable = block_on(crate::commands::settings::disable_encryption("pw".into(), handle.clone()));
        assert_eq!(disable.unwrap_err().code, Some("training.pendingRides"));

        let finished = block_on(training_finish_pending(START, handle.clone())).unwrap();
        assert_eq!(finished.kind, FinishKind::Imported);
        assert!(finished.activity_id.is_some());
        let empty = block_on(training_finish_pending(START + 2000, handle.clone())).unwrap();
        assert_eq!(empty, TrainingFinish { kind: FinishKind::Empty, activity_id: None });
        // Sealed, and no lock says which key: it waits.
        let sealed = block_on(training_finish_pending(START + 1000, handle.clone()));
        assert_eq!(sealed.unwrap_err().code, Some("training.journalLocked"));
        let ruined = block_on(training_finish_pending(START + 3000, handle.clone()));
        assert_eq!(ruined.unwrap_err().code, None, "the journal's own words");
        training_discard_pending(START + 3000, app.state(), app.state()).unwrap();
        let gone = training_discard_pending(START + 3000, app.state(), app.state()).unwrap_err();
        assert_eq!(gone, CommandError::coded("training.noSuchRide", "That ride is gone"));

        // With the lock and the key, the sealed one finishes.
        crate::crypto::write_vault_lock(&vault, &lock(true)).unwrap();
        let locked = block_on(training_finish_pending(START + 1000, handle.clone()));
        assert_eq!(locked.unwrap_err().code, Some("vault.locked"));
        *app.state::<AppState>().encryption_key.lock().unwrap() = Some([5; 32]);
        assert_eq!(block_on(training_pending(handle.clone())).unwrap()[0].ride_s, Some(60));
        assert_eq!(block_on(training_finish_pending(START + 1000, handle.clone())).unwrap().kind, FinishKind::Imported);
        assert!(block_on(training_pending(handle.clone())).unwrap().is_empty());
        library::ensure_no_left_over_rides(&app.state::<AppState>()).unwrap();
    }

    #[test]
    fn the_same_ride_twice_is_a_duplicate() {
        let vault = ScratchDir::new("training_twice");
        let app = app_at(&vault);
        left_over_ride(&vault, START, None, 30);
        let ride = journal::read(&journal::path_of(&vault, START), None).unwrap().ride();
        let state = app.state::<AppState>();
        crate::trainer::ride::import_ride(&state.db.lock().unwrap(), &vault, &ride, None).unwrap();
        let first: String = state.db.lock().unwrap().query_row("SELECT id FROM activity", [], |r| r.get(0)).unwrap();
        let again = block_on(training_finish_pending(START, app.handle().clone())).unwrap();
        assert_eq!(again, TrainingFinish { kind: FinishKind::Duplicate, activity_id: Some(first) }, "the one in the library");
    }

    /// A ride thread that dies still frees the session and the vault.
    #[test]
    fn a_ride_thread_that_panics_frees_the_session() {
        use crate::trainer::link::{Control, ControlId, Identity, LinkEvent, SensorLink, TrainerLink};
        use crate::trainer::profile::TrainerProfile;
        struct Boom;
        impl SensorLink for Boom {
            fn identity(&self) -> Option<&Identity> {
                None
            }
            fn poll(&mut self, now: Duration) -> Vec<LinkEvent> {
                assert!(now < Duration::from_secs(5), "the radio task died");
                Vec::new()
            }
        }
        impl TrainerLink for Boom {
            fn use_profile(&mut self, _: &'static TrainerProfile) {}
            fn send(&mut self, _: Control) -> ControlId {
                ControlId(0)
            }
        }
        assert!(Boom.identity().is_none());
        Boom.use_profile(&crate::trainer::profile::GENERIC);
        assert_eq!(Boom.send(Control::Release), ControlId(0));
        let vault = ScratchDir::new("training_panic");
        let app = app_at(&vault);
        let handle = app.handle().clone();
        let clock = StepClock::new();
        clock.stop_at.store(1_000, Ordering::SeqCst);
        start_session(&handle, Session::pairing(Box::new(Boom), None, None), clock.clone()).unwrap();
        block_on(training_begin(handle.clone())).unwrap();
        assert!(app.state::<AppState>().vault_flight.try_begin().is_none());
        clock.stop_at.store(10_000, Ordering::SeqCst);
        let deadline = Instant::now() + Duration::from_secs(20);
        while ensure_free(&handle).is_err() {
            assert!(Instant::now() < deadline, "the session never let go");
            std::thread::yield_now();
        }
        assert!(app.state::<AppState>().vault_flight.try_begin().is_some());
        assert_eq!(training_live(app.state()).unwrap(), None);
        assert_eq!(journal::pending(&vault).len(), 1, "the journal stays for next time");
    }

    #[test]
    fn bluetooth_failures_are_told_by_kind() {
        let code = |f| link_failure(f).code;
        assert_eq!(code(LinkFailure::PermissionDenied), Some("training.bluetoothDenied"));
        assert_eq!(code(LinkFailure::RadioOff), Some("training.bluetoothOff"));
        assert_eq!(code(LinkFailure::AdapterGone), Some("training.noBluetooth"));
        assert_eq!(link_failure(LinkFailure::Other("boom".into())), CommandError::from("boom"));
        assert_eq!(unsupported().code, Some("training.unsupported"));
    }

    #[test]
    fn an_erg_target_is_checked_and_handed_to_the_ride() {
        let vault = ScratchDir::new("training_target");
        let app = app_at(&vault);
        let handle = app.handle().clone();
        let out_of_range = |w: u32| CommandError::coded("training.badTarget", format!("A target of {w} W is out of range")).param("watts", w);
        assert_eq!(training_set_target(Some(0), handle.clone()).unwrap_err(), out_of_range(0));
        assert_eq!(training_set_target(Some(2001), handle.clone()).unwrap_err(), out_of_range(2001));
        assert_eq!(training_set_target(Some(150), handle.clone()).unwrap_err().code, Some("training.none"));

        let clock = StepClock::new();
        clock.stop_at.store(3_000, Ordering::SeqCst);
        start_session(&handle, Session::pairing(avanti(), None, None), clock.clone()).unwrap();
        block_on(training_begin(handle.clone())).unwrap();
        training_set_target(Some(2000), handle.clone()).unwrap();
        clock.stop_at.store(6_000, Ordering::SeqCst);
        wait_for(&app, |l| l.erg.is_some_and(|e| e.target_w == 2000));
        training_set_target(None, handle.clone()).unwrap();
        wait_for(&app, |l| l.erg.is_none());
        clock.stop_at.store(u64::MAX, Ordering::SeqCst);
        block_on(training_discard(handle)).unwrap();
    }

    #[test]
    fn a_trainer_must_be_named_and_blank_sensors_are_none() {
        let no_trainer = targets("  ".into(), vec!["P".into()], None).unwrap_err();
        assert_eq!(no_trainer, CommandError::coded("training.noTrainer", "Pick a trainer to ride with"));
        let picked = targets("T".into(), vec![" ".into()], Some("H".into())).unwrap();
        assert_eq!(picked, ("T".into(), vec![], Some("H".into())));
        let pair = targets("T".into(), vec!["L".into(), "R".into()], Some(" ".into())).unwrap();
        assert_eq!(pair, ("T".into(), vec!["L".into(), "R".into()], None));
        let three = targets("T".into(), vec!["A".into(), "B".into(), "C".into()], None).unwrap_err();
        assert_eq!(three, CommandError::coded("training.tooManyPedals", "Pick one pair of pedals"));
        let twice = CommandError::coded("training.sameDevice", "One device was picked twice");
        assert_eq!(targets("T".into(), vec!["A".into(), "A".into()], None).unwrap_err(), twice);
        assert_eq!(targets("T".into(), vec!["T".into()], None).unwrap_err(), twice, "the trainer as pedals");
        assert_eq!(targets("T".into(), vec![], Some("T".into())).unwrap_err(), twice, "the trainer as the strap");
    }

    /// While a ride runs, nothing ends the process under it, and the vault
    /// says the ride holds it; a quit finishes the ride first.
    #[test]
    fn a_ride_under_way_holds_off_restarts_and_finishes_before_a_quit() {
        let vault = ScratchDir::new("training_guards");
        let app = app_at(&vault);
        let handle = app.handle().clone();
        let quit = Arc::new(AtomicBool::new(false));
        let quitter = || {
            let quit = quit.clone();
            move || quit.store(true, Ordering::SeqCst)
        };
        // No ride: nothing to wait for.
        ensure_no_ride(&handle).unwrap();
        assert!(!finish_before_exit(&handle, quitter()));
        // Without the training state at all (an app that never had one).
        let bare = mock_app_at(&vault);
        ensure_no_ride(bare.handle()).unwrap();
        assert!(!finish_before_exit(bare.handle(), quitter()));

        let clock = StepClock::new();
        clock.stop_at.store(3_000, Ordering::SeqCst);
        start_session(&handle, Session::pairing(avanti(), None, None), clock.clone()).unwrap();
        wait_for(&app, |_| true);
        ensure_no_ride(&handle).unwrap(); // pairing is no ride yet
        assert!(!finish_before_exit(&handle, quitter()), "pairing: quit at once");

        block_on(training_begin(handle.clone())).unwrap();
        wait_for(&app, |l| l.phase == TrainingPhase::Riding);
        let ride = Some("training.rideUnderWay");
        assert_eq!(ensure_no_ride(&handle).unwrap_err().code, ride);
        assert_eq!(crate::commands::settings::restart_app(handle.clone()).unwrap_err().code, ride);
        assert_eq!(block_on(crate::commands::settings::install_update(handle.clone())).unwrap_err().code, ride);
        let scopes = crate::crypto::EncryptionScopes { activities: true, database: false, photos: false };
        let encrypt = block_on(crate::commands::settings::enable_encryption("pw".into(), scopes, handle.clone()));
        assert_eq!(encrypt.unwrap_err().code, ride, "the vault names the ride that holds it");

        clock.stop_at.store(u64::MAX, Ordering::SeqCst);
        assert!(!hold_exit(&handle, Some(tauri::RESTART_EXIT_CODE), quitter()), "a restart cannot be held");
        assert!(hold_exit(&handle, Some(0), quitter()), "the quit waits for the ride");
        assert!(!finish_before_exit(&handle, quitter()), "asked again: quit at once");
        let deadline = Instant::now() + Duration::from_secs(20);
        while !quit.load(Ordering::SeqCst) {
            assert!(Instant::now() < deadline, "never quit");
            std::thread::yield_now();
        }
        assert!(journal::pending(&vault).is_empty(), "the ride was finished, not left");
        let n: i64 = app.state::<AppState>().db.lock().unwrap().query_row("SELECT COUNT(*) FROM activity", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
        assert!(!app.state::<TrainingState>().quitting.load(Ordering::SeqCst), "the next session starts afresh");
        ensure_no_ride(&handle).unwrap();
    }

    /// A ride whose thread is gone cannot be finished: the quit goes at once.
    #[test]
    fn a_quit_whose_ride_thread_is_gone_goes_at_once() {
        let vault = ScratchDir::new("training_gone");
        let app = app_at(&vault);
        let training = app.state::<TrainingState>();
        let (tx, rx) = channel();
        drop(rx);
        *training.orders.lock().unwrap() = Some(tx);
        *training.live.lock().unwrap() = Some(TrainingLive {
            phase: TrainingPhase::Riding,
            ride_s: 0,
            lap_s: 0,
            lap: 1,
            watts: None,
            power_from: None,
            trainer_watts: None,
            cadence_rpm: None,
            heart_rate: None,
            speed_kmh: None,
            distance_m: 0.0,
            avg_watts: None,
            profile: None,
            links: vec![],
            warnings: vec![],
            slept_s: 0,
            erg: None,
        });
        assert!(!finish_before_exit(app.handle(), || {}));
    }

    /// A sealed journal says why it waits: the vault is locked, or it was
    /// sealed under another vault's key.
    #[test]
    fn a_sealed_left_over_ride_says_why_it_cannot_open() {
        let vault = ScratchDir::new("training_sealed");
        let app = app_at(&vault);
        left_over_ride(&vault, START, Some([5; 32]), 10);
        let one = || block_on(training_pending(app.handle().clone())).unwrap().remove(0);
        let elsewhere = one();
        assert!(elsewhere.sealed_elsewhere && !elsewhere.locked, "a plain vault: sealed elsewhere");
        crate::crypto::write_vault_lock(&vault, &lock(true)).unwrap();
        let locked = one();
        assert!(locked.locked && !locked.sealed_elsewhere, "the vault is locked");
    }

    /// Elsewhere than macOS there is nothing to scan or connect.
    #[cfg(not(target_os = "macos"))]
    #[test]
    fn other_systems_have_no_training() {
        let vault = ScratchDir::new("training_elsewhere");
        let app = app_at(&vault);
        assert_eq!(block_on(training_scan(None)).unwrap_err(), unsupported());
        let connect = training_connect("T".into(), vec![], None, app.handle().clone());
        assert_eq!(connect.unwrap_err(), unsupported());
        let nameless = training_connect("".into(), vec![], None, app.handle().clone());
        assert_eq!(nameless.unwrap_err().code, Some("training.noTrainer"));
    }
}
