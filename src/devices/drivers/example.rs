//! Pattern for a driver. This one never matches; copy it for a real family.

use crate::devices::{Driver, HidHandle, HidInfo, Reading};

/// Family members: (vid, pid), pid 0 = any product of that vendor.
const IDS: &[(u16, u16)] = &[];

pub struct Example;

impl Driver for Example {
    fn name(&self) -> &'static str {
        "example"
    }

    fn matches(&self, info: &HidInfo) -> bool {
        // 1) known ids, 2) AND the interface that speaks the protocol
        //    (a structural test here would also catch unlisted brands).
        super::id_listed(IDS, info) && super::usage_is(info, 0xFF00, 0x01)
    }

    fn read(&self, dev: &HidHandle, _info: &HidInfo) -> Option<Reading> {
        // READ-ONLY requests only. Example shape:
        //   dev.write(&[0x00, CMD_GET_BATTERY, ..])?;
        //   let rsp = dev.read_timeout(150)?;           // None => asleep => stale
        //   validate header/length, then fill the fields:
        let _ = dev;
        Some(Reading { battery: None, charging: None, polling_hz: None, ..Default::default() })
    }
}
