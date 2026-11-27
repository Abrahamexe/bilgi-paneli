//! Family A: CompX "report 8" 0x55-checksum protocol (ATK / VXE / Pulsar Nordic /
//! Lamzu Atlantis / Scyrox / VGN / Kysona / G-Wolves XVI:0 / Dareu / Endgame WE).
//! Docs: docs/protocols/mouse-family-compx-report8.md.
//!
//! Only the read requests below are ever sent: link 0x03, battery 0x04, flash read
//! 0x08 at address 0, CID/MID 0x10, identity 0x01. Never 0x05/0x07/0x09/0x0B/0x0D/0x0F/0xF0.

use super::id_listed;
use super::mouse_io::{FEAT, IN, OUT, declares};
use super::util::recv_until;
use crate::devices::{Driver, HidHandle, HidInfo, Reading, enumerate, hid};

/// Variant A1: output report 8 (control SET_REPORT) -> input report 8. pid 0 = any.
const A1_IDS: &[(u16, u16)] = &[
    (0x3554, 0), // ATK / VXE / Pulsar Nordic / Lamzu Atlantis / Scyrox / VGN / Kysona
    (0x373b, 0), // ATK dongles
    (0x3710, 0x5406), (0x3710, 0x3414), (0x3710, 0x3527), (0x3710, 0x5504), (0x3710, 0x7507), // Pulsar CrazyLight / Noctua
    (0x33e4, 0x5618), (0x33e4, 0x3854), // G-Wolves HTX Ultra (XVI:0)
    (0x260d, 0x1117), (0x260d, 0x1114), // Dareu
];
/// Variant A2: feature report 8 on the command collection -> input report 9 on a sibling.
const A2_IDS: &[(u16, u16)] = &[
    (0x25a7, 0xfa7b), (0x25a7, 0xfa7c), // Pulsar X2 gen 1
    (0x3367, 0x1962), (0x3367, 0x1972), (0x3367, 0x1961), (0x3367, 0x1970),
    (0x3367, 0x1960), (0x3367, 0x1968), (0x3367, 0x1982), // Endgame Gear OP1we / XM2we
];

/// 17-byte frame; byte 16 = (0x55 - sum of bytes 0..16), report id included.
const fn frame(head: &[u8]) -> [u8; 17] {
    let mut f = [0u8; 17];
    let mut sum = 0u8;
    let mut i = 0;
    while i < head.len() {
        f[i] = head[i];
        sum = sum.wrapping_add(head[i]);
        i += 1;
    }
    f[16] = 0x55u8.wrapping_sub(sum);
    f
}

fn checksum_ok(f: &[u8]) -> bool {
    f.len() >= 17 && f[16] == 0x55u8.wrapping_sub(f[..16].iter().fold(0u8, |a, &b| a.wrapping_add(b)))
}

const LINK: [u8; 17] = frame(&[8, 3]);
const BATTERY: [u8; 17] = frame(&[8, 4]);
const CID_MID: [u8; 17] = frame(&[8, 0x10]);
/// Read 2 bytes of flash at address 0x0000 (polling code, value + 0x55-value).
const FLASH_RATE: [u8; 17] = frame(&[8, 8, 0, 0, 0, 2]);
/// inferred: fixed nonce instead of a random one; only cid/mid ([10],[11]) are used from the reply.
const IDENTITY: [u8; 17] = frame(&[8, 1, 0, 0, 0, 8, 1, 2, 3, 4]);

/// (battery percent, charging) from a cmd 0x04 reply.
fn parse_battery(r: &[u8]) -> Option<(u8, Option<bool>)> {
    if r.len() < 8 || r[1] != 4 || r[6] > 100 {
        return None;
    }
    Some((r[6], match r[7] { 0 => Some(false), 1 => Some(true), _ => None }))
}

/// Polling code -> Hz. CompX table; the Lamzu Atlantis 1K receiver / wired PIDs use
/// the Lamzu table for the ambiguous codes >= 0x10 (docs section 5).
fn hz_for(vid: u16, pid: u16, code: u8) -> Option<u32> {
    let lamzu = vid == 0x3554 && matches!(pid, 0xf50d | 0xf50f);
    Some(match (code, lamzu) {
        (0x08, _) => 125,
        (0x04, _) => 250,
        (0x02, _) => 500,
        (0x01, _) => 1000,
        (0x10, false) => 2000,
        (0x20, false) => 4000,
        (0x40, false) => 8000,
        (0x10, true) => 1000,
        (0x20, true) => 2000,
        (0x40, true) => 4000,
        (0x80, true) => 8000,
        _ => return None,
    })
}

/// Flash reply: bytes [6],[7] = code, 0x55-code.
fn parse_hz(r: &[u8], vid: u16, pid: u16) -> Option<u32> {
    if r.len() < 8 || r[1] != 8 || r[6].wrapping_add(r[7]) != 0x55 {
        return None;
    }
    hz_for(vid, pid, r[6])
}

fn model_name(cid: u8, mid: u8) -> Option<&'static str> {
    Some(match (cid, mid) {
        (6, 7) => "Pulsar Xlite V3",
        (6, 8) => "Pulsar X2A",
        (6, 0x0a) => "Pulsar X2V2",
        (6, 0x0c) => "Pulsar X2H",
        (0x57, 3) => "Pulsar X2 CrazyLight",
        (0x57, 0x68) => "Pulsar X3 LHD CrazyLight",
        (0x3e, 5) => "Scyrox V8S",
        (2, 27) => "VXE R1 Pro Max",
        _ => return None,
    })
}

struct Link<'a> {
    tx: &'a HidHandle,
    /// A2 only: handle of the sibling collection that carries input report 9.
    rx: Option<HidHandle>,
}

impl Link<'_> {
    /// Sends one request and returns the reply that echoes its command.
    fn call(&self, req: &[u8; 17]) -> Option<Vec<u8>> {
        let a2 = self.rx.is_some();
        let sent = if a2 { self.tx.set_feature(req) } else { self.tx.set_output(req) || self.tx.write(req) };
        if !sent {
            return None;
        }
        let id = if a2 { 9 } else { 8 };
        // inferred (A2): the checksum of input report 9 is not validated.
        recv_until(self.rx.as_ref().unwrap_or(self.tx), 400, |r| {
            (r.len() >= 17 && r[0] == id && r[1] == req[1] && (a2 || checksum_ok(r))).then(|| r.to_vec())
        })
    }

    fn name(&self, info: &HidInfo) -> Option<String> {
        let (vid, pid) = (info.vid, info.pid);
        let ids = if vid == 0x3554 && matches!(pid, 0xf58a | 0xf58c | 0xf58e | 0xf58f) {
            let r = self.call(&CID_MID)?;
            (r[6], r[7])
        } else if (vid == 0x3554 && matches!(pid, 0xf507..=0xf509 | 0xf515 | 0xf517 | 0xf5f7)) || vid == 0x3710 {
            let r = self.call(&IDENTITY)?;
            (r[10], r[11])
        } else {
            return None;
        };
        model_name(ids.0, ids.1).map(String::from)
    }
}

pub struct CompxReport8;

impl Driver for CompxReport8 {
    fn name(&self) -> &'static str {
        "compx-report8"
    }

    fn matches(&self, info: &HidInfo) -> bool {
        (id_listed(A1_IDS, info) && (info.vid, info.pid) != (0x3554, 0xFA09) /* MCHOSE G75 keyboard */ && info.is_vendor() && info.input_len >= 17 && info.output_len >= 17)
            || (id_listed(A2_IDS, info) && info.usage_page == 0xFF02 && info.feature_len >= 17)
    }

    fn read(&self, dev: &HidHandle, info: &HidInfo) -> Option<Reading> {
        let rx = if id_listed(A2_IDS, info) {
            if !declares(info, FEAT, 8) {
                return None;
            }
            // inferred: the notify collection is the sibling interface of the same device declaring input 9.
            let sib = enumerate::list_hid().into_iter().find(|s| {
                s.path != info.path && (s.vid, s.pid, &s.container) == (info.vid, info.pid, &info.container) && declares(s, IN, 9)
            })?;
            Some(HidHandle::open(&sib)?)
        } else {
            let ids = hid::report_ids(&info.path);
            if !(ids[IN].contains(&8) && ids[OUT].contains(&8)) {
                return None;
            }
            None
        };
        let link = Link { tx: dev, rx };
        let _ = link.call(&LINK); // wakes the dongle's radio link; the answer itself is not needed
        let (battery, charging) = parse_battery(&link.call(&BATTERY)?)?;
        Some(Reading {
            name: link.name(info),
            battery: Some(battery),
            charging,
            polling_hz: link.call(&FLASH_RATE).and_then(|r| parse_hz(&r, info.vid, info.pid)),
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_match_documented_checksums() {
        assert_eq!(BATTERY[16], 0x49);
        assert_eq!(LINK[16], 0x4A);
        assert_eq!(CID_MID[16], 0x3D);
        assert_eq!(frame(&[8, 0x12])[16], 0x3B); // firmware request, from the docs
        assert_eq!(&BATTERY[..3], &[8, 4, 0]);
        assert_eq!(FLASH_RATE[5], 2);
        assert!(checksum_ok(&BATTERY) && checksum_ok(&FLASH_RATE) && checksum_ok(&IDENTITY));
        let mut bad = BATTERY;
        bad[6] = 1;
        assert!(!checksum_ok(&bad));
    }

    #[test]
    fn battery_reply() {
        // 98 %, charging, 4000 mV
        let r = frame(&[8, 4, 0, 0, 0, 0, 0x62, 1, 0x0F, 0xA0]);
        assert!(checksum_ok(&r));
        assert_eq!(parse_battery(&r), Some((98, Some(true))));
        assert_eq!(parse_battery(&frame(&[8, 4, 0, 0, 0, 0, 101, 0])), None);
        assert_eq!(parse_battery(&frame(&[8, 4, 0, 0, 0, 0, 50, 7])), Some((50, None)));
    }

    #[test]
    fn polling_tables() {
        let rate = |code: u8, pid| parse_hz(&frame(&[8, 8, 0, 0, 0, 2, code, 0x55 - code]), 0x3554, pid);
        assert_eq!(rate(0x01, 0xf507), Some(1000));
        assert_eq!(rate(0x20, 0xf509), Some(4000)); // Pulsar: CompX table
        assert_eq!(rate(0x40, 0xf510), Some(8000)); // Atlantis 4K receiver: CompX table
        assert_eq!(rate(0x10, 0xf50f), Some(1000)); // Atlantis wired: Lamzu table
        assert_eq!(rate(0x20, 0xf50d), Some(2000));
        assert_eq!(rate(0x80, 0xf507), None); // ambiguous outside the Lamzu table
        assert_eq!(rate(0x03, 0xf507), None);
        assert_eq!(parse_hz(&frame(&[8, 8, 0, 0, 0, 2, 1, 0x50]), 0x3554, 0xf507), None); // bad pair
    }

    #[test]
    fn names() {
        assert_eq!(model_name(2, 27), Some("VXE R1 Pro Max"));
        assert_eq!(model_name(6, 8), Some("Pulsar X2A"));
        assert_eq!(model_name(1, 1), None);
    }
}
