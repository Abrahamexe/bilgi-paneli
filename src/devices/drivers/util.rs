//! Helpers shared by the mouse drivers.

use std::time::{Duration, Instant};

use crate::devices::HidHandle;

/// Piecewise-linear lookup. `table` is `(x, percent)` sorted by ascending `x`;
/// values outside the table clamp to its first / last percent.
pub fn interp(table: &[(u32, u8)], x: u32) -> u8 {
    let (Some(&(x0, p0)), Some(&(x1, p1))) = (table.first(), table.last()) else { return 0 };
    if x <= x0 {
        return p0;
    }
    if x >= x1 {
        return p1;
    }
    let i = table.partition_point(|&(tx, _)| tx <= x);
    let ((xa, pa), (xb, pb)) = (table[i - 1], table[i]);
    let (pa, pb) = (i32::from(pa), i32::from(pb));
    let t = (x - xa) as i32 * (pb - pa) / (xb - xa) as i32;
    (pa + t).clamp(0, 100) as u8
}

/// Li-ion single cell, millivolts -> percent (Logitech HID++ 0x1001 curve).
/// Solaar-style 13-point table from docs/protocols/mouse-logitech-hidpp.md 4.3.
pub const LIION_MV: &[(u32, u8)] = &[
    (3500, 0), (3579, 2), (3646, 5), (3671, 10), (3717, 20), (3751, 30), (3778, 40),
    (3811, 50), (3859, 60), (3922, 70), (3989, 80), (4067, 90), (4186, 100),
];

/// `resp` with the leading report-id byte (0) removed when `head` only matches
/// after it. `None` when the reply does not begin with `head`.
pub fn payload<'a>(resp: &'a [u8], head: &[u8]) -> Option<&'a [u8]> {
    [0usize, 1].into_iter().find_map(|o| resp.get(o..).filter(|r| r.starts_with(head) && (o == 0 || resp[0] == 0)))
}

/// Waits up to `ms` for an input report for which `f` returns `Some`; other
/// reports (pushes, stale replies) are skipped.
pub fn recv_until<T>(dev: &HidHandle, ms: u32, mut f: impl FnMut(&[u8]) -> Option<T>) -> Option<T> {
    let end = Instant::now() + Duration::from_millis(u64::from(ms));
    loop {
        let left = end.saturating_duration_since(Instant::now()).as_millis() as u32;
        if left == 0 {
            return None;
        }
        if let Some(v) = f(&dev.read_timeout(left)?) {
            return Some(v);
        }
    }
}

/// Discards queued input reports (unsolicited pushes) before a request.
pub fn drain(dev: &HidHandle) {
    for _ in 0..8 {
        if dev.read_timeout(1).is_none() {
            break;
        }
    }
}

pub fn sleep_ms(ms: u64) {
    std::thread::sleep(Duration::from_millis(ms));
}

/// Hex string -> bytes (spaces ignored). For test fixtures.
#[cfg(test)]
pub fn hex(s: &str) -> Vec<u8> {
    s.split_whitespace().map(|b| u8::from_str_radix(b, 16).unwrap()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interp_points_and_between() {
        assert_eq!(interp(LIION_MV, 4186), 100);
        assert_eq!(interp(LIION_MV, 4300), 100);
        assert_eq!(interp(LIION_MV, 3400), 0);
        assert_eq!(interp(LIION_MV, 3811), 50);
        // halfway between 3811 (50) and 3859 (60)
        assert_eq!(interp(LIION_MV, 3835), 55);
        assert_eq!(interp(&[], 5), 0);
    }

    #[test]
    fn payload_strips_report_id() {
        assert_eq!(payload(&[0, 0xD2, 4], &[0xD2]), Some(&[0xD2, 4][..]));
        assert_eq!(payload(&[0xD2, 4], &[0xD2]), Some(&[0xD2, 4][..]));
        assert_eq!(payload(&[0, 0x92, 4], &[0xD2]), None);
        assert_eq!(payload(&[], &[1]), None);
    }
}
