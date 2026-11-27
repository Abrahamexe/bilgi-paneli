//! SteelSeries mice (Aerox 3/5/9 Wireless, Prime Wireless, Rival 3 Wireless, Rival 650).
//! rivalcfg-style: output report id 0 + opcode, reply is an input report.
//! See docs/protocols/mouse-steelseries.md.
//!
//! READ-ONLY allow-list: `92` (cable), `D2` (dongle) and `AA 01` (Rival 3 / 650)
//! battery queries. Polling rate is set-only on these mice: not provided.

use super::util::{drain, payload, recv_until};
use crate::devices::hid::IO_TIMEOUT_MS;
use crate::devices::{Driver, HidHandle, HidInfo, Reading};

const VID: u16 = 0x1038;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Op {
    /// `92`, reply `92 <battery byte>` (mouse on its cable).
    Cable,
    /// `D2` = `92 | 0x40` (2.4 GHz dongle).
    Dongle,
    /// `AA 01` (Rival 3 / Rival 3 Gen 2 / Rival 650).
    Rival,
}

const REQ_CABLE: [u8; 2] = [0x00, 0x92];
const REQ_DONGLE: [u8; 2] = [0x00, 0xD2];
const REQ_RIVAL: [u8; 3] = [0x00, 0xAA, 0x01];

/// (pid, op, USB interface, name)
const DEVICES: &[(u16, Op, u8, &str)] = &[
    (0x1838, Op::Dongle, 3, "SteelSeries Aerox 3 Wireless"),
    (0x1878, Op::Dongle, 3, "SteelSeries Aerox 3 Wireless"),
    (0x183A, Op::Cable, 3, "SteelSeries Aerox 3 Wireless"),
    (0x187A, Op::Cable, 3, "SteelSeries Aerox 3 Wireless"),
    (0x1852, Op::Dongle, 3, "SteelSeries Aerox 5 Wireless"),
    (0x185C, Op::Dongle, 3, "SteelSeries Aerox 5 Wireless"),
    (0x1860, Op::Dongle, 3, "SteelSeries Aerox 5 Wireless"),
    (0x1854, Op::Cable, 3, "SteelSeries Aerox 5 Wireless"),
    (0x185E, Op::Cable, 3, "SteelSeries Aerox 5 Wireless"),
    (0x1862, Op::Cable, 3, "SteelSeries Aerox 5 Wireless"),
    (0x1858, Op::Dongle, 3, "SteelSeries Aerox 9 Wireless"),
    (0x1874, Op::Dongle, 3, "SteelSeries Aerox 9 Wireless"),
    (0x185A, Op::Cable, 3, "SteelSeries Aerox 9 Wireless"),
    (0x1876, Op::Cable, 3, "SteelSeries Aerox 9 Wireless"),
    (0x1840, Op::Dongle, 3, "SteelSeries Prime Wireless"),
    (0x1848, Op::Dongle, 3, "SteelSeries Prime Mini Wireless"),
    (0x1842, Op::Cable, 3, "SteelSeries Prime Wireless"),
    (0x184A, Op::Cable, 3, "SteelSeries Prime Mini Wireless"),
    (0x1830, Op::Rival, 3, "SteelSeries Rival 3 Wireless"),
    (0x1872, Op::Rival, 3, "SteelSeries Rival 3 Wireless Gen 2"),
    (0x1726, Op::Rival, 0, "SteelSeries Rival 650 Wireless"),
    (0x172B, Op::Rival, 0, "SteelSeries Rival 650 Wireless"),
];

fn device(pid: u16) -> Option<&'static (u16, Op, u8, &'static str)> {
    DEVICES.iter().find(|d| d.0 == pid)
}

/// Aerox / Prime battery byte: bit 7 charging, low 7 bits `1 + level/5`
/// (0x01..=0x15 -> 0..100 %), 0 = mouse off / asleep.
fn parse_aerox(b: u8) -> Option<(u8, bool)> {
    let level = b & 0x7F;
    // inferred: the charging bit on the dongle path is not hardware-confirmed
    (1..=21).contains(&level).then(|| ((level - 1) * 5, b & 0x80 != 0))
}

/// Rival 3 / 650 reply: either `<level> <?> <charging>` or `AA <level> <?> <charging>`.
/// inferred: the two shapes come from references that disagree; neither is hardware-confirmed.
fn parse_rival(resp: &[u8]) -> Option<(u8, bool)> {
    let d = payload(resp, &[0xAA]).map(|d| &d[1..]).or_else(|| resp.strip_prefix(&[0]))?;
    let (level, chg) = (*d.first()?, *d.get(2)?);
    (1..=100).contains(&level).then_some((level, chg != 0))
}

pub struct SteelSeriesMouse;

impl Driver for SteelSeriesMouse {
    fn name(&self) -> &'static str {
        "steelseries_mouse"
    }

    fn matches(&self, info: &HidInfo) -> bool {
        info.vid == VID
            && device(info.pid).is_some_and(|d| info.interface.unwrap_or(d.2) == d.2 && info.is_vendor())
    }

    fn read(&self, dev: &HidHandle, info: &HidInfo) -> Option<Reading> {
        let &(_, op, _, name) = device(info.pid)?;
        drain(dev);
        let req: &[u8] = match op {
            Op::Cable => &REQ_CABLE,
            Op::Dongle => &REQ_DONGLE,
            Op::Rival => &REQ_RIVAL,
        };
        if !dev.write(req) {
            return None;
        }
        let (battery, charging) = recv_until(dev, IO_TIMEOUT_MS, |r| match op {
            Op::Cable | Op::Dongle => parse_aerox(*payload(r, &req[1..])?.get(1)?),
            Op::Rival => parse_rival(r),
        })?;
        Some(Reading {
            name: Some(name.to_string()),
            battery: Some(battery),
            charging: Some(charging),
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::util::hex;
    use super::*;

    #[test]
    fn aerox_battery_byte() {
        // doc: Aerox 9 Wireless dongle reply `D2 04 ...` = 15 %
        assert_eq!(parse_aerox(0x04), Some((15, false)));
        assert_eq!(parse_aerox(0x15), Some((100, false)));
        assert_eq!(parse_aerox(0x01), Some((0, false)));
        assert_eq!(parse_aerox(0x84), Some((15, true)));
        assert_eq!(parse_aerox(0x00), None); // asleep
        assert_eq!(parse_aerox(0x7F), None);
    }

    #[test]
    fn reply_framing() {
        let r = hex("00 D2 04 00 00 00");
        assert_eq!(payload(&r, &REQ_DONGLE[1..]).and_then(|d| parse_aerox(d[1])), Some((15, false)));
        // the cable opcode must not be accepted from a dongle exchange
        assert!(payload(&r, &REQ_CABLE[1..]).is_none());
    }

    #[test]
    fn rival_shapes() {
        assert_eq!(parse_rival(&hex("00 64 00 01")), Some((100, true)));
        assert_eq!(parse_rival(&hex("00 37 00 00")), Some((55, false)));
        assert_eq!(parse_rival(&hex("00 AA 37 00 01")), Some((55, true)));
        assert_eq!(parse_rival(&hex("00 00 00 00")), None);
    }

    #[test]
    fn table() {
        assert_eq!(device(0x1726).map(|d| d.2), Some(0));
        assert_eq!(device(0x1840).map(|d| d.1), Some(Op::Dongle));
        assert_eq!(device(0x1842).map(|d| d.1), Some(Op::Cable));
    }
}
