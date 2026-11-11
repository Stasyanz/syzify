//! One radio, one scan: every link shares the process's adapter, and
//! CoreBluetooth keeps a single scan per adapter — a new scan replaces the
//! filter, a stop stops it for everyone. The coordinator keeps what each
//! link is looking for and says what the radio should scan for: the union
//! of the services wanted, and a stop only when nobody wants any.

use std::collections::BTreeMap;

/// What the radio should do after a change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanOrder {
    /// Scan for these services (sorted, no repeats).
    Scan(Vec<u16>),
    Stop,
    /// Nothing changed for the radio.
    Keep,
}

#[derive(Debug, Default)]
pub struct Scans {
    wants: BTreeMap<u64, u16>,
}

impl Scans {
    pub const fn new() -> Scans {
        Scans { wants: BTreeMap::new() }
    }

    fn services(&self) -> Vec<u16> {
        let mut s: Vec<u16> = self.wants.values().copied().collect();
        s.sort_unstable();
        s.dedup();
        s
    }

    /// Link `link` looks for `service`.
    pub fn start(&mut self, link: u64, service: u16) -> ScanOrder {
        let before = self.services();
        self.wants.insert(link, service);
        let after = self.services();
        if after == before {
            ScanOrder::Keep
        } else {
            ScanOrder::Scan(after)
        }
    }

    /// Link `link` found its device, or ended.
    pub fn stop(&mut self, link: u64) -> ScanOrder {
        let before = self.services();
        if self.wants.remove(&link).is_none() {
            return ScanOrder::Keep;
        }
        let after = self.services();
        if after.is_empty() {
            ScanOrder::Stop
        } else if after == before {
            ScanOrder::Keep
        } else {
            ScanOrder::Scan(after)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FTMS: u16 = 0x1826;
    const CPS: u16 = 0x1818;
    const HRS: u16 = 0x180D;

    #[test]
    fn links_share_one_scan_for_the_union_of_their_services() {
        // `new` is what the driver's static uses.
        let mut scans = Scans::new();
        assert_eq!(scans.start(1, FTMS), ScanOrder::Scan(vec![FTMS]));
        assert_eq!(scans.start(2, CPS), ScanOrder::Scan(vec![CPS, FTMS]), "pedals add to the trainer's scan");
        assert_eq!(scans.start(3, CPS), ScanOrder::Keep, "the other pedal wants the same");
        assert_eq!(scans.start(1, FTMS), ScanOrder::Keep, "asking twice changes nothing");
        // The trainer is found: the pedals keep scanning, for themselves only.
        assert_eq!(scans.stop(1), ScanOrder::Scan(vec![CPS]));
        assert_eq!(scans.stop(2), ScanOrder::Keep, "pedal R still looks");
        assert_eq!(scans.stop(3), ScanOrder::Stop, "nobody looks any more");
        assert_eq!(scans.stop(3), ScanOrder::Keep, "a link that was not looking");
    }

    #[test]
    fn a_link_that_changes_what_it_looks_for() {
        let mut scans = Scans::default();
        scans.start(1, FTMS);
        assert_eq!(scans.start(1, HRS), ScanOrder::Scan(vec![HRS]));
    }
}
