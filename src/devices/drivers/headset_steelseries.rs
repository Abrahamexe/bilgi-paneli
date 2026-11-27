//! SteelSeries Arctis 2.4 GHz bases (docs/protocols/headset-steelseries.md).
//! Read-only: one status query per model family, listed in the `REQ_*` consts.

use super::hs::{Bat, Port, reading};
use crate::devices::{Connection, Driver, HidHandle, HidInfo, Reading};

const VID: u16 = 0x1038;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Fam {
    Arctis1,
    Arctis7,
    /// Arctis 7 (2019): firmware returns a constant 56 %, so no battery.
    Arctis7Bad,
    Arctis9,
    ProWireless,
    Arctis7Plus,
    Nova3_5,
    /// Nova 7 family; `true` = discrete 0..4 steps (pre-2026 firmware).
    Nova7(bool),
    NovaPro,
    GameBuds,
}

#[rustfmt::skip]
const MODELS: &[(u16, Fam)] = &[
    (0x12b3, Fam::Arctis1), (0x12b6, Fam::Arctis1), (0x12d7, Fam::Arctis1), (0x12d5, Fam::Arctis1),
    (0x1260, Fam::Arctis7), (0x1252, Fam::Arctis7), (0x1280, Fam::Arctis7), (0x12ad, Fam::Arctis7Bad),
    (0x12c2, Fam::Arctis9), (0x12c4, Fam::Arctis9),
    (0x1290, Fam::ProWireless),
    (0x220e, Fam::Arctis7Plus), (0x2212, Fam::Arctis7Plus), (0x2216, Fam::Arctis7Plus), (0x2236, Fam::Arctis7Plus),
    (0x2269, Fam::Nova3_5), (0x226d, Fam::Nova3_5), (0x2232, Fam::Nova3_5), (0x2253, Fam::Nova3_5),
    (0x2202, Fam::Nova7(true)), (0x22a1, Fam::Nova7(false)), (0x227e, Fam::Nova7(false)),
    (0x2206, Fam::Nova7(true)), (0x2258, Fam::Nova7(false)), (0x229e, Fam::Nova7(false)),
    (0x22ad, Fam::Nova7(false)), (0x223a, Fam::Nova7(true)), (0x22a9, Fam::Nova7(false)),
    (0x227a, Fam::Nova7(true)), (0x22a4, Fam::Nova7(true)), (0x22a5, Fam::Nova7(false)),
    (0x220a, Fam::Nova7(true)), (0x22a7, Fam::Nova7(false)), (0x2298, Fam::Nova7(false)),
    (0x12e0, Fam::NovaPro), (0x12e5, Fam::NovaPro),
    (0x230a, Fam::GameBuds),
];

/// Allow-list: the only bytes this driver ever sends.
const REQ_ARCTIS1: [u8; 2] = [0x06, 0x12];
const REQ_ARCTIS7: [u8; 2] = [0x06, 0x18];
const REQ_ARCTIS9: [u8; 2] = [0x00, 0x20];
const REQ_PRO_LINK: [u8; 2] = [0x41, 0xAA];
const REQ_PRO_BATT: [u8; 2] = [0x40, 0xAA];
const REQ_NOVA: [u8; 2] = [0x00, 0xB0];
const REQ_NOVA_PRO: [u8; 2] = [0x06, 0xB0];

fn family(pid: u16) -> Option<Fam> {
    MODELS.iter().find(|m| m.0 == pid).map(|m| m.1)
}

/// (usage page:usage if the doc gives one, interface number).
fn spec(f: Fam) -> (Option<(u16, u16)>, u8) {
    match f {
        Fam::Arctis1 => (Some((0xFF43, 0x0202)), 3),
        Fam::Arctis7 | Fam::Arctis7Bad => (Some((0xFF43, 0x0202)), 5),
        Fam::Arctis9 | Fam::ProWireless => (None, 0),
        Fam::NovaPro => (None, 4),
        _ => (Some((0xFFC0, 0x0001)), 3),
    }
}

fn pct4(v: u8) -> Option<u8> {
    (v <= 4).then_some(v * 25)
}

fn arctis1(r: &[u8]) -> Option<Bat> {
    let (st, b) = (*r.get(2)?, *r.get(3)?);
    if st == 1 { Some(Bat::OFF) } else { Bat::new(b, None) }
}

fn arctis7(r: &[u8]) -> Option<Bat> {
    Bat::new(*r.get(2)?, None)
}

fn arctis9(r: &[u8]) -> Option<Bat> {
    let (raw, st) = (*r.get(3)?, *r.get(4)?);
    if st == 0xFF {
        // inferred: the source calls 0xFF "unused"; treated as headset offline.
        return Some(Bat::OFF);
    }
    let pct = (u32::from(raw.saturating_sub(100)) * 100 / 54).min(100) as u8;
    Bat::new(pct, Some(st == 1))
}

fn pro_link(r: &[u8]) -> Option<bool> {
    match *r.first()? {
        0x04 => Some(true),
        0x02 => Some(false),
        _ => None,
    }
}

fn arctis7_plus(r: &[u8]) -> Option<Bat> {
    if *r.first()? != 0xB0 {
        return None;
    }
    if *r.get(1)? == 1 {
        return Some(Bat::OFF);
    }
    Bat::new(pct4(*r.get(2)?)?, Some(*r.get(3)? == 1))
}

fn nova3_5(r: &[u8]) -> Option<Bat> {
    if *r.first()? != 0xB0 {
        return None;
    }
    if *r.get(1)? == 2 {
        return Some(Bat::OFF);
    }
    Bat::new(*r.get(3)?, Some(*r.get(4)? == 1))
}

fn nova7(r: &[u8], discrete: bool) -> Option<Bat> {
    if *r.first()? != 0xB0 {
        return None;
    }
    let (b, st) = (*r.get(2)?, *r.get(3)?);
    if st == 0 {
        return Some(Bat::OFF);
    }
    // Firmware from Jan 2026 switched several PIDs from steps to percent: a
    // "discrete" PID answering above 4 is already a percentage.
    let pct = if discrete && b <= 4 { b * 25 } else { b };
    Bat::new(pct, Some(st == 1 || st == 2))
}

fn nova_pro(r: &[u8]) -> Option<Bat> {
    if r.len() < 16 {
        return None;
    }
    match r[15] {
        0x01 => Some(Bat::OFF),
        0x02 | 0x08 => (r[6] <= 8).then(|| Bat { pct: Some((u16::from(r[6]) * 100 / 8) as u8), chg: Some(r[15] == 2) }),
        _ => None,
    }
}

fn game_buds(r: &[u8]) -> Option<Bat> {
    if *r.first()? != 0xB0 || r.len() < 7 {
        return None;
    }
    // Buds out of the case report 3, docked ones 2; the lower active bud counts.
    let active = [(r[3], r[5]), (r[4], r[6])].into_iter().filter(|b| b.0 == 3).map(|b| b.1).min();
    match active {
        Some(p) => Bat::new(p, None),
        None => Some(Bat::OFF),
    }
}

fn is_b0(r: &[u8]) -> bool {
    r.first() == Some(&0xB0)
}

pub struct SteelSeries;

impl Driver for SteelSeries {
    fn name(&self) -> &'static str {
        "steelseries-arctis"
    }

    fn matches(&self, info: &HidInfo) -> bool {
        let Some(f) = (info.vid == VID).then(|| family(info.pid)).flatten() else { return false };
        let (usage, iface) = spec(f);
        usage.is_none_or(|(p, u)| info.usage_page == p && info.usage == u) && info.interface.unwrap_or(0) == iface
    }

    fn read(&self, dev: &HidHandle, info: &HidInfo) -> Option<Reading> {
        let f = family(info.pid)?;
        let any = |_: &[u8]| true;
        let legacy = Port::new(dev, info).flex();
        let bat = match f {
            Fam::Arctis7Bad => Bat::OFF,
            Fam::Arctis1 => arctis1(&legacy.ask(&REQ_ARCTIS1, 250, 3, any)?)?,
            Fam::Arctis7 => arctis7(&legacy.ask(&REQ_ARCTIS7, 250, 3, any)?)?,
            Fam::Arctis9 => arctis9(&legacy.ask(&REQ_ARCTIS9, 250, 3, any)?)?,
            Fam::ProWireless => {
                if !pro_link(&legacy.ask(&REQ_PRO_LINK, 250, 3, any)?)? {
                    Bat::OFF
                } else {
                    let r = legacy.ask(&REQ_PRO_BATT, 250, 3, any)?;
                    Bat::new(pct4(*r.first()?)?, None)?
                }
            }
            Fam::NovaPro => nova_pro(&legacy.ask(&REQ_NOVA_PRO, 300, 6, |r| r.len() >= 16)?)?,
            Fam::Arctis7Plus => arctis7_plus(&legacy.ask(&REQ_NOVA, 300, 8, is_b0)?)?,
            Fam::Nova3_5 => nova3_5(&legacy.ask(&REQ_NOVA, 300, 8, is_b0)?)?,
            Fam::Nova7(d) => nova7(&legacy.ask(&REQ_NOVA, 300, 8, is_b0)?, d)?,
            Fam::GameBuds => game_buds(&legacy.ask(&REQ_NOVA, 300, 8, is_b0)?)?,
        };
        Some(reading(bat, Connection::Dongle))
    }
}

#[cfg(test)]
mod tests {
    use super::super::util::hex;
    use super::*;

    fn b(pct: u8, chg: Option<bool>) -> Option<Bat> {
        Some(Bat { pct: Some(pct), chg })
    }

    #[test]
    fn legacy_arctis() {
        assert_eq!(arctis1(&hex("00 00 00 55 00 00 00 00")), b(0x55, None));
        assert_eq!(arctis1(&hex("00 00 01 55")), Some(Bat::OFF));
        assert_eq!(arctis1(&hex("00 00 00 65")), None);
        assert_eq!(arctis7(&hex("00 00 4B")), b(75, None));
        assert_eq!(arctis9(&hex("00 00 00 9A 01")), b(100, Some(true)));
        assert_eq!(arctis9(&hex("00 00 00 7F 00")), b(50, Some(false)));
        assert_eq!(arctis9(&hex("00 00 00 7F FF")), Some(Bat::OFF));
    }

    #[test]
    fn pro_wireless_steps() {
        assert_eq!(pro_link(&[2, 0]), Some(false));
        assert_eq!(pro_link(&[4, 0]), Some(true));
        assert_eq!(pro_link(&[7]), None);
        assert_eq!(pct4(3), Some(75));
        assert_eq!(pct4(5), None);
    }

    #[test]
    fn nova_family() {
        assert_eq!(arctis7_plus(&hex("B0 01 03 00")), Some(Bat::OFF));
        assert_eq!(arctis7_plus(&hex("B0 00 03 01")), b(75, Some(true)));
        assert_eq!(arctis7_plus(&hex("B0 00 09 01")), None);
        assert_eq!(nova3_5(&hex("B0 02 00 00 00")), Some(Bat::OFF));
        assert_eq!(nova3_5(&hex("B0 00 00 64 01")), b(100, Some(true)));
        assert_eq!(nova7(&hex("B0 00 02 05"), true), b(50, Some(false)));
        assert_eq!(nova7(&hex("B0 00 37 05"), true), b(55, Some(false)));
        assert_eq!(nova7(&hex("B0 00 02 05"), false), b(2, Some(false)));
        assert_eq!(nova7(&hex("B0 00 63 01"), false), b(99, Some(true)));
        assert_eq!(nova7(&hex("B0 00 63 00"), false), Some(Bat::OFF));
        assert_eq!(nova7(&hex("00 00 63 05"), false), None);
    }

    #[test]
    fn nova_pro_and_buds() {
        let mut r = vec![0u8; 16];
        r[6] = 4;
        r[15] = 8;
        assert_eq!(nova_pro(&r), b(50, Some(false)));
        r[15] = 2;
        assert_eq!(nova_pro(&r), b(50, Some(true)));
        r[15] = 1;
        assert_eq!(nova_pro(&r), Some(Bat::OFF));
        r[15] = 5;
        assert_eq!(nova_pro(&r), None);
        assert_eq!(nova_pro(&r[..10]), None);
        assert_eq!(game_buds(&hex("B0 00 00 03 03 50 40")), b(0x40, None));
        assert_eq!(game_buds(&hex("B0 00 00 02 03 50 40")), b(0x40, None));
        assert_eq!(game_buds(&hex("B0 00 00 02 02 50 40")), Some(Bat::OFF));
    }

    #[test]
    fn model_table_is_unique_and_known_bad_has_no_battery() {
        for (i, m) in MODELS.iter().enumerate() {
            assert!(MODELS[i + 1..].iter().all(|n| n.0 != m.0), "dup {:04x}", m.0);
        }
        assert_eq!(family(0x12ad), Some(Fam::Arctis7Bad));
    }
}
