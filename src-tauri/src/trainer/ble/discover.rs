//! Finding devices to pick from: one scan for trainers, power meters and
//! heart rate straps at once, and what advertised during it. The radio work
//! is the driver's (`driver::find_devices`); here is what an advertisement
//! makes of a device.

use super::gatt;
use super::machine::Advert;
pub use crate::models::training::FoundDevice as Found;
use crate::models::training::SensorRole as Source;

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
                None => self.found.push(Found { role, address: ad.address.clone(), name: ad.name.clone() }),
            }
        }
    }

    /// By role (trainer, pedals, strap), named ones first, by name.
    pub fn into_found(mut self) -> Vec<Found> {
        let rank = |r: Source| match r {
            Source::Trainer => 0,
            Source::Pedals => 1,
            Source::Strap => 2,
        };
        self.found.sort_by(|a, b| {
            (rank(a.role), a.name.is_none(), &a.name, &a.address).cmp(&(rank(b.role), b.name.is_none(), &b.name, &b.address))
        });
        self.found
    }
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
    }
}
