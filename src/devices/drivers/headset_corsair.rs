//! Corsair headsets (docs/protocols/headset-corsair.md). VID 0x1B1C.
//!
//! * Legacy Void family: `C9 64` battery query.
//! * Bragi unnumbered (Void Wireless V2 / Virtuoso MAX / HS80 MAX): GET-only
//!   heartbeat + battery property (never enters software mode).
//! * Bragi report 0x02 (Virtuoso XT / SE): GET battery level + status.
//!
//! Not covered: HS55 / HS80 RGB / HS70 / Virtuoso RGB (protocol unknown).

use super::hs::{Bat, Port, reading};
use crate::devices::{Connection, Driver, HidHandle, HidInfo, Reading};

const VID: u16 = 0x1B1C;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Fam {
    Void,
    /// Bragi, unnumbered 64-byte reports, interface 4.
    V2w,
    /// Bragi, report id 2, headset behind a receiver (target 0x09).
    XtWireless,
    /// Bragi, report id 2, cabled headset itself (target 0x08).
    XtWired,
}

const VOID_PIDS: &[u16] = &[
    0x1B27, 0x0A14, 0x0A16, 0x0A17, 0x0A1D, 0x0A1A, 0x1B2A, 0x1B29, 0x0A55, 0x0A51, 0x0A52, 0x0A38, 0x0A4F, 0x0A2B,
    0x0A75, 0x0A56,
];

fn family(pid: u16) -> Option<Fam> {
    match pid {
        0x2A08 | 0x2A02 | 0x0A97 => Some(Fam::V2w),
        0x0A64 | 0x0A3E => Some(Fam::XtWireless),
        0x0A62 | 0x0A3D => Some(Fam::XtWired), // inferred: wired PIDs answer for themselves (target 0x08)
        p if VOID_PIDS.contains(&p) => Some(Fam::Void),
        _ => None,
    }
}

// Allow-list. Bragi GET frames: `02 <target 08 self / 09 headset> 02 <property> 00`.
const REQ_VOID: [u8; 2] = [0xC9, 0x64];
const V2W_RX_FW: [u8; 5] = [0x00, 0x02, 0x08, 0x02, 0x13];
const V2W_RX_BEAT: [u8; 5] = [0x00, 0x02, 0x08, 0x02, 0x12];
const V2W_HS_BEAT: [u8; 5] = [0x00, 0x02, 0x09, 0x02, 0x12];
const V2W_BATT: [u8; 5] = [0x00, 0x02, 0x09, 0x02, 0x0F];
const XT_BATT_HS: [u8; 5] = [0x02, 0x09, 0x02, 0x0F, 0x00];
const XT_CHG_HS: [u8; 5] = [0x02, 0x09, 0x02, 0x10, 0x00];
const XT_BATT_SELF: [u8; 5] = [0x02, 0x08, 0x02, 0x0F, 0x00];
const XT_CHG_SELF: [u8; 5] = [0x02, 0x08, 0x02, 0x10, 0x00];

/// Void reply `64 00 <batt | mic-up bit 7> B1 <status>`.
fn void(r: &[u8]) -> Option<Bat> {
    if r.len() < 5 || r[0] != 0x64 || r[3] != 0xB1 {
        return None;
    }
    match r[4] {
        0 => Some(Bat::OFF), // headset not connected to the receiver
        1 | 2 => Bat::new(r[2] & 0x7F, Some(false)),
        4 | 5 => Bat::new(r[2] & 0x7F, Some(true)),
        _ => None,
    }
}

/// Bragi GET reply `01 <src> <echo> <status> <LE value...>`; the value, or `None`.
fn bragi(r: &[u8]) -> Option<u16> {
    (r.len() > 5 && r[0] == 0x01 && r[3] == 0x00).then(|| u16::from(r[4]) | u16::from(r[5]) << 8)
}

fn is_bragi(r: &[u8]) -> bool {
    r.first() == Some(&0x01)
}

/// Battery property is in 0.1 % units; 0 and > 1000 are the receiver's junk answers.
fn tenths(v: u16) -> Option<u8> {
    (1..=1000).contains(&v).then_some((v / 10) as u8)
}

pub struct Corsair;

impl Driver for Corsair {
    fn name(&self) -> &'static str {
        "corsair-headset"
    }

    fn matches(&self, info: &HidInfo) -> bool {
        if info.vid != VID {
            return false;
        }
        match family(info.pid) {
            Some(Fam::Void) => info.usage_page == 0xFFC5 && info.usage == 0x0001 && info.interface.unwrap_or(3) == 3,
            Some(Fam::V2w) => info.interface == Some(4),
            Some(Fam::XtWireless | Fam::XtWired) => {
                info.usage_page == 0xFF42 && info.usage == 0x0001 && info.interface.unwrap_or(3) == 3
            }
            None => false,
        }
    }

    fn read(&self, dev: &HidHandle, info: &HidInfo) -> Option<Reading> {
        let fam = family(info.pid)?;
        let p = Port::new(dev, info);
        let mut conn = Connection::Dongle;
        let bat = match fam {
            Fam::Void => void(&p.ask(&REQ_VOID, 300, 4, |r| r.first() == Some(&0x64))?)?,
            Fam::V2w => {
                p.ask(&V2W_RX_FW, 200, 3, is_bragi)?;
                p.ask(&V2W_RX_BEAT, 200, 3, is_bragi)?;
                // No answer to the headset heartbeat = headset off.
                if p.ask(&V2W_HS_BEAT, 300, 3, is_bragi).is_none() {
                    Bat::OFF
                } else {
                    let pct = (0..3).find_map(|_| bragi(&p.ask(&V2W_BATT, 300, 3, is_bragi)?).and_then(tenths))?;
                    Bat { pct: Some(pct), chg: None }
                }
            }
            Fam::XtWireless | Fam::XtWired => {
                let (batt, chg) = if fam == Fam::XtWired {
                    conn = Connection::Wired;
                    (&XT_BATT_SELF, &XT_CHG_SELF)
                } else {
                    (&XT_BATT_HS, &XT_CHG_HS)
                };
                // The receiver answers nothing while the headset is off.
                let Some(r) = p.ask(batt, 300, 4, is_bragi) else { return Some(reading(Bat::OFF, conn)) };
                let pct = tenths(bragi(&r)?)?;
                let chg = p.ask(chg, 300, 4, is_bragi).and_then(|r| bragi(&r)).map(|v| v == 1);
                Bat { pct: Some(pct), chg }
            }
        };
        Some(reading(bat, conn))
    }
}

#[cfg(test)]
mod tests {
    use super::super::util::hex;
    use super::*;

    #[test]
    fn void_reply() {
        assert_eq!(void(&hex("64 00 5A B1 01")), Some(Bat { pct: Some(90), chg: Some(false) }));
        // bit 7 = mic boom up, must be masked
        assert_eq!(void(&hex("64 00 DA B1 01")), Some(Bat { pct: Some(90), chg: Some(false) }));
        assert_eq!(void(&hex("64 00 32 B1 05")), Some(Bat { pct: Some(50), chg: Some(true) }));
        assert_eq!(void(&hex("64 00 32 B1 00")), Some(Bat::OFF));
        assert_eq!(void(&hex("64 00 32 B1 03")), None);
        assert_eq!(void(&hex("64 00 32 B0 01")), None);
    }

    #[test]
    fn bragi_values() {
        // 0x0258 = 600 -> 60.0 %
        assert_eq!(bragi(&hex("01 01 0F 00 58 02")), Some(600));
        assert_eq!(tenths(600), Some(60));
        assert_eq!(tenths(0), None);
        assert_eq!(tenths(1001), None);
        assert_eq!(bragi(&hex("01 01 0F 05 58 02")), None); // property not supported
        assert_eq!(bragi(&hex("0E 01 0F 00 58 02")), None); // volume event
    }

    #[test]
    fn family_table() {
        assert_eq!(family(0x2A08), Some(Fam::V2w));
        assert_eq!(family(0x0A64), Some(Fam::XtWireless));
        assert_eq!(family(0x1B23), None); // wired Void USB: not polled
        assert_eq!(family(0x1B2A), Some(Fam::Void));
        // Every frame is a GET (op 0x02) and never the software-mode property 0x03.
        for q in [XT_BATT_HS, XT_CHG_HS, XT_BATT_SELF, XT_CHG_SELF] {
            assert_eq!((q[2], q[3] != 0x03), (0x02, true));
        }
        for q in [V2W_RX_FW, V2W_RX_BEAT, V2W_HS_BEAT, V2W_BATT] {
            assert_eq!((q[0], q[3]), (0x00, 0x02));
        }
    }
}
