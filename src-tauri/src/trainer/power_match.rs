//! Power match: ERG held by the pedals' watts, not the trainer's.
//!
//! In ERG a trainer holds its own reading at the target it is sent, and
//! that reading is off the pedals by an amount that moves with the cadence
//! (the Avanti: −4 W at ≤ 66 rpm, up to +24 W at 79–85 rpm, and not the
//! same from one ride to the next — docs/trainers/elite-avanti.md). So the
//! trainer is sent `target − gap`, the gap being `pedals − trainer`:
//!
//! - **learnt per cadence band**, slowly (a second's gap is noisy), a band
//!   never ridden starting from the latest learnt; read between the two
//!   nearest bands' centres at the smoothed cadence, so riding across a
//!   band edge moves it little, and learnt into both by their share, so
//!   what is read at a cadence comes to the gap there;
//! - **only from calm seconds**: the pedals heard in that very second (not
//!   held over), the trainer's watts close on what it confirmed, the
//!   cadence steady — `CALM_FOR` of them in a row. A cadence step or a new
//!   target overshoots for seconds and says nothing about the gap;
//! - **moved by `JUMP` at once, by less only after `SETTLE_FOR` standing
//!   still**, so ERG does not pump against the trainer's own lag;
//! - **clamped** to `MAX_CORRECTION` and a share of the target (pedals off
//!   their calibration must not move the target much);
//! - **off while the pedals are not heard**: plain ERG (the screen says so).
//!
//! Known limits: the gap is learnt by cadence, not by power (an interval
//! at another power on the same cadence relearns it over its first
//! seconds); and cadences closer than a band share bands, so riding one
//! for long moves what is read at the other (82 rpm after 15 min at 85:
//! ~12 W off for a while; alternating every 20 s: ±4 W, every 2 min: ±0.5).

use super::erg::MAX_TARGET;

/// The most the pedals may move the trainer's target, either way…
pub const MAX_CORRECTION: f32 = 30.0;
/// …and at most this share of the target.
pub const MAX_SHARE: f32 = 0.25;
/// The correction moves at once by this much or more…
const JUMP: f32 = 6.0;
/// …and by this much or more once it has not moved for `SETTLE_FOR`
/// seconds.
const NUDGE: f32 = 1.0;
const SETTLE_FOR: u32 = 15;
/// Cadence bands this wide (rpm) learn apart.
const BAND: u8 = 5;
const BANDS: usize = 256 / BAND as usize + 1;
/// Each calm second moves the learnt gap this share of the way.
const ALPHA: f32 = 0.08;
/// Calm seconds in a row before one is learnt from…
const CALM_FOR: u32 = 5;
/// …the cadence steady meaning within this of the second before.
const CADENCE_STEP: u8 = 5;
/// Each second moves the smoothed cadence this share of the way.
const CADENCE_ALPHA: f32 = 0.25;

/// One closed second, as power match needs it.
#[derive(Debug, Clone, Copy, Default)]
pub struct Heard {
    /// The pedals' watts (held over a little when they miss a second).
    pub pedals: Option<u16>,
    /// The pedals were heard in this very second.
    pub fresh: bool,
    pub trainer: Option<u16>,
    pub cadence: Option<u8>,
    /// The trainer's watts are close on the target it confirmed.
    pub settled: bool,
}

#[derive(Debug)]
pub struct PowerMatch {
    bands: [Option<f32>; BANDS],
    /// The latest gap learnt, in any band.
    latest: Option<f32>,
    /// The correction the trainer is sent, and seconds since it moved.
    applied: f32,
    unmoved: u32,
    /// The pedals' watts were there the last second.
    heard: bool,
    /// The last second's cadence (for steadiness), and the cadence smoothed
    /// (where the gap is read and learnt: a cadence jittering by a couple
    /// of rpm must not move the target).
    cadence: Option<u8>,
    smooth: Option<f32>,
    calm: u32,
}

impl Default for PowerMatch {
    fn default() -> PowerMatch {
        PowerMatch {
            bands: [None; BANDS],
            latest: None,
            applied: 0.0,
            unmoved: 0,
            heard: false,
            cadence: None,
            smooth: None,
            calm: 0,
        }
    }
}

/// The band below `cadence`'s place between band centres, and how far
/// towards the next one it is (0…1).
fn place(cadence: f32) -> (usize, f32) {
    let x = cadence / f32::from(BAND) - 0.5;
    let lo = x.floor();
    // Below the first centre (2.5 rpm) the cast saturates to band 0.
    (lo as usize, x - lo)
}

impl PowerMatch {
    pub fn second(&mut self, h: Heard) {
        self.heard = h.pedals.is_some();
        let steady = matches!((h.cadence, self.cadence), (Some(c), Some(before)) if c.abs_diff(before) <= CADENCE_STEP);
        self.calm = if steady && h.fresh && h.settled { self.calm + 1 } else { 0 };
        self.cadence = h.cadence.or(self.cadence);
        if let Some(c) = h.cadence.map(f32::from) {
            self.smooth = Some(self.smooth.map_or(c, |s| s + (c - s) * CADENCE_ALPHA));
        }
        // Calm implies a cadence this second (steady).
        if let (Some(p), Some(t), Some(at), true) = (h.pedals, h.trainer, self.smooth, self.calm >= CALM_FOR) {
            self.learn(at, f32::from(p) - f32::from(t));
        }
        let wanted = self.smooth.map_or(self.latest.unwrap_or(0.0), |at| self.gap_at(at)).round();
        let by = (wanted - self.applied).abs();
        self.unmoved += 1;
        if by >= JUMP || (by >= NUDGE && self.unmoved >= SETTLE_FOR) {
            self.applied = wanted;
            self.unmoved = 0;
        }
    }

    /// The gap at `cadence`: between the two nearest bands' centres.
    fn gap_at(&self, cadence: f32) -> f32 {
        let (lo, share) = place(cadence);
        let band = |i: usize| self.bands.get(i).copied().flatten();
        match (band(lo), band(lo + 1)) {
            (Some(a), Some(b)) => a + (b - a) * share,
            (Some(v), None) | (None, Some(v)) => v,
            (None, None) => self.latest.unwrap_or(0.0),
        }
    }

    /// One calm second's `gap` at `cadence`: both bands it is read from
    /// move, each by its share (so what is read there comes to the gap,
    /// whatever the neighbour learnt); a band never ridden starts from
    /// what was read.
    fn learn(&mut self, cadence: f32, gap: f32) {
        let (lo, share) = place(cadence);
        let read = self.gap_at(cadence);
        let err = gap - read;
        for (i, weight) in [(lo, 1.0 - share), (lo + 1, share)] {
            if let Some(band) = self.bands.get_mut(i) {
                *band = Some(band.unwrap_or(read) + ALPHA * weight * err);
            }
        }
        self.latest = Some(self.gap_at(cadence));
    }

    /// The pedals' watts were there the last second.
    pub fn heard(&self) -> bool {
        self.heard
    }

    /// What the trainer is sent for `target`: corrected while the pedals
    /// are heard, `target` itself otherwise.
    pub fn trainer_target(&self, target: u16) -> u16 {
        if !self.heard {
            return target;
        }
        let limit = (f32::from(target) * MAX_SHARE).min(MAX_CORRECTION);
        (f32::from(target) - self.applied.clamp(-limit, limit)).round().clamp(1.0, f32::from(MAX_TARGET)) as u16
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn calm(pedals: u16, trainer: u16, cadence: u8) -> Heard {
        Heard { pedals: Some(pedals), fresh: true, trainer: Some(trainer), cadence: Some(cadence), settled: true }
    }

    /// `n` calm seconds.
    fn ride(m: &mut PowerMatch, n: u32, pedals: u16, trainer: u16, cadence: u8) {
        for _ in 0..n {
            m.second(calm(pedals, trainer, cadence));
        }
    }

    #[test]
    fn the_gap_is_learnt_slowly_after_calm_seconds_and_applied_in_steps() {
        let mut m = PowerMatch::default();
        assert_eq!(m.trainer_target(200), 200, "nothing heard: plain ERG");
        // The first second has no cadence before it, then 5 calm in a row.
        let mut targets = Vec::new();
        for _ in 0..5 {
            m.second(calm(215, 200, 85));
            targets.push(m.trainer_target(200));
        }
        assert_eq!(m.latest, None, "not calm long enough");
        m.second(calm(215, 200, 85));
        targets.push(m.trainer_target(200));
        // 85 rpm is half-way between two centres: each band takes half.
        let first = m.latest.expect("learnt");
        assert!((first - 0.6).abs() < 1e-4, "15 W × 0.08 × ½, from nothing: {first}");
        for _ in 0..120 {
            m.second(calm(215, 200, 85));
            targets.push(m.trainer_target(200));
        }
        assert_eq!(*targets.last().unwrap(), 185, "{targets:?}");
        assert!(targets.windows(2).all(|w| w[1] <= w[0]), "one way, no pumping: {targets:?}");
        // A move under JUMP only on the SETTLE_FOR-th second standing still.
        for (i, w) in targets.windows(2).enumerate() {
            let by = w[0] - w[1];
            let still = targets[..=i].iter().rev().take_while(|&&t| t == w[0]).count() as u32 + 1;
            assert!(by == 0 || by >= JUMP as u16 || (by >= NUDGE as u16 && still >= SETTLE_FOR), "at {i}: {targets:?}");
        }
    }

    #[test]
    fn only_calm_seconds_teach() {
        let quiet = |m: &PowerMatch| m.latest;
        let mut m = PowerMatch::default();
        // A cadence step of more than CADENCE_STEP breaks the calm.
        for c in [80, 86, 80, 86, 80, 86, 80, 86] {
            m.second(calm(250, 200, c));
        }
        assert_eq!(quiet(&m), None);
        // Within CADENCE_STEP is steady.
        for c in [80, 85, 80, 85, 80, 85, 80] {
            m.second(calm(210, 200, c));
        }
        assert!(quiet(&m).is_some());
        let before = quiet(&m);
        let mut m2 = PowerMatch::default();
        m2.second(calm(210, 200, 85)); // no cadence before it: not calm
        for h in [
            Heard { settled: false, ..calm(300, 200, 85) },
            Heard { fresh: false, ..calm(300, 200, 85) },
            Heard { cadence: None, ..calm(300, 200, 85) },
        ] {
            ride(&mut m2, CALM_FOR - 1, 210, 200, 85);
            m2.second(h);
        }
        assert_eq!(quiet(&m2), None, "unsettled, held over, or no cadence: the run starts again");
        ride(&mut m2, CALM_FOR + 1, 210, 200, 85);
        assert!(quiet(&m2).is_some() && before.is_some());
    }

    #[test]
    fn each_cadence_keeps_its_own_gap() {
        let mut m = PowerMatch::default();
        ride(&mut m, 200, 196, 200, 62); // −4 W at 62 rpm
        assert_eq!(m.trainer_target(200), 204);
        ride(&mut m, 200, 216, 200, 72); // +16 W at 72 rpm
        assert_eq!(m.trainer_target(200), 184);
        // Back to 62 rpm: matched as the cadence settles, nothing learnt.
        for _ in 0..40 {
            m.second(Heard { settled: false, ..calm(196, 200, 62) });
        }
        assert_eq!(m.trainer_target(200), 204);
    }

    #[test]
    fn the_gap_is_read_between_band_centres() {
        let mut m = PowerMatch::default();
        m.bands[12] = Some(-4.0); // 60–64, centre 62.5
        m.bands[13] = Some(16.0); // 65–69, centre 67.5
        assert!((m.gap_at(66.0) - 10.0).abs() < 1e-3, "70 % of the way: {}", m.gap_at(66.0));
        assert!((m.gap_at(62.5) + 4.0).abs() < 1e-3);
        // One of the two known: that one.
        assert!((m.gap_at(69.0) - 16.0).abs() < 1e-3, "70–74 unknown");
        assert!((m.gap_at(61.0) + 4.0).abs() < 1e-3, "55–59 unknown");
    }

    /// The Avanti's gap steps between bands (+6 W to 84 rpm, +18 W from
    /// 85): ridden at 85 rpm after 82, it still comes to +18 there.
    #[test]
    fn a_gap_that_steps_is_matched_where_it_is_ridden() {
        let mut m = PowerMatch::default();
        ride(&mut m, 400, 206, 200, 82);
        assert_eq!(m.trainer_target(200), 194);
        ride(&mut m, 900, 218, 200, 85);
        assert_eq!(m.trainer_target(200), 182);
    }

    /// A rider's cadence jitters every second (83–88 rpm) across the
    /// Avanti's gap step (+18 W to 86 rpm, +10 W above): the target the
    /// trainer holds (pedals = it + the gap there) moves seldom.
    #[test]
    fn a_cadence_jittering_across_a_gap_step_moves_the_target_seldom() {
        let gap = |c: u8| if c <= 86 { 18 } else { 10 };
        let jitter = [83u8, 85, 87, 84, 86, 88, 85, 83, 87, 86];
        let mut m = PowerMatch::default();
        let mut last = m.trainer_target(200);
        let mut moves = 0;
        for i in 0..900 {
            let c = jitter[i % jitter.len()];
            let held = m.trainer_target(200);
            m.second(calm(held + gap(c), held, c));
            let next = m.trainer_target(200);
            if i >= 300 && next != last {
                moves += 1;
            }
            last = next;
        }
        // Smoothed: 40. Read at each second's own cadence: 240.
        assert!(moves <= 50, "{moves} moves in 10 minutes");
    }

    #[test]
    fn a_new_band_starts_from_the_latest_gap() {
        let mut m = PowerMatch::default();
        ride(&mut m, 200, 220, 200, 85);
        let latest = m.latest.unwrap();
        assert!((latest - 20.0).abs() < 0.5, "{latest}");
        ride(&mut m, 5, 230, 200, 96); // the step: not calm yet
        assert_eq!(m.latest, Some(latest));
        ride(&mut m, 1, 230, 200, 96);
        let first = m.latest.unwrap();
        assert!(first > latest && first < latest + 1.0, "from {latest}, not from 0: {first}");
    }

    #[test]
    fn a_correction_moves_at_once_by_a_jump_by_less_only_after_standing_still() {
        let mut m = PowerMatch::default();
        // No cadence ever: the latest gap.
        let still = Heard { cadence: None, settled: false, ..calm(205, 200, 0) };
        m.latest = Some(5.0);
        m.second(still);
        assert_eq!(m.trainer_target(200), 200, "5 W: not at once");
        m.latest = Some(6.0);
        m.second(still);
        assert_eq!(m.trainer_target(200), 194, "6 W: at once");
        m.latest = Some(7.0);
        for _ in 0..14 {
            m.second(still);
        }
        assert_eq!(m.trainer_target(200), 194, "a watt waits 15 s");
        m.second(still);
        assert_eq!(m.trainer_target(200), 193);
        // Standing still on it a while: a watt more goes at once.
        for _ in 0..15 {
            m.second(still);
        }
        m.latest = Some(8.0);
        m.second(still);
        assert_eq!(m.trainer_target(200), 192);
    }

    #[test]
    fn the_correction_is_clamped_and_the_pedals_lost_turn_it_off() {
        let mut m = PowerMatch::default();
        ride(&mut m, 300, 300, 200, 90);
        assert_eq!(m.trainer_target(200), 170, "at most MAX_CORRECTION");
        assert_eq!(m.trainer_target(40), 30, "at most a quarter of the target");
        assert_eq!(m.trainer_target(1), 1, "never 0 W: that would be no target");
        let mut under = PowerMatch::default();
        ride(&mut under, 300, 100, 200, 90);
        assert_eq!(under.trainer_target(1990), MAX_TARGET, "up, but never past MAX_TARGET");
        m.second(Heard { pedals: None, fresh: false, ..calm(0, 200, 90) });
        assert!(!m.heard());
        assert_eq!(m.trainer_target(200), 200, "plain ERG");
        m.second(calm(300, 200, 90));
        assert!(m.heard());
        assert_eq!(m.trainer_target(200), 170, "back, with what was learnt");
        // A second without a cadence keeps the band of the last one.
        m.second(Heard { cadence: None, ..calm(300, 200, 90) });
        assert_eq!(m.cadence, Some(90));
    }
}
