//! Logitech HID++ 1.0 / 2.0 (Unifying, Lightspeed, Bolt, Nano incl. Dell/Lenovo
//! receivers, and wired / charging-cable mice). See docs/protocols/mouse-logitech-hidpp.md.
//!
//! Receivers: every paired device becomes its own [`Reading`] (`key` = slot).
//! READ-ONLY: only ping, ROOT feature lookup, getters (battery, name, type,
//! unit id, report rate) and HID++ 1.0 register *reads* are ever sent.

use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{Duration, Instant};

use super::util::{LIION_MV, interp};
use crate::devices::hid::IO_TIMEOUT_MS;
use crate::devices::{Connection, Driver, HidHandle, HidInfo, Kind, Reading};

const VID: u16 = 0x046D;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Rx {
    /// Unifying / Lightspeed / Nano: pairing registers 0xB5 sub 0x20+n / name 0x40+n.
    Classic,
    Bolt,
}

/// Receivers (vid, pid). Other-VID entries are OEM Nano receivers (Lenovo).
const RECEIVERS: &[(u16, u16, Rx)] = &[
    (VID, 0xC548, Rx::Bolt),
    (VID, 0xC52B, Rx::Classic), (VID, 0xC532, Rx::Classic), // Unifying
    (VID, 0xC539, Rx::Classic), (VID, 0xC53A, Rx::Classic), (VID, 0xC53D, Rx::Classic), // Lightspeed
    (VID, 0xC53F, Rx::Classic), (VID, 0xC541, Rx::Classic), (VID, 0xC543, Rx::Classic),
    (VID, 0xC545, Rx::Classic), (VID, 0xC547, Rx::Classic), (VID, 0xC54D, Rx::Classic),
    (VID, 0xC517, Rx::Classic), (VID, 0xC518, Rx::Classic), (VID, 0xC51A, Rx::Classic), // Nano / EX100
    (VID, 0xC51B, Rx::Classic), (VID, 0xC521, Rx::Classic), (VID, 0xC525, Rx::Classic),
    (VID, 0xC526, Rx::Classic), (VID, 0xC52E, Rx::Classic), (VID, 0xC52F, Rx::Classic),
    (VID, 0xC531, Rx::Classic), (VID, 0xC534, Rx::Classic), (VID, 0xC535, Rx::Classic),
    (VID, 0xC537, Rx::Classic),
    (0x17EF, 0x6042, Rx::Classic), // Lenovo Nano
];

fn receiver(info: &HidInfo) -> Option<Rx> {
    RECEIVERS.iter().find(|&&(v, p, _)| v == info.vid && p == info.pid).map(|&(_, _, k)| k)
}

// ---------------------------------------------------------------- frames (pure)

const SHORT: u8 = 0x10;
const LONG: u8 = 0x11;
/// Receiver itself (HID++ 1.0) or a directly connected device.
const DIRECT: u8 = 0xFF;

/// Rotating software id 0x0A..=0x0F so late replies are not mistaken.
fn sw_id() -> u8 {
    static N: AtomicU8 = AtomicU8::new(0);
    0x0A + N.fetch_add(1, Ordering::Relaxed) % 6
}

fn long_req(idx: u8, fi: u8, func: u8, params: &[u8]) -> [u8; 20] {
    let mut f = [0u8; 20];
    f[..4].copy_from_slice(&[LONG, idx, fi, (func << 4) | sw_id()]);
    f[4..4 + params.len()].copy_from_slice(params);
    f
}

/// HID++ 1.0 register request (sub 0x81 = GET_REGISTER, 0x83 = GET_LONG_REGISTER).
fn reg_req(idx: u8, sub: u8, reg: u8, p: [u8; 3]) -> [u8; 7] {
    [SHORT, idx, sub, reg, p[0], p[1], p[2]]
}

#[derive(Debug, PartialEq, Eq)]
enum Rsp {
    /// Reply parameters (bytes from index 4).
    Ok(Vec<u8>),
    /// HID++ 1.0 error code (`10 idx 8F sub addr err`).
    Err10(u8),
    /// HID++ 2.0 error code (`11 idx FF fi fn|sw err`).
    Err20(u8),
}

/// Matches report `r` (report id at 0) to a request with `idx`, byte 2 and byte 3.
fn classify(r: &[u8], idx: u8, b2: u8, b3: u8) -> Option<Rsp> {
    if r.len() < 5 || r[1] != idx {
        return None;
    }
    match (r[0], r[2]) {
        (SHORT | LONG, x) if x == b2 && r[3] == b3 => Some(Rsp::Ok(r[4..].to_vec())),
        (LONG, 0xFF) if r[3] == b2 && r[4] == b3 && r.len() > 5 => Some(Rsp::Err20(r[5])),
        (SHORT, 0x8F) if r[3] == b2 && r[4] == b3 && r.len() > 5 => Some(Rsp::Err10(r[5])),
        _ => None,
    }
}

struct Batt {
    pct: Option<u8>,
    charging: Option<bool>,
}

/// 0x1004 UNIFIED_BATTERY: capabilities (fn 0) + status (fn 1).
fn parse_1004(caps: &[u8], st: &[u8]) -> Option<Batt> {
    let (soc, levels, status) = (*st.first()?, *st.get(1)?, *st.get(2)?);
    let exact = caps.get(1)? & 2 != 0;
    let mut pct = if exact && soc <= 100 {
        Some(soc)
    } else if levels & 8 != 0 {
        Some(100)
    } else if levels & 4 != 0 {
        Some(60) // inferred: coarse level -> percent
    } else if levels & 2 != 0 {
        Some(20) // inferred
    } else if levels & 1 != 0 {
        Some(5) // inferred
    } else {
        None
    };
    let charging = match status {
        0 => Some(false),
        1 | 2 => Some(true),
        3 => {
            pct = Some(100);
            None
        }
        4 => return None,
        _ => Some(false),
    };
    Some(Batt { pct, charging })
}

/// 0x1000 BATTERY_LEVEL_STATUS fn 0.
fn parse_1000(p: &[u8]) -> Option<Batt> {
    let (pct, status) = (*p.first()?, *p.get(2)?);
    let valid = (1..=100).contains(&pct).then_some(pct);
    match status {
        0 => Some(Batt { pct: valid, charging: Some(false) }),
        1 | 2 | 4 => Some(Batt { pct: valid, charging: Some(true) }),
        3 => Some(Batt { pct: Some(100), charging: None }),
        _ => None,
    }
}

/// 0x1001 BATTERY_VOLTAGE fn 0.
fn parse_1001(p: &[u8]) -> Option<Batt> {
    let mv = u32::from(u16::from_be_bytes([*p.first()?, *p.get(1)?]));
    let flags = *p.get(2)?;
    if !(3000..=4500).contains(&mv) {
        return None;
    }
    let pct = interp(LIION_MV, mv);
    if flags & 0x80 == 0 {
        return Some(Batt { pct: Some(pct), charging: Some(false) });
    }
    match flags & 7 {
        0 => Some(Batt { pct: None, charging: Some(true) }), // voltage inflated while charging
        1 => Some(Batt { pct: Some(100), charging: None }),
        _ => Some(Batt { pct: Some(pct), charging: Some(false) }),
    }
}

/// HID++ 1.0 register 0x0D BATTERY_MILEAGE.
fn parse_reg0d(p: &[u8]) -> Option<Batt> {
    let (pct, st) = (*p.first()?, *p.get(2)? >> 6);
    match st {
        0 => Some(Batt { pct: (pct <= 100).then_some(pct), charging: Some(false) }),
        1 => Some(Batt { pct: None, charging: Some(true) }),
        2 => Some(Batt { pct: Some(100), charging: None }),
        _ => None,
    }
}

/// HID++ 1.0 register 0x07 BATTERY_STATUS.
fn parse_reg07(p: &[u8]) -> Option<Batt> {
    let (step, st) = (*p.first()?, *p.get(1)?);
    if !(1..=7).contains(&step) {
        return None;
    }
    let pct = Some((u32::from(step) * 100 / 7) as u8); // inferred: level step -> percent
    match st {
        0x00 => Some(Batt { pct, charging: Some(false) }),
        0x21 | 0x24 | 0x25 | 0x26 => Some(Batt { pct, charging: Some(true) }),
        0x22 => Some(Batt { pct: Some(100), charging: None }),
        _ => None,
    }
}

/// 0x8061 current-rate code -> Hz.
fn rate_8061(code: u8) -> Option<u32> {
    [125, 250, 500, 1000, 2000, 4000, 8000].get(usize::from(code)).copied()
}

/// 0x8060 report interval in ms -> Hz.
fn rate_8060(ms: u8) -> Option<u32> {
    (1..=8).contains(&ms).then(|| (1000.0 / f32::from(ms)).round() as u32)
}

/// Feature 0x0005 device type -> kind.
fn kind_v2(t: u8) -> Option<Kind> {
    match t {
        0 | 2 => Some(Kind::Keyboard),
        3..=5 => Some(Kind::Mouse),
        8 => Some(Kind::Headset),
        _ => None,
    }
}

/// Receiver pairing record kind nibble -> kind.
fn kind_reg(k: u8) -> Option<Kind> {
    // inferred: Solaar numbering 1 keyboard, 2 mouse, 3 numpad, 8 trackball, 9 touchpad
    match k {
        1 | 3 => Some(Kind::Keyboard),
        2 | 8 | 9 => Some(Kind::Mouse),
        _ => None,
    }
}

/// Classic receiver pairing info (long reg 0xB5, sub 0x20+n-1): (wpid, kind). `None` = empty slot.
fn parse_pairing(p: &[u8]) -> Option<(u16, Option<Kind>)> {
    let wpid = u16::from_be_bytes([*p.get(3)?, *p.get(4)?]);
    let kind = kind_reg(p.get(7)? & 0x0F);
    (wpid != 0).then_some((wpid, kind))
}

/// ASCII text of `len` bytes at `p[off..]`, up to the first NUL.
fn ascii(p: &[u8], off: usize) -> Option<String> {
    let len = usize::from(*p.get(off.checked_sub(1)?)?);
    let s: String = p.get(off..off + len.min(p.len().saturating_sub(off)))?
        .iter()
        .take_while(|&&b| b != 0)
        .filter(|b| b.is_ascii_graphic() || **b == b' ')
        .map(|&b| b as char)
        .collect();
    (!s.is_empty()).then_some(s)
}

/// Unit id of feature 0x0003 fn 0 (entity count, then 4 bytes) as a serial.
fn unit_serial(p: &[u8]) -> Option<String> {
    // inferred: byte offsets of the unit id
    let id = p.get(1..5)?;
    id.iter().any(|&b| b != 0).then(|| id.iter().map(|b| format!("{b:02X}")).collect())
}

// ---------------------------------------------------------------- transport

/// The long collection (always) plus the short one (HID++ 1.0 requests and
/// error replies travel as short reports) when it exists.
struct Link<'a> {
    long: &'a HidHandle,
    short: Option<HidHandle>,
}

impl Link<'_> {
    fn send(&self, f: &[u8]) -> bool {
        if f[0] == SHORT { self.short.as_ref().is_some_and(|s| s.write(f)) } else { self.long.write(f) }
    }

    /// First reply for which `f` returns `Some`, polling both collections.
    fn recv<T>(&self, ms: u32, mut f: impl FnMut(&[u8]) -> Option<T>) -> Option<T> {
        let end = Instant::now() + Duration::from_millis(u64::from(ms));
        loop {
            let left = end.saturating_duration_since(Instant::now()).as_millis() as u32;
            if left == 0 {
                return None;
            }
            let slice = if self.short.is_some() { left.min(8) } else { left };
            for h in [Some(self.long), self.short.as_ref()].into_iter().flatten() {
                if let Some(v) = h.read_timeout(slice).and_then(|r| f(&r)) {
                    return Some(v);
                }
            }
        }
    }

    fn xfer(&self, req: &[u8]) -> Option<Rsp> {
        if !self.send(req) {
            return None;
        }
        let (idx, b2, b3) = (req[1], req[2], req[3]);
        self.recv(IO_TIMEOUT_MS, |r| classify(r, idx, b2, b3))
    }

    /// HID++ 2.0 call; reply parameters on success.
    fn fap(&self, idx: u8, fi: u8, func: u8, params: &[u8]) -> Option<Vec<u8>> {
        match self.xfer(&long_req(idx, fi, func, params))? {
            Rsp::Ok(p) => Some(p),
            _ => None,
        }
    }

    fn ping(&self, idx: u8) -> Proto {
        match self.xfer(&long_req(idx, 0, 1, &[0, 0, 0x5A])) {
            Some(Rsp::Ok(p)) if p.get(2) == Some(&0x5A) => Proto::V2,
            Some(Rsp::Err10(1)) => Proto::V1,
            _ => Proto::Gone,
        }
    }

    fn feature(&self, idx: u8, id: u16) -> Option<u8> {
        self.fap(idx, 0, 0, &id.to_be_bytes()).and_then(|p| p.first().copied()).filter(|&i| i != 0)
    }

    fn name_v2(&self, idx: u8, fi: u8) -> Option<String> {
        let len = usize::from(*self.fap(idx, fi, 0, &[])?.first()?).min(48);
        let mut s = String::new();
        while s.len() < len {
            let chunk = self.fap(idx, fi, 1, &[s.len() as u8])?;
            let part: String = chunk.iter().take_while(|&&b| b != 0).map(|&b| b as char).collect();
            if part.is_empty() {
                break;
            }
            s.push_str(&part);
        }
        s.truncate(len);
        let s = s.trim().to_string();
        (!s.is_empty()).then_some(s)
    }
}

#[derive(PartialEq, Eq, Debug)]
enum Proto {
    V2,
    V1,
    /// No answer (asleep / absent) or error.
    Gone,
}

// ---------------------------------------------------------------- devices

fn read_v2(l: &Link, idx: u8) -> Reading {
    let mut r = Reading::default();
    if let Some(fi) = l.feature(idx, 0x0005) {
        r.name = l.name_v2(idx, fi);
        r.kind = l.fap(idx, fi, 2, &[]).and_then(|p| kind_v2(*p.first()?));
    }
    if let Some(fi) = l.feature(idx, 0x0003) {
        r.serial = l.fap(idx, fi, 0, &[]).and_then(|p| unit_serial(&p));
    }
    let batt = if let Some(fi) = l.feature(idx, 0x1004) {
        l.fap(idx, fi, 0, &[]).zip(l.fap(idx, fi, 1, &[])).and_then(|(c, s)| parse_1004(&c, &s))
    } else if let Some(fi) = l.feature(idx, 0x1000) {
        l.fap(idx, fi, 0, &[]).and_then(|p| parse_1000(&p))
    } else if let Some(fi) = l.feature(idx, 0x1001) {
        l.fap(idx, fi, 0, &[]).and_then(|p| parse_1001(&p))
    } else {
        None
    };
    if let Some(b) = batt {
        (r.battery, r.charging) = (b.pct, b.charging);
    }
    r.polling_hz = if let Some(fi) = l.feature(idx, 0x8061) {
        l.fap(idx, fi, 2, &[]).and_then(|p| rate_8061(*p.first()?))
    } else if let Some(fi) = l.feature(idx, 0x8060) {
        l.fap(idx, fi, 1, &[]).and_then(|p| rate_8060(*p.first()?))
    } else {
        None
    };
    r
}

fn read_v1(l: &Link, idx: u8) -> Reading {
    let get = |reg| match l.xfer(&reg_req(idx, 0x81, reg, [0; 3])) {
        Some(Rsp::Ok(p)) => Some(p),
        _ => None,
    };
    let batt = get(0x0D).and_then(|p| parse_reg0d(&p)).or_else(|| get(0x07).and_then(|p| parse_reg07(&p)));
    let mut r = Reading::default();
    if let Some(b) = batt {
        (r.battery, r.charging) = (b.pct, b.charging);
    }
    r
}

/// One long-register read of the receiver (HID++ 1.0 register 0xB5, sub `sub`).
/// Outer `None` = no answer; `Some(None)` = error reply (empty slot).
fn receiver_reg(l: &Link, sub: u8, arg: u8) -> Option<Option<Vec<u8>>> {
    match l.xfer(&reg_req(DIRECT, 0x83, 0xB5, [sub, arg, 0]))? {
        Rsp::Ok(p) => Some(Some(p)),
        _ => Some(None),
    }
}

fn read_receiver(l: &Link, rx: Rx) -> Vec<Reading> {
    let mut out = Vec::new();
    for slot in 1..=6u8 {
        let rec = match rx {
            Rx::Classic => receiver_reg(l, 0x20 + slot - 1, 0).map(|o| o.and_then(|p| parse_pairing(&p))),
            // inferred: Bolt pairing record offsets unknown; any non-zero payload = paired
            Rx::Bolt => receiver_reg(l, 0x50 + slot, 0)
                .map(|o| o.and_then(|p| p.iter().skip(1).any(|&b| b != 0).then_some((0, None)))),
        };
        let Some(rec) = rec else { break }; // receiver silent: do not wait 6 times
        let Some((_, reg_kind)) = rec else { continue };
        let reg_name = match rx {
            Rx::Classic => receiver_reg(l, 0x40 + slot - 1, 0).flatten().and_then(|p| ascii(&p, 2)),
            // inferred: Bolt name register layout (length at param[2])
            Rx::Bolt => receiver_reg(l, 0x60 + slot, 1).flatten().and_then(|p| ascii(&p, 3)),
        };
        let mut r = match l.ping(slot) {
            Proto::V2 => read_v2(l, slot),
            Proto::V1 => read_v1(l, slot),
            Proto::Gone => Reading::default(),
        };
        r.fill_from(Reading {
            name: reg_name,
            kind: reg_kind,
            connection: Some(Connection::Dongle),
            key: Some(format!("s{slot}")),
            ..Default::default()
        });
        out.push(r);
    }
    out
}

pub struct LogitechHidpp;

impl Driver for LogitechHidpp {
    fn name(&self) -> &'static str {
        "logitech_hidpp"
    }

    fn matches(&self, info: &HidInfo) -> bool {
        // The long (0x11) collection: receivers and any HID++ device on VID 046D
        // (identified by HID++ itself, not by a PID table).
        super::usage_is(info, 0xFF00, 0x0002) && (info.vid == VID || receiver(info).is_some())
    }

    fn read(&self, dev: &HidHandle, info: &HidInfo) -> Option<Reading> {
        self.read_all(dev, info, &[]).into_iter().next()
    }

    fn read_all(&self, dev: &HidHandle, info: &HidInfo, peers: &[HidInfo]) -> Vec<Reading> {
        let short = peers
            .iter()
            .find(|p| p.vid == info.vid && p.interface == info.interface && super::usage_is(p, 0xFF00, 0x0001))
            .and_then(HidHandle::open);
        let link = Link { long: dev, short };
        match receiver(info) {
            Some(rx) => read_receiver(&link, rx),
            None => match link.ping(DIRECT) {
                Proto::V2 => vec![read_v2(&link, DIRECT)],
                _ => Vec::new(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::util::hex;
    use super::*;

    #[test]
    fn classifies_replies() {
        // ping reply from a HID++ 2.0 device in slot 1 (protocol 4.2, marker 5A)
        let ok = hex("11 01 00 1A 04 02 5A 00");
        assert_eq!(classify(&ok, 1, 0, 0x1A), Some(Rsp::Ok(hex("04 02 5A 00"))));
        // HID++ 1.0 "invalid sub-id" error answering the ping
        let e10 = hex("10 01 8F 00 1A 01 00");
        assert_eq!(classify(&e10, 1, 0, 0x1A), Some(Rsp::Err10(1)));
        let e20 = hex("11 FF FF 05 1B 09 00 00");
        assert_eq!(classify(&e20, 0xFF, 5, 0x1B), Some(Rsp::Err20(9)));
        // other device index / other sw id / notification: ignored
        assert_eq!(classify(&ok, 2, 0, 0x1A), None);
        assert_eq!(classify(&ok, 1, 0, 0x1B), None);
        assert_eq!(classify(&hex("10 01 81 0D 50"), 1, 0x81, 0x0D), Some(Rsp::Ok(hex("50"))));
    }

    #[test]
    fn request_frames() {
        let r = long_req(1, 0, 1, &[0, 0, 0x5A]);
        assert_eq!(&r[..3], &[0x11, 1, 0]);
        assert_eq!(r[3] >> 4, 1);
        assert!((0x0A..=0x0F).contains(&(r[3] & 0xF)));
        assert_eq!(&r[4..7], &[0, 0, 0x5A]);
        assert_eq!(reg_req(0xFF, 0x83, 0xB5, [0x20, 0, 0]), [0x10, 0xFF, 0x83, 0xB5, 0x20, 0, 0]);
    }

    #[test]
    fn unified_battery() {
        // exact SoC supported (caps byte1 bit1), 64 %, good, discharging
        let b = parse_1004(&hex("0F 02"), &hex("40 04 00 00")).unwrap();
        assert_eq!((b.pct, b.charging), (Some(64), Some(false)));
        // charging
        let b = parse_1004(&hex("0F 02"), &hex("40 04 01 01")).unwrap();
        assert_eq!((b.pct, b.charging), (Some(64), Some(true)));
        // no exact SoC: level flags only; complete -> 100
        let b = parse_1004(&hex("0F 00"), &hex("00 02 00 00")).unwrap();
        assert_eq!((b.pct, b.charging), (Some(20), Some(false)));
        let b = parse_1004(&hex("0F 02"), &hex("5A 08 03 01")).unwrap();
        assert_eq!((b.pct, b.charging), (Some(100), None));
        assert!(parse_1004(&hex("0F 02"), &hex("40 04 04 00")).is_none());
    }

    #[test]
    fn level_and_voltage_battery() {
        let b = parse_1000(&hex("37 32 00")).unwrap();
        assert_eq!((b.pct, b.charging), (Some(55), Some(false)));
        let b = parse_1000(&hex("00 00 01")).unwrap(); // charging: % unknown
        assert_eq!((b.pct, b.charging), (None, Some(true)));
        assert_eq!(parse_1000(&hex("00 00 03")).unwrap().pct, Some(100));
        // 0x0F3C = 3900 mV, between 3859 (60) and 3922 (70)
        let b = parse_1001(&hex("0F 3C 00")).unwrap();
        assert_eq!((b.pct, b.charging), (Some(66), Some(false)));
        let b = parse_1001(&hex("10 50 80")).unwrap(); // charging
        assert_eq!((b.pct, b.charging), (None, Some(true)));
        assert!(parse_1001(&hex("00 00 00")).is_none());
    }

    #[test]
    fn hidpp10_registers() {
        let b = parse_reg0d(&hex("5A 00 00")).unwrap();
        assert_eq!((b.pct, b.charging), (Some(90), Some(false)));
        assert_eq!(parse_reg0d(&hex("32 00 40")).unwrap().charging, Some(true));
        assert!(parse_reg0d(&hex("32 00 C0")).is_none());
        let b = parse_reg07(&hex("07 22 00")).unwrap();
        assert_eq!((b.pct, b.charging), (Some(100), None));
        let b = parse_reg07(&hex("03 21 00")).unwrap();
        assert_eq!((b.pct, b.charging), (Some(42), Some(true)));
        assert!(parse_reg07(&hex("00 00 00")).is_none());
    }

    #[test]
    fn rates() {
        assert_eq!(rate_8060(1), Some(1000));
        assert_eq!(rate_8060(2), Some(500));
        assert_eq!(rate_8060(8), Some(125));
        assert_eq!(rate_8060(0), None);
        assert_eq!(rate_8061(3), Some(1000));
        assert_eq!(rate_8061(6), Some(8000));
        assert_eq!(rate_8061(7), None);
    }

    #[test]
    fn pairing_and_names() {
        // reply params: echo sub, interval 8 ms, WPID 4093 (PRO X Wireless), kind nibble 2 (mouse)
        let p = hex("20 00 08 40 93 00 00 02 00 00 00 00 00 00 00 00");
        assert_eq!(parse_pairing(&p), Some((0x4093, Some(Kind::Mouse))));
        assert_eq!(parse_pairing(&hex("20 00 00 00 00 00 00 00")), None);
        // name register reply: echo sub, length 7, "G Pro X"
        let n = hex("40 07 47 20 50 72 6F 20 58 00 00 00 00 00 00 00");
        assert_eq!(ascii(&n, 2).as_deref(), Some("G Pro X"));
        assert_eq!(unit_serial(&hex("01 12 34 56 78")).as_deref(), Some("12345678"));
        assert_eq!(unit_serial(&hex("01 00 00 00 00")), None);
        assert_eq!(kind_v2(3), Some(Kind::Mouse));
        assert_eq!(kind_v2(0), Some(Kind::Keyboard));
        assert_eq!(kind_v2(7), None);
    }

    #[test]
    fn receiver_table() {
        assert!(RECEIVERS.iter().any(|&(v, p, k)| (v, p, k) == (0x046D, 0xC548, Rx::Bolt)));
        assert!(RECEIVERS.iter().any(|&(v, p, _)| (v, p) == (0x17EF, 0x6042)));
    }
}
