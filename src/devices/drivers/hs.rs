//! Plumbing shared by the headset drivers: one request/reply port over a HID
//! interface, the parsed-state type and the `Reading` builder.
//!
//! Requests are written in "hidapi layout" (byte 0 = report id), the way the
//! protocol docs list them; replies come back in the same layout.

use super::util;
use crate::devices::{Connection, HidHandle, HidInfo, Kind, Reading, hid};

/// Battery state parsed from one reply. `Bat::OFF` = headset off / not linked.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Bat {
    pub pct: Option<u8>,
    pub chg: Option<bool>,
}

impl Bat {
    pub const OFF: Bat = Bat { pct: None, chg: None };
    /// Valid percentage (0..=100) or `None` for an invalid reply.
    pub fn new(pct: u8, chg: Option<bool>) -> Option<Bat> {
        (pct <= 100).then_some(Bat { pct: Some(pct), chg })
    }
    /// Charging with an untrustworthy / unknown level.
    pub const fn charging() -> Bat {
        Bat { pct: None, chg: Some(true) }
    }
}

/// Headset reading over a dongle (kind always Headset, so a "headset off"
/// answer still lists the device with no battery).
pub fn reading(b: Bat, conn: Connection) -> Reading {
    Reading {
        kind: Some(Kind::Headset),
        connection: Some(conn),
        battery: b.pct.filter(|p| *p <= 100),
        charging: b.chg,
        ..Default::default()
    }
}

/// Voltage -> percent. `table` ascending `(mV, %)`; below the first point the
/// level is unavailable (`None`), above the last it is clamped.
pub fn mv_pct(table: &[(u32, u8)], mv: u32) -> Option<u8> {
    (mv >= table.first()?.0).then(|| util::interp(table, mv))
}

pub struct Port<'a> {
    dev: &'a HidHandle,
    out: Vec<u8>,
    unnumbered_in: bool,
    flex: bool,
}

impl<'a> Port<'a> {
    pub fn new(dev: &'a HidHandle, info: &HidInfo) -> Self {
        let [inp, out, _] = hid::report_ids(&info.path);
        Port { dev, out, unnumbered_in: inp == [0], flex: false }
    }

    /// For docs that leave open whether byte 0 is a report id or payload: when
    /// the descriptor has only an unnumbered output report, send a 0 id first.
    pub fn flex(mut self) -> Self {
        self.flex = true;
        self
    }

    pub fn has_out(&self, id: u8) -> bool {
        self.out.contains(&id)
    }

    /// Sends one allow-listed request. Refuses when the descriptor has no matching output report.
    pub fn send(&self, req: &[u8]) -> bool {
        let Some(&id) = req.first() else { return false };
        let mut buf = req.to_vec();
        if !self.has_out(id) {
            if !(self.flex && self.out == [0]) {
                return false;
            }
            buf.insert(0, 0);
        }
        (self.dev.output_len == 0 || buf.len() <= self.dev.output_len) && self.dev.write(&buf)
    }

    /// Next input report in hidapi layout (leading 0 id of unnumbered reports removed).
    pub fn recv(&self, ms: u32) -> Option<Vec<u8>> {
        let mut r = self.dev.read_timeout(ms)?;
        if self.unnumbered_in && !r.is_empty() {
            r.remove(0);
        }
        Some(r)
    }

    /// Request, then up to `tries` reports (`ms` each) until `ok` accepts one.
    /// Stale queued reports are dropped first; no reply within `ms` => `None`.
    pub fn ask(&self, req: &[u8], ms: u32, tries: u32, ok: impl Fn(&[u8]) -> bool) -> Option<Vec<u8>> {
        util::drain(self.dev);
        if !self.send(req) {
            return None;
        }
        for _ in 0..tries {
            let r = self.recv(ms)?;
            if ok(&r) {
                return Some(r);
            }
        }
        None
    }
}

/// Big-endian u16 at `i`.
pub fn be16(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from(*b.get(i)?) << 8 | u32::from(*b.get(i + 1)?))
}
