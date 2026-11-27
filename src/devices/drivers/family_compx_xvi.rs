//! Family B: CompX "XVI" 64-byte page/command feature-report protocol (WLMouse, Lamzu
//! Maya X / Inca / Paro, Attack Shark R5/R6/R8, LunaFury, CRDRAKO, Vancer, RAWM,
//! G-Wolves XVI:1, Hitscan). Docs: docs/protocols/mouse-family-compx-xvi-page64.md.
//!
//! Read-only requests only: battery 0x83, active profile 0x85, polling 0x80, paired id 0x8B.

use super::id_listed;
use super::mouse_io::{FEAT, feature_xfer};
use crate::devices::{Driver, HidHandle, HidInfo, Reading, hid};

const IDS: &[(u16, u16)] = &[
    (0x36a7, 0), // WLMouse
    (0x373e, 0), // Lamzu Maya X, Attack Shark, LunaFury, CRDRAKO, Vancer, RAWM
    (0x37b0, 0), // Lamzu Inca / Paro Aurora
    (0x33e4, 0), // G-Wolves XVI:1
    (0x3770, 0), // inferred: Hitscan (command table unverified on hardware)
];
/// Other protocols on the same VIDs: Beast X 4K (report 4) and G-Wolves XVI:0 (Family A).
const EXCLUDED: &[(u16, u16)] = &[(0x36a7, 0xa887), (0x33e4, 0x5618), (0x33e4, 0x3854)];

/// Feature report 0 request (leading byte = report id): target, length, page, command, arg0.
const fn req(target: u8, len: u8, page: u8, cmd: u8, arg: u8) -> [u8; 65] {
    let mut b = [0u8; 65];
    b[3] = target;
    b[4] = len;
    b[5] = page;
    b[6] = cmd;
    b[7] = arg;
    b
}

const BATTERY: [u8; 65] = req(2, 2, 0, 0x83, 0);
const PROFILE: [u8; 65] = req(2, 1, 0, 0x85, 0);
/// arg0 (byte 7) = 1-based profile, filled in before sending.
const RATE: [u8; 65] = req(2, 2, 1, 0x80, 1);
const PAIRED: [u8; 65] = req(1, 6, 0, 0x8B, 2);

/// Sends `q`, waits for the reply echoing its page/command. Reply body (status
/// first, as in the docs) is returned. `None` = no answer, asleep, unsupported.
fn ask(dev: &HidHandle, q: &[u8; 65]) -> Option<Vec<u8>> {
    let (page, cmd) = (q[5], q[6]);
    for _ in 0..2 {
        // The reply may or may not carry the report id byte; replies get lost, so resend once.
        let r = feature_xfer(dev, q, 30, 8, |buf| {
            let b = [1usize, 0].into_iter().find_map(|o| {
                buf.get(o..).filter(|b| b.len() >= 8 && b[4] == page && b[5] == cmd && matches!(b[0], 0xA0..=0xA3 | 2))
            })?;
            match b[0] {
                0xA1 => Some(Some(b.to_vec())),
                0xA2 | 2 => Some(None), // unsupported / mouse asleep: stop
                _ => None,              // pending or busy: keep polling
            }
        });
        match r {
            Some(Some(v)) => return Some(v),
            Some(None) => return None,
            None => {}
        }
    }
    None
}

/// (percent, charging) from a 0x83 reply body.
fn parse_battery(b: &[u8]) -> Option<(u8, bool)> {
    (b.len() >= 8 && b[6] <= 1 && b[7] <= 100).then(|| (b[7], b[6] == 1))
}

fn hz_for(code: u8) -> Option<u32> {
    Some(match code {
        0x08 => 125,
        0x04 => 250,
        0x02 => 500,
        0x01 | 0x10 => 1000,
        0x20 => 2000,
        0x40 => 4000,
        0x80 => 8000,
        _ => return None,
    })
}

/// Paired mouse product id (big endian) -> model, per receiver VID.
fn paired_name(vid: u16, id: u16) -> Option<&'static str> {
    Some(match (vid, id) {
        (0x36a7, 0xa860 | 0xa861) => "WLMouse Beast G",
        (0x36a7, 0xa863 | 0xa864) => "WLMouse Huan",
        (0x36a7, 0xa866 | 0xa867) => "WLMouse Beast Miao",
        (0x36a7, 0xa868 | 0xa869) => "WLMouse Beast Mini Pro",
        (0x36a7, 0xa870 | 0xa871) => "WLMouse Beast X Pro",
        (0x36a7, 0xa872 | 0xa873) => "WLMouse Strider",
        (0x36a7, 0xa874 | 0xa875) => "WLMouse Ying",
        (0x36a7, 0xa878 | 0xa879) => "WLMouse Sword X",
        (0x36a7, 0xa880 | 0xa881) => "WLMouse Beast Max",
        (0x36a7, 0xa883 | 0xa884) => "WLMouse Beast X",
        (0x36a7, 0xa885 | 0xa886) => "WLMouse Beast Mini",
        (0x373e, 0x001c..=0x001e) => "Lamzu Maya X",
        (0x373e, 0x0046 | 0x0047) => "Attack Shark R5 Ultra",
        (0x373e, 0x0021 | 0x0022) => "Attack Shark R6",
        (0x373e, 0x003a | 0x003b) => "Attack Shark R8",
        (0x373e, 0x0032 | 0x0033) => "LunaFury LUNA33",
        (0x373e, 0x0054 | 0x0084) => "LunaFury TYPE33",
        (0x373e, 0x006a | 0x006b) => "CRDRAKO KO-ONE",
        (0x373e, 0x0066 | 0x0067) => "Vancer Akitsu Medium 3950",
        (0x373e, 0x0098 | 0x0099) => "RAWM Leviathan V4 GT",
        (0x37b0, 0x0009 | 0x000f | 0x0010) => "Lamzu Inca 8K",
        (0x37b0, 0x0007 | 0x000d | 0x000e) => "Lamzu Paro Aurora",
        _ => return None,
    })
}

pub struct CompxXvi;

impl Driver for CompxXvi {
    fn name(&self) -> &'static str {
        "compx-xvi"
    }

    fn matches(&self, info: &HidInfo) -> bool {
        id_listed(IDS, info) && !id_listed(EXCLUDED, info) && info.is_vendor() && info.feature_len == 65
    }

    fn read(&self, dev: &HidHandle, info: &HidInfo) -> Option<Reading> {
        // The control channel is the collection with only the unnumbered feature report.
        if hid::report_ids(&info.path)[FEAT] != [0] {
            return None;
        }
        let (battery, charging) = parse_battery(&ask(dev, &BATTERY)?)?;
        // inferred: the 0x85 reply carries the 1-based active profile in its first payload byte.
        let profile = ask(dev, &PROFILE).map_or(1, |b| b[6].clamp(1, 5));
        let mut q = RATE;
        q[7] = profile;
        let polling_hz = ask(dev, &q).and_then(|b| hz_for(b[7]));
        let name = ask(dev, &PAIRED)
            .and_then(|b| paired_name(info.vid, u16::from_be_bytes([b[10], b[11]])))
            .map(String::from);
        Some(Reading { name, battery: Some(battery), charging: Some(charging), polling_hz, ..Default::default() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_layout_matches_docs() {
        // docs: battery request `00 00 02 02 00 83 00 ...` after the report id
        assert_eq!(&BATTERY[..8], &[0, 0, 0, 2, 2, 0, 0x83, 0]);
        assert_eq!(&RATE[..8], &[0, 0, 0, 2, 2, 1, 0x80, 1]);
        assert_eq!(&PAIRED[..8], &[0, 0, 0, 1, 6, 0, 0x8B, 2]);
    }

    #[test]
    fn battery_reply() {
        let body = [0xA1, 0, 2, 2, 0, 0x83, 1, 87];
        assert_eq!(parse_battery(&body), Some((87, true)));
        assert_eq!(parse_battery(&[0xA1, 0, 2, 2, 0, 0x83, 0, 101]), None);
        assert_eq!(parse_battery(&[0xA1, 0, 2, 2, 0, 0x83, 2, 50]), None);
    }

    #[test]
    fn rate_and_names() {
        assert_eq!(hz_for(0x40), Some(4000));
        assert_eq!(hz_for(0x10), Some(1000));
        assert_eq!(hz_for(0x03), None);
        assert_eq!(paired_name(0x36a7, 0xa883), Some("WLMouse Beast X"));
        assert_eq!(paired_name(0x36a7, 0x1234), None);
    }
}
