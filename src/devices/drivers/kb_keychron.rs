//! Keychron (VID 0x3434) QMK raw HID (usage 0xFF60/0x61, 32-byte reports). Docs:
//! docs/protocols/keyboard-qmk-via-keychron.md section 2. Keychron firmware has no
//! battery command (BLE BAS only; do NOT use the community 0xAC): this driver only reads
//! the polling rate, with `A7 0D` (REPORT_RATE_GET), which only 8K-capable models answer.

use super::kb_util::{echo_at, exchange};
use crate::devices::{Driver, HidHandle, HidInfo, Reading};

const REQ_RATE: [u8; 2] = [0xA7, 0x0D];

/// Rate index -> Hz (8K models). Same order as RongYuan but kept separate on purpose.
const RATE_HZ: [u32; 7] = [8000, 4000, 2000, 1000, 500, 250, 125];

pub struct Keychron;

/// Reply (with echo at 0): data[2] status (0 = ok), data[3] rate index, data[4] supported mask
/// (bit N = index N supported). An echo-only answer from firmware without the command has an
/// empty mask and is rejected. inferred: v2 (dual USB/2.4G) replies also carry the USB index at [3].
fn parse(r: &[u8]) -> Option<u32> {
    let o = echo_at(r, &REQ_RATE)?;
    let d = r.get(o..o + 5)?;
    let (status, idx, mask) = (d[2], usize::from(d[3]), d[4]);
    (status == 0 && idx < RATE_HZ.len() && mask & (1 << idx) != 0).then(|| RATE_HZ[idx])
}

impl Driver for Keychron {
    fn name(&self) -> &'static str {
        "keychron"
    }

    fn matches(&self, info: &HidInfo) -> bool {
        info.vid == 0x3434 && super::usage_is(info, 0xFF60, 0x61)
    }

    fn read(&self, dev: &HidHandle, _info: &HidInfo) -> Option<Reading> {
        let mut req = [0u8; 33];
        req[1..3].copy_from_slice(&REQ_RATE);
        let r = exchange(dev, &req, 300, |r| echo_at(r, &REQ_RATE).is_some())?;
        Some(Reading { polling_hz: Some(parse(&r)?), ..Default::default() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_replies() {
        // A7 0D status=0 idx=3 (1000) mask=0x7F
        assert_eq!(parse(&[0xA7, 0x0D, 0, 3, 0x7F]), Some(1000));
        // with Windows report-id prefix, 8000 Hz
        assert_eq!(parse(&[0, 0xA7, 0x0D, 0, 0, 0x7F]), Some(8000));
        // plain echo (no 8K support): empty mask
        assert_eq!(parse(&[0xA7, 0x0D, 0, 0, 0]), None);
        // error status / index outside the mask / unknown index
        assert_eq!(parse(&[0xA7, 0x0D, 1, 3, 0x7F]), None);
        assert_eq!(parse(&[0xA7, 0x0D, 0, 3, 0x07]), None);
        assert_eq!(parse(&[0xA7, 0x0D, 0, 9, 0xFF]), None);
    }
}
