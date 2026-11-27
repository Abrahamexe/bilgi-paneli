//! ASTRO A50 Gen 4 / Gen 5 bases (docs/protocols/headset-astro.md).
//! Read-only: link status + battery (Gen 4), battery (Gen 5).

use super::hs::{Bat, Port, reading};
use crate::devices::{Connection, Driver, HidHandle, HidInfo, Reading};

const GEN4: (u16, u16) = (0x9886, 0x002C);
const GEN5: (u16, u16) = (0x046D, 0x0B1C);

// Allow-list.
const G4_LINK: [u8; 2] = [0x02, 0x54];
const G4_BATT: [u8; 2] = [0x02, 0x7C];
const G5_BATT: [u8; 6] = [0x02, 0x0C, 0x03, 0x00, 0x06, 0x0C];

/// Gen 4 reply `02 <status> <len> <payload...>`: first payload byte, status 0x02 = OK.
fn g4_payload(r: &[u8]) -> Option<u8> {
    (r.len() > 3 && r[0] == 0x02 && r[1] == 0x02).then(|| r[3])
}

/// Bit 1 = live link to the headset (the base keeps serving the last battery
/// level after the headset powers off, so this gates the battery).
fn g4_linked(v: u8) -> bool {
    v & 0x02 != 0
}

/// Bit 7 = charging, bits 0..6 = percent.
fn g4_battery(v: u8) -> Option<Bat> {
    Bat::new(v & 0x7F, Some(v & 0x80 != 0))
}

/// Gen 5 reply `02 0C <len> 00 06 00 <level> <level2> <dock>`.
fn g5_battery(r: &[u8]) -> Option<Bat> {
    if r.len() < 9 || r[0] != 0x02 || r[1] != 0x0C || r[4] != 0x06 {
        return None;
    }
    Bat::new(r[6], Some(r[8] != 0))
}

pub struct Astro;

impl Driver for Astro {
    fn name(&self) -> &'static str {
        "astro-a50"
    }

    fn matches(&self, info: &HidInfo) -> bool {
        if info.usage_page != 0xFF32 || info.usage != 0x0074 {
            return false;
        }
        match (info.vid, info.pid) {
            GEN4 => info.interface.unwrap_or(6) == 6,
            GEN5 => info.interface.unwrap_or(8) == 8,
            _ => false,
        }
    }

    fn read(&self, dev: &HidHandle, info: &HidInfo) -> Option<Reading> {
        let p = Port::new(dev, info);
        let bat = if (info.vid, info.pid) == GEN4 {
            let link = g4_payload(&p.ask(&G4_LINK, 300, 3, |r| g4_payload(r).is_some())?)?;
            if g4_linked(link) {
                g4_battery(g4_payload(&p.ask(&G4_BATT, 300, 3, |r| g4_payload(r).is_some())?)?)?
            } else {
                Bat::OFF
            }
        } else {
            g5_battery(&p.ask(&G5_BATT, 300, 6, |r| r.len() > 4 && r[0] == 0x02 && r[1] == 0x0C && r[4] == 0x06)?)?
        };
        Some(reading(bat, Connection::Dongle))
    }
}

#[cfg(test)]
mod tests {
    use super::super::util::hex;
    use super::*;

    #[test]
    fn gen4_link_gates_battery() {
        assert_eq!(g4_payload(&hex("02 02 01 03")), Some(3));
        assert!(g4_linked(3));
        assert!(!g4_linked(1));
        assert_eq!(g4_payload(&hex("02 01 01 05")), None); // error status
        assert_eq!(g4_battery(0x80 | 64), Some(Bat { pct: Some(64), chg: Some(true) }));
        assert_eq!(g4_battery(0x4B), Some(Bat { pct: Some(75), chg: Some(false) }));
        assert_eq!(g4_battery(0x7F), None);
    }

    #[test]
    fn gen5_reply() {
        let r = hex("02 0C 06 00 06 00 55 50 01");
        assert_eq!(g5_battery(&r), Some(Bat { pct: Some(85), chg: Some(true) }));
        assert_eq!(g5_battery(&hex("02 0C 06 00 07 00 55 50 01")), None);
        assert_eq!(g5_battery(&hex("02 0C 06 00 06 00 65 50 00")), None);
    }
}
