//! Glorious wireless mice. See docs/protocols/mouse-glorious.md.
//!  * A: SinoWealth "Core 1" (Model O/O-/D/D- Wireless, O PRO, D 2 PRO), VID 258A,
//!    65-byte feature report id 0, battery function `83`.
//!  * B: Model O 2 Wireless (VID 093A), feature report 3, `03 08 <route> 14`.
//!  * C: Model I 2 Wireless only pushes input report 6 on its own (no query):
//!    not implemented (a passive listener would rarely catch the 20-40 s frame).
//!
//! READ-ONLY allow-list: A `..02 02 .. 83`, B `03 08 FB|FA 14`. No polling-rate read exists.

use super::util::sleep_ms;
use crate::devices::{Driver, HidHandle, HidInfo, Reading};

const VID_A: u16 = 0x258A;
const VID_B: u16 = 0x093A;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Fam {
    /// SinoWealth, mouse on its 2.4 GHz dongle.
    ADongle,
    /// SinoWealth, mouse on its cable (same query, only the percent byte is used).
    AWired,
    /// Model O 2 through the dongle (route 0xFB).
    BDongle,
    /// Model O 2 on its cable (route 0xFA).
    BCable,
}

/// (vid, pid, family, name)
const DEVICES: &[(u16, u16, Fam, &str)] = &[
    (VID_A, 0x2022, Fam::ADongle, "Glorious Model O Wireless"),
    (VID_A, 0x2011, Fam::AWired, "Glorious Model O Wireless"),
    (VID_A, 0x2023, Fam::ADongle, "Glorious Model D Wireless"),
    (VID_A, 0x2012, Fam::AWired, "Glorious Model D Wireless"),
    (VID_A, 0x2024, Fam::ADongle, "Glorious Model O- Wireless"),
    (VID_A, 0x2013, Fam::AWired, "Glorious Model O- Wireless"),
    (VID_A, 0x2025, Fam::ADongle, "Glorious Model D- Wireless"),
    (VID_A, 0x2014, Fam::AWired, "Glorious Model D- Wireless"),
    (VID_A, 0x2027, Fam::ADongle, "Glorious Model O PRO Wireless"),
    (VID_A, 0x2034, Fam::ADongle, "Glorious Model D 2 PRO Wireless"),
    (VID_B, 0x822D, Fam::BDongle, "Glorious Model O 2 Wireless"),
    (VID_B, 0x822A, Fam::BCable, "Glorious Model O 2 Wireless"),
];

/// Family A request: 65-byte feature report (id 0), DevSel 2, CmdType 2 = battery, function 0x83.
fn request_a() -> [u8; 65] {
    let mut b = [0u8; 65];
    b[3] = 0x02;
    b[4] = 0x02;
    b[6] = 0x83;
    b
}

/// Family A reply: `[1]` status (A1 awake, A4 asleep), `[6]` echo 0x83, `[7]` charging, `[8]` percent.
/// Wired PIDs answer with other status bytes, so only the echo and the percent are used there.
fn parse_a(b: &[u8], wired: bool) -> Option<(u8, Option<bool>)> {
    if b.len() < 9 || b[6] != 0x83 || (!wired && b[1] != 0xA1) {
        return None;
    }
    let pct = b[8];
    if !(1..=100).contains(&pct) {
        return None; // 0 = no data
    }
    // inferred: the charging byte is unreliable on the dongle; only a set flag is trusted
    let charging = if wired { Some(true) } else { (b[7] != 0).then_some(true) };
    Some((pct, charging))
}

/// Family B request: feature report 3 `03 08 <route> 14`.
fn request_b(route: u8) -> [u8; 4] {
    [0x03, 0x08, route, 0x14]
}

/// Family B reply: `03 08 14 <level> <charging>` (report id first).
fn parse_b(b: &[u8]) -> Option<(u8, bool)> {
    if b.len() < 5 || b[..3] != [0x03, 0x08, 0x14] || b[3] > 100 {
        return None;
    }
    let chg = b[4] == 1;
    (b[3] > 0 || chg).then_some((b[3], chg))
}

fn read_a(dev: &HidHandle, wired: bool) -> Option<(u8, Option<bool>)> {
    if !dev.set_feature(&request_a()) {
        return None;
    }
    for _ in 0..3 {
        sleep_ms(60); // RF round trip: 50-100 ms
        if let Some(r) = dev.get_feature(0, 65) {
            // A4 = asleep, anything else non-matching = stale: ask the next round
            if !wired && r.get(1) == Some(&0xA4) {
                return None;
            }
            if let Some(v) = parse_a(&r, wired) {
                return Some(v);
            }
        }
    }
    None
}

fn read_b(dev: &HidHandle, route: u8) -> Option<(u8, bool)> {
    if !dev.set_feature(&request_b(route)) {
        return None;
    }
    for _ in 0..2 {
        sleep_ms(30); // the dongle relays asynchronously
        if let Some(v) = dev.get_feature(3, 64).and_then(|r| parse_b(&r)) {
            return Some(v);
        }
    }
    None
}

pub struct Glorious;

impl Glorious {
    fn entry(info: &HidInfo) -> Option<&'static (u16, u16, Fam, &'static str)> {
        DEVICES.iter().find(|d| (d.0, d.1) == (info.vid, info.pid))
    }
}

impl Driver for Glorious {
    fn name(&self) -> &'static str {
        "glorious"
    }

    fn matches(&self, info: &HidInfo) -> bool {
        Self::entry(info).is_some_and(|d| match d.2 {
            // vendor collection with the 65-byte feature report (0xFFA0 sorts first)
            Fam::ADongle | Fam::AWired => {
                info.interface.unwrap_or(2) == 2 && info.is_vendor() && info.feature_len == 65
            }
            Fam::BDongle | Fam::BCable => info.interface.unwrap_or(1) == 1 && super::usage_is(info, 0xFF00, 1),
        })
    }

    fn read(&self, dev: &HidHandle, info: &HidInfo) -> Option<Reading> {
        let &(_, _, fam, name) = Self::entry(info)?;
        let (battery, charging) = match fam {
            Fam::ADongle | Fam::AWired => read_a(dev, fam == Fam::AWired)?,
            Fam::BDongle | Fam::BCable => {
                let (p, c) = read_b(dev, if fam == Fam::BDongle { 0xFB } else { 0xFA })?;
                (p, Some(c))
            }
        };
        Some(Reading { name: Some(name.into()), battery: Some(battery), charging, ..Default::default() })
    }
}

#[cfg(test)]
mod tests {
    use super::super::util::hex;
    use super::*;

    #[test]
    fn request_a_layout() {
        let r = request_a();
        assert_eq!((r[0], r[3], r[4], r[6]), (0, 0x02, 0x02, 0x83));
        assert_eq!(r.iter().filter(|&&b| b != 0).count(), 3);
    }

    #[test]
    fn reply_a() {
        // [0] id, [1] A1 awake, [6] echo 83, [7] charging, [8] percent (index in the 65-byte buffer)
        let ok = hex("00 A1 00 00 00 00 83 00 4B 00");
        assert_eq!(parse_a(&ok, false), Some((75, None)));
        let chg = hex("00 A1 00 00 00 00 83 01 4B 00");
        assert_eq!(parse_a(&chg, false), Some((75, Some(true))));
        assert_eq!(parse_a(&hex("00 A4 00 00 00 00 83 00 4B 00"), false), None); // asleep
        assert_eq!(parse_a(&hex("00 A1 00 00 00 00 00 00 4B 00"), false), None); // stale
        assert_eq!(parse_a(&hex("00 A1 00 00 00 00 83 00 00 00"), false), None); // no data
        // wired PID: status bytes differ, only echo + percent
        assert_eq!(parse_a(&hex("00 00 00 00 00 00 83 00 20 00"), true), Some((32, Some(true))));
    }

    #[test]
    fn model_o2() {
        assert_eq!(request_b(0xFB), [0x03, 0x08, 0xFB, 0x14]);
        // doc: over the cable `08 14 64 01` = 100 %, charging (feature report 3)
        assert_eq!(parse_b(&hex("03 08 14 64 01 00")), Some((100, true)));
        assert_eq!(parse_b(&hex("03 08 14 40 00 00")), Some((64, false)));
        assert_eq!(parse_b(&hex("03 08 15 40 00")), None);
        assert_eq!(parse_b(&hex("03 08 14 65 00")), None);
        assert_eq!(parse_b(&hex("03 08 14 00 00")), None);
    }

    #[test]
    fn table() {
        assert!(DEVICES.iter().any(|d| d.1 == 0x2034 && d.2 == Fam::ADongle));
        assert!(DEVICES.iter().any(|d| d.1 == 0x822A && d.2 == Fam::BCable));
    }
}
