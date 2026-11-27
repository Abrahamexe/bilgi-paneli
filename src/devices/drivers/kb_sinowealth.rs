//! SinoWealth report-0x13 family (MCHOSE G87/Z75S/G75 Pro/G98V2 dongles, VGN V108).
//! Docs: docs/protocols/keyboard-sinowealth-0x13-mchose.md. 20-byte reports, id 0x13,
//! checksum = sum of bytes 0..18 at [19]. Only commands 0x4A (battery) and 0x05 (session
//! open, needed once by the 41E4 dongle) are ever sent.
//! Note: the dongle answers from a cached snapshot (can be hours old when the keyboard
//! idles); the value is returned as is, the caller handles age.

use super::kb_util::exchange;
use crate::devices::{Driver, HidHandle, HidInfo, Reading};

const IDS: &[(u16, u16)] = &[
    (0x41E4, 0x2001), // MCHOSE G87 dongle (old generation, needs session open)
    (0x258A, 0x01BB), // MCHOSE Z75S
    (0x3554, 0xFA09), // MCHOSE G75 PRO (param 0)
    (0x41E4, 0x2004), // MCHOSE G98V2
    (0x391D, 0x2A02), // VGN V108
];

const CMD_BATTERY: u8 = 0x4A;
#[cfg(test)]
const CMD_SESSION: u8 = 0x05; // documented, deliberately never sent
const CMD_PUSH: u8 = 0x0A;

pub struct SinoWealth;

fn frame(cmd: u8, param: u8) -> [u8; 20] {
    let mut b = [0u8; 20];
    b[0] = 0x13;
    b[1] = cmd;
    b[2] = param;
    b[19] = b[..19].iter().fold(0u8, |a, &x| a.wrapping_add(x));
    b
}

/// Frames that carry a battery value: the 0x4A reply, and push `13 0A 01 00 04 05 <pct> <flag>`.
/// Push type 0x02 (link/awake state) is never battery.
fn is_battery_frame(r: &[u8]) -> bool {
    r.len() >= 8 && r[0] == 0x13 && (r[1] == CMD_BATTERY || (r[1] == CMD_PUSH && r[5] == 0x05))
}

/// (percent, charging). Percent <= 1 is a firmware placeholder; high nibble 1 of the state = charging.
fn parse(r: &[u8]) -> Option<(u8, bool)> {
    if !is_battery_frame(r) {
        return None;
    }
    let (pct, st) = if r[1] == CMD_BATTERY { (r[5], r[6]) } else { (r[6], r[7]) };
    (2..=100).contains(&pct).then_some((pct, st >> 4 == 1))
}

impl Driver for SinoWealth {
    fn name(&self) -> &'static str {
        "sinowealth-13"
    }

    fn matches(&self, info: &HidInfo) -> bool {
        // 0xFF02 carries the protocol; 0xFF04 on the same dongle is a decoy mailbox.
        super::id_listed(IDS, info) && info.usage_page == 0xFF02 && info.output_len == 20
    }

    fn read(&self, dev: &HidHandle, info: &HidInfo) -> Option<Reading> {
        // The old 0x41E4 dongle stays silent until a session (`13 05 01`) is
        // opened, but that session stays open and can lock out the vendor app.
        // We never open it: such dongles answer only while the vendor app has
        // a session, otherwise battery stays unknown.
        let mut r = exchange(dev, &frame(CMD_BATTERY, 1), 500, is_battery_frame);
        if r.is_none() && info.vid == 0x3554 {
            r = exchange(dev, &frame(CMD_BATTERY, 0), 500, is_battery_frame);
        }
        let (battery, charging) = parse(&r?)?;
        Some(Reading { battery: Some(battery), charging: Some(charging), ..Default::default() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rsp(pct: u8, st: u8) -> [u8; 20] {
        let mut r = [0u8; 20];
        r[..5].copy_from_slice(&[0x13, 0x4A, 0x01, 0x00, 0x02]);
        r[5] = pct;
        r[6] = st;
        r
    }

    #[test]
    fn checksums() {
        assert_eq!(frame(CMD_BATTERY, 1)[19], 0x5E);
        assert_eq!(frame(CMD_BATTERY, 0)[19], 0x5D);
        assert_eq!(frame(CMD_SESSION, 1)[19], 0x19);
    }

    #[test]
    fn z75s_samples() {
        assert_eq!(parse(&rsp(0x55, 0x00)), Some((85, false)));
        assert_eq!(parse(&rsp(0x32, 0x10)), Some((50, true)));
        assert_eq!(parse(&rsp(0x64, 0x01)), Some((100, false)));
        assert_eq!(parse(&rsp(1, 0x01)), None); // placeholder
    }

    #[test]
    fn push_frames() {
        let mut p = [0u8; 20];
        p[..8].copy_from_slice(&[0x13, 0x0A, 0x01, 0x00, 0x04, 0x05, 60, 0x10]);
        assert_eq!(parse(&p), Some((60, true)));
        // type 2 = link/awake state, never battery
        p[5] = 0x02;
        p[6] = 0x01;
        assert!(!is_battery_frame(&p));
        assert_eq!(parse(&p), None);
    }
}
