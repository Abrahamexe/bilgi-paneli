//! VAXEE Control Center protocol (VID 0x3057). Docs: docs/protocols/mouse-family-vaxee.md.
//!
//! Feature report 0x0E on usage page 0xFF05: request `A5 cmd 01(read) len`, reply
//! `A5 cmd 03 01 len data...`. Read-only commands: 0x0A model, 0x0B battery,
//! 0x10 charging, 0x07 polling.

use super::mouse_io::{FEAT, feature_xfer};
use crate::devices::{Driver, HidHandle, HidInfo, Reading, hid};

const VID: u16 = 0x3057;

const fn req(cmd: u8) -> [u8; 5] {
    [0x0E, 0xA5, cmd, 1, 1]
}
const MODEL: [u8; 5] = req(0x0A);
const BATTERY: [u8; 5] = req(0x0B);
const CHARGING: [u8; 5] = req(0x10);
const POLLING: [u8; 5] = req(0x07);

/// Data bytes of the reply to `q`, or `None` (stale answer of another command, no answer).
fn ask(dev: &HidHandle, q: &[u8; 5]) -> Option<Vec<u8>> {
    // Report id, then `A5 cmd 03 1 len data...`.
    feature_xfer(dev, q, 100, 5, |r| (r.len() >= 8 && r[1] == 0xA5 && r[2] == q[2] && r[3] == 3).then(|| r[6..].to_vec()))
}

/// Battery steps 0..=20 -> percent.
fn percent(steps: u8) -> Option<u8> {
    (steps <= 20).then_some(steps * 5)
}

fn hz_for(code: u8) -> Option<u32> {
    [500, 1000, 2000, 4000, 8000].get(usize::from(code).checked_sub(1)?).copied()
}

/// inferred: the ids of cmd 0x0A are taken as hex like every other PID in the docs.
fn model_name(id: u16) -> Option<&'static str> {
    Some(match id {
        0x1003 => "VAXEE XE Wireless",
        0x1004 => "VAXEE NP-01S",
        0x1005 => "VAXEE AX",
        0x1006 => "VAXEE NP-01",
        0x1007 => "VAXEE XE-S",
        0x1008 => "VAXEE XE-S-L",
        0x1009 => "VAXEE x Ninjutso Sora",
        0x1010 => "VAXEE E1",
        0x1011 => "VAXEE NP-01S V2",
        0x1012 => "VAXEE XE V2",
        0x1013 => "VAXEE NP-01S Ergo",
        0x1014 => "VAXEE NP-01S V3",
        0x1015 => "VAXEE NP-01 Ergo",
        _ => return None,
    })
}

pub struct Vaxee;

impl Driver for Vaxee {
    fn name(&self) -> &'static str {
        "vaxee"
    }

    fn matches(&self, info: &HidInfo) -> bool {
        info.vid == VID && info.usage_page == 0xFF05 && info.usage == 1
    }

    fn read(&self, dev: &HidHandle, info: &HidInfo) -> Option<Reading> {
        if !hid::report_ids(&info.path)[FEAT].contains(&0x0E) {
            return None;
        }
        let battery = percent(*ask(dev, &BATTERY)?.first()?)?;
        Some(Reading {
            name: ask(dev, &MODEL)
                .filter(|d| d.len() >= 2)
                .and_then(|d| model_name(u16::from_le_bytes([d[0], d[1]])))
                .map(String::from),
            battery: Some(battery),
            charging: ask(dev, &CHARGING).and_then(|d| d.first().map(|&c| c != 0)),
            polling_hz: ask(dev, &POLLING).and_then(|d| hz_for(*d.first()?)),
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_bytes_from_docs() {
        // docs: `0E A5 0B 01 01 00 ...`
        assert_eq!(BATTERY, [0x0E, 0xA5, 0x0B, 1, 1]);
        assert_eq!(CHARGING[2], 0x10);
    }

    #[test]
    fn decoders() {
        assert_eq!(percent(20), Some(100));
        assert_eq!(percent(13), Some(65));
        assert_eq!(percent(21), None);
        assert_eq!(hz_for(0), None);
        assert_eq!(hz_for(1), Some(500));
        assert_eq!(hz_for(5), Some(8000));
        assert_eq!(hz_for(6), None);
        assert_eq!(model_name(0x1003), Some("VAXEE XE Wireless"));
    }
}
