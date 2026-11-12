//! Dev utility (macOS): ride a real trainer through the app's BLE link —
//! the manual check of #271. Prints a line a second, every drop and
//! return, and a summary; with `--erg` it holds the target the way the
//! trainer's profile says (resend interval) and lets go at the end.
//!
//!   cargo run --manifest-path src-tauri/Cargo.toml --example ble_ride -- \
//!     [--trainer NAME] [--pedals NAME [--pedals NAME]] [--erg WATTS] [--secs N]
//!
//! An Assioma Duo is two pedals over BLE, each its own leg: name both
//! (`--pedals ASSIOMA55430L --pedals ASSIOMA55430R`) and their power is summed.
//!
//! Run it from your own terminal, in a debug build: macOS asks the terminal
//! for Bluetooth, and the debug build carries the app's Info.plist (the
//! Bluetooth usage text) without which macOS kills the process. Zwift and
//! other trainer apps closed: a trainer takes one BLE controller.

#[cfg(target_os = "macos")]
fn main() {
    macos::main();
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("ble_ride runs on macOS only");
}

#[cfg(target_os = "macos")]
mod macos {
    use std::time::{Duration, Instant};

    use syzify_lib::trainer::ble::driver::BleLink;
    use syzify_lib::trainer::ble::machine::{Role, Target};
    use syzify_lib::trainer::link::{Control, ControlOutcome, LinkEvent, Reading, SensorLink, TrainerLink};
    use syzify_lib::trainer::pedals::PedalPair;
    use syzify_lib::trainer::profile::{profile_for, TrainerProfile, GENERIC};

    struct Args {
        trainer: Option<String>,
        /// One name, or two (an Assioma Duo's L and R: summed).
        pedals: Vec<String>,
        erg: Option<u16>,
        secs: u64,
    }

    fn args() -> Args {
        let mut a = Args { trainer: None, pedals: Vec::new(), erg: None, secs: 300 };
        let mut it = std::env::args().skip(1);
        while let Some(flag) = it.next() {
            let value = it.next();
            match flag.as_str() {
                "--trainer" => a.trainer = value,
                "--pedals" => a.pedals.extend(value),
                "--erg" => a.erg = value.and_then(|v| v.parse().ok()),
                "--secs" => a.secs = value.and_then(|v| v.parse().ok()).unwrap_or(a.secs),
                other => eprintln!("ignored argument {other}"),
            }
        }
        a
    }

    #[derive(Default)]
    struct Summary {
        lost: u32,
        outages: Vec<f32>,
        acked: u32,
        not_acked: u32,
        timed_out: u32,
        refused: u32,
        trainer_watts: Vec<f32>,
        pedal_watts: Vec<f32>,
    }

    fn line(r: &Option<Reading>) -> String {
        match r {
            Some(r) => format!(
                "{:>4} W {:>3} rpm {:>5}",
                r.watts.map(|w| w.to_string()).unwrap_or("—".into()),
                r.cadence_rpm.map(|c| c.to_string()).unwrap_or("—".into()),
                r.speed_kmh.map(|s| format!("{s:.1}")).unwrap_or_default()
            ),
            None => "—".into(),
        }
    }

    pub fn main() {
        let a = args();
        let epoch = Instant::now();
        let mut trainer = BleLink::start(Target { name_contains: a.trainer.clone(), ..Target::any(Role::Trainer) }, epoch);
        let side = |name: &String| -> Box<dyn SensorLink> {
            Box::new(BleLink::start(Target { name_contains: Some(name.clone()), ..Target::any(Role::Pedals) }, epoch))
        };
        if a.pedals.len() > 2 {
            eprintln!("only two --pedals count (left, then right); the rest are ignored");
        }
        let mut pedals: Option<Box<dyn SensorLink>> = match a.pedals.as_slice() {
            [] => None,
            [one] => Some(side(one)),
            [left, right, ..] => Some(Box::new(PedalPair::new(side(left), side(right)))),
        };
        let mut profile: &'static TrainerProfile = &GENERIC;
        let mut connected = false;
        let mut down = false;
        let mut next_send = Duration::from_secs(10);
        let mut last_trainer: Option<Reading> = None;
        let mut last_pedals: Option<Reading> = None;
        let mut sum = Summary::default();
        println!("looking for a trainer{} — pedal so it wakes up…", a.trainer.as_ref().map(|n| format!(" named like {n:?}")).unwrap_or_default());

        while epoch.elapsed() < Duration::from_secs(a.secs) {
            std::thread::sleep(Duration::from_secs(1));
            let now = epoch.elapsed();
            let t = now.as_secs_f32();
            for event in trainer.poll(now) {
                match event {
                    LinkEvent::Connected { .. } => {
                        connected = true;
                        println!("{t:>6.0} s  trainer connected");
                    }
                    LinkEvent::Identified { identity, .. } => {
                        profile = profile_for(&identity);
                        trainer.use_profile(profile);
                        println!(
                            "{t:>6.0} s  {} ({}, {}, firmware {}) → profile {}",
                            identity.name.as_deref().unwrap_or("?"),
                            identity.manufacturer.as_deref().unwrap_or("?"),
                            identity.model.as_deref().unwrap_or("?"),
                            identity.firmware.as_deref().unwrap_or("?"),
                            profile.key
                        );
                    }
                    LinkEvent::Lost { at } => {
                        down = true;
                        sum.lost += 1;
                        println!("{t:>6.0} s  trainer LOST at {:.1} s — scanning for it", at.as_secs_f32());
                    }
                    LinkEvent::Back { outage, .. } => {
                        down = false;
                        next_send = now; // the target goes again at once
                        sum.outages.push(outage.as_secs_f32());
                        println!("{t:>6.0} s  trainer back, {:.1} s without it", outage.as_secs_f32());
                    }
                    LinkEvent::Reading(r) => {
                        if let Some(w) = r.watts.filter(|&w| w > 30) {
                            sum.trainer_watts.push(w as f32);
                        }
                        last_trainer = Some(r);
                    }
                    LinkEvent::Control { control, outcome, .. } => {
                        match outcome {
                            ControlOutcome::Acknowledged => sum.acked += 1,
                            ControlOutcome::NotAcknowledged => sum.not_acked += 1,
                            ControlOutcome::TimedOut => sum.timed_out += 1,
                            ControlOutcome::Refused => sum.refused += 1,
                        }
                        if outcome != ControlOutcome::Acknowledged {
                            println!("{t:>6.0} s  {control:?}: {outcome:?}");
                        }
                    }
                    LinkEvent::Failed { reason, .. } => {
                        println!("{t:>6.0} s  the link failed: {reason:?}");
                        return;
                    }
                }
            }
            if let Some(p) = pedals.as_mut() {
                for event in p.poll(now) {
                    match event {
                        LinkEvent::Reading(r) => {
                            if let Some(w) = r.watts.filter(|&w| w > 30) {
                                sum.pedal_watts.push(w as f32);
                            }
                            last_pedals = Some(r);
                        }
                        LinkEvent::Failed { reason, .. } => println!("{t:>6.0} s  pedals failed: {reason:?}"),
                        other => println!("{t:>6.0} s  pedals: {other:?}"),
                    }
                }
            }
            if let Some(w) = a.erg {
                if connected && !down && now >= next_send {
                    trainer.send(Control::TargetPower(w));
                    next_send = now + profile.erg_resend;
                }
            }
            println!(
                "{t:>6.0} s  T {}{}{}",
                line(&last_trainer),
                pedals.as_ref().map(|_| format!("  | P {}", line(&last_pedals))).unwrap_or_default(),
                if down { "  [lost]" } else { "" }
            );
        }

        if a.erg.is_some() && connected {
            trainer.send(Control::Release);
            let until = epoch.elapsed() + profile.release_budget;
            'release: while epoch.elapsed() < until {
                std::thread::sleep(Duration::from_millis(250));
                for event in trainer.poll(epoch.elapsed()) {
                    if let LinkEvent::Control { control: Control::Release, outcome, .. } = event {
                        println!("release: {outcome:?}");
                        break 'release;
                    }
                }
            }
        }

        let mean = |v: &[f32]| if v.is_empty() { 0.0 } else { v.iter().sum::<f32>() / v.len() as f32 };
        println!("\n--- {} s ---", epoch.elapsed().as_secs());
        let outages: Vec<String> = sum.outages.iter().map(|o| format!("{o:.1}")).collect();
        println!("drops: {}; back after {} s", sum.lost, outages.join(" / "));
        println!(
            "controls: {} acknowledged, {} not acknowledged, {} timed out, {} refused",
            sum.acked, sum.not_acked, sum.timed_out, sum.refused
        );
        println!("trainer: mean {:.0} W over {} readings above 30 W", mean(&sum.trainer_watts), sum.trainer_watts.len());
        if pedals.is_some() {
            println!("pedals: mean {:.0} W over {} readings above 30 W", mean(&sum.pedal_watts), sum.pedal_watts.len());
        }
    }
}
