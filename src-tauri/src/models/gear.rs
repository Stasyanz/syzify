use serde::{Deserialize, Serialize};

use crate::error::CommandError;
use crate::models::activity::SportType;

/// What an item is. Decides its icon and which sports offer it by
/// default; nothing stops the user from putting anything on anything.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GearKind {
    Bike,
    Shoes,
    Other,
}

impl GearKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            GearKind::Bike => "bike",
            GearKind::Shoes => "shoes",
            GearKind::Other => "other",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "bike" => Some(GearKind::Bike),
            "shoes" => Some(GearKind::Shoes),
            "other" => Some(GearKind::Other),
            _ => None,
        }
    }
}

/// A `gear` row.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Gear {
    pub id: String,
    pub kind: GearKind,
    pub name: String,
    pub brand: Option<String>,
    pub model: Option<String>,
    /// "YYYY-MM-DD".
    pub purchased_at: Option<String>,
    /// Mileage before Syzify, so the odometer continues from it.
    pub initial_distance_m: f64,
    /// Wear warning threshold; None = no warning.
    pub distance_limit_m: Option<f64>,
    pub retired_at: Option<String>,
    pub notes: Option<String>,
    pub created_at: String,
}

/// What the Garage edits: the item's own fields plus the sports it is the
/// default for. The same shape creates and updates.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GearInput {
    pub kind: GearKind,
    pub name: String,
    pub brand: Option<String>,
    pub model: Option<String>,
    pub purchased_at: Option<String>,
    pub initial_distance_m: f64,
    pub distance_limit_m: Option<f64>,
    pub notes: Option<String>,
    /// Sport types (the app's snake_case ids) this item is the default
    /// for. Replaces the item's previous defaults; a sport named here is
    /// taken over from whichever item held it.
    pub default_for: Vec<String>,
    /// What puts an activity on this item at import (checked before the
    /// sport default). Replaces the item's previous rules; a value named
    /// here is taken over from whichever item held it.
    #[serde(default)]
    pub rules: Vec<GearRule>,
}

/// What a rule matches on (ADR 0003, stage 4).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GearRuleKind {
    /// The activity profile the device recorded under (FIT `sport.name`).
    ProfileName,
    /// A paired sensor's serial number (a power meter on one bike).
    SensorSerial,
}

impl GearRuleKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            GearRuleKind::ProfileName => "profile_name",
            GearRuleKind::SensorSerial => "sensor_serial",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "profile_name" => Some(GearRuleKind::ProfileName),
            "sensor_serial" => Some(GearRuleKind::SensorSerial),
            _ => None,
        }
    }
}

/// "Put the activity on this item when …". A value belongs to one item.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GearRule {
    pub kind: GearRuleKind,
    pub value: String,
}

/// What the vault has seen that a rule could match, with how often.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct RuleCandidates {
    pub profiles: Vec<ProfileCandidate>,
    pub sensors: Vec<SensorCandidate>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProfileCandidate {
    pub value: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SensorCandidate {
    pub serial: String,
    pub device_type: Option<String>,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
    pub count: i64,
}

/// The longest name the registry stores; the modal caps at the same.
pub const MAX_GEAR_NAME_CHARS: usize = 60;
/// Brand and model share the name's cap; notes get a paragraph. The IPC
/// is open to plugins later, so the limits live here, not in the form.
pub const MAX_GEAR_FIELD_CHARS: usize = 60;
pub const MAX_GEAR_NOTES_CHARS: usize = 2000;

/// The value, or `Err(max)` when it is longer — the caller names the field.
fn capped(value: Option<String>, max: usize) -> Result<Option<String>, usize> {
    match value {
        Some(v) if v.chars().count() > max => Err(max),
        other => Ok(other),
    }
}

impl GearInput {
    /// The input with its text trimmed and blanks turned into None, or why
    /// it cannot be saved. Pure, so the command stays a one-liner.
    pub fn normalized(&self) -> Result<GearInput, CommandError> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err(CommandError::coded("gear.nameRequired", "Name is required"));
        }
        if name.chars().count() > MAX_GEAR_NAME_CHARS {
            return Err(CommandError::coded(
                "gear.nameTooLong",
                format!("Name is longer than {MAX_GEAR_NAME_CHARS} characters"),
            )
            .param("count", MAX_GEAR_NAME_CHARS));
        }
        if !self.initial_distance_m.is_finite() || self.initial_distance_m < 0.0 {
            return Err(CommandError::coded("gear.initialDistance", "Initial distance must be zero or more"));
        }
        if let Some(limit) = self.distance_limit_m {
            if !limit.is_finite() || limit <= 0.0 {
                return Err(CommandError::coded("gear.distanceLimit", "Distance limit must be more than zero"));
            }
        }
        let purchased_at = match blank_to_none(&self.purchased_at) {
            Some(d) => {
                chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d")
                    .map_err(|_| CommandError::coded("gear.purchaseDate", "Purchase date must be YYYY-MM-DD"))?;
                Some(d)
            }
            None => None,
        };
        // Only the app's own sport ids: an alias ("cycling") or a stray
        // string would sit in gear_default and never match an import.
        let mut default_for: Vec<String> = Vec::new();
        for s in &self.default_for {
            let s = s.trim();
            if s.is_empty() {
                continue;
            }
            if SportType::from_str(s).as_str() != s {
                return Err(CommandError::coded("gear.unknownSport", format!("Unknown sport: {s}")).param("sport", s));
            }
            default_for.push(s.to_string());
        }
        default_for.sort();
        default_for.dedup();
        let mut rules: Vec<GearRule> = Vec::new();
        for r in &self.rules {
            let value = r.value.trim();
            if value.is_empty() {
                continue;
            }
            if value.chars().count() > MAX_GEAR_FIELD_CHARS {
                return Err(CommandError::coded(
                    "gear.ruleTooLong",
                    format!("A rule value is longer than {MAX_GEAR_FIELD_CHARS} characters"),
                )
                .param("count", MAX_GEAR_FIELD_CHARS));
            }
            // Case folds like the table's NOCASE: "Road" and "ROAD" are one rule.
            let dup = rules.iter().any(|x| x.kind == r.kind && x.value.eq_ignore_ascii_case(value));
            if !dup {
                rules.push(GearRule { kind: r.kind, value: value.to_string() });
            }
        }
        Ok(GearInput {
            kind: self.kind,
            name: name.to_string(),
            brand: capped(blank_to_none(&self.brand), MAX_GEAR_FIELD_CHARS).map_err(|max| {
                CommandError::coded("gear.brandTooLong", format!("Brand is longer than {max} characters")).param("count", max)
            })?,
            model: capped(blank_to_none(&self.model), MAX_GEAR_FIELD_CHARS).map_err(|max| {
                CommandError::coded("gear.modelTooLong", format!("Model is longer than {max} characters")).param("count", max)
            })?,
            purchased_at,
            initial_distance_m: self.initial_distance_m,
            distance_limit_m: self.distance_limit_m,
            notes: capped(blank_to_none(&self.notes), MAX_GEAR_NOTES_CHARS).map_err(|max| {
                CommandError::coded("gear.notesTooLong", format!("Notes is longer than {max} characters")).param("count", max)
            })?,
            default_for,
            rules,
        })
    }
}

fn blank_to_none(s: &Option<String>) -> Option<String> {
    s.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
}

/// What the activities on an item add up to. Computed on read.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct GearStats {
    pub activities: i64,
    pub distance_m: f64,
    pub duration_s: f64,
    pub elev_gain_m: f64,
    /// Start time of the latest activity on the item.
    pub last_used: Option<String>,
}

/// What a bulk assignment from the library would do: the activities the
/// filter matches that can carry gear and are not on the target item
/// already, and which other items they would be moved off, by name.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct GearTargets {
    pub eligible: i64,
    pub moved_from: Vec<MovedFrom>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MovedFrom {
    pub name: String,
    pub count: i64,
}

/// A Garage card: the item, its totals and the sports it is the default for.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GearItem {
    #[serde(flatten)]
    pub gear: Gear,
    pub stats: GearStats,
    pub default_for: Vec<String>,
    pub rules: Vec<GearRule>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> GearInput {
        GearInput {
            kind: GearKind::Bike,
            name: "  Canyon Ultimate ".into(),
            brand: Some("  ".into()),
            model: Some(" CF SL ".into()),
            purchased_at: Some(" 2025-03-01 ".into()),
            initial_distance_m: 1200.0,
            distance_limit_m: None,
            notes: None,
            default_for: vec![" ride".into(), "ride".into(), "".into(), "mountain_bike".into()],
            rules: vec![
                GearRule { kind: GearRuleKind::ProfileName, value: " ROAD ".into() },
                GearRule { kind: GearRuleKind::ProfileName, value: "Road".into() },
                GearRule { kind: GearRuleKind::SensorSerial, value: "".into() },
                GearRule { kind: GearRuleKind::SensorSerial, value: "3632674300".into() },
            ],
        }
    }

    #[test]
    fn normalizes_text_and_defaults() {
        let n = input().normalized().unwrap();
        assert_eq!(n.name, "Canyon Ultimate");
        assert_eq!(n.brand, None);
        assert_eq!(n.model.as_deref(), Some("CF SL"));
        assert_eq!(n.purchased_at.as_deref(), Some("2025-03-01"));
        assert_eq!(n.default_for, vec!["mountain_bike", "ride"]);
        assert_eq!(
            n.rules,
            vec![
                GearRule { kind: GearRuleKind::ProfileName, value: "ROAD".into() },
                GearRule { kind: GearRuleKind::SensorSerial, value: "3632674300".into() },
            ]
        );
    }

    #[test]
    fn refuses_what_cannot_be_saved() {
        // Each refusal carries its code and, for the frontend's fallback, the English words.
        let refused = |change: &dyn Fn(&mut GearInput)| {
            let mut i = input();
            change(&mut i);
            let e = i.normalized().unwrap_err();
            (e.code.unwrap(), e.message)
        };
        let max = |n: usize| serde_json::Value::from(n);
        assert_eq!(refused(&|i| i.name = "   ".into()), ("gear.nameRequired", "Name is required".into()));
        let mut i = input();
        i.name = "x".repeat(MAX_GEAR_NAME_CHARS + 1);
        let e = i.normalized().unwrap_err();
        assert_eq!((e.code, e.message.as_str()), (Some("gear.nameTooLong"), "Name is longer than 60 characters"));
        assert_eq!(e.params.get("count"), Some(&max(MAX_GEAR_NAME_CHARS)));
        assert_eq!(
            refused(&|i| i.initial_distance_m = -1.0),
            ("gear.initialDistance", "Initial distance must be zero or more".into())
        );
        assert_eq!(refused(&|i| i.initial_distance_m = f64::NAN).0, "gear.initialDistance");
        assert_eq!(
            refused(&|i| i.distance_limit_m = Some(0.0)),
            ("gear.distanceLimit", "Distance limit must be more than zero".into())
        );
        assert_eq!(
            refused(&|i| i.purchased_at = Some("01.03.2025".into())),
            ("gear.purchaseDate", "Purchase date must be YYYY-MM-DD".into())
        );
        // A blank date is simply none.
        let mut i = input();
        i.purchased_at = Some("  ".into());
        assert_eq!(i.normalized().unwrap().purchased_at, None);
        // Only the app's own sport ids: no aliases, no strays; "other" is one.
        let mut i = input();
        i.default_for = vec!["cycling".into()];
        let e = i.normalized().unwrap_err();
        assert_eq!((e.code, e.message.as_str()), (Some("gear.unknownSport"), "Unknown sport: cycling"));
        assert_eq!(e.params.get("sport"), Some(&serde_json::Value::from("cycling")));
        let mut i = input();
        i.default_for = vec!["Ride".into()];
        assert!(i.normalized().is_err());
        let mut i = input();
        i.default_for = vec!["other".into(), "swim".into()];
        assert_eq!(i.normalized().unwrap().default_for, vec!["other", "swim"]);
        // Brand, model and notes are capped like the name.
        assert_eq!(
            refused(&|i| i.brand = Some("b".repeat(MAX_GEAR_FIELD_CHARS + 1))),
            ("gear.brandTooLong", "Brand is longer than 60 characters".into())
        );
        assert_eq!(
            refused(&|i| i.model = Some("m".repeat(MAX_GEAR_FIELD_CHARS + 1))),
            ("gear.modelTooLong", "Model is longer than 60 characters".into())
        );
        let mut i = input();
        i.notes = Some("n".repeat(MAX_GEAR_NOTES_CHARS + 1));
        let e = i.normalized().unwrap_err();
        assert_eq!((e.code, e.message.as_str()), (Some("gear.notesTooLong"), "Notes is longer than 2000 characters"));
        assert_eq!(e.params.get("count"), Some(&max(MAX_GEAR_NOTES_CHARS)));
        let mut i = input();
        i.notes = Some("n".repeat(MAX_GEAR_NOTES_CHARS));
        assert!(i.normalized().is_ok());
        let mut i = input();
        i.rules = vec![GearRule { kind: GearRuleKind::ProfileName, value: "p".repeat(MAX_GEAR_FIELD_CHARS + 1) }];
        assert_eq!(
            i.normalized().unwrap_err(),
            CommandError::coded("gear.ruleTooLong", "A rule value is longer than 60 characters").param("count", 60)
        );
    }

    #[test]
    fn rule_kind_round_trips_its_string() {
        for k in [GearRuleKind::ProfileName, GearRuleKind::SensorSerial] {
            assert_eq!(GearRuleKind::parse(k.as_str()), Some(k));
        }
        assert_eq!(GearRuleKind::parse("colour"), None);
    }

    #[test]
    fn kind_round_trips_its_string() {
        for k in [GearKind::Bike, GearKind::Shoes, GearKind::Other] {
            assert_eq!(GearKind::parse(k.as_str()), Some(k));
        }
        assert_eq!(GearKind::parse("helmet"), None);
    }
}
