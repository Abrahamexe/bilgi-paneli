//! Small helpers shared by the keyboard drivers (`kb_*.rs`); builds on `util`.

use super::util::{drain, recv_until};
use crate::devices::HidHandle;

/// Drops queued pushes, sends one output report, then waits up to `timeout_ms` for an
/// input report accepted by `ok` (others are skipped).
pub fn exchange(dev: &HidHandle, req: &[u8], timeout_ms: u32, ok: impl Fn(&[u8]) -> bool) -> Option<Vec<u8>> {
    drain(dev);
    if !dev.write(req) {
        return None;
    }
    recv_until(dev, timeout_ms, |r| ok(r).then(|| r.to_vec()))
}

/// Index where `hdr` starts in report `r`: 0, or 1 when Windows prepended a
/// report-id byte. `None` when `r` does not begin with `hdr`.
pub fn echo_at(r: &[u8], hdr: &[u8]) -> Option<usize> {
    (0..2).find(|&o| r.get(o..o + hdr.len()) == Some(hdr))
}

#[cfg(test)]
mod tests {
    use super::echo_at;

    #[test]
    fn echo_offsets() {
        assert_eq!(echo_at(&[0xA7, 0x0D, 0], &[0xA7, 0x0D]), Some(0));
        assert_eq!(echo_at(&[0, 0xA7, 0x0D], &[0xA7, 0x0D]), Some(1));
        assert_eq!(echo_at(&[0, 0, 0xA7, 0x0D], &[0xA7, 0x0D]), None);
        assert_eq!(echo_at(&[0xA7], &[0xA7, 0x0D]), None);
    }
}
