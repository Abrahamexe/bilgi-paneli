//! HyperX mice: Pulsefire Dart ("heartbeat" `51`) and the NGENUITY 3 "universal"
//! protocol (Pulsefire Haste 2 Wireless family). See docs/protocols/mouse-hyperx.md.
//!
//! READ-ONLY allow-list: Dart `51`; NGENUITY 3 `50 02` (battery), `52 01` (link),
//! `32 02 00` (DPI block, polling rate). Never `12 01`, `F0/FA/FC` or any setter.

use super::util::{drain, recv_until, sleep_ms};
use crate::devices::hid::IO_TIMEOUT_MS;
use crate::devices::{Driver, HidHandle, HidInfo, Reading};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Proto {
    Dart,
    Ngenuity,
}

/// (vid, pid, protocol, USB interface, usage page)
const DEVICES: &[(u16, u16, Proto, u8, u16)] = &[
    (0x0951, 0x16E1, Proto::Dart, 2, 0xFF00), // dongle
    (0x03F0, 0x068E, Proto::Dart, 2, 0xFF00),
    (0x0951, 0x16E2, Proto::Dart, 1, 0xFF13), // wired
    (0x03F0, 0x088E, Proto::Dart, 1, 0xFF13),
    (0x03F0, 0x0F98, Proto::Ngenuity, 2, 0xFF00), // Haste 2 Wireless dongle
    (0x03F0, 0x0D97, Proto::Ngenuity, 2, 0xFF00),
];

// Report id 0 + 64 bytes (Dart); the NGENUITY 3 report id is the command itself.
const REQ_DART: [u8; 2] = [0x00, 0x51];
const REQ_BATTERY: [u8; 2] = [0x50, 0x02];
const REQ_LINK: [u8; 2] = [0x52, 0x01];
const REQ_DPI: [u8; 3] = [0x32, 0x02, 0x00]; // profile 0 = live

fn device(info: &HidInfo) -> Option<&'static (u16, u16, Proto, u8, u16)> {
    DEVICES.iter().find(|d| (d.0, d.1) == (info.vid, info.pid))
}

/// Dart heartbeat reply `51 .. .. 03 <battery> <charging>`.
fn parse_dart(d: &[u8]) -> Option<(u8, bool)> {
    let (len, pct, chg) = (*d.get(3)?, *d.get(4)?, *d.get(5)?);
    (len == 3 && pct <= 100 && chg <= 1).then_some((pct, chg == 1))
}

/// NGENUITY 3 `51 02 <battery> <status> ...`: status 0 no, 1 charging, 2 full, 3 error, 0xFF unknown.
fn parse_battery(d: &[u8]) -> Option<(u8, Option<bool>)> {
    if !d.starts_with(&[0x51, 0x02]) {
        return None;
    }
    let (pct, st) = (*d.get(2)?, *d.get(3)?);
    (pct <= 100).then_some((pct, match st {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    }))
}

/// NGENUITY 3 `53 01 <connected>`.
fn parse_link(d: &[u8]) -> Option<bool> {
    d.starts_with(&[0x53, 0x01]).then(|| *d.get(2).unwrap_or(&0) == 1)
}

/// `33 02 <polling code> ...`.
// inferred: byte position (the read form drops the two profile bytes of the write form)
fn parse_rate(d: &[u8]) -> Option<u32> {
    if !d.starts_with(&[0x33, 0x02]) {
        return None;
    }
    match d.get(2)? {
        0x01 => Some(8000),
        0x02 => Some(4000),
        0x04 => Some(2000),
        0x08 => Some(1000),
        0x10 => Some(500),
        0x20 => Some(250),
        0x40 => Some(125),
        _ => None,
    }
}

fn ask<T>(dev: &HidHandle, req: &[u8], f: impl FnMut(&[u8]) -> Option<T>) -> Option<T> {
    if !dev.write(req) {
        return None;
    }
    recv_until(dev, IO_TIMEOUT_MS, f)
}

fn read_dart(dev: &HidHandle) -> Option<Reading> {
    let (battery, charging) = ask(dev, &REQ_DART, |r| parse_dart(super::util::payload(r, &[0x51])?))?;
    Some(Reading {
        name: Some("HyperX Pulsefire Dart".into()),
        battery: Some(battery),
        charging: Some(charging),
        ..Default::default()
    })
}

fn read_ngenuity(dev: &HidHandle) -> Option<Reading> {
    // The dongle answers the link query even when the mouse sleeps.
    if !ask(dev, &REQ_LINK, parse_link)? {
        return None;
    }
    sleep_ms(10); // NGENUITY paces packets >= 10 ms
    let (battery, charging) = ask(dev, &REQ_BATTERY, parse_battery)?;
    sleep_ms(10);
    let polling_hz = ask(dev, &REQ_DPI, parse_rate);
    Some(Reading { battery: Some(battery), charging, polling_hz, ..Default::default() })
}

pub struct HyperxMouse;

impl Driver for HyperxMouse {
    fn name(&self) -> &'static str {
        "hyperx_mouse"
    }

    fn matches(&self, info: &HidInfo) -> bool {
        device(info).is_some_and(|d| info.interface.unwrap_or(d.3) == d.3 && info.usage_page == d.4)
            && (info.usage_page != 0xFF00 || info.usage == 1)
    }

    fn read(&self, dev: &HidHandle, info: &HidInfo) -> Option<Reading> {
        drain(dev);
        match device(info)?.2 {
            Proto::Dart => read_dart(dev),
            Proto::Ngenuity => read_ngenuity(dev),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::util::hex;
    use super::*;

    #[test]
    fn dart_heartbeat() {
        // doc layout: 51, ., ., 03, battery, charging, 0F, ...
        assert_eq!(parse_dart(&hex("51 00 00 03 39 00 0F 12 34")), Some((57, false)));
        assert_eq!(parse_dart(&hex("51 00 00 03 64 01 0F 12 34")), Some((100, true)));
        assert_eq!(parse_dart(&hex("51 00 00 02 39 00")), None);
        assert_eq!(parse_dart(&hex("51 00 00 03 65 00")), None);
    }

    #[test]
    fn ngenuity_replies() {
        assert_eq!(parse_battery(&hex("51 02 4B 00 00 00 00 00")), Some((75, Some(false))));
        assert_eq!(parse_battery(&hex("51 02 4B 01")), Some((75, Some(true))));
        assert_eq!(parse_battery(&hex("51 02 4B 02")), Some((75, None)));
        assert_eq!(parse_battery(&hex("51 02 FF 00")), None);
        assert_eq!(parse_battery(&hex("33 02 4B 00")), None);
        assert_eq!(parse_link(&hex("53 01 01")), Some(true));
        assert_eq!(parse_link(&hex("53 01 00")), Some(false));
        assert_eq!(parse_link(&hex("51 02 00")), None);
        assert_eq!(parse_rate(&hex("33 02 08 07 00")), Some(1000));
        assert_eq!(parse_rate(&hex("33 02 01")), Some(8000));
        assert_eq!(parse_rate(&hex("33 02 03")), None);
        assert_eq!(parse_rate(&hex("51 02 08")), None);
    }

    #[test]
    fn table() {
        assert!(DEVICES.iter().any(|d| d.1 == 0x0F98 && d.2 == Proto::Ngenuity));
        assert!(DEVICES.iter().any(|d| d.1 == 0x16E2 && d.4 == 0xFF13));
    }
}
