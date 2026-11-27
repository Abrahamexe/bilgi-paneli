//! Sony INZONE H5 / H9 II dongles ("Sony vendor HCI over HID"), INZONE Buds and
//! PULSE 3D (passive status reports). docs/protocols/headset-sony-inzone.md.
//!
//! Read-only: the only request ever built is event 0x04 (BATTERY_INFO), type GET.

use super::hs::{Bat, Port, reading};
use crate::devices::{Connection, Driver, HidHandle, HidInfo, Reading};

const VID: u16 = 0x054C;
const H5: u16 = 0x0EBF;
const H9_II: u16 = 0x0FA8;
const BUDS: u16 = 0x0EC2;
const PULSE_3D: u16 = 0x0D5E;

/// Allow-list (event id, event type) of the one frame this driver builds.
const EVENT_BATTERY_INFO: u8 = 0x04;
const TYPE_GET: u8 = 0x01;
const TYPE_RET: u8 = 0x10;
/// Transaction id (the dongle's own pushes use 1).
const TID: u16 = 2;

/// `02 <len> 01 00 FC <plen> 96 C3 <addr 0x41 PC->receiver> <event> <type> <tid> <checksum>`; empty payload.
fn battery_request() -> Vec<u8> {
    let mut v = vec![0x02, 0x0C, 0x01, 0x00, 0xFC, 0x08, 0x96, 0xC3, 0x41, EVENT_BATTERY_INFO, TYPE_GET];
    v.extend(TID.to_le_bytes());
    let sum = v[6..].iter().fold(0u8, |a, b| a.wrapping_add(*b));
    v.push(sum);
    v
}

/// Validated BATTERY_INFO reply: `[charger flag, percent]`; percent 0xFF = headset offline.
fn battery_reply(r: &[u8]) -> Option<Bat> {
    if r.len() < 15 || r[0] != 0x02 || r[2] != 0x04 || r[3] != 0xFF || r[6..8] != [0x96, 0xC3] {
        return None;
    }
    let hl = usize::from(r[1]);
    let tid = u16::from(r[11]) | u16::from(r[12]) << 8;
    if r[8] >> 4 != 1 || r[9] != EVENT_BATTERY_INFO || r[10] != TYPE_RET || tid != TID || hl < 14 || r.len() <= hl + 1 {
        return None;
    }
    let sum = r[5..=hl].iter().fold(0u8, |a, b| a.wrapping_add(*b));
    if sum != r[hl + 1] {
        return None;
    }
    match r[14] {
        0xFF => Some(Bat::OFF),
        p => Bat::new(p, Some(r[13] != 0)),
    }
}

/// INZONE Buds push report `02 12 04 ...`; min of both buds, checksum `b14 + b16 + 117`.
fn buds(r: &[u8]) -> Option<Bat> {
    if r.len() < 20 || r[..3] != [0x02, 0x12, 0x04] || r[19] != r[14].wrapping_add(r[16]).wrapping_add(117) {
        return None;
    }
    Bat::new(r[14].min(r[16]), None)
}

/// PULSE 3D push report `B0 .. .. <battery> <flags>`.
fn pulse(r: &[u8]) -> Option<Bat> {
    if r.len() < 5 || r[0] != 0xB0 {
        return None;
    }
    if r[4] & 0x08 == 0 {
        return Some(Bat::OFF); // headset powered off
    }
    match r[3] {
        0x80 => Some(Bat::charging()),
        p => Bat::new(p, Some(false)),
    }
}

pub struct SonyInzone;

impl Driver for SonyInzone {
    fn name(&self) -> &'static str {
        "sony-inzone"
    }

    fn matches(&self, info: &HidInfo) -> bool {
        if info.vid != VID {
            return false;
        }
        match info.pid {
            H5 => info.usage_page == 0xFF04 && info.usage == 0x0002,
            H9_II => info.usage_page == 0xFF04 && info.usage == 0x0001 && info.interface == Some(5),
            BUDS => info.input_len >= 20,
            PULSE_3D => info.usage_page == 0xFF01 && info.usage == 0x0020,
            _ => false,
        }
    }

    fn read(&self, dev: &HidHandle, info: &HidInfo) -> Option<Reading> {
        let p = Port::new(dev, info);
        let bat = match info.pid {
            H5 | H9_II => battery_reply(&p.ask(&battery_request(), 250, 8, |r| battery_reply(r).is_some())?)?,
            // Passive devices: wait briefly for the next push (they never answer queries).
            BUDS => (0..4).find_map(|_| buds(&p.recv(150)?))?,
            _ => (0..4).find_map(|_| pulse(&p.recv(150)?))?,
        };
        Some(reading(bat, Connection::Dongle))
    }
}

#[cfg(test)]
mod tests {
    use super::super::util::hex;
    use super::*;

    #[test]
    fn request_matches_documented_example() {
        assert_eq!(battery_request(), hex("02 0C 01 00 FC 08 96 C3 41 04 01 02 00 A1"));
    }

    fn reply(payload: [u8; 2], tid: u16) -> Vec<u8> {
        let mut v = vec![0x02, 14, 0x04, 0xFF, 10, 0x00, 0x96, 0xC3, 0x11, 0x04, 0x10];
        v.extend(tid.to_le_bytes());
        v.extend(payload);
        let sum = v[5..].iter().fold(0u8, |a, b| a.wrapping_add(*b));
        v.push(sum);
        v
    }

    #[test]
    fn battery_info_reply() {
        assert_eq!(battery_reply(&reply([1, 80], 2)), Some(Bat { pct: Some(80), chg: Some(true) }));
        assert_eq!(battery_reply(&reply([0, 0xFF], 2)), Some(Bat::OFF));
        assert_eq!(battery_reply(&reply([0, 101], 2)), None);
        // pushed frame (tid 1) and corrupt checksum are rejected
        assert_eq!(battery_reply(&reply([0, 50], 1)), None);
        let mut bad = reply([0, 50], 2);
        *bad.last_mut().unwrap() ^= 1;
        assert_eq!(battery_reply(&bad), None);
    }

    #[test]
    fn passive_reports() {
        let mut b = vec![0u8; 20];
        b[..3].copy_from_slice(&[0x02, 0x12, 0x04]);
        (b[14], b[16]) = (80, 60);
        b[19] = (80u8 + 60).wrapping_add(117);
        assert_eq!(buds(&b), Some(Bat { pct: Some(60), chg: None }));
        b[19] ^= 1;
        assert_eq!(buds(&b), None);
        assert_eq!(pulse(&hex("B0 00 00 50 08 00 00 00")), Some(Bat { pct: Some(80), chg: Some(false) }));
        assert_eq!(pulse(&hex("B0 00 00 80 08 00 00 00")), Some(Bat::charging()));
        assert_eq!(pulse(&hex("B0 00 00 50 00 00 00 00")), Some(Bat::OFF));
        assert_eq!(pulse(&hex("B1 00 00 50 08")), None);
    }
}
