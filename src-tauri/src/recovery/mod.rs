//! The recovery index (ADR 0002) — pure: it takes the stored monitoring
//! days and the daily training load and returns the dashboard card. No
//! SQL, no clock: the caller passes "today".
//!
//! Per night (the morning's date D):
//! - valid = ≥120 heart-rate samples between 00:00 and 07:00;
//! - baseline = median of the last 7 valid nights within 90 days before D
//!   (at least 3), else no index yet;
//! - awake = median > baseline + 25 (or > 90 without a baseline): the watch
//!   was worn but nobody slept — skipped, never part of the baseline;
//! - hr = clamp(100 − 8·(median − baseline));
//!   stress = clamp(100 − 2.5·max(0, nightStress − 10));
//!   load = clamp(100 − 40·max(0, hrTSS(D−1) / max(CTL(D−1), 30) − 1));
//! - index = (0.5·hr + 0.3·stress + 0.2·load) / Σ weights present.
//!
//! Bands: ≥80 intervals OK, 60–79 easy day, <60 rest. Validated on the
//! user's July 2026 nights: 23–25.07 → 90 / 94 / 86, 29.07 → 58.

use std::collections::BTreeMap;

use chrono::{Duration, NaiveDate};

use crate::models::monitoring::MonitoringDay;
use crate::models::recovery::{
    HrComponent, LoadComponent, RecoveryCard, RecoveryPoint, StressComponent,
};

pub const MIN_NIGHT_SAMPLES: i64 = 120;
pub const BASELINE_WINDOW_DAYS: i64 = 90;
pub const BASELINE_NIGHTS: usize = 7;
pub const BASELINE_MIN_NIGHTS: usize = 3;
pub const AWAKE_OVER_BASELINE: f64 = 25.0;
pub const AWAKE_ABSOLUTE: f64 = 90.0;
pub const HISTORY_DAYS: i64 = 28;
const WARN_DELTA: f64 = 8.0;
const W_HR: f64 = 0.5;
const W_STRESS: f64 = 0.3;
const W_LOAD: f64 = 0.2;
/// Days of the chronic-load exponential average.
const CTL_DAYS: f64 = 42.0;

/// Chronic training load per day: a 42-day exponentially weighted average
/// of daily hrTSS, walked from the first day with load through `until`
/// (days without activities count as zero, so the average decays). The
/// value for day D already includes 1/42 of D's own load — that is the
/// CTL the ADR's load formula divides by, not the "before the day" one.
pub fn ctl_series(daily_tss: &BTreeMap<String, f64>, until: NaiveDate) -> BTreeMap<String, f64> {
    let mut out = BTreeMap::new();
    let Some(first) = daily_tss.keys().filter_map(|d| parse(d)).next() else {
        return out;
    };
    let mut ctl = 0.0;
    let mut day = first;
    while day <= until {
        let key = day.to_string();
        let tss = daily_tss.get(&key).copied().unwrap_or(0.0);
        ctl += (tss - ctl) / CTL_DAYS;
        out.insert(key, ctl);
        day += Duration::days(1);
    }
    out
}

fn parse(date: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()
}

fn clamp(x: f64) -> f64 {
    x.clamp(0.0, 100.0)
}

fn median(sorted: &[f64]) -> f64 {
    let n = sorted.len();
    if n % 2 == 1 {
        sorted[n / 2]
    } else {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    }
}

/// One night's outcome.
#[derive(Debug, Clone, PartialEq)]
pub struct NightIndex {
    pub date: NaiveDate,
    pub index: i64,
    pub hr: HrComponent,
    pub stress: Option<StressComponent>,
    pub load: Option<LoadComponent>,
}

/// The index of every night that has one, oldest first, plus the count of
/// valid nights seen. `days` may be in any order; only `night_samples`,
/// `night_hr_median` and `night_stress_avg` are read.
pub fn night_indices(
    days: &[MonitoringDay],
    daily_tss: &BTreeMap<String, f64>,
    ctl: &BTreeMap<String, f64>,
) -> (Vec<NightIndex>, Vec<NaiveDate>) {
    let mut nights: Vec<(NaiveDate, f64, Option<f64>)> = days
        .iter()
        .filter(|d| d.night_samples >= MIN_NIGHT_SAMPLES)
        .filter_map(|d| Some((parse(&d.date)?, d.night_hr_median?, d.night_stress_avg)))
        .collect();
    nights.sort_by_key(|n| n.0);

    let mut history: Vec<(NaiveDate, f64)> = Vec::new(); // valid, asleep nights
    let mut out = Vec::new();
    for (date, med, stress) in nights {
        let recent: Vec<f64> = history
            .iter()
            .filter(|(d, _)| date - *d <= Duration::days(BASELINE_WINDOW_DAYS))
            .map(|(_, m)| *m)
            .rev()
            .take(BASELINE_NIGHTS)
            .collect();
        let baseline = if recent.len() >= BASELINE_MIN_NIGHTS {
            let mut sorted = recent.clone();
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            Some(median(&sorted))
        } else {
            None
        };
        let awake = match baseline {
            Some(b) => med > b + AWAKE_OVER_BASELINE,
            None => med > AWAKE_ABSOLUTE,
        };
        if awake {
            continue;
        }
        history.push((date, med));
        let Some(base) = baseline else { continue };

        let hr = HrComponent {
            night_median: med,
            baseline: base,
            delta: med - base,
            score: clamp(100.0 - 8.0 * (med - base)),
        };
        let stress = stress.map(|s| StressComponent {
            night_avg: s,
            score: clamp(100.0 - 2.5 * (s - 10.0).max(0.0)),
        });
        let yesterday = (date - Duration::days(1)).to_string();
        let load = ctl.get(&yesterday).map(|&c| {
            let tss = daily_tss.get(&yesterday).copied().unwrap_or(0.0);
            LoadComponent {
                tss_yesterday: tss,
                ctl: c,
                score: clamp(100.0 - 40.0 * (tss / c.max(30.0) - 1.0).max(0.0)),
            }
        });

        let mut num = W_HR * hr.score;
        let mut den = W_HR;
        if let Some(s) = &stress {
            num += W_STRESS * s.score;
            den += W_STRESS;
        }
        if let Some(l) = &load {
            num += W_LOAD * l.score;
            den += W_LOAD;
        }
        out.push(NightIndex { date, index: (num / den).round() as i64, hr, stress, load });
    }
    let valid: Vec<NaiveDate> = history.into_iter().map(|(d, _)| d).collect();
    (out, valid)
}

pub fn band_of(index: i64) -> (&'static str, &'static str) {
    if index >= 80 {
        ("intervals_ok", "Intervals are fine today")
    } else if index >= 60 {
        ("easy_day", "Keep it easy — Z2 only")
    } else {
        ("rest", "Rest day")
    }
}

/// The card for `today`: the last computed night with its age, and the
/// sparse 28-day history.
pub fn card(
    days: &[MonitoringDay],
    daily_tss: &BTreeMap<String, f64>,
    today: NaiveDate,
) -> RecoveryCard {
    let ctl = ctl_series(daily_tss, today);
    let (indices, valid) = night_indices(days, daily_tss, &ctl);
    let within = |d: NaiveDate, days: i64| (0..=days).contains(&(today - d).num_days());
    let recorded_90d = valid.iter().filter(|d| within(**d, BASELINE_WINDOW_DAYS)).count() as i64;
    let history: Vec<RecoveryPoint> = indices
        .iter()
        .filter(|n| within(n.date, HISTORY_DAYS))
        .map(|n| RecoveryPoint { date: n.date.to_string(), index: n.index })
        .collect();
    // Always from the nights inside the window: an old index does not mean
    // the NEXT night will get one — its baseline needs recent nights too.
    let nights_needed = (BASELINE_MIN_NIGHTS as i64 - recorded_90d).max(0);
    let Some(last) = indices.last() else {
        return RecoveryCard {
            computed_for: today.to_string(),
            date: None,
            age_days: None,
            index: None,
            band: None,
            advice: None,
            hr: None,
            stress: None,
            load: None,
            warning: None,
            nights_recorded_90d: recorded_90d,
            nights_needed,
            history,
        };
    };
    let (band, advice) = band_of(last.index);
    RecoveryCard {
        computed_for: today.to_string(),
        date: Some(last.date.to_string()),
        age_days: Some((today - last.date).num_days()),
        index: Some(last.index),
        band: Some(band.to_string()),
        advice: Some(advice.to_string()),
        hr: Some(last.hr.clone()),
        stress: last.stress.clone(),
        load: last.load.clone(),
        warning: (last.hr.delta >= WARN_DELTA).then(|| "hr_above_baseline".to_string()),
        nights_recorded_90d: recorded_90d,
        nights_needed,
        history,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(date: &str, samples: i64, median: Option<f64>, stress: Option<f64>) -> MonitoringDay {
        MonitoringDay {
            date: date.to_string(),
            tz_offset_s: 10_800,
            tz_confirmed: true,
            night_samples: samples,
            night_hr_min: None,
            night_hr_p10: None,
            night_hr_median: median,
            night_stress_avg: stress,
            day_stress_avg: None,
            resp_night_avg: None,
            spo2_night_avg: None,
            rhr_garmin: None,
            rhr_garmin_7d: None,
            steps: None,
            distance_m: None,
            active_calories: None,
            active_time_s: None,
            moderate_min: None,
            vigorous_min: None,
            computed_at: Some("x".into()),
        }
    }

    fn d(s: &str) -> NaiveDate {
        parse(s).unwrap()
    }

    /// The user's July 2026 nights and the loads around them, as the
    /// prototype measured them (hrTSS from HR zones, CTL ≈ 64–68).
    fn july() -> (Vec<MonitoringDay>, BTreeMap<String, f64>, BTreeMap<String, f64>) {
        let days = vec![
            day("2026-07-20", 220, Some(55.0), Some(6.0)),
            day("2026-07-21", 246, Some(51.0), Some(7.3)),
            day("2026-07-22", 219, Some(50.0), Some(7.0)),
            day("2026-07-23", 211, Some(53.0), Some(12.9)),
            day("2026-07-24", 236, Some(53.0), Some(13.3)),
            day("2026-07-25", 275, Some(55.0), Some(18.0)),
            day("2026-07-29", 255, Some(60.0), Some(24.7)),
        ];
        let tss: BTreeMap<String, f64> = [
            ("2026-07-22", 22.0),
            ("2026-07-23", 56.0),
            ("2026-07-24", 0.0),
            ("2026-07-28", 93.0),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
        let ctl: BTreeMap<String, f64> =
            [("2026-07-22", 64.0), ("2026-07-23", 64.0), ("2026-07-24", 63.0), ("2026-07-28", 68.0)]
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect();
        (days, tss, ctl)
    }

    #[test]
    fn reproduces_the_july_golden_numbers() {
        let (days, tss, ctl) = july();
        let (idx, valid) = night_indices(&days, &tss, &ctl);
        assert_eq!(valid.len(), 7);
        let got: Vec<(String, i64)> = idx.iter().map(|n| (n.date.to_string(), n.index)).collect();
        assert_eq!(
            got,
            vec![
                ("2026-07-23".to_string(), 90),
                ("2026-07-24".to_string(), 94),
                ("2026-07-25".to_string(), 86),
                ("2026-07-29".to_string(), 58),
            ]
        );
        // 29.07: median 60 vs base 53 → hr 44; stress 24.7 → 63; load 93/68 → 85.
        let last = idx.last().unwrap();
        assert_eq!(last.hr.baseline, 53.0);
        assert_eq!(last.hr.score, 44.0);
        assert!((last.stress.as_ref().unwrap().score - 63.25).abs() < 0.01);
        assert!((last.load.as_ref().unwrap().score - 85.29).abs() < 0.01);
    }

    #[test]
    fn the_first_three_nights_only_build_the_baseline() {
        let (days, tss, ctl) = july();
        let (idx, _) = night_indices(&days[..3], &tss, &ctl);
        assert!(idx.is_empty());
        let c = card(&days[..3], &tss, d("2026-07-23"));
        assert_eq!(c.index, None);
        assert_eq!(c.nights_recorded_90d, 3);
        assert_eq!(c.nights_needed, 0, "three nights: the next one gets an index");
        let c = card(&days[..2], &tss, d("2026-07-22"));
        assert_eq!(c.nights_needed, 1);
    }

    #[test]
    fn an_awake_night_is_skipped_and_never_enters_the_baseline() {
        let (mut days, tss, ctl) = july();
        days.push(day("2026-07-18", 50, Some(123.0), Some(71.0))); // too few samples anyway
        days.push(day("2026-07-26", 200, Some(95.0), Some(40.0))); // worn, awake
        let (idx, valid) = night_indices(&days, &tss, &ctl);
        assert!(!valid.contains(&d("2026-07-26")));
        assert!(!valid.contains(&d("2026-07-18")));
        assert_eq!(idx.last().unwrap().hr.baseline, 53.0, "26.07 did not move the base");
        // Without any baseline the absolute threshold applies.
        let lone = vec![day("2026-08-01", 200, Some(95.0), None)];
        let (idx, valid) = night_indices(&lone, &tss, &ctl);
        assert!(idx.is_empty() && valid.is_empty());
    }

    #[test]
    fn weights_renormalize_when_a_component_is_missing() {
        let (mut days, tss, ctl) = july();
        // 29.07 without stress: hr 44 and load 85.29 over weights 0.5 + 0.2.
        days[6].night_stress_avg = None;
        let (idx, _) = night_indices(&days, &tss, &ctl);
        let last = idx.last().unwrap();
        assert!(last.stress.is_none());
        let expected = ((0.5_f64 * 44.0 + 0.2 * 85.29) / 0.7).round() as i64;
        assert_eq!(last.index, expected);
        // Without a CTL for the day before, load drops out too: index = hr.
        let (idx, _) = night_indices(&days, &tss, &BTreeMap::new());
        assert_eq!(idx.last().unwrap().index, 44);
    }

    #[test]
    fn the_baseline_window_is_90_days_and_the_last_seven_nights() {
        let (days, tss, ctl) = july();
        // Same nights, seen from September: all older than 90 days → the
        // September night has no baseline yet.
        let mut all = days.clone();
        all.push(day("2026-11-05", 242, Some(53.0), Some(12.4)));
        let (idx, _) = night_indices(&all, &tss, &ctl);
        assert_eq!(idx.len(), 4, "November gets no index — July is out of the window");
        // Within the window it does (05.09 is 38 days after 29.07).
        let mut all = days;
        all.push(day("2026-09-05", 242, Some(53.0), Some(12.4)));
        let (idx, _) = night_indices(&all, &tss, &ctl);
        assert_eq!(idx.last().unwrap().date, d("2026-09-05"));
        assert_eq!(idx.last().unwrap().hr.baseline, 53.0);
    }

    #[test]
    fn an_old_index_still_shows_but_nights_needed_counts_the_window_only() {
        // Four nights ~110 days ago (the 4th gets an index) and one fresh
        // night whose baseline cannot form: the July nights are out of its
        // 90-day window.
        let (mut days, tss, _ctl) = july();
        days.truncate(4);
        days.push(day("2026-11-05", 300, Some(53.0), Some(10.0)));
        let c = card(&days, &tss, d("2026-11-10"));
        assert_eq!(c.date.as_deref(), Some("2026-07-23"));
        assert_eq!(c.age_days, Some(110));
        assert_eq!(c.nights_recorded_90d, 1);
        assert_eq!(c.nights_needed, 2, "two more recent nights before the next index");
    }

    #[test]
    fn card_reports_the_last_night_with_its_age_and_a_sparse_history() {
        let (days, tss, _ctl) = july();
        let c = card(&days, &tss, d("2026-09-05"));
        assert_eq!(c.date.as_deref(), Some("2026-07-29"));
        assert_eq!(c.age_days, Some(38));
        assert_eq!(c.band.as_deref(), Some("rest"));
        assert_eq!(c.warning, None);
        assert_eq!(c.nights_recorded_90d, 7);
        assert!(c.history.is_empty(), "nothing in the last 28 days");
        let c = card(&days, &tss, d("2026-07-30"));
        assert_eq!(c.age_days, Some(1));
        assert_eq!(c.history.len(), 4);
        assert_eq!(c.band.as_deref(), Some("rest"));
    }

    #[test]
    fn warns_when_the_night_ran_well_over_the_baseline() {
        let (mut days, tss, ctl) = july();
        days[6].night_hr_median = Some(62.0); // +9 over 53
        let (idx, _) = night_indices(&days, &tss, &ctl);
        assert_eq!(idx.last().unwrap().hr.delta, 9.0);
        let c = card(&days, &tss, d("2026-07-30"));
        assert_eq!(c.warning.as_deref(), Some("hr_above_baseline"));
        assert_eq!(band_of(80).0, "intervals_ok");
        assert_eq!(band_of(79).0, "easy_day");
        assert_eq!(band_of(59).0, "rest");
    }

    #[test]
    fn ctl_is_a_42_day_ewma_that_decays_on_empty_days() {
        let daily: BTreeMap<String, f64> =
            [("2026-07-01", 84.0)].into_iter().map(|(k, v)| (k.to_string(), v)).collect();
        let ctl = ctl_series(&daily, d("2026-07-03"));
        assert!((ctl["2026-07-01"] - 2.0).abs() < 1e-9);
        assert!((ctl["2026-07-02"] - 2.0 * (1.0 - 1.0 / 42.0)).abs() < 1e-9);
        assert_eq!(ctl.len(), 3);
        assert!(ctl_series(&BTreeMap::new(), d("2026-07-03")).is_empty());
    }

    /// The whole chain on a real database: SQL → daily hrTSS → CTL →
    /// card. This is where the pure module meets its input; a corrupt
    /// zone row (days' worth of seconds, as pre-dedup imports left) must
    /// not reach the load component.
    #[test]
    fn a_corrupt_zone_row_in_the_database_does_not_reach_the_load_score() {
        let conn = crate::db::test_db();
        conn.execute(
            "INSERT INTO activity (id, start_time, sport_type, duration_s)
             VALUES ('a', '2026-07-28T18:00:00+03:00', 'ride', 2000)",
            [],
        )
        .unwrap();
        for (zone, secs) in [(2, 500.0), (3, 1200.0), (4, 300.0), (3, 400_000.0)] {
            conn.execute(
                "INSERT INTO time_in_zone (activity_id, zone_type, zone_index, time_s)
                 VALUES ('a', 'hr', ?1, ?2)",
                rusqlite::params![zone, secs],
            )
            .unwrap();
        }
        let daily = crate::db::training_load::daily_hrtss(&conn).unwrap();
        let tss = daily["2026-07-28"];
        assert!((tss - (500.0 * 0.4225 + 1200.0 * 0.64 + 300.0 * 0.9025) / 36.0).abs() < 0.01);
        let (mut days, _, _) = july();
        days.retain(|d| d.date.as_str() < "2026-07-26");
        days.push(day("2026-07-29", 255, Some(53.0), Some(10.0)));
        let c = card(&days, &daily, d("2026-07-29"));
        let load = c.load.expect("CTL exists for the 28th");
        assert!((load.tss_yesterday - tss).abs() < 0.01);
        assert!((load.ctl - tss / CTL_DAYS).abs() < 1e-9, "first day of history");
        // 34.7 / max(0.83, 30) → 1.157 over → 93.7; with the corrupt row
        // the hrTSS would be ~7100 and the score 0.
        assert!((load.score - 93.7).abs() < 0.1, "{}", load.score);
    }

    /// Smoke on the LIVE vault, read-only — run by hand:
    /// `SYZIFY_VAULT_DB=~/…/vault.db cargo test --lib recovery::tests::smoke_live_vault \
    ///  -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn smoke_live_vault() {
        let Ok(path) = std::env::var("SYZIFY_VAULT_DB") else {
            eprintln!("smoke_live_vault: SYZIFY_VAULT_DB not set — skipped");
            return;
        };
        let conn = rusqlite::Connection::open_with_flags(
            &path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .expect("open read-only");
        let today = chrono::Local::now().date_naive();
        let days =
            crate::db::monitoring::get_days(&conn, "0000-01-01", &today.to_string()).unwrap();
        let daily = crate::db::training_load::daily_hrtss(&conn).unwrap();
        let ctl = ctl_series(&daily, today);
        let (idx, valid) = night_indices(&days, &daily, &ctl);
        eprintln!("days {} valid nights {} indexed {}", days.len(), valid.len(), idx.len());
        for n in &idx {
            eprintln!(
                "{} index {} hr {:.0}/{:.0} (base {:.1}) stress {:?} load {:?}",
                n.date,
                n.index,
                n.hr.night_median,
                n.hr.score,
                n.hr.baseline,
                n.stress.as_ref().map(|s| (s.night_avg, s.score.round())),
                n.load.as_ref().map(|l| (l.tss_yesterday.round(), l.ctl.round(), l.score.round()))
            );
        }
        let c = card(&days, &daily, today);
        eprintln!("card: {c:?}");
    }
}
