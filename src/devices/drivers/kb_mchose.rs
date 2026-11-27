//! MCHOSE 3837 generation, "65-byte 0x55/0xAA" channel (G87 V2 / K99 V3 / G75 V2: dongle
//! 3837:3033, wired 3837:2020). Docs: docs/protocols/keyboard-sinowealth-0x13-mchose.md
//! section 6.2; frame codec shared with `kb_nuphy`. One config-RAM read (cmd 0x05, offset 0,
//! len 56); battery + charge share reply byte [41].
//! Skipped: the QHW 0x4D channel (section 6.1): its XOR checksum range is unresolved (U).
//! The older report-0x13 MCHOSE dongles live in `kb_sinowealth`.

use super::kb_nuphy::{frame55, is_reply};
use super::kb_util::exchange;
use crate::devices::{Driver, HidHandle, HidInfo, Reading};

const IDS: &[(u16, u16)] = &[(0x3837, 0x3033), (0x3837, 0x2020)];
const CMD_READ: u8 = 0x05;
/// Reply byte carrying `bit7 = charging, bits0..6 = percent` (config RAM offset 32).
const BATTERY_IDX: usize = 41;

pub struct Mchose;

/// (percent, charging). `0x80` alone = charging but not initialised: no percent.
fn parse(r: &[u8]) -> Option<(Option<u8>, bool)> {
    if !is_reply(r, CMD_READ) {
        return None;
    }
    let b = *r.get(BATTERY_IDX)?;
    let pct = b & 0x7F;
    Some(((1..=100).contains(&pct).then_some(pct), b & 0x80 != 0))
}

impl Driver for Mchose {
    fn name(&self) -> &'static str {
        "mchose-55"
    }

    fn matches(&self, info: &HidInfo) -> bool {
        super::id_listed(IDS, info)
            && info.input_len == 65
            && info.output_len == 65
            && !info.is_keyboard()
            && !info.is_mouse()
            && info.usage_page != 0x0C
    }

    fn read(&self, dev: &HidHandle, _info: &HidInfo) -> Option<Reading> {
        let r = exchange(dev, &frame55(CMD_READ, 0, 56), 400, |r| is_reply(r, CMD_READ))?;
        let (battery, charging) = parse(&r)?;
        Some(Reading { battery, charging: Some(charging), ..Default::default() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rsp(b41: u8) -> [u8; 65] {
        let mut r = [0u8; 65];
        r[1] = 0xAA;
        r[2] = 5;
        r[BATTERY_IDX] = b41;
        r
    }

    #[test]
    fn samples() {
        assert_eq!(parse(&rsp(0xC3)), Some((Some(67), true)));
        assert_eq!(parse(&rsp(0x44)), Some((Some(68), false)));
        assert_eq!(parse(&rsp(0x80)), Some((None, true)));
        assert_eq!(parse(&rsp(0x00)), Some((None, false)));
        let mut other = rsp(0x44);
        other[2] = 4;
        assert_eq!(parse(&other), None);
    }
}
