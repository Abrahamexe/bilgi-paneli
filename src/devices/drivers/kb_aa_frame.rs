//! "AA-frame" boards. Docs: docs/protocols/keyboard-aula-compx-aa.md section B.
//! * PMO Wave 75 dongle (36B0:3002): cmd 0x30 battery + state (hardware-verified).
//! * AULA F87 Pro (wired 38A6:271x / 0C45:8803, dongles 0C45:FEFC/FEFE): polling-rate code
//!   from the settings read (cmd 0x11). inferred: field order is taken from the F87 JS
//!   (`AA cmd len offLo offHi`), untested on hardware.
//!
//! Read requests only (0x30, 0x11); never the write cmd 0x21.

use super::kb_util::{echo_at, exchange};
use crate::devices::{Driver, HidHandle, HidInfo, Reading};

const PMO: (u16, u16) = (0x36B0, 0x3002);
const F87: &[(u16, u16)] = &[
    (0x38A6, 0x2710),
    (0x38A6, 0x2711),
    (0x38A6, 0x2712),
    (0x38A6, 0x2713),
    (0x0C45, 0x8803),
    (0x0C45, 0xFEFC),
    (0x0C45, 0xFEFE),
];
/// Vendor usage pages: wired 0xFF68 (64 B), dongles 0xFF80 / 0xFF60.
const F87_PAGES: &[u16] = &[0xFF68, 0xFF80, 0xFF60];

const CMD_BATTERY: u8 = 0x30;
const CMD_SETTINGS: u8 = 0x11;

/// F87 settings polling code (data[5]): 3..=6 = 1000..8000 Hz. Codes 0..=2 are unlisted: unknown.
const F87_HZ: [(u8, u32); 4] = [(3, 1000), (4, 2000), (5, 4000), (6, 8000)];

pub struct AaFrame;

fn is_pmo(i: &HidInfo) -> bool {
    (i.vid, i.pid) == PMO
}

/// `[report id 0, AA, cmd, b2, b3, b4, 0...]`
fn request(cmd: u8, b2: u8, b3: u8, b4: u8) -> [u8; 9] {
    [0, 0xAA, cmd, b2, b3, b4, 0, 0, 0]
}

/// PMO: (percent, charging). Frame idx 8 = percent, idx 9 = 0 discharging / 1 charging / 2 full.
fn parse_pmo(r: &[u8]) -> Option<(u8, Option<bool>)> {
    let o = echo_at(r, &[0xAA, CMD_BATTERY])?;
    let pct = *r.get(o + 8).filter(|p| (1..=100).contains(*p))?;
    let charging = match r.get(o + 9) {
        Some(0 | 2) => Some(false),
        Some(1) => Some(true),
        _ => None,
    };
    Some((pct, charging))
}

/// F87 settings reply: data starts at frame byte 8, polling code at data[5].
fn parse_f87_hz(r: &[u8]) -> Option<u32> {
    let o = echo_at(r, &[0xAA, CMD_SETTINGS])?;
    let code = *r.get(o + 8 + 5)?;
    F87_HZ.iter().find(|&&(c, _)| c == code).map(|&(_, hz)| hz)
}

impl Driver for AaFrame {
    fn name(&self) -> &'static str {
        "aa-frame"
    }

    fn matches(&self, info: &HidInfo) -> bool {
        if is_pmo(info) {
            return info.is_vendor() && info.input_len >= 33 && info.output_len >= 33;
        }
        super::id_listed(F87, info) && F87_PAGES.contains(&info.usage_page) && info.output_len >= 33
    }

    fn read(&self, dev: &HidHandle, info: &HidInfo) -> Option<Reading> {
        if is_pmo(info) {
            // `AA 30 00 00 18`: offset 0, length 24 (PMO field order).
            let r = exchange(dev, &request(CMD_BATTERY, 0, 0, 0x18), 400, |r| {
                echo_at(r, &[0xAA, CMD_BATTERY]).is_some()
            })?;
            let (battery, charging) = parse_pmo(&r)?;
            return Some(Reading { battery: Some(battery), charging, ..Default::default() });
        }
        // F87 field order: len 16, offset 0.
        let r = exchange(dev, &request(CMD_SETTINGS, 16, 0, 0), 400, |r| {
            echo_at(r, &[0xAA, CMD_SETTINGS]).is_some()
        })?;
        Some(Reading { polling_hz: Some(parse_f87_hz(&r)?), ..Default::default() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pmo_sample() {
        // `AA 30 00 00 18 00 00 55 40 00` = 64 % discharging
        let s = [0xAA, 0x30, 0, 0, 0x18, 0, 0, 0x55, 0x40, 0x00];
        assert_eq!(parse_pmo(&s), Some((64, Some(false))));
        let mut p = vec![0u8];
        p.extend_from_slice(&s);
        p[10] = 1;
        assert_eq!(parse_pmo(&p), Some((64, Some(true))));
        let mut z = s;
        z[8] = 0;
        assert_eq!(parse_pmo(&z), None);
        z[8] = 101;
        assert_eq!(parse_pmo(&z), None);
    }

    #[test]
    fn f87_hz() {
        let mut r = [0u8; 32];
        r[..2].copy_from_slice(&[0xAA, 0x11]);
        r[8 + 5] = 6;
        assert_eq!(parse_f87_hz(&r), Some(8000));
        r[8 + 5] = 3;
        assert_eq!(parse_f87_hz(&r), Some(1000));
        r[8 + 5] = 2; // unlisted code
        assert_eq!(parse_f87_hz(&r), None);
    }
}
