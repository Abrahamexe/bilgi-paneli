//! Corsair wireless mice: Bragi ("Slipstream") property GET and legacy NXP.
//! See docs/protocols/mouse-corsair.md.
//!
//! READ-ONLY allow-list: Bragi `GET` (command 0x02) of properties 0x0F (battery),
//! 0x10 (status), 0x01 (poll rate); NXP `0E 50` (GET battery field). No SET is built.

use super::util::{drain, payload, recv_until};
use crate::devices::hid::IO_TIMEOUT_MS;
use crate::devices::{Driver, HidHandle, HidInfo, Reading};

const VID: u16 = 0x1B1C;
const BRAGI_PAGE: u16 = 0xFF42;
const NXP_PAGE: u16 = 0xFFC2;

const BRAGI_GET: u8 = 0x02;
const P_POLLRATE: u8 = 0x01;
const P_BATTERY: u8 = 0x0F;
const P_STATUS: u8 = 0x10;

/// Route byte: 0x08 = the device that was opened, 0x09 = first paired mouse behind a dongle.
const DIRECT: u8 = 0x08;
const CHILD: u8 = 0x09;

/// Bragi devices: (pid, routes to try in order, name when it is a dongle).
/// Wired / cable PIDs use route 0x08; Slipstream dongles route 0x09.
const BRAGI: &[(u16, &[u8], &str)] = &[
    (0x1B4C, &[DIRECT], ""), (0x1B5E, &[DIRECT], ""), (0x1B80, &[DIRECT], ""), (0x1B7E, &[DIRECT], ""),
    (0x1B70, &[DIRECT], ""), (0x1B93, &[DIRECT], ""), (0x1BAC, &[DIRECT], ""), (0x1BBA, &[DIRECT], ""),
    (0x1B9E, &[DIRECT], ""), (0x1BF0, &[DIRECT], ""), (0x1BF1, &[DIRECT, CHILD], ""), (0x1BE3, &[DIRECT], ""),
    (0x1B9B, &[DIRECT], ""), (0x1BC9, &[DIRECT], ""), (0x2B32, &[DIRECT], ""),
    (0x1B66, &[CHILD], ""), (0x1B65, &[CHILD], ""), (0x1BA6, &[CHILD], ""),
    (0x1B81, &[CHILD], "Corsair Dark Core RGB Pro"), (0x1B7F, &[CHILD], "Corsair Dark Core RGB Pro SE"),
    (0x1BDC, &[CHILD], "Corsair M75 Wireless"), (0x2B00, &[CHILD], "Corsair Ironclaw RGB Wireless SE"),
    // inferred: the doc lists 1BB5 only as "wireless"; try the cable route, then the dongle route
    (0x1BB5, &[DIRECT, CHILD], ""),
];

/// NXP Dark Core RGB / SE (wired and dongle PIDs).
const NXP: &[u16] = &[0x1B35, 0x1B64, 0x1B4B, 0x1B51];
/// NXP coarse battery steps -> percent.
const NXP_STEPS: [u8; 5] = [0, 15, 30, 50, 100];
const REQ_NXP: [u8; 3] = [0x00, 0x0E, 0x50];

fn bragi_get(route: u8, prop: u8) -> [u8; 5] {
    [0x00, route, BRAGI_GET, prop, 0x00] // report id 0, then the 4-byte request
}

/// Reply to a GET: `[route] 02 <err> <v0> <v1> <v2>` (little-endian 24-bit value),
/// with a leading report-id byte 0 when Windows adds one.
/// `Err(code)` = device error (e.g. 5 not supported).
fn parse_bragi(resp: &[u8]) -> Option<Result<u32, u8>> {
    let o = usize::from(resp.first() == Some(&0));
    if *resp.get(o + 1)? != BRAGI_GET {
        return None;
    }
    match *resp.get(o + 2)? {
        0 => {
            let v = resp.get(o + 3..o + 6)?;
            Some(Ok(u32::from(v[0]) | u32::from(v[1]) << 8 | u32::from(v[2]) << 16))
        }
        e => Some(Err(e)),
    }
}

/// `Some(Some(v))` value, `Some(None)` device error, `None` no answer.
/// Another app may leave answers for other properties on the channel (the reply
/// echoes only the command), so the value is accepted once two answers agree.
fn get_prop(dev: &HidHandle, route: u8, prop: u8) -> Option<Option<u32>> {
    let mut last = None;
    for _ in 0..4 {
        drain(dev);
        if !dev.write(&bragi_get(route, prop)) {
            return None;
        }
        match recv_until(dev, IO_TIMEOUT_MS, parse_bragi)? {
            Err(_) => return Some(None),
            Ok(v) if last == Some(v) => return Some(Some(v)),
            Ok(v) => last = Some(v),
        }
    }
    None
}

/// Property 0x0F: tenths of a percent.
fn battery_pct(v: u32) -> Option<u8> {
    (1..=1000).contains(&v).then_some((v / 10) as u8)
}

/// Property 0x10: 1 charging, 2 discharging, 3 charged (-> unknown, "on cable").
fn charging(v: u32) -> Option<bool> {
    match v {
        1 => Some(true),
        2 => Some(false),
        _ => None,
    }
}

/// Property 0x01: 1 = 8 ms ... 4 = 1 ms. Higher codes are not documented.
fn rate_hz(v: u32) -> Option<u32> {
    match v {
        1..=4 => Some(1000 / (1 << (4 - v))),
        _ => None,
    }
}

/// NXP reply to `0E 50`: `0E 50 .. .. <level 0..4> <status>`; status 1 discharging, 2 charging, 3 full.
fn parse_nxp(resp: &[u8]) -> Option<(u8, Option<bool>)> {
    let d = payload(resp, &REQ_NXP[1..])?;
    let pct = *NXP_STEPS.get(usize::from(*d.get(4)?))?;
    let chg = match d.get(5)? {
        1 => Some(false),
        2 => Some(true),
        3 => None,
        _ => return None,
    };
    Some((pct, chg))
}

fn read_bragi(dev: &HidHandle, routes: &[u8], name: &str) -> Option<Reading> {
    for &route in routes {
        let mut r = Reading::default();
        if !name.is_empty() {
            r.name = Some(name.to_string());
        }
        // Silence on the first query = asleep / wrong route: skip this route.
        let Some(batt) = get_prop(dev, route, P_BATTERY) else { continue };
        if let Some(pct) = batt.and_then(battery_pct) {
            r.battery = Some(pct);
            // inferred: status is not hardware-confirmed (HaloBattery sees none on dongles)
            r.charging = get_prop(dev, route, P_STATUS).flatten().and_then(charging);
        }
        r.polling_hz = get_prop(dev, route, P_POLLRATE).flatten().and_then(rate_hz);
        if r.battery.is_some() || r.polling_hz.is_some() {
            return Some(r);
        }
    }
    None
}

fn read_nxp(dev: &HidHandle) -> Option<Reading> {
    drain(dev);
    if !dev.write(&REQ_NXP) {
        return None;
    }
    let (pct, chg) = recv_until(dev, IO_TIMEOUT_MS, parse_nxp)?;
    Some(Reading { battery: Some(pct), charging: chg, ..Default::default() })
}

pub struct CorsairMouse;

impl Driver for CorsairMouse {
    fn name(&self) -> &'static str {
        "corsair_mouse"
    }

    fn matches(&self, info: &HidInfo) -> bool {
        info.vid == VID
            && ((info.usage_page == BRAGI_PAGE && BRAGI.iter().any(|d| d.0 == info.pid))
                // inferred: NXP interface 1, usage page 0xFFC2 (from OpenRGB)
                || (info.usage_page == NXP_PAGE && NXP.contains(&info.pid)))
    }

    fn read(&self, dev: &HidHandle, info: &HidInfo) -> Option<Reading> {
        if info.usage_page == NXP_PAGE {
            return read_nxp(dev);
        }
        let &(_, routes, name) = BRAGI.iter().find(|d| d.0 == info.pid)?;
        read_bragi(dev, routes, name)
    }
}

#[cfg(test)]
mod tests {
    use super::super::util::hex;
    use super::*;

    #[test]
    fn request_layout() {
        assert_eq!(bragi_get(CHILD, P_BATTERY), [0x00, 0x09, 0x02, 0x0F, 0x00]);
        assert_eq!(bragi_get(DIRECT, P_POLLRATE)[2..4], [0x02, 0x01]);
    }

    #[test]
    fn battery_reply_from_halobattery() {
        // doc: `01 02 00 <lo> <hi>`, `26 02` = 550 tenths = 55 %
        let raw = hex("01 02 00 26 02 00");
        let v = parse_bragi(&raw).unwrap().unwrap();
        assert_eq!((v, battery_pct(v)), (550, Some(55)));
        // same with Windows' leading report id
        assert_eq!(parse_bragi(&hex("00 01 02 00 26 02 00")), Some(Ok(550)));
    }

    #[test]
    fn errors_and_garbage() {
        assert_eq!(parse_bragi(&hex("00 09 02 05 00 00 00")), Some(Err(5))); // not supported
        assert_eq!(parse_bragi(&hex("00 09 08 00 01 00 00")), None); // not a GET reply
        assert_eq!(parse_bragi(&hex("00 09")), None);
        assert_eq!(battery_pct(0), None);
        assert_eq!(battery_pct(1001), None);
    }

    #[test]
    fn status_and_rate() {
        assert_eq!(charging(1), Some(true));
        assert_eq!(charging(2), Some(false));
        assert_eq!(charging(3), None);
        assert_eq!(charging(0), None);
        assert_eq!([1, 2, 3, 4].map(rate_hz), [Some(125), Some(250), Some(500), Some(1000)]);
        assert_eq!(rate_hz(5), None);
        assert_eq!(rate_hz(0), None);
    }

    #[test]
    fn nxp_reply() {
        assert_eq!(parse_nxp(&hex("00 0E 50 00 00 03 02")), Some((50, Some(true))));
        assert_eq!(parse_nxp(&hex("0E 50 00 00 04 03")), Some((100, None)));
        assert_eq!(parse_nxp(&hex("0E 50 00 00 05 01")), None); // level > 4 invalid
        assert_eq!(parse_nxp(&hex("0E 51 00 00 02 01")), None);
    }
}
