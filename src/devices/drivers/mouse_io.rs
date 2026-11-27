//! Transfer helpers shared by the enthusiast-mouse family drivers.

use super::util::sleep_ms;
use crate::devices::{HidHandle, HidInfo, hid};

/// Index into [`hid::report_ids`]: input, output, feature.
pub const IN: usize = 0;
pub const OUT: usize = 1;
pub const FEAT: usize = 2;

/// Does the interface's descriptor declare report `id` of the given kind?
pub fn declares(info: &HidInfo, kind: usize, id: u8) -> bool {
    hid::report_ids(&info.path)[kind].contains(&id)
}

/// SET_FEATURE `req`, then GET_FEATURE (report id `req[0]`) every `gap_ms`, at
/// most `polls` times, until `f` accepts the buffer (which starts with the id).
pub fn feature_xfer<T>(
    dev: &HidHandle,
    req: &[u8],
    gap_ms: u64,
    polls: u32,
    mut f: impl FnMut(&[u8]) -> Option<T>,
) -> Option<T> {
    if !dev.set_feature(req) {
        return None;
    }
    for _ in 0..polls {
        sleep_ms(gap_ms);
        if let Some(v) = dev.get_feature(req[0], req.len()).and_then(|b| f(&b)) {
            return Some(v);
        }
    }
    None
}
