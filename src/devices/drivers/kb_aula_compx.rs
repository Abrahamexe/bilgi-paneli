//! AULA / Epomaker x AULA / generic CompX 2.4G dongle (05AC:024F, Apple VID borrowed).
//! Docs: docs/protocols/keyboard-aula-compx-aa.md section A. Only the `20 01` battery
//! query is sent. No charging flag and no Hz query exist in this family.

use super::kb_util::{echo_at, exchange};
use crate::devices::{Driver, HidHandle, HidInfo, Reading};

const REQ_BATTERY: [u8; 2] = [0x20, 0x01];

pub struct AulaCompx;

/// 32-byte frame `[cmd, len, seq, data[28], checksum]` behind a report id 0;
/// checksum = sum of the first 31 bytes at index 31.
fn frame(cmd: [u8; 2]) -> [u8; 33] {
    let mut b = [0u8; 33];
    b[1] = cmd[0];
    b[2] = cmd[1];
    b[32] = b[1..32].iter().fold(0u8, |a, &x| a.wrapping_add(x));
    b
}

/// Battery percent at payload byte 3 (1..=100; 0 and >100 are sleep frames).
fn parse(r: &[u8]) -> Option<u8> {
    let o = echo_at(r, &REQ_BATTERY)?;
    r.get(o + 3).copied().filter(|p| (1..=100).contains(p))
}

impl Driver for AulaCompx {
    fn name(&self) -> &'static str {
        "aula-compx"
    }

    fn matches(&self, info: &HidInfo) -> bool {
        // 05AC is Apple's VID: only the CompX dongle string, never a real Apple device.
        info.vid == 0x05AC
            && info.pid == 0x024F
            && info.product.to_ascii_lowercase().contains("2.4g wireless receiver")
            && info.usage_page == 0xFF60
    }

    fn read(&self, dev: &HidHandle, _info: &HidInfo) -> Option<Reading> {
        let r = exchange(dev, &frame(REQ_BATTERY), 300, |r| echo_at(r, &REQ_BATTERY).is_some())?;
        Some(Reading { battery: Some(parse(&r)?), ..Default::default() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_checksum() {
        let f = frame(REQ_BATTERY);
        assert_eq!(&f[..3], &[0, 0x20, 0x01]);
        assert_eq!(f[32], 0x21);
    }

    #[test]
    fn replies() {
        let mut r = [0u8; 32];
        r[..4].copy_from_slice(&[0x20, 0x01, 0x00, 77]);
        assert_eq!(parse(&r), Some(77));
        let mut p = [0u8; 33];
        p[1..5].copy_from_slice(&[0x20, 0x01, 0x00, 55]);
        assert_eq!(parse(&p), Some(55));
        r[3] = 0;
        assert_eq!(parse(&r), None);
        r[3] = 101;
        assert_eq!(parse(&r), None);
    }
}
