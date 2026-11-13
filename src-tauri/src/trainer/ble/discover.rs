//! Finding devices to pick from: one scan for trainers, power meters and
//! heart rate straps at once, and what advertised during it. The radio work
//! is the driver's (`driver::find_devices`); here is what an advertisement
//! makes of a device.

use super::gatt;
use super::machine::Advert;
pub use crate::models::training::FoundDevice as Found;
use crate::models::training::{Radio, SensorRole as Source};
use crate::trainer::pedals::{model_of, side_of, Side};

/// The roles an advertisement offers: a trainer (FTMS), pedals (Cycling
/// Power from something that is not a trainer — trainers advertise it too)
/// and a strap (Heart Rate). A trainer with a heart rate service of its own
/// is offered as both.
pub fn roles_of(ad: &Advert) -> Vec<Source> {
    let has = |service| ad.services.contains(&service);
    let mut roles = Vec::new();
    if has(gatt::FITNESS_MACHINE) {
        roles.push(Source::Trainer);
    } else if has(gatt::CYCLING_POWER) {
        roles.push(Source::Pedals);
    }
    if has(gatt::HEART_RATE) {
        roles.push(Source::Strap);
    }
    roles
}

/// The services a discovery scans for.
pub const SERVICES: [u16; 3] = [gatt::FITNESS_MACHINE, gatt::CYCLING_POWER, gatt::HEART_RATE];

/// What a discovery saw so far: each device once per role, with the
/// latest name it gave (a name often comes in a later advertisement).
#[derive(Debug, Default)]
pub struct Discovery {
    found: Vec<Found>,
}

impl Discovery {
    pub fn saw(&mut self, ad: &Advert) {
        for role in roles_of(ad) {
            match self.found.iter_mut().find(|f| f.role == role && f.address == ad.address) {
                Some(known) => {
                    if ad.name.is_some() {
                        known.name = ad.name.clone();
                    }
                }
                None => self.found.push(Found {
                    role,
                    radio: Radio::Ble,
                    address: ad.address.clone(),
                    name: ad.name.clone(),
                    other_side: None,
                    lone_side: false,
                }),
            }
        }
    }

    /// By role (trainer, pedals, strap), named ones first, by name; the two
    /// pedals of a pair as one.
    pub fn into_found(self) -> Vec<Found> {
        let rank = |r: Source| match r {
            Source::Trainer => 0,
            Source::Pedals => 1,
            Source::Strap => 2,
        };
        let mut found = pair_pedals(self.found);
        found.sort_by(|a, b| {
            (rank(a.role), a.name.is_none(), &a.name, &a.address).cmp(&(rank(b.role), b.name.is_none(), &b.name, &b.address))
        });
        found
    }
}

/// The two sides of one pedal pair become one choice, named for both, the
/// left's address first. Each pedal has its own serial (`ASSIOMA55430L`,
/// `ASSIOMA61026R`), so a pair is one left and one right of a model; with
/// more than one of a side about (a group ride) which goes with which is
/// not known, and each stays apart. A side not paired is marked.
fn pair_pedals(found: Vec<Found>) -> Vec<Found> {
    fn side(f: &Found) -> Option<(&str, Side)> {
        if f.role != Source::Pedals {
            return None;
        }
        let name = f.name.as_deref()?;
        Some((model_of(name)?, side_of(name)?.1))
    }
    let count = |found: &[Found], model: &str, which: Side| {
        found.iter().filter(|f| side(f).is_some_and(|(m, s)| m == model && s == which)).count()
    };
    let mut out: Vec<Found> = Vec::new();
    for f in &found {
        let Some((model, which)) = side(f) else {
            out.push(f.clone());
            continue;
        };
        let paired = count(&found, model, Side::Left) == 1 && count(&found, model, Side::Right) == 1;
        let right = found.iter().find(|r| side(r) == Some((model, Side::Right)));
        match (paired, which, right) {
            (true, Side::Left, Some(right)) => out.push(Found {
                name: Some(format!("{} + {}", f.name.as_deref().unwrap_or_default(), right.name.as_deref().unwrap_or_default())),
                other_side: Some(right.address.clone()),
                ..f.clone()
            }),
            // The right goes with its left.
            (true, Side::Right, _) => {}
            _ => out.push(Found { lone_side: true, ..f.clone() }),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ad(address: &str, name: Option<&str>, services: &[u16]) -> Advert {
        Advert { address: address.into(), name: name.map(str::to_string), services: services.to_vec() }
    }

    #[test]
    fn an_advertisement_offers_its_roles() {
        let both = ad("T", None, &[gatt::FITNESS_MACHINE, gatt::CYCLING_POWER, gatt::HEART_RATE]);
        assert_eq!(roles_of(&both), vec![Source::Trainer, Source::Strap], "a trainer's own power is not pedals");
        assert_eq!(roles_of(&ad("P", None, &[gatt::CYCLING_POWER])), vec![Source::Pedals]);
        assert_eq!(roles_of(&ad("H", None, &[gatt::HEART_RATE])), vec![Source::Strap]);
        assert!(roles_of(&ad("X", None, &[0x180F])).is_empty(), "a battery service alone is nothing to ride with");
    }

    #[test]
    fn each_device_is_found_once_with_its_latest_name_in_order() {
        let mut d = Discovery::default();
        d.saw(&ad("H1", Some("HRM-Pro"), &[gatt::HEART_RATE]));
        d.saw(&ad("P2", None, &[gatt::CYCLING_POWER]));
        d.saw(&ad("T1", None, &[gatt::FITNESS_MACHINE]));
        d.saw(&ad("T1", Some("AVANTI COG"), &[gatt::FITNESS_MACHINE]));
        d.saw(&ad("T1", None, &[gatt::FITNESS_MACHINE])); // a nameless one keeps the name
        d.saw(&ad("P1", Some("ASSIOMA55430L"), &[gatt::CYCLING_POWER]));
        d.saw(&ad("X", Some("Watch"), &[0x180F]));
        let found = d.into_found();
        let seen: Vec<(Source, &str, Option<&str>)> =
            found.iter().map(|f| (f.role, f.address.as_str(), f.name.as_deref())).collect();
        assert_eq!(
            seen,
            vec![
                (Source::Trainer, "T1", Some("AVANTI COG")),
                (Source::Pedals, "P1", Some("ASSIOMA55430L")),
                (Source::Pedals, "P2", None),
                (Source::Strap, "H1", Some("HRM-Pro")),
            ]
        );
        assert!(found[1].lone_side, "one pedal of a pair, seen alone");
        assert!(!found[2].lone_side && found[2].other_side.is_none());
    }

    #[test]
    fn the_two_pedals_of_a_pair_are_one_choice() {
        // Each pedal its own serial: the user's, as they advertise.
        let mut d = Discovery::default();
        d.saw(&ad("PR", Some("ASSIOMA61026R"), &[gatt::CYCLING_POWER]));
        d.saw(&ad("PL", Some("ASSIOMA55430L"), &[gatt::CYCLING_POWER]));
        d.saw(&ad("FR", Some("FAVERO1R"), &[gatt::CYCLING_POWER])); // another model, its left asleep
        d.saw(&ad("K", Some("KICKR"), &[gatt::CYCLING_POWER])); // ends in R, no serial: not a side
        d.saw(&ad("H", Some("HRM1R"), &[gatt::HEART_RATE])); // a strap is never a side
        let found = d.into_found();
        let strap = found.iter().find(|f| f.address == "H").unwrap();
        assert!(!strap.lone_side && strap.other_side.is_none() && strap.name.as_deref() == Some("HRM1R"));
        assert_eq!(
            pedals(found),
            vec![
                row("PL", Some("ASSIOMA55430L + ASSIOMA61026R"), Some("PR"), false),
                row("FR", Some("FAVERO1R"), None, true),
                row("K", Some("KICKR"), None, false),
            ]
        );
    }

    /// A pedal choice: address, name, the other side, alone.
    type Row = (String, Option<String>, Option<String>, bool);

    fn row(address: &str, name: Option<&str>, other: Option<&str>, lone: bool) -> Row {
        (address.into(), name.map(Into::into), other.map(Into::into), lone)
    }

    fn pedals(found: Vec<Found>) -> Vec<Row> {
        found.into_iter().filter(|f| f.role == Source::Pedals).map(|f| (f.address, f.name, f.other_side, f.lone_side)).collect()
    }

    /// Two riders' pedals about: which left goes with which right is not
    /// known, so none is paired and each is marked.
    #[test]
    fn pedals_of_two_pairs_stay_apart() {
        let mut d = Discovery::default();
        for (address, name) in [("A", "ASSIOMA1L"), ("B", "ASSIOMA2R"), ("C", "ASSIOMA3L"), ("D", "ASSIOMA4R")] {
            d.saw(&ad(address, Some(name), &[gatt::CYCLING_POWER]));
        }
        let seen = pedals(d.into_found());
        assert_eq!(seen.len(), 4);
        assert!(seen.iter().all(|(_, _, other, lone)| other.is_none() && *lone), "{seen:?}");
    }

    /// One more pedal of a side about (a second bike, a studio): which goes
    /// with which is not known — never another rider's leg in the sum.
    #[test]
    fn an_extra_pedal_of_a_side_pairs_nothing() {
        for names in [["ASSIOMA1L", "ASSIOMA2R", "ASSIOMA3R"], ["ASSIOMA1L", "ASSIOMA3L", "ASSIOMA2R"]] {
            let mut d = Discovery::default();
            for (i, name) in names.iter().enumerate() {
                d.saw(&ad(&format!("A{i}"), Some(name), &[gatt::CYCLING_POWER]));
            }
            let seen = pedals(d.into_found());
            assert_eq!(seen.len(), 3, "{names:?}");
            assert!(seen.iter().all(|(_, _, other, lone)| other.is_none() && *lone), "{names:?}: {seen:?}");
        }
    }

    /// A right of another model, seen first, is not the left's other side.
    #[test]
    fn a_left_pairs_with_the_right_of_its_own_model() {
        let mut d = Discovery::default();
        d.saw(&ad("FR", Some("FAVERO1R"), &[gatt::CYCLING_POWER]));
        d.saw(&ad("PL", Some("ASSIOMA55430L"), &[gatt::CYCLING_POWER]));
        d.saw(&ad("PR", Some("ASSIOMA61026R"), &[gatt::CYCLING_POWER]));
        let seen = pedals(d.into_found());
        assert!(seen.contains(&row("PL", Some("ASSIOMA55430L + ASSIOMA61026R"), Some("PR"), false)), "{seen:?}");
        assert!(seen.contains(&row("FR", Some("FAVERO1R"), None, true)));
    }
}
