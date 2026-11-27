//! NuPhy "0x55/0xAA chunk frame" (Console boards, WH80; VID 0x19F5). Docs:
//! docs/protocols/keyboard-nuphy-55frame.md. Also provides the frame codec used by
//! `kb_mchose` (same layout). Read commands only: 0x04 (mode), 0x05 (config RAM).
//! Skipped: NuPhy V2+ dongle PIDs 0x8F02..0x8F07 (QMK/VIA boards, protocol unknown).

use super::kb_util::exchange;
use crate::devices::{Driver, HidHandle, HidInfo, Reading};

const WH80_WIRED: u16 = 0xA011;
const IDS: &[(u16, u16)] = &[(0x19F5, WH80_WIRED), (0x19F5, 0x8F01)];

const CMD_MODE: u8 = 0x04;
const CMD_READ: u8 = 0x05;

/// Config block offsets inside a per-mode 64-byte block.
const OFF_RATE: u16 = 4;
const OFF_BATTERY: u16 = 32;

/// Rate code (low nibble) -> Hz. inferred: only 1..=4 are documented (source comment).
const RATE_HZ: [u32; 4] = [125, 250, 500, 1000];

pub struct NuPhy;

/// 65-byte read request `[0, 55, cmd, 0, chk, len, offLo, offHi, 0, ...]`;
/// chk = sum of bytes 5..=64.
pub(super) fn frame55(cmd: u8, off: u16, len: u8) -> [u8; 65] {
    let mut b = [0u8; 65];
    b[1] = 0x55;
    b[2] = cmd;
    b[5] = len;
    b[6] = off as u8;
    b[7] = (off >> 8) as u8;
    b[4] = b[5..].iter().fold(0u8, |a, &x| a.wrapping_add(x));
    b
}

/// Reply to `cmd`: marker 0xAA (some firmware echoes 0x55) at [1], echo at [2], data from [9].
pub(super) fn is_reply(r: &[u8], cmd: u8) -> bool {
    r.len() >= 10 && (r[1] == 0xAA || r[1] == 0x55) && r[2] == cmd
}

/// Reads one byte of config RAM at `off`.
pub(super) fn read_byte(dev: &HidHandle, off: u16) -> Option<u8> {
    let r = exchange(dev, &frame55(CMD_READ, off, 1), 300, |r| is_reply(r, CMD_READ))?;
    Some(r[9])
}

fn battery(b: u8) -> Option<u8> {
    let p = b & 0x7F;
    (1..=100).contains(&p).then_some(p)
}

fn rate(b: u8) -> Option<u32> {
    RATE_HZ.get(usize::from((b & 0x0F).wrapping_sub(1))).copied()
}

impl Driver for NuPhy {
    fn name(&self) -> &'static str {
        "nuphy-55"
    }

    fn matches(&self, info: &HidInfo) -> bool {
        // 19F5 is shared with DrunkDeer: PID list plus the 65/65-byte collection shape.
        super::id_listed(IDS, info)
            && info.input_len == 65
            && info.output_len == 65
            && !info.is_keyboard()
            && !info.is_mouse()
            && info.usage_page != 0x0C
    }

    fn read(&self, dev: &HidHandle, info: &HidInfo) -> Option<Reading> {
        let r = exchange(dev, &frame55(CMD_MODE, 0, 16), 300, |r| is_reply(r, CMD_MODE))?;
        let mode = u16::from(r[9]);
        if mode > 3 {
            return None;
        }
        let base = mode * 64;
        let battery = battery(read_byte(dev, base + OFF_BATTERY)?);
        let polling_hz = read_byte(dev, base + OFF_RATE).and_then(rate);
        // inferred: no charge field exists; the wired PID implies the cable is attached.
        let charging = (info.pid == WH80_WIRED).then_some(true);
        (battery.is_some() || polling_hz.is_some()).then_some(Reading {
            battery,
            charging,
            polling_hz,
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_checksum() {
        // battery: len 1 at mode 1 (+32): chk = 1 + 0x60
        let f = frame55(CMD_READ, 64 + 32, 1);
        assert_eq!(&f[..8], &[0, 0x55, 0x05, 0, 0x61, 1, 0x60, 0]);
        // MCHOSE `00 55 05 00 38 38`: len 56 at offset 0
        let g = frame55(CMD_READ, 0, 56);
        assert_eq!(&g[..6], &[0, 0x55, 0x05, 0, 0x38, 0x38]);
    }

    #[test]
    fn replies_and_values() {
        let mut r = [0u8; 65];
        r[1] = 0xAA;
        r[2] = 5;
        assert!(is_reply(&r, 5));
        assert!(!is_reply(&r, 4));
        assert_eq!(battery(0x64), Some(100));
        assert_eq!(battery(0x80 | 42), Some(42));
        assert_eq!(battery(0), None);
        assert_eq!(rate(0x13), Some(500));
        assert_eq!(rate(0x04), Some(1000));
        assert_eq!(rate(0), None);
        assert_eq!(rate(5), None);
    }
}
