//! Rapoo VT / VT9 Pro family (VID 0x24AE). Docs: docs/protocols/mouse-family-rapoo-vt.md.
//!
//! Passive only: the mouse pushes input report 0xBB about every 3 s,
//! `b0 51 20 03 <link> <percent>` after the report id. The worker thread blocks on
//! `read_timeout` for up to 3.5 s (no CPU). The lossy active path (cmd 0xAA via
//! GET_REPORT, EEPROM 0x880 polling) is not used, so there is no charging flag and
//! no polling rate here.

use super::mouse_io::{IN, declares};
use super::util::recv_until;
use crate::devices::{Driver, HidHandle, HidInfo, Reading};

const LISTEN_MS: u32 = 3500;

/// Percent from a 0xBB push. inferred: the docs sample `b0 51 20 03 01 62` is the
/// report body, so the link code is body[4] (1 wireless, 2 wired) and percent body[5].
fn parse_push(r: &[u8]) -> Option<u8> {
    (r.len() >= 7 && r[0] == 0xBB && matches!(r[5], 1 | 2) && r[6] <= 100).then_some(r[6])
}

pub struct RapooVt;

impl Driver for RapooVt {
    fn name(&self) -> &'static str {
        "rapoo-vt"
    }

    fn matches(&self, info: &HidInfo) -> bool {
        info.vid == 0x24AE && info.is_vendor() && info.input_len >= 7
    }

    fn read(&self, dev: &HidHandle, info: &HidInfo) -> Option<Reading> {
        if !declares(info, IN, 0xBB) {
            return None;
        }
        let battery = recv_until(dev, LISTEN_MS, parse_push)?;
        Some(Reading { battery: Some(battery), ..Default::default() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_from_docs() {
        // docs sample after report id 0xBB: b0 51 20 03 01 62 -> 98 %
        assert_eq!(parse_push(&[0xBB, 0xb0, 0x51, 0x20, 0x03, 0x01, 0x62]), Some(98));
        assert_eq!(parse_push(&[0xBB, 0xb0, 0x51, 0x20, 0x03, 0x02, 0x62]), Some(98));
        assert_eq!(parse_push(&[0xBB, 0xb0, 0x51, 0x20, 0x03, 0x07, 0x62]), None);
        assert_eq!(parse_push(&[0xBA, 0xb0, 0x51, 0x20, 0x03, 0x01, 0x62]), None);
        assert_eq!(parse_push(&[0xBB, 0xb0, 0x51, 0x20, 0x03, 0x01, 0x65]), None);
    }
}
