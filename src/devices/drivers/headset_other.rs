//! Fifine HT6, MCHOSE X9 / V9 Pro and Lenovo Wireless VoIP dongles
//! (docs/protocols/headset-other-2g4.md). Read-only status queries.
//!
//! Not covered (no protocol known): ASUS ROG, JBL Quantum, Cooler Master,
//! Attack Shark; Poly BT600 is out of scope. Other C-Media style dongles that
//! may answer `55 65 01 00` are deliberately not probed.

use super::hs::{Bat, Port, reading};
use super::util::drain;
use crate::devices::{Connection, Driver, HidHandle, HidInfo, Reading};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Model {
    Fifine,
    Mchose,
    Lenovo,
}

/// (vid, pid, model): matched on both.
const MODELS: &[(u16, u16, Model)] = &[
    (0x3142, 0x00C3, Model::Fifine),
    (0x3837, 0x6045, Model::Mchose),
    (0x291D, 0x385D, Model::Mchose),
    (0x17EF, 0xA07D, Model::Lenovo),
];

/// Allow-list: state query of the 0x55 family (report 0x55, command 0x65) and
/// Lenovo's status command (feature report 0x24, command 0x01).
const REQ_STATE: [u8; 4] = [0x55, 0x65, 0x01, 0x00];
const REQ_LENOVO: [u8; 2] = [0x24, 0x01];

fn model(vid: u16, pid: u16) -> Option<Model> {
    MODELS.iter().find(|m| m.0 == vid && m.1 == pid).map(|m| m.2)
}

fn is_state_frame(r: &[u8]) -> bool {
    r.len() > 3 && r[0] == 0x55 && (r[1] == 0x65 || r[1] == 0x0A || r[3] == 0x1A)
}

/// Fifine reply `55 65 <battery> <status> ...`: status 0x03 = charging;
/// `55 0A` = headset powered off, status 0x1A = link lost.
fn fifine(r: &[u8]) -> Option<Bat> {
    if !is_state_frame(r) {
        return None;
    }
    if r[1] == 0x0A || r[3] == 0x1A {
        return Some(Bat::OFF);
    }
    Bat::new(r[2], Some(r[3] == 0x03))
}

/// MCHOSE reply `55 65 <percent> 02`: the 0x02 marks the battery field.
fn mchose(r: &[u8]) -> Option<Bat> {
    (r.len() > 3 && r[..2] == [0x55, 0x65] && r[3] == 0x02).then(|| Bat::new(r[2], None)).flatten()
}

/// Lenovo status reply (report 0x27): byte 7 = percent.
fn lenovo(r: &[u8]) -> Option<Bat> {
    (r.len() > 7 && r[0] == 0x27).then(|| Bat::new(r[7], None)).flatten()
}

pub struct OtherHeadsets;

impl Driver for OtherHeadsets {
    fn name(&self) -> &'static str {
        "headset-2g4-other"
    }

    fn matches(&self, info: &HidInfo) -> bool {
        match model(info.vid, info.pid) {
            Some(Model::Fifine | Model::Mchose) => info.usage_page == 0xFF90 && info.usage == 0x0001,
            Some(Model::Lenovo) => info.usage_page == 0xFF07 && info.usage == 0x0222 && info.interface.unwrap_or(3) == 3,
            None => false,
        }
    }

    fn read(&self, dev: &HidHandle, info: &HidInfo) -> Option<Reading> {
        let p = Port::new(dev, info);
        let bat = match model(info.vid, info.pid)? {
            Model::Fifine => fifine(&p.ask(&REQ_STATE, 300, 8, is_state_frame)?)?,
            Model::Mchose => mchose(&p.ask(&REQ_STATE, 300, 8, |r| mchose(r).is_some())?)?,
            Model::Lenovo => {
                if dev.feature_len == 0 {
                    return None;
                }
                drain(dev);
                if !dev.set_feature(&REQ_LENOVO) {
                    return None;
                }
                (0..4).find_map(|_| lenovo(&p.recv(300)?))?
            }
        };
        Some(reading(bat, Connection::Dongle))
    }
}

#[cfg(test)]
mod tests {
    use super::super::util::hex;
    use super::*;

    #[test]
    fn fifine_states() {
        assert_eq!(fifine(&hex("55 65 50 00 02 FF")), Some(Bat { pct: Some(80), chg: Some(false) }));
        assert_eq!(fifine(&hex("55 65 50 03 02 FF")), Some(Bat { pct: Some(80), chg: Some(true) }));
        assert_eq!(fifine(&hex("55 0A 00 00")), Some(Bat::OFF));
        assert_eq!(fifine(&hex("55 65 50 1A")), Some(Bat::OFF));
        assert_eq!(fifine(&hex("55 65 65 00")), None);
        assert_eq!(fifine(&hex("55 66 50 00")), None);
    }

    #[test]
    fn mchose_battery_field() {
        // documented sample: `55 65 32 02` = 50 %
        assert_eq!(mchose(&hex("55 65 32 02")), Some(Bat { pct: Some(50), chg: None }));
        assert_eq!(mchose(&hex("55 65 32 01")), None);
        assert_eq!(mchose(&hex("55 65 65 02")), None);
    }

    #[test]
    fn lenovo_status() {
        assert_eq!(lenovo(&hex("27 00 00 00 00 00 00 4B")), Some(Bat { pct: Some(75), chg: None }));
        assert_eq!(lenovo(&hex("26 00 00 00 00 00 00 4B")), None);
        assert_eq!(model(0x3837, 0x6045), Some(Model::Mchose));
        assert_eq!(model(0x3837, 0x0001), None);
    }
}
