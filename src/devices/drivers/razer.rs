//! Razer mice (wired, HyperSpeed / Wireless dongles, HyperPolling 4K/8K): 90-byte
//! feature report, report id 0. See docs/protocols/mouse-razer.md.
//!
//! READ-ONLY allow-list: class 0x07 ids 0x80 (battery) / 0x84 (charging) and
//! class 0x00 ids 0x85 / 0xC0 (polling rate). No setter is ever built.

use std::time::Duration;

use super::util::sleep_ms;
use crate::devices::{Driver, HidHandle, HidInfo, Reading};

const VID: u16 = 0x1532;

/// (class, id, data size) of the allowed requests.
const BATTERY: (u8, u8, u8) = (0x07, 0x80, 2);
const CHARGING: (u8, u8, u8) = (0x07, 0x84, 2);
const RATE_LEGACY: (u8, u8, u8) = (0x00, 0x85, 1);
const RATE_EXT: (u8, u8, u8) = (0x00, 0xC0, 1);

const F_BATTERY: u8 = 1;
const F_CHARGING: u8 = 2;
const F_EXT_RATE: u8 = 4;

type Dev = (u16, u8, u8, u32, u8, &'static str);

/// (pid, transaction id, flags, wait after set in us, USB interface, receiver name).
/// From the OpenRazer PID table in the doc (section 8). Entries without a battery
/// use txn 0xFF (inferred: OpenRazer's default). The last 4 PIDs come from
/// razer-battery-report's catalog (txn / ext-rate inferred).
const DEVICES: &[Dev] = &[
    (0x0013, 0xFF, 0, 600, 0, ""), (0x0015, 0xFF, 0, 600, 0, ""), (0x0016, 0xFF, 0, 600, 0, ""),
    (0x001F, 0xFF, 3, 600, 0, ""), (0x0020, 0xFF, 0, 600, 0, ""), (0x0024, 0xFF, 3, 600, 0, ""),
    (0x0025, 0xFF, 3, 600, 0, ""), (0x0029, 0xFF, 0, 600, 0, ""), (0x002E, 0xFF, 0, 600, 0, ""),
    (0x002F, 0xFF, 0, 600, 0, ""), (0x0032, 0xFF, 3, 600, 0, ""), (0x0034, 0xFF, 0, 600, 0, ""),
    (0x0036, 0xFF, 0, 600, 0, ""), (0x0037, 0xFF, 0, 600, 0, ""), (0x0038, 0xFF, 0, 600, 0, ""),
    (0x0039, 0xFF, 0, 600, 0, ""), (0x003E, 0xFF, 3, 600, 0, ""), (0x003F, 0xFF, 3, 600, 0, ""),
    (0x0040, 0xFF, 0, 600, 0, ""), (0x0041, 0xFF, 0, 600, 0, ""), (0x0042, 0xFF, 0, 600, 0, ""),
    (0x0043, 0xFF, 0, 600, 0, ""), (0x0044, 0xFF, 3, 600, 0, ""), (0x0045, 0xFF, 3, 600, 0, ""),
    (0x0048, 0xFF, 0, 600, 0, ""), (0x004F, 0xFF, 0, 600, 0, ""), (0x0050, 0xFF, 0, 600, 0, ""),
    (0x0053, 0xFF, 0, 600, 0, ""), (0x0054, 0xFF, 0, 600, 0, ""), (0x0059, 0x3F, 3, 600, 0, ""),
    (0x005A, 0x3F, 3, 600, 0, ""), (0x005B, 0xFF, 0, 600, 0, ""), (0x005C, 0xFF, 0, 600, 0, ""),
    (0x005E, 0xFF, 0, 600, 0, ""), (0x005F, 0xFF, 0, 600, 0, ""), (0x0060, 0xFF, 0, 600, 0, ""),
    (0x0062, 0x1F, 1, 400000, 0, "Atheris"), (0x0064, 0xFF, 0, 600, 0, ""), (0x0065, 0xFF, 0, 600, 0, ""),
    (0x0067, 0xFF, 0, 600, 0, ""), (0x006A, 0xFF, 0, 600, 0, ""), (0x006B, 0xFF, 0, 600, 0, ""),
    (0x006C, 0xFF, 0, 600, 0, ""), (0x006E, 0xFF, 0, 600, 0, ""),
    (0x006F, 0x1F, 3, 31000, 0, "Lancehead"), (0x0070, 0x1F, 3, 31000, 0, ""),
    (0x0071, 0xFF, 0, 600, 0, ""), (0x0072, 0x3F, 3, 31000, 0, "Mamba"), (0x0073, 0x3F, 3, 31000, 0, ""),
    (0x0077, 0x1F, 3, 31000, 0, "Pro Click"), (0x0078, 0xFF, 0, 600, 0, ""),
    (0x007A, 0xFF, 3, 59900, 0, ""), (0x007B, 0xFF, 3, 59900, 0, ""), (0x007C, 0x3F, 3, 59900, 0, ""),
    (0x007D, 0x3F, 3, 59900, 0, ""), (0x0080, 0x1F, 3, 31000, 0, ""), (0x0083, 0xFF, 1, 31000, 0, ""),
    (0x0084, 0xFF, 0, 600, 0, ""), (0x0085, 0xFF, 0, 600, 0, ""), (0x0086, 0x1F, 3, 31000, 0, ""),
    (0x0088, 0x1F, 3, 31000, 0, "Basilisk Ultimate"), (0x008A, 0xFF, 0, 600, 0, ""),
    (0x008C, 0xFF, 0, 600, 0, ""), (0x008D, 0xFF, 0, 600, 0, ""), (0x008F, 0x1F, 3, 600, 0, ""),
    (0x0090, 0x1F, 3, 31000, 0, ""), (0x0091, 0xFF, 4, 600, 0, ""),
    (0x0094, 0x1F, 1, 400000, 0, "Orochi V2"), (0x0095, 0x1F, 1, 400000, 0, ""),
    (0x0096, 0xFF, 0, 600, 3, ""), (0x0098, 0xFF, 0, 600, 0, ""), (0x0099, 0xFF, 0, 600, 3, ""),
    (0x009A, 0x1F, 3, 31000, 0, "Pro Click Mini"), (0x009C, 0x1F, 1, 31000, 0, ""),
    (0x009E, 0x1F, 7, 59900, 0, ""), (0x009F, 0x1F, 7, 59900, 0, ""), (0x00A1, 0xFF, 0, 600, 0, ""),
    (0x00A3, 0xFF, 0, 600, 0, ""), (0x00A5, 0x1F, 3, 31000, 0, ""), (0x00A6, 0x1F, 3, 31000, 0, ""),
    (0x00A7, 0x1F, 3, 31000, 0, ""), (0x00A8, 0x1F, 3, 31000, 0, ""), (0x00AA, 0x1F, 3, 31000, 0, ""),
    (0x00AB, 0x1F, 3, 31000, 0, ""), (0x00AF, 0x1F, 3, 31000, 0, ""), (0x00B0, 0x1F, 3, 31000, 0, ""),
    (0x00B2, 0xFF, 4, 600, 0, ""), (0x00B3, 0x1F, 7, 59900, 0, "HyperPolling Dongle"),
    (0x00B4, 0x1F, 1, 31000, 0, "Naga V2 HyperSpeed"), (0x00B6, 0x1F, 3, 31000, 0, ""),
    (0x00B7, 0x1F, 3, 31000, 0, ""), (0x00B8, 0x1F, 1, 59900, 0, ""), (0x00B9, 0x1F, 1, 31000, 0, ""),
    (0x00BE, 0x1F, 7, 31000, 0, ""), (0x00BF, 0x1F, 7, 31000, 0, ""), (0x00C0, 0x1F, 3, 31000, 0, ""),
    (0x00C1, 0x1F, 7, 59900, 0, ""), (0x00C2, 0x1F, 3, 31000, 0, ""), (0x00C3, 0x1F, 7, 31000, 0, ""),
    (0x00C4, 0x1F, 3, 31000, 0, ""), (0x00C5, 0x1F, 3, 31000, 0, ""), (0x00C7, 0x1F, 3, 31000, 0, ""),
    (0x00C8, 0x1F, 3, 31000, 0, ""), (0x00CB, 0x1F, 1, 600, 3, ""), (0x00CC, 0x1F, 3, 31000, 0, ""),
    (0x00CD, 0x1F, 3, 31000, 0, ""), (0x00D0, 0x1F, 3, 31000, 0, ""), (0x00D1, 0x1F, 3, 31000, 0, ""),
    (0x00D3, 0x1F, 1, 31000, 0, ""), (0x00D4, 0x1F, 1, 31000, 0, "Basilisk Mobile"),
    (0x00D6, 0x1F, 3, 31000, 0, ""), (0x00D7, 0x1F, 3, 31000, 0, ""), (0x00DA, 0x1F, 3, 31000, 0, ""),
    (0x00DB, 0x1F, 3, 31000, 0, ""), (0x00DE, 0x1F, 3, 31000, 0, ""), (0x00DF, 0x1F, 3, 59900, 0, ""),
    (0x00E0, 0x1F, 1, 400000, 0, "Orochi V2 2026"), (0x00E5, 0x1F, 7, 59900, 3, ""),
    (0x00E6, 0x1F, 7, 59900, 3, ""),
    // inferred: txn 0x1F and the extended rate query; interface 3 per the catalog
    (0x00E2, 0x1F, 7, 31000, 3, ""), (0x00E3, 0x1F, 7, 31000, 3, "Basilisk V4 Pro"),
    (0x00E7, 0x1F, 7, 31000, 3, ""), (0x00E8, 0x1F, 7, 99900, 3, "Naga V3 Pro"),
];

fn device(pid: u16) -> Option<&'static Dev> {
    DEVICES.iter().find(|d| d.0 == pid)
}

fn crc(r: &[u8]) -> u8 {
    r[2..88].iter().fold(0, |a, b| a ^ b)
}

/// The 90-byte request for one of the getters above.
fn request(txn: u8, (class, id, size): (u8, u8, u8)) -> [u8; 90] {
    let mut r = [0u8; 90];
    r[1] = txn;
    r[5] = size;
    r[6] = class;
    r[7] = id;
    r[88] = crc(&r);
    r
}

#[derive(Debug, PartialEq, Eq)]
enum Reply {
    /// Argument bytes (80).
    Ok(Vec<u8>),
    /// Busy or someone else's answer: ask again.
    Retry,
    /// Asleep / unsupported / failed: give up.
    No,
}

/// Validates a GET_FEATURE result (report id first, then 90 bytes) against the request.
fn parse_reply(req: &[u8; 90], resp: &[u8]) -> Reply {
    let Some(r) = resp.get(1..91) else { return Reply::No };
    if r[6] != req[6] || r[7] != req[7] || r[2..4] != req[2..4] || r[88] != crc(r) {
        return Reply::Retry;
    }
    match r[0] {
        0x02 => Reply::Ok(r[8..88].to_vec()),
        0x00 | 0x01 => Reply::Retry,
        _ => Reply::No, // 3 failure, 4 timeout (asleep), 5 unsupported, garbage
    }
}

fn exchange(dev: &HidHandle, txn: u8, cmd: (u8, u8, u8), wait_us: u32) -> Option<Vec<u8>> {
    let req = request(txn, cmd);
    for _ in 0..3 {
        let mut out = [0u8; 91];
        out[1..].copy_from_slice(&req);
        if !dev.set_feature(&out) {
            return None;
        }
        std::thread::sleep(Duration::from_micros(u64::from(wait_us)));
        match parse_reply(&req, &dev.get_feature(0, 91)?) {
            Reply::Ok(a) => return Some(a),
            Reply::Retry => sleep_ms(10),
            Reply::No => return None,
        }
    }
    None
}

fn percent(arg1: u8) -> u8 {
    ((u32::from(arg1) * 100 + 127) / 255) as u8
}

fn rate_legacy(arg0: u8) -> Option<u32> {
    match arg0 {
        1 => Some(1000),
        2 => Some(500),
        8 => Some(125),
        _ => None,
    }
}

fn rate_ext(arg1: u8) -> Option<u32> {
    match arg1 {
        0x01 => Some(8000),
        0x02 => Some(4000),
        0x04 => Some(2000),
        0x08 => Some(1000),
        0x10 => Some(500),
        0x20 => Some(250),
        0x40 => Some(125),
        _ => None,
    }
}

pub struct Razer;

impl Driver for Razer {
    fn name(&self) -> &'static str {
        "razer"
    }

    fn matches(&self, info: &HidInfo) -> bool {
        info.vid == VID
            && device(info.pid).is_some_and(|d| info.interface.unwrap_or(0) == d.4 && (info.is_mouse() || d.4 == 3))
    }

    fn read(&self, dev: &HidHandle, info: &HidInfo) -> Option<Reading> {
        let &(_, txn, flags, wait, _, name) = device(info.pid)?;
        let mut r = Reading::default();
        if !name.is_empty() {
            r.name = Some(format!("Razer {name}"));
        }
        if flags & F_BATTERY != 0 {
            // No answer here = asleep: nothing else is worth asking.
            let a = exchange(dev, txn, BATTERY, wait)?;
            r.battery = Some(percent(a[1])).filter(|&p| p > 0); // 0 % with status OK = no data
            if flags & F_CHARGING != 0 {
                r.charging = exchange(dev, txn, CHARGING, wait).map(|a| a[1] != 0);
            }
        }
        let order = if flags & F_EXT_RATE != 0 { [RATE_EXT, RATE_LEGACY] } else { [RATE_LEGACY, RATE_EXT] };
        r.polling_hz = order.into_iter().find_map(|cmd| {
            let a = exchange(dev, txn, cmd, wait)?;
            if cmd == RATE_EXT { rate_ext(a[1]) } else { rate_legacy(a[0]) }
        });
        Some(r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A device reply to `req` carrying `args` (status given), CRC computed.
    fn reply(req: &[u8; 90], status: u8, args: &[u8]) -> Vec<u8> {
        let mut r = vec![0u8; 91];
        r[1..91].copy_from_slice(req);
        r[1] = status;
        r[9..9 + args.len()].copy_from_slice(args);
        r[89] = crc(&r[1..91]);
        r
    }

    #[test]
    fn requests_match_doc() {
        let q = request(0x1F, BATTERY);
        assert_eq!(&q[..8], &[0x00, 0x1F, 0, 0, 0, 0x02, 0x07, 0x80]);
        assert_eq!(q[88], 0x85); // 0x02 ^ 0x07 ^ 0x80
        assert_eq!(request(0x1F, CHARGING)[88], 0x81);
        assert_eq!(request(0x1F, RATE_LEGACY)[88], 0x84);
        assert_eq!(request(0x1F, RATE_EXT)[88], 0xC1);
    }

    #[test]
    fn capture_deathadder_v4_pro_dongle() {
        // doc: `02 1f 00 00 00 02 07 80 00 ab ...` -> 0xAB = 171 -> 67 %
        let q = request(0x1F, BATTERY);
        let Reply::Ok(a) = parse_reply(&q, &reply(&q, 0x02, &[0x00, 0xAB])) else { panic!() };
        assert_eq!(percent(a[1]), 67);
    }

    #[test]
    fn rejects_bad_replies() {
        let q = request(0x1F, BATTERY);
        assert_eq!(parse_reply(&q, &reply(&q, 0x04, &[0, 0xAB])), Reply::No); // asleep
        assert_eq!(parse_reply(&q, &reply(&q, 0x05, &[])), Reply::No);
        assert_eq!(parse_reply(&q, &reply(&q, 0x01, &[])), Reply::Retry); // busy
        let mut bad = reply(&q, 0x02, &[0, 0xAB]);
        bad[89] ^= 0xFF; // CRC
        assert_eq!(parse_reply(&q, &bad), Reply::Retry);
        let other = request(0x1F, CHARGING); // someone else's answer
        assert_eq!(parse_reply(&q, &reply(&other, 0x02, &[0, 1])), Reply::Retry);
        assert_eq!(parse_reply(&q, &[0; 10]), Reply::No);
    }

    #[test]
    fn rate_tables() {
        assert_eq!(rate_legacy(1), Some(1000));
        assert_eq!(rate_legacy(8), Some(125));
        assert_eq!(rate_legacy(3), None);
        assert_eq!(rate_ext(0x01), Some(8000));
        assert_eq!(rate_ext(0x08), Some(1000));
        assert_eq!(rate_ext(0x03), None);
    }

    #[test]
    fn pid_table() {
        assert_eq!(device(0x00BF).map(|d| (d.1, d.4)), Some((0x1F, 0)));
        assert_eq!(device(0x00E6).map(|d| d.4), Some(3));
        assert_eq!(device(0x0088).map(|d| d.5), Some("Basilisk Ultimate"));
        assert!(device(0x0001).is_none());
        assert_eq!(DEVICES.len(), 123);
    }
}
