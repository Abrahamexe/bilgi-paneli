//! HyperX Cloud wireless family (docs/protocols/headset-hyperx.md).
//!
//! Matched on (VID, PID) together: the same PID can exist under Kingston
//! 0x0951 and HP 0x03F0. Read-only: battery / charge queries only. The Cloud II
//! (HP) "wireless status" query (command 0x01) resets the mic mute, so it is
//! deliberately absent from every allow-list below.

use super::hs::{Bat, Port, be16, mv_pct, reading};
use crate::devices::{Connection, Driver, HidHandle, HidInfo, Reading};

const KINGSTON: u16 = 0x0951;
const HP: u16 = 0x03F0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Dialect {
    /// Cloud II Wireless (Kingston), 62-byte report 0x06.
    A,
    /// Cloud II Wireless (HP), `06 FF BB cmd`.
    B,
    /// Cloud Flight, voltage based.
    Flight,
    /// Cloud Alpha Wireless, `21 BB cmd`.
    Alpha,
    /// Cloud Alpha 2 Wireless, `50 02` / `52 01`.
    Alpha2,
    /// Cloud II Core / Cloud III, `66 cmd`.
    F,
    /// Cloud III S, `0C 02 03 01 00 cmd`.
    G,
}
use Dialect::*;

#[rustfmt::skip]
const MODELS: &[(u16, u16, Dialect)] = &[
    (KINGSTON, 0x1718, A),
    (HP, 0x0696, B), (HP, 0x018B, B),
    (HP, 0x0D93, B), // inferred: Cloud Stinger 2 Wireless, only listed in HyperHeadset's DTS table
    (KINGSTON, 0x16C4, Flight), (KINGSTON, 0x1723, Flight), (HP, 0x0E90, Flight), (HP, 0x1749, Flight),
    (HP, 0x098D, Alpha), (HP, 0x1743, Alpha), (HP, 0x1765, Alpha),
    (HP, 0x08BE, Alpha2),
    (HP, 0x069F, F), (HP, 0x0995, F), (HP, 0x05B7, F), (HP, 0x0C9D, F),
    (HP, 0x06BE, G), (HP, 0x02CC, G),
];

// Allow-list (battery / charge / link queries only).
const A_HDR: [u8; 15] = [0x06, 0x00, 0x02, 0x00, 0x9A, 0x00, 0x00, 0x68, 0x4A, 0x8E, 0x0A, 0x00, 0x00, 0x00, 0xBB];
const A_BATT: u8 = 0x02;
const A_CHG: u8 = 0x03;
const B_BATT: [u8; 4] = [0x06, 0xFF, 0xBB, 0x02];
const B_CHG: [u8; 4] = [0x06, 0xFF, 0xBB, 0x03];
const FLIGHT_VOLT: [u8; 3] = [0x21, 0xFF, 0x05];
const ALPHA_LINK: [u8; 3] = [0x21, 0xBB, 0x03];
const ALPHA_CHG: [u8; 3] = [0x21, 0xBB, 0x0C];
const ALPHA_BATT: [u8; 3] = [0x21, 0xBB, 0x0B];
const ALPHA2_LINK: [u8; 2] = [0x52, 0x01];
const ALPHA2_BATT: [u8; 2] = [0x50, 0x02];
const F_BATT: [u8; 2] = [0x66, 0x89];
const F_CHG: [u8; 2] = [0x66, 0x8A];
const G_BATT: [u8; 6] = [0x0C, 0x02, 0x03, 0x01, 0x00, 0x06];
const G_CHG: [u8; 6] = [0x0C, 0x02, 0x03, 0x01, 0x00, 0x48];
const G_LINK: [u8; 6] = [0x0C, 0x02, 0x03, 0x01, 0x00, 0x02];

fn dialect(vid: u16, pid: u16) -> Option<Dialect> {
    MODELS.iter().find(|m| m.0 == vid && m.1 == pid).map(|m| m.2)
}

fn req_a(cmd: u8) -> Vec<u8> {
    debug_assert!(cmd == A_BATT || cmd == A_CHG);
    let mut v = A_HDR.to_vec();
    v.extend([cmd, 0x00]);
    v
}

/// HyperHeadset's charge-state encoding: 0 no, 1 charging, 2 full on charger.
fn chg_state(v: u8) -> Option<bool> {
    match v {
        0 => Some(false),
        1 | 2 => Some(true),
        _ => None,
    }
}

fn a_battery(r: &[u8]) -> Option<u8> {
    (r.len() > 7 && r[0] == 0x0B && r[2] == 0xBB && r[3] == A_BATT && (1..=100).contains(&r[7])).then(|| r[7])
}

fn a_charging(r: &[u8]) -> Option<bool> {
    (r.len() > 4 && r[0] == 0x0B && r[2] == 0xBB && r[3] == A_CHG).then(|| r[4]).and_then(chg_state)
}

fn b_ok(r: &[u8], cmd: u8) -> bool {
    r.len() > 7 && r[..4] == [0x06, 0xFF, 0xBB, cmd]
}

fn b_battery(r: &[u8]) -> Option<u8> {
    // 0 % is what a dongle answers with no headset linked: treated as off.
    (b_ok(r, 0x02) && (1..=100).contains(&r[7])).then(|| r[7])
}

/// Cloud Flight voltage -> percent (HyperHeadset table, 5 % steps).
#[rustfmt::skip]
const FLIGHT_MV: &[(u32, u8)] = &[
    (3328, 5), (3584, 10), (3674, 15), (3704, 20), (3732, 25), (3744, 30), (3754, 35), (3764, 40),
    (3774, 45), (3784, 50), (3794, 55), (3804, 60), (3824, 65), (3840, 70), (3860, 75), (3890, 80),
    (3910, 85), (3940, 90), (3960, 95), (3970, 100),
];

fn flight(r: &[u8]) -> Option<Bat> {
    let mv = be16(r, 3)?;
    if !(2800..=4500).contains(&mv) {
        return None;
    }
    // Above 4116 mV the headset is on the charger and the voltage reads high.
    if mv >= 4116 {
        return Some(Bat::charging());
    }
    Some(Bat { pct: Some(mv_pct(FLIGHT_MV, mv).unwrap_or(5)), chg: Some(false) })
}

fn alpha_reply(r: &[u8], cmd: u8) -> Option<u8> {
    (r.len() > 3 && r[2] == cmd).then(|| r[3])
}

fn alpha2_link(r: &[u8]) -> Option<bool> {
    (r.len() > 2 && r[0] == 0x53 && r[1] == 0x01).then(|| r[2] != 0)
}

fn alpha2_batt(r: &[u8]) -> Option<Bat> {
    if r.len() < 4 || r[0] != 0x51 || r[1] != 0x02 {
        return None;
    }
    Bat::new(r[2], Some(r[3] == 1))
}

fn f_battery(r: &[u8]) -> Option<Bat> {
    if r.len() < 5 || r[0] != 0x66 || r[1] != 0x89 {
        return None;
    }
    // Both state bytes zero: the headset is not reporting (off).
    if r[2] == 0 && r[3] == 0 { Some(Bat::OFF) } else { Bat::new(r[4], None) }
}

fn f_charging(r: &[u8]) -> Option<bool> {
    (r.len() > 2 && r[0] == 0x66 && r[1] == 0x8A).then(|| r[2]).and_then(chg_state)
}

fn g_value(r: &[u8], cmd: u8) -> Option<u8> {
    (r.len() > 6 && r[0] == 0x0C && r[5] == cmd && r[6] != 0xFF).then(|| r[6])
}

pub struct HyperX;

impl Driver for HyperX {
    fn name(&self) -> &'static str {
        "hyperx-cloud"
    }

    fn matches(&self, info: &HidInfo) -> bool {
        let Some(d) = dialect(info.vid, info.pid) else { return false };
        match d {
            B => info.usage_page == 0xFF90 && info.usage == 0x0303 && info.interface.unwrap_or(0) == 0,
            Alpha2 => info.usage_page == 0xFF13 && info.usage == 0xFF00 && info.interface == Some(2),
            _ => info.is_vendor(),
        }
    }

    fn read(&self, dev: &HidHandle, info: &HidInfo) -> Option<Reading> {
        let p = Port::new(dev, info);
        let bat = match dialect(info.vid, info.pid)? {
            A => {
                let r = p.ask(&req_a(A_BATT), 250, 4, |r| a_battery(r).is_some() || r.first() == Some(&0x0B))?;
                let pct = a_battery(&r);
                let chg = p.ask(&req_a(A_CHG), 250, 4, |r| a_charging(r).is_some()).and_then(|r| a_charging(&r));
                Bat { pct, chg }
            }
            B => {
                let r = p.ask(&B_BATT, 250, 4, |r| b_ok(r, 0x02))?;
                let chg = p.ask(&B_CHG, 250, 4, |r| b_ok(r, 0x03)).map(|r| r[4] == 1);
                Bat { pct: b_battery(&r), chg }
            }
            Flight => flight(&p.ask(&FLIGHT_VOLT, 300, 5, |r| !matches!(r.first(), Some(0x64 | 0x65)) && r.len() > 4)?)?,
            Alpha => {
                let link = p.ask(&ALPHA_LINK, 300, 4, |r| alpha_reply(r, 0x03).is_some())?;
                if alpha_reply(&link, 0x03)? == 1 {
                    Bat::OFF
                } else {
                    let chg = alpha_reply(&p.ask(&ALPHA_CHG, 300, 4, |r| alpha_reply(r, 0x0C).is_some())?, 0x0C)?;
                    if chg == 1 {
                        Bat::charging()
                    } else {
                        let b = p.ask(&ALPHA_BATT, 300, 4, |r| alpha_reply(r, 0x0B).is_some())?;
                        Bat::new(alpha_reply(&b, 0x0B)?, Some(false))?
                    }
                }
            }
            Alpha2 => {
                let link = p.ask(&ALPHA2_LINK, 300, 6, |r| alpha2_link(r).is_some())?;
                if alpha2_link(&link)? {
                    alpha2_batt(&p.ask(&ALPHA2_BATT, 300, 6, |r| alpha2_batt(r).is_some())?)?
                } else {
                    Bat::OFF
                }
            }
            F => {
                let r = p.ask(&F_BATT, 300, 4, |r| r.first() == Some(&0x66) && r.get(1) == Some(&0x89))?;
                let mut b = f_battery(&r)?;
                if b.pct.is_some() {
                    b.chg = p
                        .ask(&F_CHG, 300, 4, |r| r.first() == Some(&0x66) && r.get(1) == Some(&0x8A))
                        .and_then(|r| f_charging(&r));
                }
                b
            }
            G => {
                // WriteFile only: the SetFeature fallback of HyperHeadset is not used.
                let link = g_value(&p.ask(&G_LINK, 300, 4, |r| g_value(r, 0x02).is_some())?, 0x02)?;
                if link != 2 {
                    Bat::OFF
                } else {
                    let b = g_value(&p.ask(&G_BATT, 300, 4, |r| g_value(r, 0x06).is_some())?, 0x06)?;
                    let chg = p.ask(&G_CHG, 300, 4, |r| g_value(r, 0x48).is_some()).and_then(|r| chg_state(r[6]));
                    Bat::new(b, chg)?
                }
            }
        };
        Some(reading(bat, Connection::Dongle))
    }
}

#[cfg(test)]
mod tests {
    use super::super::util::hex;
    use super::*;

    #[test]
    fn requests_match_documented_bytes() {
        let r = req_a(A_BATT);
        assert_eq!(r.len(), 17);
        assert_eq!(&r[..3], &[0x06, 0x00, 0x02]);
        assert_eq!(&r[14..], &[0xBB, 0x02, 0x00]);
        // The state-resetting "wireless status" command (0x01) is never sent.
        assert!(![B_BATT, B_CHG].iter().any(|q| q[3] == 0x01));
    }

    #[test]
    fn vid_and_pid_are_matched_together() {
        assert_eq!(dialect(KINGSTON, 0x1718), Some(A));
        assert_eq!(dialect(HP, 0x1718), None);
        assert_eq!(dialect(HP, 0x0696), Some(B));
        assert_eq!(dialect(KINGSTON, 0x0696), None);
    }

    #[test]
    fn cloud_ii() {
        let r = hex("0B 00 BB 02 00 00 00 4B 00");
        assert_eq!(a_battery(&r), Some(75));
        assert_eq!(a_battery(&hex("0B 00 BB 02 00 00 00 00")), None);
        assert_eq!(a_charging(&hex("0B 00 BB 03 01")), Some(true));
        assert_eq!(a_charging(&hex("0B 00 BB 03 00")), Some(false));
        assert_eq!(a_charging(&hex("0B 00 BB 03 09")), None);
        let r = hex("06 FF BB 02 00 0F 1C 50 00 00 00 00");
        assert_eq!(b_battery(&r), Some(80));
        assert_eq!(b_battery(&hex("06 FF BB 03 00 0F 1C 50")), None);
    }

    #[test]
    fn flight_voltage() {
        // 3784 mV = 0x0EC8
        assert_eq!(flight(&hex("21 FF 05 0E C8 00")), Some(Bat { pct: Some(50), chg: Some(false) }));
        assert_eq!(flight(&hex("21 FF 05 0C 00")), Some(Bat { pct: Some(5), chg: Some(false) }));
        // 4116 mV = 0x1014: charging, no level
        assert_eq!(flight(&hex("21 FF 05 10 14 00")), Some(Bat::charging()));
        assert_eq!(flight(&hex("21 FF 05 00 00 00")), None);
    }

    #[test]
    fn alpha_family() {
        assert_eq!(alpha_reply(&hex("21 BB 0B 5A"), 0x0B), Some(0x5A));
        assert_eq!(alpha_reply(&hex("21 BB 0C 01"), 0x0B), None);
        assert_eq!(alpha2_link(&hex("53 01 00")), Some(false));
        assert_eq!(alpha2_link(&hex("53 01 01")), Some(true));
        let r = hex("51 02 4B 01 10 00 AC 0E");
        assert_eq!(alpha2_batt(&r), Some(Bat { pct: Some(75), chg: Some(true) }));
        assert_eq!(alpha2_batt(&hex("51 02 65 00")), None);
    }

    #[test]
    fn cloud_iii_family() {
        assert_eq!(f_battery(&hex("66 89 01 00 5A")), Some(Bat { pct: Some(90), chg: None }));
        assert_eq!(f_battery(&hex("66 89 00 00 5A")), Some(Bat::OFF));
        assert_eq!(f_charging(&hex("66 8A 02")), Some(true));
        assert_eq!(g_value(&hex("0C 02 03 01 00 06 4B"), 0x06), Some(0x4B));
        assert_eq!(g_value(&hex("0C 02 03 01 00 06 FF"), 0x06), None);
        assert_eq!(g_value(&hex("0C 02 03 01 00 48 01"), 0x06), None);
    }
}
