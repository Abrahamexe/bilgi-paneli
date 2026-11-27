//! Logitech G headsets (docs/protocols/headset-logitech.md).
//!
//! * HID++ voltage family (G533, G535, G633/G733/G933/G935, G PRO / PRO X):
//!   one long HID++ read of the battery-voltage feature, percent from a curve.
//! * G PRO X 2 (0x0AF7) Centurion: only the documented fixed "legacy vendor"
//!   battery query; the bridged feature-discovery path is not implemented.
//!
//! Not covered: G PRO X 3 / G522 (Centurion bridge framing not documented
//! precisely enough to send safely), G930, G435 (no battery interface known).

use super::hs::{Bat, Port, be16, mv_pct, reading};
use crate::devices::{Connection, Driver, HidHandle, HidInfo, Reading};

const VID: u16 = 0x046D;
const PRO_X2: u16 = 0x0AF7;

/// (mV, %) ascending; HeadsetControl calibration points, linearly interpolated.
const G533_MV: &[(u32, u8)] = &[(3330, 0), (3680, 5), (3750, 20), (3790, 30), (3850, 50), (4200, 100)];
const G535_MV: &[(u32, u8)] = &[(3310, 0), (3664, 5), (3730, 20), (3766, 30), (3817, 50), (4175, 100)];
const G633_MV: &[(u32, u8)] =
    &[(3150, 0), (3300, 5), (3500, 10), (3650, 20), (3750, 40), (3850, 60), (3950, 80), (4100, 100)];
const GPRO_MV: &[(u32, u8)] = &[(3320, 0), (3670, 5), (3740, 20), (3780, 30), (3830, 50), (4150, 100)];

struct Model {
    pid: u16,
    /// HID++ feature index and function|swId byte of the battery-voltage query.
    feature: u8,
    func: u8,
    curve: &'static [(u32, u8)],
    /// Usage page:usage of the HID++ collection.
    usage: (u16, u16),
}

const FF43: (u16, u16) = (0xFF43, 0x0202);
const fn m(pid: u16, feature: u8, func: u8, curve: &'static [(u32, u8)], usage: (u16, u16)) -> Model {
    Model { pid, feature, func, curve, usage }
}

/// The table is the allow-list: requests are `11 FF <feature> <func>`.
const MODELS: &[Model] = &[
    m(0x0A66, 0x07, 0x01, G533_MV, FF43),
    m(0x0AC4, 0x05, 0x0D, G535_MV, (0x000C, 0x0001)),
    m(0x0A5C, 0x08, 0x0A, G633_MV, FF43),
    m(0x0A89, 0x08, 0x0A, G633_MV, FF43),
    m(0x0A5B, 0x08, 0x0A, G633_MV, FF43),
    m(0x0A87, 0x08, 0x0A, G633_MV, FF43),
    m(0x0AB5, 0x08, 0x0A, G633_MV, FF43),
    m(0x0AFE, 0x08, 0x0A, G633_MV, FF43),
    m(0x0B1F, 0x08, 0x0A, G633_MV, FF43),
    m(0x0AA7, 0x06, 0x0D, GPRO_MV, FF43),
    m(0x0AAA, 0x06, 0x0D, GPRO_MV, FF43),
    m(0x0ABA, 0x06, 0x0D, GPRO_MV, FF43),
    m(0x0AFB, 0x06, 0x0D, GPRO_MV, FF43),
    m(0x0AFC, 0x06, 0x0D, GPRO_MV, FF43),
];

/// PRO X 2 Centurion "legacy vendor" battery query (report 0x51, 64 bytes).
const REQ_PRO_X2: [u8; 10] = [0x51, 0x08, 0x00, 0x03, 0x1A, 0x00, 0x03, 0x00, 0x04, 0x0A];

fn model(pid: u16) -> Option<&'static Model> {
    MODELS.iter().find(|m| m.pid == pid)
}

/// Parses a HID++ voltage reply. `0xFF` in the feature byte = error / headset off.
fn hidpp(r: &[u8], md: &Model) -> Option<Bat> {
    if r.len() < 7 || r[0] != 0x11 || r[1] != 0xFF {
        return None;
    }
    if r[2] == 0xFF {
        return Some(Bat::OFF);
    }
    if r[2] != md.feature {
        return None;
    }
    let mv = be16(r, 4)?;
    // The cell voltage reads high on the charger, so no level while charging.
    if r[6] == 0x03 {
        return Some(Bat::charging());
    }
    Some(Bat { pct: mv_pct(md.curve, mv), chg: Some(false) })
}

fn hidpp_is_reply(r: &[u8], md: &Model) -> bool {
    r.len() >= 7 && r[0] == 0x11 && r[1] == 0xFF && (r[2] == md.feature || r[2] == 0xFF)
}

// inferred: the legacy path is the documented fallback for firmwares without
// the Centurion battery feature; it is the only fixed frame available.
fn pro_x2(r: &[u8]) -> Option<Bat> {
    if r.len() >= 7 && r[0] == 0x51 && r[1] == 0x05 && r[6] == 0 {
        return Some(Bat::OFF);
    }
    if r.len() < 13 || r[0] != 0x51 || r[1] != 0x0B || r[8] != 0x04 {
        return None;
    }
    Bat::new(r[10], Some(r[12] == 0x02))
}

fn pro_x2_is_reply(r: &[u8]) -> bool {
    r.first() == Some(&0x51) && (r.get(1) == Some(&0x0B) && r.get(8) == Some(&0x04) || r.get(1) == Some(&0x05) && r.get(6) == Some(&0))
}

pub struct Logitech;

impl Driver for Logitech {
    fn name(&self) -> &'static str {
        "logitech-headset"
    }

    fn matches(&self, info: &HidInfo) -> bool {
        if info.vid != VID {
            return false;
        }
        if info.pid == PRO_X2 {
            return info.usage_page == 0xFFA0 && info.usage == 0x0001 && info.interface.unwrap_or(3) == 3;
        }
        model(info.pid).is_some_and(|md| (info.usage_page, info.usage) == md.usage)
    }

    fn read(&self, dev: &HidHandle, info: &HidInfo) -> Option<Reading> {
        let p = Port::new(dev, info);
        let bat = if info.pid == PRO_X2 {
            pro_x2(&p.ask(&REQ_PRO_X2, 300, 6, pro_x2_is_reply)?)?
        } else {
            let md = model(info.pid)?;
            let req = [0x11, 0xFF, md.feature, md.func];
            hidpp(&p.ask(&req, 300, 4, |r| hidpp_is_reply(r, md))?, md)?
        };
        Some(reading(bat, Connection::Dongle))
    }
}

#[cfg(test)]
mod tests {
    use super::super::util::hex;
    use super::*;

    #[test]
    fn hidpp_voltage_replies() {
        let g533 = model(0x0A66).unwrap();
        // 3850 mV = 0x0F0A -> 50 %, idle
        let r = hex("11 FF 07 01 0F 0A 01 00 00");
        assert_eq!(hidpp(&r, g533), Some(Bat { pct: Some(50), chg: Some(false) }));
        // charging: no level
        assert_eq!(hidpp(&hex("11 FF 07 01 10 68 03"), g533), Some(Bat::charging()));
        // HID++ error = headset off
        assert_eq!(hidpp(&hex("11 FF FF 07 01 08 00"), g533), Some(Bat::OFF));
        // below the lowest calibration point: unavailable
        assert_eq!(hidpp(&hex("11 FF 07 01 0B B8 01"), g533), Some(Bat { pct: None, chg: Some(false) }));
        // wrong feature / header
        assert_eq!(hidpp(&hex("11 FF 05 01 0F 0A 01"), g533), None);
        assert_eq!(hidpp(&hex("10 FF 07 01 0F 0A 01"), g533), None);
        assert!(hidpp_is_reply(&r, g533));
        assert!(!hidpp_is_reply(&hex("11 01 07 01 0F 0A 01"), g533));
    }

    #[test]
    fn curves_are_ascending_and_hit_documented_points() {
        for md in MODELS {
            assert!(md.curve.windows(2).all(|w| w[0].0 < w[1].0 && w[0].1 < w[1].1));
        }
        assert_eq!(mv_pct(G633_MV, 3750), Some(40));
        assert_eq!(mv_pct(G633_MV, 4300), Some(100));
        assert_eq!(mv_pct(G633_MV, 3100), None);
    }

    #[test]
    fn pro_x2_legacy_reply() {
        let r = hex("51 0B 00 00 00 00 00 00 04 00 5E 00 02");
        assert!(pro_x2_is_reply(&r));
        assert_eq!(pro_x2(&r), Some(Bat { pct: Some(94), chg: Some(true) }));
        let off = hex("51 05 00 00 00 00 00");
        assert!(pro_x2_is_reply(&off));
        assert_eq!(pro_x2(&off), Some(Bat::OFF));
        assert!(!pro_x2_is_reply(&hex("51 03 00 00")));
        assert_eq!(pro_x2(&hex("51 0B 00 00 00 00 00 00 05 00 5E 00 02")), None);
    }
}
