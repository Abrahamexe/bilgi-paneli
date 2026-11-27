//! Smaller OEM families. Docs: docs/protocols/mouse-family-ninjutso-incott-ajazz.md.
//! * Ninjutso current gen (VID 0x093A, feature report 6): cmd 18 battery, 17 charging,
//!   6 polling, 167 paired model. Hardware status unknown in the docs.
//! * Ajazz NJ07 / NJ08 / MC (VID 0xA8A4 wired, 0xA8A5 dongle, usage 0xFF01:0x10):
//!   output report 0xF0, battery cmd 0x30, GET_CONFIG for polling.
//! * Skipped: Incott (battery only in an unsolicited report while the mouse moves),
//!   Ninjutso Sora V2 (settings request not documented), K-snake (decoder unverified).

use super::id_listed;
use super::mouse_io::{FEAT, OUT, feature_xfer, declares};
use super::util::recv_until;
use crate::devices::{Driver, HidHandle, HidInfo, Reading, hid};

const NINJUTSO: &[(u16, u16)] = &[(0x093A, 0xe010), (0x093A, 0xe020), (0x093A, 0xea01), (0x093A, 0xeb01), (0x093A, 0xeb02)];
const AJAZZ_VIDS: &[u16] = &[0xA8A4, 0xA8A5];

/// Ninjutso request: report id 6, `cmd, 0, 0, 1, 0, nargs = 0, profile = 0`.
const fn nj(cmd: u8) -> [u8; 8] {
    [6, cmd, 0, 0, 1, 0, 0, 0]
}
const NJ_CHARGING: [u8; 8] = nj(17);
const NJ_BATTERY: [u8; 8] = nj(18);
const NJ_POLLING: [u8; 8] = nj(6);
const NJ_PAIRED: [u8; 8] = nj(167);

/// Ajazz output report 0xF0 bodies.
const AJ_BATTERY: [u8; 3] = [0xF0, 0x30, 0x01];
const AJ_CONFIG: [u8; 5] = [0xF0, 0x0E, 0x01, 0x0B, 0x2E];

/// First two value bytes of the reply to `q`: value starts at index 8 of the report
/// (command echoed at [1]); some stacks drop the id byte, then everything shifts by one.
fn nj_ask(dev: &HidHandle, q: &[u8; 8]) -> Option<[u8; 2]> {
    feature_xfer(dev, q, 20, 10, |b| {
        let o = if b.get(1) == Some(&q[1]) { 1 } else if b.first() == Some(&q[1]) { 0 } else { return None };
        Some([*b.get(o + 7)?, b.get(o + 8).copied().unwrap_or(0)])
    })
}

fn nj_hz(code: u8) -> Option<u32> {
    [1000, 2000, 4000, 8000].get(usize::from(code).checked_sub(1)?).copied()
}

fn nj_name(id: u16) -> Option<&'static str> {
    match id {
        0xe010 => Some("Ninjutso Sora V3"),
        0xe020 => Some("Ninjutso TEN"),
        _ => None,
    }
}

fn read_ninjutso(dev: &HidHandle) -> Option<Reading> {
    let battery = nj_ask(dev, &NJ_BATTERY)?[0];
    // 0 % = receiver without a mouse / asleep: no data.
    (1..=100).contains(&battery).then_some(())?;
    Some(Reading {
        name: nj_ask(dev, &NJ_PAIRED).and_then(|v| nj_name(u16::from_le_bytes(v))).map(String::from),
        battery: Some(battery),
        charging: nj_ask(dev, &NJ_CHARGING).map(|v| v[0] != 0),
        polling_hz: nj_ask(dev, &NJ_POLLING).and_then(|v| nj_hz(v[0])),
        ..Default::default()
    })
}

/// Echo position of `cmd` in an input report (report id, then the body starting with the command).
fn aj_body(r: &[u8], cmd: u8) -> Option<&[u8]> {
    r.get(1..).filter(|b| b.first() == Some(&cmd))
}

/// Body of the reply to a 0xF0 request.
fn aj_ask(dev: &HidHandle, q: &[u8]) -> Option<Vec<u8>> {
    if !dev.write(q) {
        return None;
    }
    recv_until(dev, 300, |r| aj_body(r, q[1]).map(<[u8]>::to_vec))
}

/// (percent, charging) from the 0x30 reply body: `[7]` percent, `[8]` charging.
fn aj_battery(b: &[u8]) -> Option<(u8, bool)> {
    (b.len() > 8 && b[7] <= 100).then(|| (b[7], b[8] != 0))
}

/// GET_CONFIG body: `[9] - 1` indexes [125, 250, 500, 1000].
fn aj_hz(b: &[u8]) -> Option<u32> {
    [125, 250, 500, 1000].get(usize::from(*b.get(9)?).checked_sub(1)?).copied()
}

fn read_ajazz(dev: &HidHandle) -> Option<Reading> {
    let (battery, charging) = aj_battery(&aj_ask(dev, &AJ_BATTERY)?)?;
    Some(Reading {
        battery: Some(battery),
        charging: Some(charging),
        polling_hz: aj_ask(dev, &AJ_CONFIG).and_then(|b| aj_hz(&b)),
        ..Default::default()
    })
}

pub struct NinjutsoAjazz;

impl Driver for NinjutsoAjazz {
    fn name(&self) -> &'static str {
        "ninjutso-ajazz"
    }

    fn matches(&self, info: &HidInfo) -> bool {
        (id_listed(NINJUTSO, info) && info.is_vendor())
            || (AJAZZ_VIDS.contains(&info.vid) && info.usage_page == 0xFF01 && info.usage == 0x10)
    }

    fn read(&self, dev: &HidHandle, info: &HidInfo) -> Option<Reading> {
        if id_listed(NINJUTSO, info) {
            return hid::report_ids(&info.path)[FEAT].contains(&6).then(|| read_ninjutso(dev)).flatten();
        }
        declares(info, OUT, 0xF0).then(|| read_ajazz(dev)).flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_from_docs() {
        assert_eq!(NJ_BATTERY, [6, 18, 0, 0, 1, 0, 0, 0]);
        assert_eq!(NJ_CHARGING[1], 17);
        assert_eq!(NJ_PAIRED[1], 167);
        assert_eq!(AJ_BATTERY, [0xF0, 0x30, 0x01]);
        assert_eq!(&AJ_CONFIG[1..], &[0x0E, 0x01, 0x0B, 0x2E]);
    }

    #[test]
    fn ninjutso_decoders() {
        assert_eq!(nj_hz(1), Some(1000));
        assert_eq!(nj_hz(4), Some(8000));
        assert_eq!(nj_hz(0), None);
        assert_eq!(nj_hz(5), None);
        assert_eq!(nj_name(u16::from_le_bytes([0x10, 0xe0])), Some("Ninjutso Sora V3"));
    }

    #[test]
    fn ajazz_decoders() {
        // body: echo 0x30, ..., [7] percent 64, [8] charging
        let b = [0x30, 1, 0, 0, 0, 0, 0, 64, 1];
        assert_eq!(aj_battery(&b), Some((64, true)));
        assert_eq!(aj_battery(&[0x30, 0, 0, 0, 0, 0, 0, 101, 0]), None);
        assert_eq!(aj_body(&[0xF0, 0x30, 9], 0x30), Some(&[0x30, 9][..]));
        assert_eq!(aj_body(&[0xF0, 0x31, 9], 0x30), None);
        let mut cfg = [0x0E, 0, 0, 0, 0, 0, 0, 0, 0, 4];
        assert_eq!(aj_hz(&cfg), Some(1000));
        cfg[9] = 1;
        assert_eq!(aj_hz(&cfg), Some(125));
        cfg[9] = 0;
        assert_eq!(aj_hz(&cfg), None);
    }
}
