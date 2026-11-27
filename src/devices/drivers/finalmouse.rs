//! Finalmouse xpanel protocol (VID 0x361D): UltralightX (ULX) and Starlight X (SLX).
//! Docs: docs/protocols/mouse-family-finalmouse-xpanel.md.
//!
//! The mouse answers no single queries: the "wake all" (cmd 0x60) and dongle-info
//! (cmd 10) requests make it push a burst of input reports, collected for ~1.1 s.
//! Side effect: "wake all" wakes a sleeping mouse, so keep the poll interval long.
//! A silent mouse gives no reading. No charging flag is known for ULX.
//! inferred: SLX report ids (output 1 / input 2) and cmd ids are unverified without hardware.

use super::mouse_io::{OUT, declares};
use super::util::{drain, interp};
use crate::devices::{Driver, HidHandle, HidInfo, Reading};
use std::time::{Duration, Instant};

const VID: u16 = 0x361D;
/// ULX: dongle 0x0100, mouse direct 0x0102.
const ULX: &[u16] = &[0x0100, 0x0102];
const SLX: &[u16] = &[0x0300, 0x0301, 0x0310, 0x0311];
const COLLECT_MS: u64 = 1075;

/// `[report id, innerLen = 2 + payload, 0x80 | cmd, payloadLen]` with no payload.
const fn req(id: u8, cmd: u8) -> [u8; 4] {
    [id, 2, 0x80 | cmd, 0]
}
const WAKE_ALL: u8 = 0x60;
const DONGLE_INFO: u8 = 10;

/// ULX battery: millivolts -> percent (piecewise-linear calibration of the xpanel app).
const ULX_MV: &[(u32, u8)] = &[(3000, 0), (3620, 5), (3660, 10), (3740, 25), (3880, 50), (4170, 75), (4380, 100)];

/// Payload of the last pushed report with command `cmd` (body: `innerLen, cmd, payloadLen, payload`).
fn find(reports: &[Vec<u8>], cmd: u8) -> Option<&[u8]> {
    reports.iter().rev().find_map(|r| {
        let b = r.get(1..)?; // skip the report id
        let n = usize::from(*b.get(2)?);
        (b.len() >= 3 + n && b[1] & 0x7F == cmd).then(|| &b[3..3 + n])
    })
}

fn hz(p: &[u8]) -> Option<u32> {
    let h = u32::from(u16::from_le_bytes([*p.first()?, *p.get(1)?]));
    (125..=8000).contains(&h).then_some(h)
}

fn parse(reports: &[Vec<u8>], ulx: bool) -> Option<Reading> {
    let (battery, charging, polling_hz, name) = if ulx {
        let mv = find(reports, 5).filter(|p| p.len() >= 2)?;
        let pct = interp(ULX_MV, u32::from(u16::from_le_bytes([mv[0], mv[1]])));
        (pct, None, find(reports, 4).and_then(hz), "Finalmouse UltralightX")
    } else {
        let pct = *find(reports, 38)?.first().filter(|&&p| p <= 100)?;
        let chg = find(reports, 37).and_then(|p| p.first()).map(|&c| c == 1);
        (pct, chg, find(reports, 17).and_then(hz), "Finalmouse Starlight X")
    };
    Some(Reading { name: Some(name.into()), battery: Some(battery), charging, polling_hz, ..Default::default() })
}

pub struct Finalmouse;

impl Driver for Finalmouse {
    fn name(&self) -> &'static str {
        "finalmouse"
    }

    fn matches(&self, info: &HidInfo) -> bool {
        info.vid == VID && info.usage_page == 0xFF00 && info.output_len >= 4 && (ULX.contains(&info.pid) || SLX.contains(&info.pid))
    }

    fn read(&self, dev: &HidHandle, info: &HidInfo) -> Option<Reading> {
        // "Wake all" wakes a sleeping mouse: only read when the user opens the
        // panel or asks for a refresh, never from the background timer.
        if crate::devices::is_background() {
            return None;
        }
        let ulx = ULX.contains(&info.pid);
        // ULX: main output 4 (wake all) and dongle output 2 (dongle info); SLX: output 1.
        let (main, dongle) = if ulx { (4, Some(2)) } else { (1, None) };
        if !declares(info, OUT, main) {
            return None;
        }
        // Drain stale reports first so that only this burst is parsed.
        drain(dev);
        if !dev.write(&req(main, WAKE_ALL)) {
            return None;
        }
        if let Some(d) = dongle.filter(|&d| declares(info, OUT, d)) {
            let _ = dev.write(&req(d, DONGLE_INFO));
        }
        let end = Instant::now() + Duration::from_millis(COLLECT_MS);
        let mut reports = Vec::new();
        loop {
            let left = end.saturating_duration_since(Instant::now()).as_millis() as u32;
            let Some(r) = (left > 0).then(|| dev.read_timeout(left)).flatten() else { break };
            reports.push(r);
        }
        parse(&reports, ulx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push(id: u8, cmd: u8, payload: &[u8]) -> Vec<u8> {
        let mut v = vec![id, 2 + payload.len() as u8, cmd, payload.len() as u8];
        v.extend_from_slice(payload);
        v
    }

    #[test]
    fn request_layout() {
        assert_eq!(req(4, WAKE_ALL), [4, 2, 0xE0, 0]);
        assert_eq!(req(2, DONGLE_INFO), [2, 2, 0x8A, 0]);
    }

    #[test]
    fn ulx_battery_curve_and_hz() {
        let rep = [push(5, 5, &3880u16.to_le_bytes()), push(5, 4, &1000u16.to_le_bytes())];
        let r = parse(&rep, true).unwrap();
        assert_eq!((r.battery, r.polling_hz, r.charging), (Some(50), Some(1000), None));
        let hi = parse(&[push(5, 5, &4400u16.to_le_bytes())], true).unwrap();
        assert_eq!(hi.battery, Some(100));
        assert!(parse(&[push(5, 4, &1000u16.to_le_bytes())], true).is_none()); // silent mouse
    }

    #[test]
    fn slx_fields() {
        let rep = [push(2, 38, &[66, 0x20, 0x0F]), push(2, 37, &[1]), push(2, 17, &4000u16.to_le_bytes())];
        let r = parse(&rep, false).unwrap();
        assert_eq!((r.battery, r.charging, r.polling_hz), (Some(66), Some(true), Some(4000)));
        assert!(parse(&[push(2, 38, &[101])], false).is_none());
    }
}
