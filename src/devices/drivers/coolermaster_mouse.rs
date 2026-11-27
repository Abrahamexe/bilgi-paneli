//! Cooler Master wireless mice (MM712 / MM731 and the "Mice Dongle"): vendor
//! interface 3, no report id. See docs/protocols/mouse-coolermaster.md.
//!
//! READ-ONLY allow-list: `46 81 00` (status report 0x47) and `4A 81 03` (performance
//! block, polling rate). Never `4A 02/04`, `44 01`, `54 01`, `48 ..`, `4C ..`.

use super::util::{drain, payload, recv_until};
use crate::devices::hid::IO_TIMEOUT_MS;
use crate::devices::{Driver, HidHandle, HidInfo, Reading};

const VID: u16 = 0x2516;
const REQ_STATUS: [u8; 4] = [0x00, 0x46, 0x81, 0x00];
const REQ_PERF: [u8; 4] = [0x00, 0x4A, 0x81, 0x03];

/// (pid, model byte of a cable mouse, 0 = the shared dongle)
const DEVICES: &[(u16, u8)] = &[(0x01B7, 0), (0x0169, 0x50), (0x0167, 0x70)];

struct Status {
    battery: Option<u8>,
    charging: bool,
    name: Option<&'static str>,
}

fn model_name(model: u8) -> Option<&'static str> {
    match model {
        0x50 => Some("Cooler Master MM712"),
        0x70 => Some("Cooler Master MM731"), // seen once
        _ => None,
    }
}

/// Status report `47 81 <model> ... [10] battery % [11] charging [12] link`.
fn parse_status(r: &[u8]) -> Option<Status> {
    let d = payload(r, &[0x47, 0x81])?;
    let (batt, chg) = (*d.get(10)?, *d.get(11)?);
    Some(Status { battery: (1..=100).contains(&batt).then_some(batt), charging: chg == 1, name: model_name(d[2]) })
}

/// Performance block short read `4B 81 03 ...`: byte 10 = polling code 1..4.
// inferred: byte 10 derives from "block data byte 7, data starts at byte 3"
fn parse_rate(r: &[u8]) -> Option<u32> {
    match *payload(r, &[0x4B, 0x81, 0x03])?.get(10)? {
        c @ 1..=4 => Some(125 << (c - 1)),
        _ => None,
    }
}

pub struct CoolerMasterMouse;

impl Driver for CoolerMasterMouse {
    fn name(&self) -> &'static str {
        "coolermaster_mouse"
    }

    fn matches(&self, info: &HidInfo) -> bool {
        info.vid == VID
            && DEVICES.iter().any(|d| d.0 == info.pid)
            && info.interface.unwrap_or(3) == 3
            && info.usage_page == 0xFF0A
    }

    fn read(&self, dev: &HidHandle, info: &HidInfo) -> Option<Reading> {
        let cable_model = DEVICES.iter().find(|d| d.0 == info.pid)?.1;
        drain(dev);
        if !dev.write(&REQ_STATUS) {
            return None;
        }
        // Unsolicited status reports also arrive here; anything else is skipped.
        let st = recv_until(dev, IO_TIMEOUT_MS, parse_status)?;
        let mut r = Reading {
            name: st.name.or_else(|| model_name(cable_model)).map(String::from),
            battery: st.battery,
            charging: st.battery.map(|_| st.charging),
            ..Default::default()
        };
        if dev.write(&REQ_PERF) {
            r.polling_hz = recv_until(dev, IO_TIMEOUT_MS, parse_rate);
        }
        Some(r)
    }
}

#[cfg(test)]
mod tests {
    use super::super::util::hex;
    use super::*;

    #[test]
    fn status_reply() {
        // 47 81 <MM712> .. .. .. .. .. [10]=63 % [11]=charging [12]=wireless
        let r = hex("00 47 81 50 00 00 00 00 00 00 00 3F 01 01 00 00");
        let s = parse_status(&r).unwrap();
        assert_eq!((s.battery, s.charging, s.name), (Some(63), true, Some("Cooler Master MM712")));
        let s = parse_status(&hex("47 81 70 00 00 00 00 00 00 00 00 00 00 01")).unwrap();
        assert_eq!((s.battery, s.name), (None, Some("Cooler Master MM731"))); // 0 = invalid
        assert!(parse_status(&hex("4B 81 03 00 00 00 00 00 00 00 20 00")).is_none());
        assert!(parse_status(&hex("47 81 50")).is_none());
    }

    #[test]
    fn polling_reply() {
        let r = hex("00 4B 81 03 00 00 00 00 00 00 00 04 00"); // with Windows' report id
        assert_eq!(parse_rate(&r), Some(1000));
        assert_eq!(parse_rate(&hex("4B 81 03 00 00 00 00 00 00 00 04 00")), Some(1000));
        assert_eq!(parse_rate(&hex("4B 81 03 00 00 00 00 00 00 00 01")), Some(125));
        assert_eq!(parse_rate(&hex("4B 81 03 00 00 00 00 00 00 00 03")), Some(500));
        assert_eq!(parse_rate(&hex("4B 81 03 00 00 00 00 00 00 00 05")), None);
        assert_eq!(parse_rate(&hex("47 81 03 00 00 00 00 00 00 00 04")), None);
    }

    #[test]
    fn requests_are_reads() {
        assert_eq!(REQ_STATUS[1..], [0x46, 0x81, 0x00]);
        assert_eq!(REQ_PERF[1..], [0x4A, 0x81, 0x03]);
    }
}
