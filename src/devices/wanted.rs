//! "Wanted" devices: models whose protocol is unknown or only inferred, so a
//! test report from a real unit is valuable. The tray app offers to create a
//! report (see `crate::report`) the first time one of them shows up.

use super::enumerate::Physical;
use super::Kind;

/// Why a device is on the list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wanted {
    /// Mouse / keyboard / headset with a vendor-defined HID collection and no driver.
    NoDriver,
    /// Model in [`LISTED`] (protocol unknown or inferred).
    Listed(&'static str),
}

impl Wanted {
    /// One line for the report (Turkish).
    pub fn explain(self) -> String {
        match self {
            Wanted::NoDriver => {
                "Evet: sürücü eşleşmedi ve cihazda üreticiye özel (vendor) HID koleksiyonu var.".to_string()
            }
            Wanted::Listed(model) => format!("Evet: protokolü bilinmiyor veya tahmin edilmiş model ({model})."),
        }
    }
}

/// (vendor id, product id, model). `pid == 0` = every product of that vendor.
/// Built from docs/protocols/README-*.md entries tagged Unknown / Inferred and
/// from the `// inferred:` matches in the drivers.
#[rustfmt::skip]
pub const LISTED: &[(u16, u16, &str)] = &[
    // Unknown protocol (docs: README-mouse-enthusiast.md, headset-razer.md, keyboard-razer.md)
    (0x36A7, 0xA887, "WLMouse Beast X 4K"),                        // own report-4 protocol, battery unknown
    (0x1532, 0x1014, "Razer BlackShark V2 Pro (PS)"),              // documented in HeadsetControl, no driver here yet
    (0x1532, 0x0290, "Razer DeathStalker V2 Pro (kablosuz)"),      // keyboard-razer.md
    (0x1532, 0x0292, "Razer DeathStalker V2 Pro"),
    (0x1532, 0x0296, "Razer DeathStalker V2 Pro TKL (kablosuz)"),
    (0x1532, 0x0298, "Razer DeathStalker V2 Pro TKL"),
    (0x1532, 0x028D, "Razer BlackWidow V4 Pro"),                   // PID from OpenRazer (not in our docs)
    // Inferred: matched by a driver, but the values are not confirmed on hardware
    (0x3770, 0,      "Hitscan"),                                   // family_compx_xvi: command table unverified
    (0x1B1C, 0x1BB5, "Corsair kablosuz fare (1BB5)"),              // corsair_mouse: route inferred
    (0x1B1C, 0x0A62, "Corsair Virtuoso XT (kablolu)"),             // headset_corsair: target 0x08 inferred
    (0x1B1C, 0x0A3D, "Corsair Virtuoso XT (kablolu)"),
    (0x03F0, 0x0D93, "HyperX Cloud Stinger 2 Wireless"),           // headset_hyperx: only in a DTS table
    (0x3710, 0x5405, "Pulsar Pro alıcısı"),                        // family_pulsar_fusion2: untested read_pro
    (0x361D, 0x0300, "Finalmouse Starlight X (SLX)"),              // finalmouse: report/cmd ids unverified
    (0x361D, 0x0301, "Finalmouse Starlight X (SLX)"),
    (0x361D, 0x0310, "Finalmouse Starlight X (SLX)"),
    (0x361D, 0x0311, "Finalmouse Starlight X (SLX)"),
    (0x38A6, 0x2710, "AULA F87 Pro"),                              // kb_aa_frame: settings field order inferred
    (0x38A6, 0x2711, "AULA F87 Pro"),
    (0x38A6, 0x2712, "AULA F87 Pro"),
    (0x38A6, 0x2713, "AULA F87 Pro"),
    (0x0C45, 0x8803, "AULA F87 Pro"),
    (0x0C45, 0xFEFC, "AULA F87 Pro (alıcı)"),
    (0x0C45, 0xFEFE, "AULA F87 Pro (alıcı)"),
    (0x1532, 0x00E2, "Razer (E2)"),                                // razer: txn / rate query inferred (battery-report catalog)
    (0x1532, 0x00E3, "Razer Basilisk V4 Pro"),
    (0x1532, 0x00E7, "Razer (E7)"),
    (0x1532, 0x00E8, "Razer Naga V3 Pro"),
];

/// Model name when (vid, pid) is on the list.
pub fn listed(vid: u16, pid: u16) -> Option<&'static str> {
    LISTED.iter().find(|&&(v, p, _)| v == vid && (p == 0 || p == pid)).map(|&(_, _, n)| n)
}

/// The wanted-list decision from plain facts (testable without hardware).
/// `ids` = (vid, pid) of every HID interface of the device.
pub fn classify(kind: Kind, has_driver: bool, has_vendor_collection: bool, ids: &[(u16, u16)]) -> Option<Wanted> {
    if let Some(model) = ids.iter().find_map(|&(v, p)| listed(v, p)) {
        return Some(Wanted::Listed(model));
    }
    let wants_kind = matches!(kind, Kind::Mouse | Kind::Keyboard | Kind::Headset);
    (!has_driver && wants_kind && has_vendor_collection).then_some(Wanted::NoDriver)
}

/// [`classify`] for an enumerated device. `kind` is the final kind shown in the
/// panel, `driver` the matched driver name (if any).
pub fn for_physical(p: &Physical, kind: Kind, driver: Option<&str>) -> Option<Wanted> {
    let ids: Vec<(u16, u16)> = p.interfaces.iter().map(|i| (i.vid, i.pid)).collect();
    classify(kind, driver.is_some(), p.interfaces.iter().any(|i| i.is_vendor()), &ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listed_models_match_by_id() {
        assert_eq!(listed(0x36A7, 0xA887), Some("WLMouse Beast X 4K"));
        assert_eq!(listed(0x36A7, 0xA883), None); // Beast X (verified family B)
        assert_eq!(listed(0x3770, 0x1234), Some("Hitscan")); // pid 0 = any
        assert_eq!(listed(0x046D, 0xC547), None);
    }

    #[test]
    fn unmatched_vendor_devices_are_wanted() {
        let ids = [(0x1234, 0x5678)];
        assert_eq!(classify(Kind::Mouse, false, true, &ids), Some(Wanted::NoDriver));
        assert_eq!(classify(Kind::Keyboard, false, true, &ids), Some(Wanted::NoDriver));
        assert_eq!(classify(Kind::Headset, false, true, &ids), Some(Wanted::NoDriver));
        // A driver matched, no vendor collection, or an unrelated kind: not wanted.
        assert_eq!(classify(Kind::Mouse, true, true, &ids), None);
        assert_eq!(classify(Kind::Mouse, false, false, &ids), None);
        assert_eq!(classify(Kind::Other, false, true, &ids), None);
    }

    #[test]
    fn listed_wins_even_with_a_driver() {
        let ids = [(0x1532, 0x00E8)];
        assert_eq!(classify(Kind::Mouse, true, true, &ids), Some(Wanted::Listed("Razer Naga V3 Pro")));
        assert!(Wanted::Listed("X").explain().contains("X"));
    }

    #[test]
    fn table_has_no_duplicates() {
        for (i, a) in LISTED.iter().enumerate() {
            assert!(LISTED[i + 1..].iter().all(|b| (a.0, a.1) != (b.0, b.1)), "duplicate {:04X}:{:04X}", a.0, a.1);
        }
    }
}
