//! EPOS H3PRO Hybrid 2.4 GHz dongle (docs/protocols/headset-epos-sennheiser.md).
//!
//! Framed 106-byte reports (report id 1). Read-only: "headset connected?" and
//! "battery percent" queries. Not covered: Sennheiser GSP 370 (serial port, not
//! HID), Jabra GN (not requested), the cabled H3PRO (0x0101, no documented
//! battery target).

use super::hs::{Bat, Port, reading};
use crate::devices::{Connection, Driver, HidHandle, HidInfo, Reading};

const VID: u16 = 0x1395;
const DONGLE: u16 = 0x0303;

/// Allow-list: (target, payload words group/command) of the two queries.
const TARGET_DONGLE: u16 = 0x0001;
const TARGET_HEADSET: u16 = 0x0005;
const Q_CONNECTED: [u8; 4] = [0x01, 0x00, 0x09, 0x00];
const Q_BATTERY: [u8; 4] = [0x12, 0x00, 0x03, 0x00];

/// CRC-8, polynomial 0x07, init 0xFF, no reflection, no final xor.
fn crc8(data: &[u8]) -> u8 {
    data.iter().fold(0xFF, |mut c, b| {
        c ^= b;
        for _ in 0..8 {
            c = if c & 0x80 != 0 { c << 1 ^ 0x07 } else { c << 1 };
        }
        c
    })
}

/// `01 EB FE <ver 01 00> <type 04 00> <target> <len 04 00> <crc 00> <payload>`.
fn frame(target: u16, payload: &[u8; 4]) -> Vec<u8> {
    debug_assert!(*payload == Q_CONNECTED || *payload == Q_BATTERY);
    let mut v = vec![0x01, 0xEB, 0xFE, 0x01, 0x00, 0x04, 0x00];
    v.extend(target.to_le_bytes());
    v.extend([0x04, 0x00, crc8(payload), 0x00]);
    v.extend(payload);
    v
}

enum Answer {
    /// Value word of the reply.
    Value(u16),
    /// The dongle itself answered a headset-targeted request: headset off.
    Unanswered,
}

fn parse(r: &[u8], want: &[u8; 4]) -> Option<Answer> {
    if r.len() < 19 || r[0] != 0x01 || r[1..3] != [0xEB, 0xFE] || r[13..17] != *want {
        return None;
    }
    match u16::from_le_bytes([r[7], r[8]]) {
        TARGET_DONGLE if *want == Q_BATTERY => Some(Answer::Unanswered),
        // Replies carry the request target minus one.
        t if t == TARGET_DONGLE - 1 || t == TARGET_HEADSET - 1 => Some(Answer::Value(u16::from_le_bytes([r[17], r[18]]))),
        _ => None,
    }
}

pub struct Epos;

impl Driver for Epos {
    fn name(&self) -> &'static str {
        "epos-h3pro"
    }

    fn matches(&self, info: &HidInfo) -> bool {
        info.vid == VID
            && info.pid == DONGLE
            && info.usage_page == 0xFFFF
            && info.usage == 0x0001
            && info.interface.unwrap_or(3) == 3
    }

    fn read(&self, dev: &HidHandle, info: &HidInfo) -> Option<Reading> {
        let p = Port::new(dev, info);
        let ask = |target, q: &[u8; 4]| p.ask(&frame(target, q), 400, 6, |r| parse(r, q).is_some());
        let link = parse(&ask(TARGET_DONGLE, &Q_CONNECTED)?, &Q_CONNECTED)?;
        let bat = match (link, ask(TARGET_HEADSET, &Q_BATTERY)) {
            (Answer::Value(0), _) => Bat::OFF,
            (_, Some(r)) => match parse(&r, &Q_BATTERY)? {
                Answer::Value(v) => Bat::new(u8::try_from(v).ok()?, None)?,
                Answer::Unanswered => Bat::OFF,
            },
            (_, None) => Bat::OFF,
        };
        Some(reading(bat, Connection::Dongle))
    }
}

#[cfg(test)]
mod tests {
    use super::super::util::hex;
    use super::*;

    #[test]
    fn crc_and_frames_match_documented_examples() {
        assert_eq!(crc8(&Q_BATTERY), 0xA5);
        assert_eq!(crc8(&Q_CONNECTED), 0x7A);
        assert_eq!(frame(TARGET_HEADSET, &Q_BATTERY), hex("01 EB FE 01 00 04 00 05 00 04 00 A5 00 12 00 03 00"));
        assert_eq!(frame(TARGET_DONGLE, &Q_CONNECTED), hex("01 EB FE 01 00 04 00 01 00 04 00 7A 00 01 00 09 00"));
    }

    #[test]
    fn replies() {
        let base = |t: u8, q: [u8; 4], v: u8| {
            let mut r = hex("01 EB FE 01 00 04 00");
            r.extend([t, 0, 6, 0, 0, 0]);
            r.extend(q);
            r.extend([v, 0]);
            r
        };
        assert!(matches!(parse(&base(4, Q_BATTERY, 60), &Q_BATTERY), Some(Answer::Value(60))));
        assert!(matches!(parse(&base(1, Q_BATTERY, 0), &Q_BATTERY), Some(Answer::Unanswered)));
        assert!(matches!(parse(&base(0, Q_CONNECTED, 1), &Q_CONNECTED), Some(Answer::Value(1))));
        assert!(parse(&base(4, Q_CONNECTED, 60), &Q_BATTERY).is_none());
        assert!(parse(&hex("02 EB FE"), &Q_BATTERY).is_none());
    }
}
