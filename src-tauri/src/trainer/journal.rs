//! The ride's journal: every second written to the vault as it happens,
//! so a crash, a closed lid or a dead battery loses nothing. One entry per
//! line, appended and synced. A crash can only cut the last line; a write
//! that failed half-way is fenced off by starting the next on a new line.
//! Reading skips lines it cannot read and counts them.
//!
//! The journal is a new file in the vault, so it follows the activities
//! encryption scope, asked afresh for every line (`KeyFn`, which the app
//! backs with `AppState::encryption_key_for(|s| s.activities)`): with a key,
//! the line is sealed (AES-GCM) and the ride never sits on disk in the
//! clear. A sealed line is bound to its journal (the associated data names
//! the ride's start), so lines cannot be moved between journals; their
//! order within one is not protected.
//!
//! Turning encryption on or off while a ride runs, or with a journal left
//! by a crash, is for the session's owner to prevent (the ride holds the
//! vault's flight slot; leftover journals are finished or discarded first).

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use serde::{Deserialize, Serialize};

use super::fit::{lap_indexes, Device, DeviceKind, Ride, Sample, SubSport};
use crate::crypto;

/// Where unfinished rides live, inside the vault.
pub const DIR: &str = "rides";
const EXTENSION: &str = "journal";
const VERSION: u32 = 1;
/// A sealed line starts with this; a plain one is JSON.
const SEALED: &str = "s:";

/// Gives the key for the next line: the activities key while that scope
/// is encrypted, None while it is not, an error while the vault is locked.
pub type KeyFn = Box<dyn Fn() -> Result<Option<[u8; 32]>, String> + Send>;

/// A key that never changes (no encryption: `fixed(None)`).
pub fn fixed(key: Option<[u8; 32]>) -> KeyFn {
    Box::new(move || Ok(key))
}

fn aad(start_unix: i64) -> Vec<u8> {
    format!("syzify-ride-journal|{start_unix}").into_bytes()
}

/// Which sensor a value came from (the screen's `SensorRole`).
pub use crate::models::training::SensorRole as Source;

/// One second as recorded: the values, where power and cadence came from,
/// and the trainer's own watts (what power match compares the pedals with).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Second {
    #[serde(flatten)]
    pub sample: Sample,
    pub power_from: Option<Source>,
    pub cadence_from: Option<Source>,
    #[serde(default)]
    pub trainer_watts: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Entry {
    Start { version: u32, start_unix: i64, sub_sport: SubSport },
    Device(Device),
    Second(Second),
    /// A new lap begins at this second (its `t`, not its place in the
    /// journal: a lost line must not move the lap).
    Lap { at: usize },
    /// The timer stops from this second on…
    Pause { at: u32 },
    /// …and runs again from this one.
    Resume { at: u32 },
}

/// An open journal, appended to during the ride.
pub struct Journal {
    file: File,
    path: PathBuf,
    start_unix: i64,
    key: KeyFn,
    /// The last write may have left half a line.
    torn: bool,
}

impl Journal {
    /// A new journal in `<vault>/rides/`, starting with its header.
    pub fn create(vault: &Path, start_unix: i64, sub_sport: SubSport, key: KeyFn) -> Result<Journal, String> {
        let dir = vault.join(DIR);
        std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        let path = path_of(vault, start_unix);
        let file = OpenOptions::new()
            .create_new(true)
            .append(true)
            .open(&path)
            .map_err(|e| format!("cannot create {}: {e}", path.display()))?;
        // The new name must survive a crash too.
        if let Ok(d) = File::open(&dir) {
            let _ = d.sync_all();
        }
        let mut journal = Journal { file, path, start_unix, key, torn: false };
        if let Err(e) = journal.append(&Entry::Start { version: VERSION, start_unix, sub_sport }) {
            let _ = std::fs::remove_file(&journal.path);
            return Err(e);
        }
        Ok(journal)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Writes the entry and makes it durable before returning. A failure
    /// costs this entry only: the next one starts on a fresh line.
    pub fn append(&mut self, entry: &Entry) -> Result<(), String> {
        let json = serde_json::to_string(entry).map_err(|e| e.to_string())?;
        let key = (self.key)()?;
        let mut line = String::new();
        if self.torn {
            line.push('\n');
        }
        match key {
            Some(key) => {
                let sealed = crypto::seal(&key, &aad(self.start_unix), json.as_bytes())?;
                line.push_str(SEALED);
                line.push_str(&base64::engine::general_purpose::STANDARD.encode(sealed));
            }
            None => line.push_str(&json),
        }
        line.push('\n');
        self.torn = true;
        self.file.write_all(line.as_bytes()).map_err(|e| format!("journal write failed: {e}"))?;
        self.file.sync_data().map_err(|e| format!("journal sync failed: {e}"))?;
        self.torn = false;
        Ok(())
    }
}

/// A journal read back.
#[derive(Debug, Clone, PartialEq)]
pub struct Recovered {
    pub start_unix: i64,
    pub sub_sport: SubSport,
    pub devices: Vec<Device>,
    pub seconds: Vec<Second>,
    /// The seconds (`t`) laps begin at.
    pub lap_starts: Vec<usize>,
    /// Paused spans `[stop, start)`; one still open at the crash is left
    /// out (no seconds follow it).
    pub pauses: Vec<(u32, u32)>,
    /// Lines that could not be read (a line cut by a crash).
    pub skipped: usize,
}

impl Recovered {
    /// The ride as the session had it: devices in the session's order
    /// (trainer, pedals, strap), so the FIT file is the same bytes.
    pub fn ride(&self) -> Ride {
        let rank = |d: &Device| match d.kind {
            DeviceKind::FitnessEquipment => 0,
            DeviceKind::BikePower => 1,
            DeviceKind::HeartRate => 2,
        };
        let mut devices = self.devices.clone();
        devices.sort_by_key(rank);
        let samples: Vec<_> = self.seconds.iter().map(|s| s.sample).collect();
        // The encoder splits laps by place; a lost second shifts the places.
        let lap_starts = lap_indexes(&samples, &self.lap_starts);
        Ride { start_unix: self.start_unix, sub_sport: self.sub_sport, samples, lap_starts, devices, pauses: self.pauses.clone() }
    }
}

/// Why a journal does not read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadError {
    /// It is sealed and no key was given: unlock the vault.
    Locked,
    /// No readable start (another key, a ruined file).
    Unreadable(String),
    Io(String),
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReadError::Locked => write!(f, "the journal is encrypted: unlock the vault first"),
            ReadError::Unreadable(why) => write!(f, "the journal cannot be read: {why}"),
            ReadError::Io(e) => write!(f, "{e}"),
        }
    }
}

enum Line {
    Entry(Entry),
    Locked,
    Bad(String),
}

fn decode(line: &str, key: Option<&[u8; 32]>, start_unix: Option<i64>) -> Line {
    let json = match line.strip_prefix(SEALED) {
        Some(b64) => {
            let Some(key) = key else { return Line::Locked };
            let Ok(blob) = base64::engine::general_purpose::STANDARD.decode(b64) else {
                return Line::Bad("not base64".into());
            };
            // Sealed lines open with their ride's start: the header's once
            // read, the file name's for the header itself.
            let Some(start) = start_unix else { return Line::Bad("no start to open a sealed line with".into()) };
            match crypto::open(key, &aad(start), &blob).map(String::from_utf8) {
                Ok(Ok(json)) => json,
                Ok(Err(e)) => return Line::Bad(e.to_string()),
                Err(e) => return Line::Bad(e),
            }
        }
        None => line.to_string(),
    };
    match serde_json::from_str(&json) {
        Ok(entry) => Line::Entry(entry),
        Err(e) => Line::Bad(e.to_string()),
    }
}

/// The start a journal's name gives (`<start>.journal`).
pub fn start_of(path: &Path) -> Option<i64> {
    path.file_stem()?.to_str()?.parse().ok()
}

/// Reads a journal back; lines that do not read are skipped and counted.
/// One sealed line and no key is `Locked`, however much else reads: a
/// ride encrypted half-way must not be finished (and its journal removed)
/// with half its seconds.
pub fn read(path: &Path, key: Option<&[u8; 32]>) -> Result<Recovered, ReadError> {
    let bytes = std::fs::read(path).map_err(|e| ReadError::Io(format!("cannot open {}: {e}", path.display())))?;
    let named_start = start_of(path);
    let mut header = None;
    let mut out = Recovered {
        start_unix: 0,
        sub_sport: SubSport::IndoorCycling,
        devices: Vec::new(),
        seconds: Vec::new(),
        lap_starts: Vec::new(),
        pauses: Vec::new(),
        skipped: 0,
    };
    let mut paused_at = None;
    let mut locked = false;
    let mut first_error = None;
    for bytes in bytes.split(|&b| b == b'\n') {
        let Ok(line) = std::str::from_utf8(bytes) else {
            out.skipped += 1;
            first_error.get_or_insert_with(|| "a line is not UTF-8".to_string());
            continue;
        };
        if line.trim().is_empty() {
            continue;
        }
        let start = header.map(|(s, _)| s).or(named_start);
        match decode(line, key, start) {
            Line::Entry(Entry::Start { start_unix, sub_sport, .. }) => header = Some((start_unix, sub_sport)),
            Line::Entry(Entry::Device(d)) => {
                // One device per kind (a session has one link per role): a
                // later identity replaces an earlier one.
                out.devices.retain(|known| known.kind != d.kind);
                out.devices.push(d);
            }
            Line::Entry(Entry::Second(s)) => {
                // A second recorded at or after an open pause's stop: the
                // Resume line was lost, and this is where riding went on.
                if let Some(stop) = paused_at.filter(|&stop| s.sample.t >= stop) {
                    paused_at = None;
                    out.pauses.push((stop, s.sample.t));
                }
                out.seconds.push(s);
            }
            Line::Entry(Entry::Lap { at }) => out.lap_starts.push(at),
            Line::Entry(Entry::Pause { at }) => paused_at = paused_at.or(Some(at)),
            Line::Entry(Entry::Resume { at }) => {
                // A lost Pause line: the pause began after the last second
                // recorded.
                let stop = paused_at.take().unwrap_or_else(|| out.seconds.last().map_or(0, |s| s.sample.t + 1));
                out.pauses.push((stop, at));
            }
            Line::Locked => {
                locked = true;
                out.skipped += 1;
            }
            Line::Bad(e) => {
                out.skipped += 1;
                first_error.get_or_insert(e);
            }
        }
    }
    // A pause holds no recorded second: one that does was guessed wrong (a
    // Resume within the second paused, its Pause lost), as is an empty one.
    let seconds = &out.seconds;
    out.pauses.retain(|&(a, b)| a < b && !seconds.iter().any(|s| (a..b).contains(&s.sample.t)));
    if locked {
        return Err(ReadError::Locked);
    }
    let Some((start_unix, sub_sport)) = header else {
        return Err(ReadError::Unreadable(first_error.unwrap_or_else(|| "empty".into())));
    };
    out.start_unix = start_unix;
    out.sub_sport = sub_sport;
    Ok(out)
}

/// Where the journal of the ride that started at `start_unix` lives.
pub fn path_of(vault: &Path, start_unix: i64) -> PathBuf {
    vault.join(DIR).join(format!("{start_unix}.{EXTENSION}"))
}

/// Unfinished rides in the vault, oldest first.
pub fn pending(vault: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(vault.join(DIR)) else { return Vec::new() };
    let mut out: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == EXTENSION))
        .collect();
    out.sort();
    out
}

/// Throws an unfinished ride away (the user said so, or it cannot be read).
pub fn discard(path: &Path) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("cannot remove {}: {e}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::ScratchDir;
    use crate::trainer::fit::Radio;
    use std::sync::{Arc, Mutex};

    const KEY: [u8; 32] = [7; 32];
    const START: i64 = 1_794_420_000;

    fn avanti() -> Device {
        Device {
            kind: DeviceKind::FitnessEquipment,
            manufacturer: Some(86),
            name: Some("AVANTI COG".into()),
            serial: Some(14830),
            radio: Some(Radio::Ant),
        }
    }

    fn second(t: u32, watts: u16) -> Entry {
        Entry::Second(Second {
            sample: Sample { t, watts: Some(watts), cadence_rpm: Some(85), ..Sample::default() },
            power_from: Some(Source::Pedals),
            cadence_from: Some(Source::Trainer),
            trainer_watts: Some(watts - 5),
        })
    }

    fn write(vault: &Path, key: Option<[u8; 32]>) -> PathBuf {
        let mut j = Journal::create(vault, START, SubSport::IndoorCycling, fixed(key)).unwrap();
        j.append(&Entry::Device(avanti())).unwrap();
        j.append(&second(0, 150)).unwrap();
        j.append(&second(1, 152)).unwrap();
        j.append(&Entry::Lap { at: 2 }).unwrap();
        j.append(&second(2, 200)).unwrap();
        j.path().to_path_buf()
    }

    #[test]
    fn a_journal_reads_back_as_written() {
        let vault = ScratchDir::new("journal");
        let path = write(&vault, None);
        let r = read(&path, None).unwrap();
        assert_eq!(r.start_unix, START);
        assert_eq!(r.seconds.len(), 3);
        assert_eq!(r.seconds[2].sample.watts, Some(200));
        assert_eq!((r.seconds[0].power_from, r.seconds[0].trainer_watts), (Some(Source::Pedals), Some(145)));
        assert_eq!(r.lap_starts, vec![2]);
        assert_eq!(r.devices, vec![avanti()]);
        assert_eq!(r.skipped, 0);
        let ride = r.ride();
        assert_eq!((ride.samples.len(), ride.lap_starts.clone()), (3, vec![2]));
        assert_eq!(pending(&vault), vec![path]);
    }

    #[test]
    fn a_lost_second_does_not_move_a_lap() {
        let vault = ScratchDir::new("journal_lap_gap");
        let mut j = Journal::create(&vault, START, SubSport::IndoorCycling, fixed(None)).unwrap();
        j.append(&second(0, 150)).unwrap();
        // Second 1 never reached the disk.
        j.append(&second(2, 160)).unwrap();
        j.append(&Entry::Lap { at: 3 }).unwrap();
        j.append(&second(3, 200)).unwrap();
        j.append(&second(4, 200)).unwrap();
        let ride = read(j.path(), None).unwrap().ride();
        assert_eq!(ride.lap_starts, vec![2], "the lap begins at t = 3, the third sample");
        assert_eq!(ride.samples[ride.lap_starts[0]].t, 3);
    }

    #[test]
    fn pauses_read_back_and_an_open_or_orphan_one_is_left_out() {
        let vault = ScratchDir::new("journal_pauses");
        let mut j = Journal::create(&vault, START, SubSport::IndoorCycling, fixed(None)).unwrap();
        j.append(&second(0, 150)).unwrap();
        j.append(&Entry::Resume { at: 1 }).unwrap(); // its Pause was lost
        j.append(&Entry::Pause { at: 1 }).unwrap();
        j.append(&Entry::Pause { at: 2 }).unwrap(); // twice: the first stands
        j.append(&Entry::Resume { at: 30 }).unwrap();
        j.append(&second(30, 160)).unwrap();
        j.append(&Entry::Pause { at: 31 }).unwrap();
        j.append(&Entry::Resume { at: 31 }).unwrap(); // within the second: none
        j.append(&Entry::Pause { at: 31 }).unwrap(); // the crash came while paused
        let r = read(j.path(), None).unwrap();
        assert_eq!(r.pauses, vec![(1, 30)]);
        assert_eq!(r.ride().pauses, vec![(1, 30)]);
    }

    /// A pause whose Pause or Resume line was lost is found from the seconds
    /// around it, not counted as riding.
    #[test]
    fn a_pause_with_a_lost_line_is_still_a_pause() {
        let read_back = |entries: Vec<Entry>| {
            let vault = ScratchDir::new("journal_lost_pause");
            let mut j = Journal::create(&vault, START, SubSport::IndoorCycling, fixed(None)).unwrap();
            for e in entries {
                j.append(&e).unwrap();
            }
            read(j.path(), None).unwrap().pauses
        };
        let ride = |range: std::ops::Range<u32>| range.map(|t| second(t, 150)).collect::<Vec<_>>();
        let lost_resume = [ride(0..60), vec![Entry::Pause { at: 60 }], ride(120..180)].concat();
        assert_eq!(read_back(lost_resume), vec![(60, 120)]);
        let lost_pause = [ride(0..60), vec![Entry::Resume { at: 120 }], ride(120..180)].concat();
        assert_eq!(read_back(lost_pause), vec![(60, 120)]);
        // Resumed within the second paused, the Pause lost: second 10 was
        // ridden, so there was no pause.
        let within = [ride(0..10), vec![Entry::Resume { at: 11 }], ride(10..20)].concat();
        assert_eq!(read_back(within), vec![]);
    }

    #[test]
    fn a_line_cut_by_a_crash_or_not_utf8_is_skipped() {
        let vault = ScratchDir::new("journal_cut");
        let path = write(&vault, None);
        let mut f = OpenOptions::new().append(true).open(&path).unwrap();
        f.write_all(&[0xFF, 0xFE, b'\n']).unwrap();
        f.write_all(b"{\"second\":{\"t\":3,\"wat").unwrap();
        let r = read(&path, None).unwrap();
        assert_eq!((r.seconds.len(), r.skipped), (3, 2));
    }

    #[test]
    fn with_a_key_nothing_is_on_disk_in_the_clear() {
        let vault = ScratchDir::new("journal_sealed");
        let path = write(&vault, Some(KEY));
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.lines().all(|l| l.starts_with(SEALED)), "{text}");
        assert!(!text.contains("AVANTI") && !text.contains("watts"));
        assert_eq!(read(&path, Some(&KEY)).unwrap().seconds.len(), 3);
        assert_eq!(read(&path, None).unwrap_err(), ReadError::Locked);
        assert!(ReadError::Locked.to_string().contains("unlock the vault"));
        assert!(matches!(read(&path, Some(&[8; 32])), Err(ReadError::Unreadable(_))), "the wrong key reads nothing");
    }

    #[test]
    fn sealed_lines_belong_to_their_journal() {
        let vault = ScratchDir::new("journal_splice");
        let a = write(&vault, Some(KEY));
        let mut other = Journal::create(&vault, START + 1, SubSport::IndoorCycling, fixed(Some(KEY))).unwrap();
        other.append(&second(0, 999)).unwrap();
        // Move journal B's second into journal A.
        let foreign = std::fs::read_to_string(other.path()).unwrap().lines().last().unwrap().to_string();
        let mut f = OpenOptions::new().append(true).open(&a).unwrap();
        writeln!(f, "{foreign}").unwrap();
        let r = read(&a, Some(&KEY)).unwrap();
        assert_eq!((r.seconds.len(), r.skipped), (3, 1), "the foreign line does not open here");
    }

    #[test]
    fn the_key_is_asked_for_every_line() {
        // Encryption switched on mid-ride: the lines after it are sealed.
        let vault = ScratchDir::new("journal_switch");
        let key = Arc::new(Mutex::new(None));
        let k = key.clone();
        let mut j = Journal::create(&vault, START, SubSport::IndoorCycling, Box::new(move || Ok(*k.lock().unwrap()))).unwrap();
        j.append(&second(0, 150)).unwrap();
        *key.lock().unwrap() = Some(KEY);
        j.append(&second(1, 150)).unwrap();
        let text = std::fs::read_to_string(j.path()).unwrap();
        let sealed: Vec<bool> = text.lines().map(|l| l.starts_with(SEALED)).collect();
        assert_eq!(sealed, vec![false, false, true]);
        assert_eq!(read(j.path(), Some(&KEY)).unwrap().seconds.len(), 2);
    }

    #[test]
    fn a_ride_encrypted_half_way_reads_only_with_the_key() {
        // Encryption switched on at the third second, then a crash: without
        // the key the plain half must not pass for the whole ride.
        let vault = ScratchDir::new("journal_mixed");
        let key = Arc::new(Mutex::new(None));
        let k = key.clone();
        let mut j = Journal::create(&vault, START, SubSport::IndoorCycling, Box::new(move || Ok(*k.lock().unwrap()))).unwrap();
        j.append(&second(0, 150)).unwrap();
        j.append(&second(1, 150)).unwrap();
        *key.lock().unwrap() = Some(KEY);
        j.append(&second(2, 150)).unwrap();
        assert_eq!(read(j.path(), None).unwrap_err(), ReadError::Locked);
        assert_eq!(read(j.path(), Some(&KEY)).unwrap().seconds.len(), 3);
    }

    #[test]
    fn sealed_lines_that_are_not_base64_or_not_text_are_skipped() {
        let vault = ScratchDir::new("journal_bad_sealed");
        let path = write(&vault, Some(KEY));
        let not_text = crypto::seal(&KEY, &aad(START), &[0xFF, 0xFE, 0x00]).unwrap();
        let mut f = OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(f, "{SEALED}!!! not base64").unwrap();
        writeln!(f, "{SEALED}{}", base64::engine::general_purpose::STANDARD.encode(not_text)).unwrap();
        let r = read(&path, Some(&KEY)).unwrap();
        assert_eq!((r.seconds.len(), r.skipped), (3, 2));
    }

    #[test]
    fn devices_come_back_in_the_sessions_order() {
        let vault = ScratchDir::new("journal_device_order");
        let mut j = Journal::create(&vault, START, SubSport::IndoorCycling, fixed(None)).unwrap();
        let strap = Device { kind: DeviceKind::HeartRate, manufacturer: None, name: None, serial: None, radio: None };
        j.append(&Entry::Device(avanti())).unwrap();
        j.append(&Entry::Device(strap.clone())).unwrap();
        j.append(&Entry::Device(Device { name: Some("again".into()), ..avanti() })).unwrap(); // re-identified
        let kinds: Vec<DeviceKind> = read(j.path(), None).unwrap().ride().devices.iter().map(|d| d.kind).collect();
        assert_eq!(kinds, vec![DeviceKind::FitnessEquipment, DeviceKind::HeartRate]);
    }

    #[test]
    fn a_failed_line_costs_only_itself() {
        let vault = ScratchDir::new("journal_fail");
        let fail = Arc::new(Mutex::new(false));
        let f = fail.clone();
        let provider: KeyFn = Box::new(move || if *f.lock().unwrap() { Err("vault locked".into()) } else { Ok(None) });
        let mut j = Journal::create(&vault, START, SubSport::IndoorCycling, provider).unwrap();
        j.append(&second(0, 150)).unwrap();
        *fail.lock().unwrap() = true;
        assert_eq!(j.append(&second(1, 150)), Err("vault locked".into()));
        *fail.lock().unwrap() = false;
        j.append(&second(2, 150)).unwrap();
        // A write torn half-way: the next entry starts on a new line.
        j.torn = true;
        j.file.write_all(b"{\"second\":{\"t\":3").unwrap();
        j.append(&second(4, 150)).unwrap();
        let r = read(j.path(), None).unwrap();
        let ts: Vec<u32> = r.seconds.iter().map(|s| s.sample.t).collect();
        assert_eq!((ts, r.skipped), (vec![0, 2, 4], 1));
    }

    #[test]
    fn a_header_that_cannot_be_written_leaves_no_journal() {
        let vault = ScratchDir::new("journal_headless_create");
        let r = Journal::create(&vault, START, SubSport::IndoorCycling, Box::new(|| Err("locked".into())));
        assert!(r.is_err());
        assert!(pending(&vault).is_empty());
    }

    #[test]
    fn the_same_start_twice_is_refused_not_overwritten() {
        let vault = ScratchDir::new("journal_twice");
        write(&vault, None);
        assert!(Journal::create(&vault, START, SubSport::IndoorCycling, fixed(None)).is_err());
    }

    #[test]
    fn a_file_without_its_start_is_an_error() {
        let vault = ScratchDir::new("journal_headless");
        let path = vault.join("x.journal");
        std::fs::write(&path, "\n{\"lap\":{\"at\":1}}\n").unwrap();
        assert_eq!(read(&path, None).unwrap_err(), ReadError::Unreadable("empty".into()));
        std::fs::write(&path, "garbage\n").unwrap();
        assert!(matches!(read(&path, None), Err(ReadError::Unreadable(e)) if e != "empty"));
        assert!(matches!(read(&vault.join("missing.journal"), None), Err(ReadError::Io(_))));
        assert!(ReadError::Io("x".into()).to_string() == "x" && ReadError::Unreadable("y".into()).to_string().contains('y'));
    }

    #[test]
    fn a_sealed_journal_under_another_name_does_not_read() {
        // The start is in the name and in the sealed lines: renamed, they no
        // longer agree.
        let vault = ScratchDir::new("journal_renamed");
        let path = write(&vault, Some(KEY));
        let moved = path.with_file_name("1.journal");
        std::fs::rename(&path, &moved).unwrap();
        assert!(matches!(read(&moved, Some(&KEY)), Err(ReadError::Unreadable(_))));
        let nameless = path.with_file_name("ride.journal");
        std::fs::rename(&moved, &nameless).unwrap();
        assert!(matches!(read(&nameless, Some(&KEY)), Err(ReadError::Unreadable(_))));
    }

    #[test]
    fn pending_lists_journals_only_oldest_first_and_discard_removes_one() {
        let vault = ScratchDir::new("journal_pending");
        assert!(pending(&vault).is_empty(), "no rides dir yet");
        Journal::create(&vault, 200, SubSport::IndoorCycling, fixed(None)).unwrap();
        Journal::create(&vault, 100, SubSport::VirtualActivity, fixed(None)).unwrap();
        std::fs::write(vault.join(DIR).join("notes.txt"), "x").unwrap();
        let found = pending(&vault);
        let names: Vec<String> = found.iter().filter_map(|p| p.file_name()?.to_str().map(String::from)).collect();
        assert_eq!(names, vec!["100.journal", "200.journal"]);
        discard(&found[0]).unwrap();
        discard(&found[0]).unwrap(); // already gone: fine
        assert_eq!(pending(&vault).len(), 1);
        assert!(discard(&vault.join(DIR)).is_err(), "a directory is not a journal");
    }

    #[test]
    fn a_device_identified_again_replaces_itself() {
        let vault = ScratchDir::new("journal_device");
        let path = write(&vault, None);
        let mut f = OpenOptions::new().append(true).open(&path).unwrap();
        let better = Device { name: Some("Elite Avanti".into()), ..avanti() };
        writeln!(f, "{}", serde_json::to_string(&Entry::Device(better.clone())).unwrap()).unwrap();
        assert_eq!(read(&path, None).unwrap().devices, vec![better]);
    }
}
