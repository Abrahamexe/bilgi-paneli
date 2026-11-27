//! ASUS ROG / TUF mice (cable, 2.4 GHz dongles, OMNI / SPEEDNOVA receivers):
//! `12 07` battery + charging, `12 04` polling rate. See docs/protocols/mouse-asus-rog.md.
//!
//! READ-ONLY allow-list: `12 07` and `12 04` (sub 00). Never the `51 xx` / `50 xx` families.

use super::util::{drain, recv_until};
use crate::devices::hid::IO_TIMEOUT_MS;
use crate::devices::{Driver, HidHandle, HidInfo, Reading};

const VID: u16 = 0x0B05;

const CMD_BATTERY: [u8; 2] = [0x12, 0x07];
const CMD_RATE: [u8; 2] = [0x12, 0x04];

/// Report id of the OMNI / SPEEDNOVA receivers (collection 3 of interface 2).
const RID_RECEIVER: u8 = 3;

/// (pid, USB interface, report id, battery is a 0..4 step level, name)
const DEVICES: &[(u16, u8, u8, bool, &str)] = &[
    (0x18E3, 0, 0, true, "ROG Chakram"), (0x18E5, 0, 0, true, "ROG Chakram"),
    (0x1958, 0, 0, true, "ROG Chakram Core"),
    (0x1A18, 0, 0, false, "ROG Chakram X"), (0x1A1A, 0, 0, false, "ROG Chakram X"),
    (0x1845, 2, 0, false, "ROG Gladius II"), (0x1877, 2, 0, false, "ROG Gladius II"),
    (0x18A0, 2, 0, false, "ROG Gladius II Wireless"),
    (0x197D, 0, 0, false, "ROG Gladius III Wireless"), (0x197F, 0, 0, false, "ROG Gladius III Wireless"),
    (0x1A72, 0, 0, false, "ROG Gladius III Wireless AimPoint"),
    (0x195C, 0, 0, false, "ROG Keris"),
    (0x195E, 0, 0, true, "ROG Keris Wireless"), (0x1960, 0, 0, true, "ROG Keris Wireless"),
    (0x1A66, 0, 0, false, "ROG Keris Wireless AimPoint"), (0x1A68, 0, 0, false, "ROG Keris Wireless AimPoint"),
    (0x1B16, 0, 0, false, "ROG Keris II Ace"), (0x1C0C, 0, 0, false, "ROG Keris II Origin"),
    (0x1A92, 0, 0, false, "ROG Harpe Ace Aim Lab Edition"), (0x1A94, 0, 0, false, "ROG Harpe Ace Aim Lab Edition"),
    (0x1B63, 0, 0, false, "ROG Harpe Ace Mini"),
    (0x1C69, 0, 0, false, "ROG Harpe II Ace"), (0x1AD0, 2, RID_RECEIVER, false, "ROG Harpe II Ace"),
    (0x1ACE, 2, RID_RECEIVER, false, ""), // OMNI: shared by several mice, model unknown
    (0x1977, 0, 0, false, "ROG Spatha X"), (0x1979, 0, 0, false, "ROG Spatha X"),
    (0x1947, 0, 0, true, "ROG Strix Impact II Wireless"), (0x1949, 0, 0, true, "ROG Strix Impact II Wireless"),
    (0x1A88, 0, 0, false, "ROG Strix Impact III"), (0x1AD7, 0, 0, false, "ROG Strix Impact III Wireless"),
    (0x18B4, 1, 0, false, "ROG Strix Carry"),
    (0x1846, 2, 0, false, "ROG Pugio"), (0x1906, 0, 0, false, "ROG Pugio II"),
    (0x1908, 0, 0, false, "ROG Pugio II"),
    (0x1A03, 0, 0, false, "TUF Gaming M4 Air"),
    (0x1C56, 0, 0, false, "TUF Gaming Mini WL"), (0x1C57, 0, 0, false, "TUF Gaming Mini WL"),
    (0x1A24, 2, 0, false, "ASUS MD200"),
];

fn device(pid: u16) -> Option<&'static (u16, u8, u8, bool, &'static str)> {
    DEVICES.iter().find(|d| d.0 == pid)
}

fn request(rid: u8, cmd: [u8; 2]) -> [u8; 4] {
    [rid, cmd[0], cmd[1], 0x00]
}

/// `<rid> 12 07 .. .. <battery> .. .. .. .. <charging>`; `None` = standby (zeros, 0 % not charging).
fn parse_battery(r: &[u8], rid: u8, steps: bool) -> Option<(u8, bool)> {
    if r.len() < 11 || r[..3] != [rid, CMD_BATTERY[0], CMD_BATTERY[1]] {
        return None;
    }
    let (raw, chg) = (r[5], r[10] != 0);
    let pct = if steps && raw <= 4 {
        raw * 25 // inferred: 0..4 level -> percent (ASUS indicator steps)
    } else if raw <= 100 {
        raw
    } else {
        return None;
    };
    (pct > 0 || chg).then_some((pct, chg))
}

/// `<rid> 12 04 00 .. byte 13`: low 3 bits (high nibble on the 8K receivers when set).
fn parse_rate(r: &[u8], rid: u8) -> Option<u32> {
    if r.len() < 14 || r[..3] != [rid, CMD_RATE[0], CMD_RATE[1]] {
        return None;
    }
    let b = r[13];
    let code = if rid == RID_RECEIVER && b >> 4 != 0 { b >> 4 } else { b & 7 };
    [125, 250, 500, 1000, 2000, 4000, 8000].get(usize::from(code)).copied()
}

pub struct AsusRogMouse;

impl Driver for AsusRogMouse {
    fn name(&self) -> &'static str {
        "asus_rog_mouse"
    }

    fn matches(&self, info: &HidInfo) -> bool {
        info.vid == VID
            && device(info.pid).is_some_and(|d| {
                info.interface.unwrap_or(d.1) == d.1
                    && info.is_vendor()
                    && (d.2 != RID_RECEIVER || info.collection == Some(3))
            })
    }

    fn read(&self, dev: &HidHandle, info: &HidInfo) -> Option<Reading> {
        let &(_, _, rid, steps, name) = device(info.pid)?;
        drain(dev);
        if !dev.write(&request(rid, CMD_BATTERY)) {
            return None;
        }
        // Unsolicited reports (`12 01 ..`) share the pipe: match on the first 3 bytes.
        let (battery, charging) = recv_until(dev, IO_TIMEOUT_MS, |r| parse_battery(r, rid, steps))?;
        let mut out = Reading { battery: Some(battery), charging: Some(charging), ..Default::default() };
        if !name.is_empty() {
            out.name = Some(name.to_string());
        }
        if dev.write(&request(rid, CMD_RATE)) {
            out.polling_hz = recv_until(dev, IO_TIMEOUT_MS, |r| parse_rate(r, rid));
        }
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::super::util::hex;
    use super::*;

    #[test]
    fn harpe_ii_ace_capture() {
        // doc (HW): `03 12 07 00 00 35 02 14 fd 0e 00 ...` = 53 %, not charging
        let r = hex("03 12 07 00 00 35 02 14 FD 0E 00 00 00 00 00 00");
        assert_eq!(parse_battery(&r, 3, false), Some((53, false)));
        // wrong report id / command / short
        assert_eq!(parse_battery(&r, 0, false), None);
        assert_eq!(parse_battery(&hex("03 12 01 00 00 35 02 14 FD 0E 00"), 3, false), None);
        assert_eq!(parse_battery(&hex("03 12 07"), 3, false), None);
    }

    #[test]
    fn standby_charging_and_steps() {
        // asleep: zeros -> no reading; charging at 0 % is still a reading
        assert_eq!(parse_battery(&hex("00 12 07 00 00 00 00 00 00 00 00"), 0, false), None);
        assert_eq!(parse_battery(&hex("00 12 07 00 00 00 00 00 00 00 01"), 0, false), Some((0, true)));
        // error packet `ff aa`
        assert_eq!(parse_battery(&hex("00 FF AA 00 00 50 00 00 00 00 00"), 0, false), None);
        // step level models: 3 -> 75 %
        assert_eq!(parse_battery(&hex("00 12 07 00 00 03 00 00 00 00 00"), 0, true), Some((75, false)));
        assert_eq!(parse_battery(&hex("00 12 07 00 00 50 00 00 00 00 00"), 0, true), Some((80, false)));
        assert_eq!(parse_battery(&hex("00 12 07 00 00 65 00 00 00 00 00"), 0, false), None);
    }

    #[test]
    fn polling_codes() {
        let mut r = hex("00 12 04 00 00 00 00 00 00 00 00 00 00 03 00");
        assert_eq!(parse_rate(&r, 0), Some(1000));
        r[13] = 0x00;
        assert_eq!(parse_rate(&r, 0), Some(125));
        r[13] = 0x06;
        assert_eq!(parse_rate(&r, 0), Some(8000));
        r[13] = 0x07;
        assert_eq!(parse_rate(&r, 0), None);
        // 8K receiver: high nibble wins when set
        let mut rx = hex("03 12 04 00 00 00 00 00 00 00 00 00 00 53 00");
        assert_eq!(parse_rate(&rx, 3), Some(4000));
        rx[13] = 0x02;
        assert_eq!(parse_rate(&rx, 3), Some(500));
    }

    #[test]
    fn requests_and_table() {
        assert_eq!(request(3, CMD_BATTERY), [3, 0x12, 0x07, 0]);
        assert_eq!(request(0, CMD_RATE), [0, 0x12, 0x04, 0]);
        assert_eq!(device(0x1AD0).map(|d| d.2), Some(3));
        assert_eq!(device(0x1846).map(|d| d.1), Some(2));
    }
}
