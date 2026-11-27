//! RongYuan web-driver family (MonsGeek, Akko, Womier, Epomaker, Yunzii, ikbc, Hator,
//! Keydous, Gamakay, Noppoo, MageGee, EWEADNV, Cherry MX8.2/K5, Valkyrie/VGN ...).
//! Docs: docs/protocols/keyboard-rongyuan-monsgeek.md. Vendor collection 0xFFFF/0x02,
//! 64-byte feature report id 0. Read-only commands only: F7 (dongle status), 83 (rate),
//! FC (cached reply). Never 0x7F / 0xF8.
//! Skipped: model name via 8F+FC (needs the vendor's id->name catalog, not shipped);
//! charging (F7 byte is documented as unreliable).

use super::util::sleep_ms;
use crate::devices::{Driver, HidHandle, HidInfo, Reading};

/// Whole vendors where every product with the vendor collection belongs to the family.
const VIDS: &[u16] = &[0x3151, 0x38EE, 0x39AB, 0x379A, 0x374A, 0x38A9];
/// Shared vendor ids: only catalog products (Yunzii/AJAZZ older, Cherry MX8.2 / K5 TMR).
const IDS: &[(u16, u16)] = &[(0x0461, 0x4002), (0x0461, 0x4003), (0x046A, 0x012D), (0x046A, 0x0141)];

const VID_VALKYRIE: u16 = 0x374A;

/// Read-only requests (cmd byte; checksum "Bit7" is 255 - cmd, parameters are all zero).
const REQ_DONGLE_STATUS: u8 = 0xF7;
const REQ_REPORT_RATE: u8 = 0x83;
const REQ_CACHED: u8 = 0xFC;

/// Rate code (reply idx 2) -> Hz. RongYuan table; other families differ.
const RATE_HZ: [u32; 7] = [8000, 4000, 2000, 1000, 500, 250, 125];

pub struct RongYuan;

fn request(cmd: u8) -> [u8; 9] {
    let mut b = [0u8; 9]; // [report id 0, cmd, p1..p6, checksum]
    b[1] = cmd;
    b[8] = 255 - cmd; // all parameters are zero
    b
}

/// Dongle status (reply idx after the report id). `Some(None)`: dongle-shaped but the
/// keyboard has not reported yet; `None`: not a dongle-status reply (wired board).
fn parse_status(r: &[u8], valkyrie: bool) -> Option<Option<u8>> {
    let pct = |p: u8| (1..=100).contains(&p).then_some(p);
    if valkyrie {
        // 374A:A272 quirk: `00 00 <batt> 00 00 00 01 01`.
        return (r.len() >= 8 && r[6] == 1 && r[7] == 1).then(|| pct(r[2]));
    }
    // Constants at idx 4 and 6 (dongle alive); idx 8 = paired.
    (r.len() >= 9 && r[4] == 1 && r[6] == 1).then(|| if r[8] == 1 { pct(r[1]) } else { None })
}

fn parse_rate(r: &[u8]) -> Option<u32> {
    (r.len() > 2 && r[0] == REQ_REPORT_RATE).then(|| RATE_HZ.get(usize::from(r[2])).copied())?
}

/// SetFeature request, then GetFeature (stale buffers: retry a few times) until `ok`.
fn query(dev: &HidHandle, cmd: u8, ok: impl Fn(&[u8]) -> bool) -> Option<Vec<u8>> {
    if !dev.set_feature(&request(cmd)) {
        return None;
    }
    for _ in 0..4 {
        sleep_ms(40);
        if let Some(v) = dev.get_feature(0, 65)
            && v.len() > 1
            && ok(&v[1..])
        {
            return Some(v[1..].to_vec());
        }
    }
    None
}

fn rate(dev: &HidHandle, forwarded: bool) -> Option<u32> {
    if let Some(r) = query(dev, REQ_REPORT_RATE, |r| r.first() == Some(&REQ_REPORT_RATE)) {
        return parse_rate(&r);
    }
    if forwarded {
        // The dongle relays 0x83 over RF (~220 ms) and parks the answer in its cache slot.
        if !dev.set_feature(&request(REQ_CACHED)) {
            return None;
        }
        sleep_ms(220);
        return parse_rate(dev.get_feature(0, 65)?.get(1..)?);
    }
    None
}

impl Driver for RongYuan {
    fn name(&self) -> &'static str {
        "rongyuan"
    }

    fn matches(&self, info: &HidInfo) -> bool {
        (VIDS.contains(&info.vid) || super::id_listed(IDS, info))
            && super::usage_is(info, 0xFFFF, 0x02)
            && info.feature_len >= 64
    }

    fn read(&self, dev: &HidHandle, info: &HidInfo) -> Option<Reading> {
        let valkyrie = info.vid == VID_VALKYRIE;
        let status = query(dev, REQ_DONGLE_STATUS, |r| r.len() >= 8).and_then(|r| parse_status(&r, valkyrie));
        let mut out = Reading::default();
        match status {
            // Dongle: battery is a cached snapshot; query the rate only once the keyboard talked.
            Some(Some(b)) => {
                out.battery = Some(b);
                out.polling_hz = rate(dev, true);
            }
            Some(None) => return None,
            // Wired board (or unknown reply shape): rate only. inferred: F7 battery on a wired
            // board is not trusted (dual-MCU boards skew it while on USB).
            None => out.polling_hz = rate(dev, false),
        }
        (out.battery.is_some() || out.polling_hz.is_some()).then_some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksums() {
        assert_eq!(request(REQ_DONGLE_STATUS), [0, 0xF7, 0, 0, 0, 0, 0, 0, 0x08]);
        assert_eq!(request(REQ_REPORT_RATE)[8], 0x7C);
    }

    #[test]
    fn dongle_status() {
        // idx: 0 cache, 1 battery 63, 3 charge, 4 const1, 5 rf_ready, 6 const1, 7 pairing, 8 paired
        let ok = [0, 63, 0, 0, 1, 1, 1, 0, 1];
        assert_eq!(parse_status(&ok, false), Some(Some(63)));
        let unpaired = [0, 63, 0, 0, 1, 1, 1, 0, 0];
        assert_eq!(parse_status(&unpaired, false), Some(None));
        let zero = [0, 0, 0, 0, 1, 0, 1, 0, 1];
        assert_eq!(parse_status(&zero, false), Some(None));
        assert_eq!(parse_status(&[0, 0, 0, 0, 0, 0, 0, 0, 0], false), None);
        // Valkyrie quirk
        assert_eq!(parse_status(&[0, 0, 77, 0, 0, 0, 1, 1], true), Some(Some(77)));
    }

    #[test]
    fn rate_codes() {
        assert_eq!(parse_rate(&[0x83, 0, 0]), Some(8000));
        assert_eq!(parse_rate(&[0x83, 0, 3]), Some(1000));
        assert_eq!(parse_rate(&[0x83, 0, 6]), Some(125));
        assert_eq!(parse_rate(&[0x83, 0, 7]), None);
        assert_eq!(parse_rate(&[0x84, 0, 3]), None);
    }
}
