//! Family C: Pulsar "Sonix / Fusion2" 64-byte feature protocol (VID 0x3710) and the
//! Pulsar Pro receiver 0x5405. Docs: docs/protocols/mouse-family-pulsar-sonix-fusion2.md.
//!
//! Read-only: battery (cat 8 reg 0x81), charging (cat 8 reg 0x83), polling (cat 1 reg
//! 0x89), paired model (cat 1 reg 0x84). The Fusion2 polling table is bit-reversed
//! relative to Family A: keep the tables apart.

use super::id_listed;
use super::mouse_io::{FEAT, feature_xfer};
use super::util::{recv_until, sleep_ms};
use crate::devices::{Driver, HidHandle, HidInfo, Reading, hid};

const IDS: &[(u16, u16)] = &[
    (0x3710, 0x1401), (0x3710, 0x1402), (0x3710, 0x1403), (0x3710, 0x1404), // wired Xlite / X2 / X2H / X2A
    (0x3710, 0x3401), (0x3710, 0x3404), (0x3710, 0x3409), (0x3710, 0x3410), // wired Xlite v4 / X3 / X3 LHD
    (0x3710, 0x5402), (0x3710, 0x5403), (0x3710, 0x5404), // 1K / 8K / Feinmann dongles
];
const DONGLES: &[u16] = &[0x5402, 0x5403, 0x5404];
const PRO: (u16, u16) = (0x3710, 0x5405);

/// Feature report 0 request: leading id byte, then dir, cat, reg, sub, 0, 0, profile ...
/// and the u16 LE sum of body bytes 0..62 at body[62..64].
const fn req(cat: u8, reg: u8, sub: u8) -> [u8; 65] {
    let mut b = [0u8; 65];
    b[2] = cat;
    b[3] = reg;
    b[4] = sub;
    let sum = cat as u16 + reg as u16 + sub as u16;
    b[63] = sum as u8;
    b[64] = (sum >> 8) as u8;
    b
}

const BATTERY: [u8; 65] = req(8, 0x81, 1);
const CHARGING: [u8; 65] = req(8, 0x83, 1);
const RATE: [u8; 65] = req(1, 0x89, 2);
const PAIRED: [u8; 65] = req(1, 0x84, 4);

/// Sends `q` once (resends once if nothing echoed), polls for the done reply (0x01)
/// echoing category/register. `Some(body)` without the report id.
fn ask(dev: &HidHandle, q: &[u8; 65], paired: bool) -> Option<Vec<u8>> {
    let (cat, reg) = (q[2], q[3]);
    for _ in 0..2 {
        let mut pending = false;
        let r = feature_xfer(dev, q, 15, 40, |buf| {
            let b = buf.get(1..).filter(|b| b.len() >= 64)?;
            let echo = b[1] == cat && b[2] == reg;
            match b[0] {
                // The 8K dongle answers the paired-model query under the previous request's header.
                1 if echo || (paired && b[6..8] == [0x10, 0x37]) => Some(Some(b.to_vec())),
                5 if echo => Some(None), // no link to the mouse: asleep or absent
                2 if echo => {
                    pending = true; // never resend while pending
                    None
                }
                _ => None,
            }
        });
        match r {
            Some(Some(v)) => return Some(v),
            Some(None) => return None,
            None if pending => return None,
            None => {}
        }
    }
    None
}

/// Fusion2 polling code (reply byte 7): 1 = 125 Hz ... 0x40 = 8000 Hz.
fn hz_for(code: u8) -> Option<u32> {
    Some(match code {
        0x01 => 125,
        0x02 => 250,
        0x04 => 500,
        0x08 => 1000,
        0x10 => 2000,
        0x20 => 4000,
        0x40 => 8000,
        _ => return None,
    })
}

/// Wired product id of the mouse behind a dongle (reply: VID LE at [6..8], PID LE at [8..10]).
fn paired_name(b: &[u8]) -> Option<&'static str> {
    if b.len() < 10 || u16::from_le_bytes([b[6], b[7]]) != 0x3710 {
        return None;
    }
    Some(match u16::from_le_bytes([b[8], b[9]]) {
        0x1401 => "Pulsar Xlite",
        0x1402 => "Pulsar X2",
        0x1403 => "Pulsar X2H",
        0x1404 => "Pulsar X2A",
        0x3401 => "Pulsar Xlite V4",
        0x3409 => "Pulsar X3",
        0x3410 => "Pulsar X3 LHD",
        _ => return None,
    })
}

/// inferred (hardware untested): Pulsar Pro receiver, output/input report 0, cmd echoed
/// in the first reply byte. Battery 0xB4 -> [1] percent, [6] charging; polling 0xB2 -> u16 LE Hz.
fn read_pro(dev: &HidHandle) -> Option<Reading> {
    const BATTERY: u8 = 0xB4;
    const RATE: u8 = 0xB2;
    let ask = |cmd: u8| -> Option<Vec<u8>> {
        for _ in 0..3 {
            if !dev.write(&[0, cmd]) {
                return None;
            }
            // Unnumbered report: Windows may or may not prefix a 0 id byte.
            let r = recv_until(dev, 300, |r| {
                [0usize, 1]
                    .into_iter()
                    .find_map(|o| r.get(o..).filter(|b| b.len() >= 8 && [cmd, 0xA6, 0xA7].contains(&b[0])))
                    .map(<[u8]>::to_vec)
            })?;
            match r[0] {
                0xA7 => sleep_ms(50),
                0xA6 => return None,
                _ => return Some(r),
            }
        }
        None
    };
    let b = ask(BATTERY)?;
    (b[1] <= 100).then_some(())?;
    Some(Reading {
        battery: Some(b[1]),
        charging: Some(b[6] == 1),
        polling_hz: ask(RATE).map(|r| u32::from(u16::from_le_bytes([r[2], r[3]]))).filter(|&h| (125..=8000).contains(&h)),
        ..Default::default()
    })
}

pub struct PulsarFusion2;

impl Driver for PulsarFusion2 {
    fn name(&self) -> &'static str {
        "pulsar-fusion2"
    }

    fn matches(&self, info: &HidInfo) -> bool {
        (id_listed(IDS, info) && info.is_vendor() && info.feature_len == 65)
            || ((info.vid, info.pid) == PRO && info.usage_page == 1 && info.usage == 0 && info.output_len >= 2)
    }

    fn read(&self, dev: &HidHandle, info: &HidInfo) -> Option<Reading> {
        if (info.vid, info.pid) == PRO {
            return read_pro(dev);
        }
        if hid::report_ids(&info.path)[FEAT] != [0] {
            return None;
        }
        let battery = ask(dev, &BATTERY, false)?[6];
        if battery > 100 {
            return None; // 0xFF = no mouse behind the dongle
        }
        let dongle = DONGLES.contains(&info.pid);
        Some(Reading {
            name: dongle
                .then(|| ask(dev, &PAIRED, true))
                .flatten()
                .and_then(|b| paired_name(&b))
                .map(String::from),
            battery: Some(battery),
            charging: ask(dev, &CHARGING, false).map(|b| b[6] == 1),
            polling_hz: ask(dev, &RATE, false).and_then(|b| hz_for(b[7])),
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

/// Documented checksum of a body (64 bytes without the report id): u16 LE sum of the first 62 bytes.
fn body_sum_ok(b: &[u8]) -> bool {
    b.len() >= 64 && b[..62].iter().map(|&x| u16::from(x)).sum::<u16>().to_le_bytes() == [b[62], b[63]]
}

    #[test]
    fn checksums_match_docs() {
        // docs: percent request sum 0x8A -> `8A 00`, charging 0x8C -> `8C 00`
        assert_eq!(&BATTERY[63..], &[0x8A, 0]);
        assert_eq!(&CHARGING[63..], &[0x8C, 0]);
        assert_eq!(&BATTERY[1..5], &[0, 8, 0x81, 1]);
        assert!(body_sum_ok(&BATTERY[1..]));
        let mut bad = BATTERY;
        bad[7] = 1;
        assert!(!body_sum_ok(&bad[1..]));
    }

    #[test]
    fn polling_table_is_reverse_of_family_a() {
        assert_eq!(hz_for(0x01), Some(125));
        assert_eq!(hz_for(0x08), Some(1000));
        assert_eq!(hz_for(0x40), Some(8000));
        assert_eq!(hz_for(0x03), None);
    }

    #[test]
    fn paired_model() {
        let b = [1, 1, 0x84, 4, 0, 0, 0x10, 0x37, 0x09, 0x34];
        assert_eq!(paired_name(&b), Some("Pulsar X3"));
        assert_eq!(paired_name(&[1, 1, 0x84, 4, 0, 0, 0x11, 0x37, 0x09, 0x34]), None);
    }
}
