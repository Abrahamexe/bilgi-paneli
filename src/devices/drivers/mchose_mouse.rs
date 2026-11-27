//! MCHOSE (M HUB) mice, VID 0x3837 (+ a few 0x5253 ids). Docs: docs/protocols/mouse-family-mchose.md.
//! Config collection usage page 0xFF01 usage 1.
//! * V2 (A7 V2, A7 Ultra, L7): feature report 0x11 (short) / 0x12 (long), every command
//!   and argument byte bit-inverted. Read-only cmds: 0x06 battery, 0x67 config blob.
//! * V3 (A5 V3, K7, K5, R7, ...): output/input report 0x4D, cmd 0x0900 device info,
//!   0x0002 settings, optional XOR checksum.
//!
//! Receivers share one PID, so the real model comes from the id inside the battery reply.
//! A percent of 0 is treated as "mouse asleep / no data".

use super::mouse_io::{FEAT, IN, OUT, feature_xfer};
use super::util::recv_until;
use crate::devices::{Driver, HidHandle, HidInfo, Reading, hid};

/// 0x5253 ("RealTek") ids used by older MCHOSE models.
const OLD_PIDS: &[u16] = &[0x1020, 0x1011, 0x00b0];
/// Wired mouse PIDs (= model ids); anything else on the VID is a shared receiver.
const MODEL_PIDS: &[u16] = &[
    0x4018, 0x4023, 0x4019, 0x4021, 0x1011, 0x00b0, 0x4015, // V2
    0x4035, 0x4026, 0x4034, 0x4027, 0x4028, 0x4030, 0x4031, 0x4032, 0x4033, 0x4037, 0x4038, 0x4036, 0x402a, 0x4029, // V3
];
const RATES: [u32; 6] = [125, 500, 1000, 2000, 4000, 8000];

/// V2 request: report id, then command and every argument inverted (0xFF = 0).
const fn v2<const N: usize>(id: u8, cmd: u8) -> [u8; N] {
    let mut b = [0xFFu8; N];
    b[0] = id;
    b[1] = !cmd;
    b
}
const V2_BATTERY: [u8; 21] = v2(0x11, 0x06);
const V2_CONFIG: [u8; 65] = v2(0x12, 0x67);

/// V3 request: `4D | 01 flags=1 len=0 cmd(LE) biz=0 seq=0 | xor(body[1..=6])`.
const fn v3(cmd: u16) -> [u8; 9] {
    let (lo, hi) = (cmd as u8, (cmd >> 8) as u8);
    [0x4D, 1, 1, 0, lo, hi, 0, 0, 1 ^ lo ^ hi]
}
const V3_INFO: [u8; 9] = v3(0x0900);
const V3_SETTINGS: [u8; 9] = v3(0x0002);

fn model_name(id: u16) -> Option<&'static str> {
    Some(match id {
        0x4018 => "MCHOSE A7 V2 Pro",
        0x4023 => "MCHOSE A7 V2 Pro+",
        0x4019 => "MCHOSE A7 V2 Ultra",
        0x4021 => "MCHOSE A7 V2 Ultra+",
        0x1011 => "MCHOSE A7 Ultra",
        0x00b0 => "MCHOSE L7 Pro",
        0x4015 => "MCHOSE L7 Pro+",
        0x4035 => "MCHOSE A5 V3 Pro",
        0x4026 | 0x4034 => "MCHOSE A5 V3 Ultra+",
        0x4027 | 0x4028 => "MCHOSE K7 V2",
        0x4037 | 0x4038 => "MCHOSE K5",
        0x4036 => "MCHOSE R7 Ultra",
        0x402a => "MCHOSE V7",
        0x4029 => "MCHOSE G3 V3",
        _ => return None, // 0x4030..=0x4033 (A7 V3) share names: not distinguishable from the docs
    })
}

/// Rate index byte (high nibble) -> Hz; `v3` applies the firmware's hidden slot.
fn rate_hz(byte: u8, v3: bool) -> Option<u32> {
    let idx = usize::from(byte >> 4);
    RATES.get(if v3 && idx > 1 { idx - 1 } else { idx }).copied()
}

/// V2 battery payload (un-inverted): `[0..2]` VID, `[2..4]` PID, `[8]` flags, `[9]` %, `[10]` charging.
fn parse_v2_battery(p: &[u8], wired: bool) -> Option<(u16, u8, bool)> {
    let connected = p.get(8)? & 0x10 != 0;
    let pct = *p.get(9).filter(|&&x| x <= 100)?;
    let charging = *p.get(10)? != 0;
    (pct > 0 && (wired || connected)).then_some((u16::from_le_bytes([p[2], p[3]]), pct, charging))
}

/// V3 device info data: `[2..4]` PID, `[11]` charge, `[12]` percent.
fn parse_v3_info(d: &[u8]) -> Option<(u16, u8, bool)> {
    let pct = *d.get(12).filter(|&&x| x <= 100 && x > 0)?;
    Some((u16::from_le_bytes([*d.get(2)?, *d.get(3)?]), pct, *d.get(11)? != 0))
}

/// Un-inverted payload of the reply to a V2 request: command echo matched, two identical answers in a row.
fn v2_ask(dev: &HidHandle, req: &[u8], keep: usize) -> Option<Vec<u8>> {
    let mut last: Option<Vec<u8>> = None;
    feature_xfer(dev, req, 30, 30, |b| {
        if b.len() < 2 + keep || b[0] != req[0] || b[1] != req[1] {
            return None;
        }
        let p: Vec<u8> = b[2..2 + keep].iter().map(|x| !x).collect();
        if last.as_ref() == Some(&p) {
            return Some(p);
        }
        last = Some(p);
        None
    })
}

/// Data bytes of the V3 reply to `q` (report id and 7 header bytes removed).
fn v3_ask(dev: &HidHandle, q: &[u8; 9]) -> Option<Vec<u8>> {
    if !dev.write(q) {
        return None;
    }
    recv_until(dev, 400, |r| {
        let n = usize::from(*r.get(3)?);
        (r.len() >= 8 + n && r[0] == 0x4D && r[4..6] == q[4..6]).then(|| r[8..8 + n].to_vec())
    })
}

pub struct MchoseMouse;

impl Driver for MchoseMouse {
    fn name(&self) -> &'static str {
        "mchose"
    }

    fn matches(&self, info: &HidInfo) -> bool {
        (info.vid == 0x3837 || (info.vid == 0x5253 && OLD_PIDS.contains(&info.pid)))
            && info.usage_page == 0xFF01
            && info.usage == 1
    }

    fn read(&self, dev: &HidHandle, info: &HidInfo) -> Option<Reading> {
        let ids = hid::report_ids(&info.path);
        let wired = MODEL_PIDS.contains(&info.pid);
        let (model, battery, charging, rate) = if ids[FEAT].contains(&0x11) {
            let (m, b, c) = parse_v2_battery(&v2_ask(dev, &V2_BATTERY, 12)?, wired)?;
            let cfg = v2_ask(dev, &V2_CONFIG, 3);
            (m, b, c, cfg.and_then(|p| rate_hz(p[if wired { 1 } else { 2 }], false)))
        } else if ids[OUT].contains(&0x4D) && ids[IN].contains(&0x4D) {
            let (m, b, c) = parse_v3_info(&v3_ask(dev, &V3_INFO)?)?;
            let cfg = v3_ask(dev, &V3_SETTINGS);
            (m, b, c, cfg.and_then(|d| rate_hz(*d.get(if wired { 1 } else { 2 })?, true)))
        } else {
            return None;
        };
        Some(Reading {
            name: model_name(model).map(String::from),
            battery: Some(battery),
            charging: Some(charging),
            polling_hz: rate,
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v2_requests_are_inverted() {
        assert_eq!(&V2_BATTERY[..3], &[0x11, 0xF9, 0xFF]); // docs: `[0xF9, 0xFF x19]` after the id
        assert_eq!(V2_BATTERY.len(), 21);
        assert_eq!(&V2_CONFIG[..3], &[0x12, 0x98, 0xFF]);
    }

    #[test]
    fn v3_request_from_docs() {
        // docs: `01 01 00 00 09 00 00 08` after report id 0x4D
        assert_eq!(V3_INFO, [0x4D, 1, 1, 0, 0x00, 0x09, 0, 0, 0x08]);
        assert_eq!(V3_SETTINGS, [0x4D, 1, 1, 0, 0x02, 0x00, 0, 0, 0x03]);
    }

    #[test]
    fn v2_battery_payload() {
        // PID 0x4021 (A7 V2 Ultra+), connected receiver, 73 %, charging
        let p = [0x37, 0x38, 0x21, 0x40, 1, 2, 3, 4, 0x10, 73, 1, 0];
        assert_eq!(parse_v2_battery(&p, false), Some((0x4021, 73, true)));
        let mut off = p;
        off[8] = 0;
        assert_eq!(parse_v2_battery(&off, false), None); // receiver without mouse
        assert_eq!(parse_v2_battery(&off, true).map(|x| x.1), Some(73)); // wired: no link flag needed
        off[9] = 0;
        assert_eq!(parse_v2_battery(&off, true), None);
        assert_eq!(model_name(0x4021), Some("MCHOSE A7 V2 Ultra+"));
    }

    #[test]
    fn v3_info_and_rates() {
        let mut d = [0u8; 14];
        d[2..4].copy_from_slice(&0x4026u16.to_le_bytes());
        d[11] = 0;
        d[12] = 64;
        assert_eq!(parse_v3_info(&d), Some((0x4026, 64, false)));
        d[12] = 101;
        assert_eq!(parse_v3_info(&d), None);
        // V2: plain index; V3: hidden slot (device idx 2 -> option 1)
        assert_eq!(rate_hz(0x20, false), Some(1000));
        assert_eq!(rate_hz(0x20, true), Some(500));
        assert_eq!(rate_hz(0x10, true), Some(500));
        assert_eq!(rate_hz(0x00, true), Some(125));
        assert_eq!(rate_hz(0x70, false), None);
    }
}
