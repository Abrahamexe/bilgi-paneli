//! ASUS ROG wireless keyboards (VID 0x0B05): Falchion RX LP via Omni receiver 1ACE, Azoth 96 HE
//! via SpeedNova receiver 1AD0 and wired 1C10. Docs: docs/protocols/keyboard-asus-rog.md.
//! Read requests only: `{rid, 12, 01}` (battery) and `{rid, 12, 00}` (wake/ping).
//! No polling-rate query is known (Armoury Crate only).

use super::kb_util::exchange;
use super::util::sleep_ms;
use crate::devices::{Driver, HidHandle, HidInfo, Reading};

const OMNI: u16 = 0x1ACE;
const SPEEDNOVA: u16 = 0x1AD0;
const AZOTH_WIRED: u16 = 0x1C10;

pub struct AsusRog;

/// Report id of the vendor collection: 2 on the receivers, 0 on the wired Azoth.
fn rid(pid: u16) -> u8 {
    if pid == AZOTH_WIRED { 0 } else { 2 }
}

fn request(rid: u8, arg: u8) -> [u8; 3] {
    [rid, 0x12, arg]
}

/// `rx` includes the report id at [0]. Falchion: percent at [6]. Azoth: valid if
/// `[5] == 0 && [6] <= 100 && [9] <= 1`, `[9] == 1` = charging. Percent 0 is treated as unknown.
fn parse(rx: &[u8], rid: u8, azoth: bool) -> Option<(u8, Option<bool>)> {
    if rx.len() < 10 || rx[0] != rid || rx[1] != 0x12 || rx[2] != 0x01 || rx[6] > 100 || rx[6] == 0 {
        return None;
    }
    if !azoth {
        return Some((rx[6], None));
    }
    (rx[5] == 0 && rx[9] <= 1).then_some((rx[6], Some(rx[9] == 1)))
}

impl Driver for AsusRog {
    fn name(&self) -> &'static str {
        "asus-rog-kb"
    }

    fn matches(&self, info: &HidInfo) -> bool {
        if info.vid != 0x0B05 {
            return false;
        }
        match info.pid {
            // Omni receiver: collection 0xFF02/1. SpeedNova: 0xFF00/1 (not the mouse slot 0xFF01).
            OMNI => super::usage_is(info, 0xFF02, 0x01) && info.output_len >= 64,
            SPEEDNOVA => super::usage_is(info, 0xFF00, 0x01) && info.output_len >= 64,
            AZOTH_WIRED => info.is_vendor() && info.input_len == 65 && info.output_len == 65,
            _ => false,
        }
    }

    fn read(&self, dev: &HidHandle, info: &HidInfo) -> Option<Reading> {
        let rid = rid(info.pid);
        let azoth = info.pid != OMNI;
        let ok = |r: &[u8]| r.len() > 2 && r[0] == rid && r[1] == 0x12 && r[2] == 0x01;
        let mut rsp = exchange(dev, &request(rid, 1), 400, ok);
        if rsp.is_none() {
            // Sleeping keyboard: ping, then ask again with a longer wait.
            let _ = dev.write(&request(rid, 0));
            sleep_ms(200);
            rsp = exchange(dev, &request(rid, 1), 750, ok);
        }
        let (battery, charging) = parse(&rsp?, rid, azoth)?;
        Some(Reading { battery: Some(battery), charging, ..Default::default() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falchion() {
        let mut r = [0u8; 64];
        r[..7].copy_from_slice(&[2, 0x12, 1, 0, 0, 0, 71]);
        assert_eq!(parse(&r, 2, false), Some((71, None)));
        r[6] = 101;
        assert_eq!(parse(&r, 2, false), None);
        // sleep frame `02 FF AA`
        assert_eq!(parse(&[2, 0xFF, 0xAA, 0, 0, 0, 50, 0, 0, 0], 2, false), None);
    }

    #[test]
    fn azoth() {
        let mut r = [0u8; 64];
        r[..7].copy_from_slice(&[2, 0x12, 1, 0, 0, 0, 88]);
        assert_eq!(parse(&r, 2, true), Some((88, Some(false))));
        r[9] = 1;
        assert_eq!(parse(&r, 2, true), Some((88, Some(true))));
        r[9] = 2;
        assert_eq!(parse(&r, 2, true), None);
        r[9] = 0;
        r[5] = 0xFA; // not ready
        assert_eq!(parse(&r, 2, true), None);
        // wired: report id 0
        let w = [0, 0x12, 1, 0, 0, 0, 40, 0, 0, 1];
        assert_eq!(parse(&w, 0, true), Some((40, Some(true))));
    }

    #[test]
    fn requests() {
        assert_eq!(request(2, 1), [2, 0x12, 1]);
        assert_eq!(rid(AZOTH_WIRED), 0);
    }
}
