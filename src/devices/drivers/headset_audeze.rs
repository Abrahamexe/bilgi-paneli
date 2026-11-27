//! Audeze Maxwell / Maxwell 2 dongles (docs/protocols/headset-audeze.md).
//!
//! Output report 0x06 carries a framed message, replies are polled from input
//! report 0x07 with GET_REPORT. Read-only: one battery request.

use super::hs::{Bat, Port, reading};
use super::util::sleep_ms;
use crate::devices::{Connection, Driver, HidHandle, HidInfo, Reading};

const VID: u16 = 0x3329;
const REPORT_IN: u8 = 0x07;

/// (pid, interface): Maxwell 0x4B19 / 0x4B18 on interface 0, Maxwell 2 0x4B29 / 0x4B28 on interface 5.
const MODELS: &[(u16, u8)] = &[(0x4B19, 0), (0x4B18, 0), (0x4B29, 5), (0x4B28, 5)];

/// Allow-list. Report 0x06, fragment length 7, route 0x80 (through the dongle),
/// message `05 5A 03 00 D6 0C 00` = request, body "get battery".
const REQ_BATTERY: [u8; 10] = [0x06, 0x07, 0x80, 0x05, 0x5A, 0x03, 0x00, 0xD6, 0x0C, 0x00];

#[derive(Debug, PartialEq, Eq)]
enum Poll {
    /// Result not complete yet.
    Wait,
    Failed,
    Percent(u8),
}

/// Walks the reassembled message stream (`05 <type> <len lo> <len hi> <body>`):
/// type 5B = ack (status at body[2]), 5D = async result with percent at body[4].
fn parse(stream: &[u8]) -> Poll {
    let mut i = 0;
    while i + 4 <= stream.len() {
        if stream[i] != 0x05 {
            i += 1; // resync on garbage
            continue;
        }
        let len = usize::from(stream[i + 2]) | usize::from(stream[i + 3]) << 8;
        let Some(body) = stream.get(i + 4..i + 4 + len) else { break };
        if body.len() >= 3 && body[..2] == [0xD6, 0x0C] {
            match stream[i + 1] {
                0x5B if body[2] != 0 => return Poll::Failed,
                0x5D if body.len() >= 5 && body[2] == 0 => {
                    return if body[4] <= 100 { Poll::Percent(body[4]) } else { Poll::Failed };
                }
                _ => {}
            }
        }
        i += 4 + len;
    }
    Poll::Wait
}

/// Appends the valid fragment of one input report (`07 <len> <route> <bytes...>`).
fn fragment(report: &[u8]) -> &[u8] {
    match report {
        [REPORT_IN, len, _, rest @ ..] => &rest[..usize::from(*len).min(59).min(rest.len())],
        _ => &[],
    }
}

pub struct Audeze;

impl Driver for Audeze {
    fn name(&self) -> &'static str {
        "audeze-maxwell"
    }

    fn matches(&self, info: &HidInfo) -> bool {
        info.vid == VID
            && info.usage_page == 0xFF13
            && info.usage == 0x0001
            && MODELS.iter().any(|&(pid, iface)| pid == info.pid && info.interface.unwrap_or(0) == iface)
    }

    fn read(&self, dev: &HidHandle, info: &HidInfo) -> Option<Reading> {
        let port = Port::new(dev, info);
        // Drop whatever is still pending, then send the request.
        dev.get_input_report(REPORT_IN, 62);
        if !port.send(&REQ_BATTERY) {
            return None;
        }
        let mut stream = Vec::new();
        for _ in 0..12 {
            sleep_ms(60);
            if let Some(r) = dev.get_input_report(REPORT_IN, 62) {
                stream.extend_from_slice(fragment(&r));
            }
            match parse(&stream) {
                Poll::Percent(p) => return Some(reading(Bat { pct: Some(p), chg: None }, Connection::Dongle)),
                Poll::Failed => return None,
                Poll::Wait => {}
            }
        }
        // The headset does not answer while switched off.
        Some(reading(Bat::OFF, Connection::Dongle))
    }
}

#[cfg(test)]
mod tests {
    use super::super::util::hex;
    use super::*;

    #[test]
    fn async_result_gives_percent() {
        let s = hex("05 5B 03 00 D6 0C 00 05 5D 05 00 D6 0C 00 00 4B");
        assert_eq!(parse(&s), Poll::Percent(0x4B));
    }

    #[test]
    fn waits_for_result_and_detects_failures() {
        assert_eq!(parse(&hex("05 5B 03 00 D6 0C 00")), Poll::Wait);
        assert_eq!(parse(&hex("05 5D 05 00 D6 0C 00 00")), Poll::Wait); // truncated
        assert_eq!(parse(&hex("05 5B 03 00 D6 0C 01")), Poll::Failed);
        assert_eq!(parse(&hex("05 5D 05 00 D6 0C 00 00 65")), Poll::Failed);
        // other command's result is skipped
        assert_eq!(parse(&hex("05 5D 05 00 D6 0D 00 00 10")), Poll::Wait);
        assert_eq!(parse(&[]), Poll::Wait);
    }

    #[test]
    fn fragments_respect_length() {
        let r = hex("07 03 80 05 5B 03 AA BB");
        assert_eq!(fragment(&r), &[0x05, 0x5B, 0x03]);
        assert_eq!(fragment(&hex("07 00 80 11 22")), &[] as &[u8]);
        assert_eq!(fragment(&hex("06 03 80 05 5B 03")), &[] as &[u8]);
        assert_eq!(fragment(&hex("07 3B 80 01")), &[0x01]);
    }
}
