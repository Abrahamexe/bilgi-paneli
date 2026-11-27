//! Family D: low-cost OEM stacks. Docs: docs/protocols/mouse-family-beken-x11-gearhub.md.
//! * D3 GearHub-V5 (VID 0x3151 PID 0x402D receiver / 0x4026 cable: Attack Shark R2,
//!   Lingbao M5 Pro, Fantech WG14P): `F7` GET_STATUS -> mouse battery. Polling and model
//!   need SELECT_TARGET plus ambiguous 9-byte frames: skipped.
//! * D1 Attack Shark X11 / R1 / X6 / Delux M600 Pro receiver (1D57:FA60): the receiver
//!   pushes `03 <model> 40 01 <value>` on its battery collection; listened for ~1 s,
//!   no push in that time = no reading. No charging flag, no polling read-back exists.
//! * D2 (0x25a7 GearHub MU): no battery decoded anywhere, not implemented.

use super::id_listed;
use super::mouse_io::{FEAT, feature_xfer};
use super::util::recv_until;
use crate::devices::{Driver, HidHandle, HidInfo, Reading, hid};

const D3_IDS: &[(u16, u16)] = &[(0x3151, 0x402D), (0x3151, 0x4026)];
const D1_RECEIVER: (u16, u16) = (0x1d57, 0xfa60);
/// GET_STATUS, raw (no checksum), feature report 0.
const GET_STATUS: [u8; 2] = [0, 0xF7];

/// Mouse battery from a GET_STATUS reply (leading report id byte): body
/// `[ready, kbd %, mouse %, kbd online, mouse online (0 = online), ready to send]`.
fn parse_status(r: &[u8]) -> Option<u8> {
    (r.len() >= 6 && r[1] == 1).then_some(())?;
    (r[5] == 0 && r[3] <= 100).then_some(r[3])
}

/// Model, name and whether the pushed value is in tenths. Only models whose scale is
/// documented (percent: X11, X11 SE, Delux M600 Pro; tenths: R1, X6).
fn d1_model(id: u8) -> Option<(&'static str, bool)> {
    Some(match id {
        0x55 => ("Attack Shark X11", false),
        0x07 => ("Attack Shark X11 SE", false),
        0x20 => ("Delux M600 Pro", false),
        0x10 => ("Attack Shark R1", true),
        0x85 => ("Attack Shark X6", true),
        _ => return None,
    })
}

/// Push report `03 <model> 40 01 <value>` -> (name, percent).
fn parse_push(r: &[u8]) -> Option<(&'static str, u8)> {
    if r.len() < 5 || r[0] != 3 || r[2] != 0x40 || r[3] != 1 {
        return None;
    }
    let (name, tenths) = d1_model(r[1])?;
    let pct = if tenths { r[4].checked_mul(10).filter(|_| r[4] <= 10)? } else { r[4] };
    (pct <= 100).then_some((name, pct))
}

pub struct GearHub;

impl Driver for GearHub {
    fn name(&self) -> &'static str {
        "gearhub"
    }

    fn matches(&self, info: &HidInfo) -> bool {
        (id_listed(D3_IDS, info) && info.is_vendor() && info.feature_len == 65)
            || ((info.vid, info.pid) == D1_RECEIVER && info.usage_page == 0x0A && info.input_len >= 5)
    }

    fn read(&self, dev: &HidHandle, info: &HidInfo) -> Option<Reading> {
        if (info.vid, info.pid) == D1_RECEIVER {
            let (name, battery) = recv_until(dev, 1000, parse_push)?;
            return Some(Reading { name: Some(name.into()), battery: Some(battery), ..Default::default() });
        }
        if hid::report_ids(&info.path)[FEAT] != [0] {
            return None;
        }
        let battery = feature_xfer(dev, &GET_STATUS, 10, 10, |r| (r.len() >= 6 && r[1] == 1).then(|| parse_status(r)))??;
        Some(Reading { battery: Some(battery), ..Default::default() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_reply_from_docs() {
        // measured `01 00 2D 01 00 01` (WebHID, id stripped): mouse 0x2D = 45 %, online
        assert_eq!(parse_status(&[0, 0x01, 0x00, 0x2D, 0x01, 0x00, 0x01]), Some(45));
        assert_eq!(parse_status(&[0, 0x01, 0x00, 0x2D, 0x01, 0x01, 0x01]), None); // mouse offline
        assert_eq!(parse_status(&[0, 0x00, 0x00, 0x2D, 0x01, 0x00, 0x01]), None); // not ready
        assert_eq!(parse_status(&[0, 0x01, 0x00, 0x65, 0x01, 0x00, 0x01]), None); // > 100
    }

    #[test]
    fn push_scales() {
        assert_eq!(parse_push(&[3, 0x55, 0x40, 1, 78]), Some(("Attack Shark X11", 78)));
        assert_eq!(parse_push(&[3, 0x10, 0x40, 1, 7]), Some(("Attack Shark R1", 70)));
        assert_eq!(parse_push(&[3, 0x10, 0x40, 1, 11]), None); // tenths model above 10
        assert_eq!(parse_push(&[3, 0x4D, 0x40, 1, 50]), None); // scale not documented
        assert_eq!(parse_push(&[3, 0x55, 0x41, 1, 50]), None);
        assert_eq!(parse_push(&[3, 0x55, 0x40, 1, 101]), None);
    }
}
