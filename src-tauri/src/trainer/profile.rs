//! Per-model trainer quirks, as data.
//!
//! Trainers share protocols (BLE FTMS, ANT+ FE-C) but not behaviour: one
//! lets go of ERG unless the target is resent, one fails FTMS `start`
//! after every reconnect, one keeps claiming "at target" after it let go.
//! The engine, the controllers and the links never ask which model they
//! drive; they read the profile. A model nobody characterised gets
//! [`GENERIC`], cautious wherever a wrong guess would cost the rider.
//!
//! Adding a model: characterise it on the real trainer, add a profile to
//! [`PROFILES`] with the measurement behind each value, and a
//! `FakeConfig` preset with a test that reproduces its quirk.

use std::time::Duration;

use super::link::{Identity, Transport};

/// One way to recognise a model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Match {
    /// ANT+ common page 0x50: manufacturer id and model number.
    Ant { manufacturer: u16, model: u16 },
    /// BLE advertised name starting with this, ignoring ASCII case and
    /// surrounding spaces.
    BleNamePrefix(&'static str),
}

impl Match {
    fn matches(&self, id: &Identity) -> bool {
        match *self {
            Match::Ant { manufacturer, model } => {
                id.transport == Transport::Ant
                    && id.ant_manufacturer == Some(manufacturer)
                    && id.ant_model == Some(model)
            }
            Match::BleNamePrefix(prefix) => {
                id.transport == Transport::Ble
                    && id.name.as_deref().is_some_and(|n| {
                        // `get` is None past the end or inside a character.
                        n.trim().get(..prefix.len()).is_some_and(|head| head.eq_ignore_ascii_case(prefix))
                    })
            }
        }
    }
}

/// Whether FTMS `start` must succeed after a reconnect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartAfterReconnect {
    /// A failed start means control was not regained.
    MustSucceed,
    /// The trainer answers `start` with a failure but takes the target
    /// anyway: go on.
    MayFail,
}

#[derive(Debug, PartialEq)]
pub struct TrainerProfile {
    /// Stable id, e.g. "elite-avanti".
    pub key: &'static str,
    /// Shown to the user.
    pub name: &'static str,
    /// Any one of these picks the profile. Empty only for [`GENERIC`].
    pub matches: &'static [Match],
    /// Resend the ERG target this often while it is unchanged.
    pub erg_resend: Duration,
    /// Whether the trainer's "at target" flag and target echo can be
    /// believed. When false, ERG is judged by the watts alone.
    pub trust_target_flags: bool,
    pub ftms_start_after_reconnect: StartAfterReconnect,
    /// After this long without an answer a control counts as timed out and
    /// the controller moves on (resends, retries). Never a wait: the
    /// readings keep flowing meanwhile.
    pub control_answer_timeout: Duration,
    /// Drops are part of riding this trainer: the screen says
    /// "reconnecting…" quietly instead of raising an alarm.
    pub link_drops_routine: bool,
    /// How long to keep trying to release the brake at the end.
    pub release_budget: Duration,
}

/// The profile for a model nobody characterised. Resends well inside the
/// shortest control timeout seen (~18 s), judges ERG by the watts, never
/// treats a failed FTMS `start` as fatal and never waits on an answer.
/// Drops are reported quietly: so far every trainer tried had them.
pub static GENERIC: TrainerProfile = TrainerProfile {
    key: "generic",
    name: "Smart trainer",
    matches: &[],
    erg_resend: Duration::from_secs(2),
    trust_target_flags: false,
    ftms_start_after_reconnect: StartAfterReconnect::MayFail,
    control_answer_timeout: Duration::from_secs(3),
    link_drops_routine: true,
    release_budget: Duration::from_secs(15),
};

/// Elite Avanti (COG), firmware 3.7, characterised 2026-10-20 … 11-11 with
/// the ftms-spike tool. Its values equal [`GENERIC`]'s: the generic profile
/// was written from what this trainer taught.
pub static ELITE_AVANTI: TrainerProfile = TrainerProfile {
    key: "elite-avanti",
    name: "Elite Avanti",
    matches: &[Match::Ant { manufacturer: 86, model: 45 }, Match::BleNamePrefix("AVANTI")],
    // Let go ~18 s after the last target; held with every 2 s, and gaps of
    // 7–10.5 s between acknowledged targets did not let go.
    erg_resend: Duration::from_secs(2),
    // The 0x19 flag said "at target" and 0x31 echoed 150 W while the brake
    // followed the wheel at 200–440 W.
    trust_target_flags: false,
    // FTMS start failed after every reconnect; set target power worked.
    ftms_start_after_reconnect: StartAfterReconnect::MayFail,
    // FE-C acks came in 0.25–2 s. FTMS answers after a reconnect often took
    // longer than 3 s: rather than wait for them, time out and resend.
    control_answer_timeout: Duration::from_secs(3),
    // BLE dropped every 2–5 min, ANT+ searched for 4–8 s now and then.
    link_drops_routine: true,
    // The 0 % release needed a second try; three tries once failed.
    release_budget: Duration::from_secs(15),
};

/// Every characterised model, searched in order.
pub static PROFILES: &[&TrainerProfile] = &[&ELITE_AVANTI];

/// The profile for this device; [`GENERIC`] when no model matches.
pub fn profile_for(identity: &Identity) -> &'static TrainerProfile {
    PROFILES
        .iter()
        .copied()
        .find(|p| p.matches.iter().any(|m| m.matches(identity)))
        .unwrap_or(&GENERIC)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ant(manufacturer: u16, model: u16) -> Identity {
        Identity { ant_manufacturer: Some(manufacturer), ant_model: Some(model), ..Identity::new(Transport::Ant) }
    }

    fn ble(name: &str) -> Identity {
        Identity { name: Some(name.to_string()), ..Identity::new(Transport::Ble) }
    }

    #[test]
    fn the_avanti_is_recognised_over_both_radios() {
        assert_eq!(profile_for(&ant(86, 45)).key, "elite-avanti");
        assert_eq!(profile_for(&ble("AVANTI COG")).key, "elite-avanti");
        assert_eq!(profile_for(&ble("  avanti cog")).key, "elite-avanti", "case and padding do not matter");
    }

    #[test]
    fn anything_else_gets_the_generic_profile() {
        assert_eq!(profile_for(&ant(86, 46)).key, "generic", "another Elite model");
        assert_eq!(profile_for(&ant(32, 45)).key, "generic", "same model number, another maker");
        assert_eq!(profile_for(&ble("KICKR CORE 1234")).key, "generic");
        assert_eq!(profile_for(&ble("AVA")).key, "generic", "a name shorter than the prefix");
        // Byte 6 falls inside "Ä": slicing there would panic.
        assert_eq!(profile_for(&ble("AVANTÄ")).key, "generic", "a cut inside a character is no match");
        assert_eq!(profile_for(&Identity::new(Transport::Ble)).key, "generic", "no name yet");
        assert_eq!(profile_for(&Identity::new(Transport::Ant)).key, "generic", "no 0x50 page yet");
    }

    #[test]
    fn a_match_only_counts_on_its_own_transport() {
        let odd_ble = Identity { ant_manufacturer: Some(86), ant_model: Some(45), ..Identity::new(Transport::Ble) };
        let odd_ant = Identity { name: Some("AVANTI".into()), ..Identity::new(Transport::Ant) };
        assert_eq!(profile_for(&odd_ble).key, "generic");
        assert_eq!(profile_for(&odd_ant).key, "generic");
    }

    /// Each value is a measurement (see the comments on `ELITE_AVANTI`);
    /// changing one should mean a new measurement, not a slip.
    #[test]
    fn the_avanti_profile_says_what_was_measured() {
        let p = &ELITE_AVANTI;
        assert_eq!(p.erg_resend, Duration::from_secs(2));
        assert!(!p.trust_target_flags, "its flags lie");
        assert_eq!(p.ftms_start_after_reconnect, StartAfterReconnect::MayFail);
        assert_eq!(p.control_answer_timeout, Duration::from_secs(3));
        assert!(p.link_drops_routine);
        assert_eq!(p.release_budget, Duration::from_secs(15));
    }

    #[test]
    fn the_generic_profile_is_the_cautious_one() {
        let p = &GENERIC;
        assert!(p.erg_resend <= Duration::from_secs(5), "well inside an ~18 s control timeout");
        assert!(!p.trust_target_flags, "judge by watts");
        assert_eq!(p.ftms_start_after_reconnect, StartAfterReconnect::MayFail);
        assert!(p.control_answer_timeout <= Duration::from_secs(3), "never hang on an answer");
        assert!(p.link_drops_routine);
        assert!(p.release_budget >= Duration::from_secs(15));
        assert!(p.matches.is_empty(), "picked only when nothing else is");
    }

    #[test]
    fn profiles_are_well_formed() {
        let all: Vec<&TrainerProfile> = PROFILES.iter().copied().chain([&GENERIC]).collect();
        let mut keys: Vec<&str> = all.iter().map(|p| p.key).collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), all.len(), "duplicate profile key");
        for p in &all {
            assert!(p.erg_resend > Duration::ZERO && p.erg_resend <= Duration::from_secs(5), "{}: resend", p.key);
            assert!(p.control_answer_timeout > Duration::ZERO, "{}: answer timeout", p.key);
            assert!(p.release_budget >= p.control_answer_timeout, "{}: release budget", p.key);
        }
        for p in PROFILES {
            assert!(!p.matches.is_empty(), "{} can never be picked", p.key);
        }
        // No device can be claimed by two profiles.
        let all_matches: Vec<(&str, Match)> =
            PROFILES.iter().flat_map(|p| p.matches.iter().map(move |m| (p.key, *m))).collect();
        for (i, (ka, a)) in all_matches.iter().enumerate() {
            for (kb, b) in &all_matches[i + 1..] {
                assert!(!overlap(a, b), "{ka} and {kb} both claim {a:?} / {b:?}");
            }
        }
    }

    /// Whether two matches could pick the same device: equal ids, or BLE
    /// prefixes one of which starts the other.
    fn overlap(a: &Match, b: &Match) -> bool {
        match (a, b) {
            (Match::BleNamePrefix(x), Match::BleNamePrefix(y)) => {
                let (x, y) = (x.to_ascii_lowercase(), y.to_ascii_lowercase());
                x.starts_with(&y) || y.starts_with(&x)
            }
            _ => a == b,
        }
    }

    #[test]
    fn overlapping_matches_are_told_apart_from_distinct_ones() {
        use Match::{Ant, BleNamePrefix};
        assert!(overlap(&BleNamePrefix("AVANTI"), &BleNamePrefix("avanti pro")), "a prefix of the other");
        assert!(overlap(&BleNamePrefix("Avanti Pro"), &BleNamePrefix("AVANTI")), "either way round");
        assert!(!overlap(&BleNamePrefix("AVANTI"), &BleNamePrefix("KICKR")));
        assert!(overlap(&Ant { manufacturer: 86, model: 45 }, &Ant { manufacturer: 86, model: 45 }));
        assert!(!overlap(&Ant { manufacturer: 86, model: 45 }, &Ant { manufacturer: 86, model: 46 }));
        assert!(!overlap(&Ant { manufacturer: 86, model: 45 }, &BleNamePrefix("AVANTI")), "different radios");
    }
}
