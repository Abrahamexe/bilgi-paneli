//! Finds every USB / Bluetooth HID interface and audio render endpoint and
//! groups them into physical devices by ContainerId.
//!
//! Internal (ACPI / I2C / PCI) and virtual (ROOT / SWD) devices are skipped
//! because their devnode chain never reaches a USB or Bluetooth bus.

use std::collections::HashMap;

use windows::Win32::Devices::HumanInterfaceDevice::GUID_DEVINTERFACE_HID;
use windows::Win32::Devices::Properties::{
    DEVPKEY_Device_BusReportedDeviceDesc, DEVPKEY_Device_DeviceDesc, DEVPKEY_Device_FriendlyName,
};
use windows::core::GUID;

use super::bt::{self, BtInfo};
use super::cm::{self, Bus};
use super::hid::{self, HidInfo};
use super::{Connection, Kind};

/// KSCATEGORY_RENDER: audio output interfaces (speakers, headphones, DACs).
/// Capture-only devices (webcam mics) are deliberately not included.
pub const KSCATEGORY_RENDER: GUID = GUID::from_u128(0x65e8773e_8f56_11d0_a3b9_00a0c9223196);

/// An audio render endpoint owner (USB audio interface / BT A2DP node).
#[derive(Clone, Debug)]
pub struct AudioDevice {
    pub name: String,
    pub bus: Bus,
    pub container: Option<String>,
    pub usb_device_id: Option<String>,
    pub instance_id: String,
}

/// One physical device: all of its HID interfaces and audio endpoints.
#[derive(Clone, Debug)]
pub struct Physical {
    pub id: String,
    pub bus: Bus,
    pub container: Option<String>,
    pub usb_device_id: Option<String>,
    pub interfaces: Vec<HidInfo>,
    pub audio: Vec<AudioDevice>,
    pub bt: Option<BtInfo>,
    pub kind: Kind,
    pub connection: Connection,
    pub name: String,
    pub serial: String,
    /// Index into `interfaces` of the mouse/keyboard (or first) interface; used for Hz.
    pub primary: Option<usize>,
}

impl Physical {
    /// False for a paired Bluetooth device that is currently away.
    pub fn is_connected(&self) -> bool {
        !(self.bus == Bus::Bluetooth && self.bt.as_ref().and_then(|b| b.connected) == Some(false))
    }
}

pub fn list_hid() -> Vec<HidInfo> {
    let mut v: Vec<HidInfo> = cm::interface_list(&GUID_DEVINTERFACE_HID, None)
        .iter()
        .filter_map(|p| hid::describe(p))
        .collect();
    v.sort_by(|a, b| a.path.cmp(&b.path));
    v
}

pub fn list_audio() -> Vec<AudioDevice> {
    let mut out: Vec<AudioDevice> = Vec::new();
    for path in cm::interface_list(&KSCATEGORY_RENDER, None) {
        let Some(instance_id) = cm::interface_instance_id(&path) else { continue };
        let Some(dn) = cm::locate(&instance_id) else { continue };
        let Some(bus) = cm::find_bus(dn) else { continue };
        if out.iter().any(|a| a.instance_id == instance_id) {
            continue;
        }
        let name = match &bus.usb_device_id {
            Some(usb) => cm::locate(usb)
                .and_then(|u| cm::prop_string(u, &DEVPKEY_Device_BusReportedDeviceDesc))
                .or_else(|| cm::prop_string(dn, &DEVPKEY_Device_FriendlyName)),
            None => cm::prop_string(dn, &DEVPKEY_Device_FriendlyName),
        }
        .or_else(|| cm::prop_string(dn, &DEVPKEY_Device_DeviceDesc))
        .unwrap_or_default();
        out.push(AudioDevice {
            name,
            bus: bus.bus,
            container: cm::container_id(dn),
            usb_device_id: bus.usb_device_id,
            instance_id,
        });
    }
    out
}

pub fn enumerate() -> Vec<Physical> {
    let hids = list_hid();
    let audio = list_audio();
    let bt_map = bt::scan();
    group(hids, audio, bt_map)
}

fn group_key(container: &Option<String>, usb: &Option<String>, instance: &str) -> String {
    container.clone().or_else(|| usb.clone()).unwrap_or_else(|| instance.to_string())
}

fn group(hids: Vec<HidInfo>, audio: Vec<AudioDevice>, bt_map: HashMap<String, BtInfo>) -> Vec<Physical> {
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut out: Vec<Physical> = Vec::new();

    let mut slot = |key: String, bus: Bus, container: &Option<String>, usb: &Option<String>| -> usize {
        *index.entry(key.clone()).or_insert_with(|| {
            out.push(Physical {
                id: key,
                bus,
                container: container.clone(),
                usb_device_id: usb.clone(),
                interfaces: Vec::new(),
                audio: Vec::new(),
                bt: container.as_ref().and_then(|c| bt_map.get(c).cloned()),
                kind: Kind::Other,
                connection: Connection::Wired,
                name: String::new(),
                serial: String::new(),
                primary: None,
            });
            out.len() - 1
        })
    };
    let mut hid_slots = Vec::new();
    for h in &hids {
        hid_slots.push(slot(group_key(&h.container, &h.usb_device_id, &h.instance_id), h.bus, &h.container, &h.usb_device_id));
    }
    let mut audio_slots = Vec::new();
    for a in &audio {
        audio_slots.push(slot(group_key(&a.container, &a.usb_device_id, &a.instance_id), a.bus, &a.container, &a.usb_device_id));
    }
    for (h, i) in hids.into_iter().zip(hid_slots) {
        out[i].interfaces.push(h);
    }
    for (a, i) in audio.into_iter().zip(audio_slots) {
        out[i].audio.push(a);
    }
    for p in &mut out {
        finish(p);
    }
    out
}

const DONGLE_WORDS: &[&str] = &["receiver", "dongle", "2.4g", "wireless", "unifying", "lightspeed"];

fn contains_any(hay: &str, words: &[&str]) -> bool {
    let l = hay.to_lowercase();
    words.iter().any(|w| l.contains(w))
}

/// Derives kind, connection, name, serial and the primary interface.
fn finish(p: &mut Physical) {
    p.interfaces.sort_by_key(|i| (i.interface.unwrap_or(0), i.collection.unwrap_or(0), i.usage_page));

    let name_hint = {
        let n = p.interfaces.iter().map(|i| i.product.as_str()).collect::<Vec<_>>().join(" ");
        n.to_lowercase()
    };
    let has_mouse = p.interfaces.iter().any(|i| i.is_mouse());
    let has_kb = p.interfaces.iter().any(|i| i.is_keyboard());
    // A real keyboard has LED output reports; a mouse's macro keyboard collection does not.
    let kb_with_leds = p.interfaces.iter().any(|i| i.is_keyboard() && i.output_len > 0);
    let has_audio = !p.audio.is_empty();

    p.kind = if has_mouse || has_kb {
        let kb_hint = ["keyboard", "klavye", "keypad"].iter().any(|w| name_hint.contains(w));
        let mouse_hint = ["mouse", "mice", "fare"].iter().any(|w| name_hint.contains(w));
        if kb_hint && !mouse_hint {
            Kind::Keyboard
        } else if mouse_hint && !kb_hint {
            Kind::Mouse
        } else if kb_with_leds {
            Kind::Keyboard
        } else if has_mouse {
            Kind::Mouse
        } else {
            Kind::Keyboard
        }
    } else if has_audio {
        Kind::Headset
    } else {
        Kind::Other
    };

    p.primary = match p.kind {
        Kind::Mouse => p.interfaces.iter().position(|i| i.is_mouse()),
        Kind::Keyboard => p.interfaces.iter().position(|i| i.is_keyboard()),
        _ => None,
    }
    .or(if p.interfaces.is_empty() { None } else { Some(0) });

    p.serial = p.interfaces.iter().map(|i| i.serial.clone()).find(|s| !s.is_empty()).unwrap_or_default();

    // Name: BT pairing name > product string (primary first) > audio device name > ids.
    let product = p
        .primary
        .and_then(|i| Some(p.interfaces[i].product.clone()).filter(|s| !s.is_empty()))
        .or_else(|| p.interfaces.iter().map(|i| i.product.clone()).find(|s| !s.is_empty()));
    p.name = p
        .bt
        .as_ref()
        .and_then(|b| b.name.clone())
        .or(product)
        .or_else(|| p.audio.iter().map(|a| a.name.clone()).find(|s| !s.is_empty()))
        .unwrap_or_else(|| match p.interfaces.first() {
            Some(i) => format!("{:04X}:{:04X}", i.vid, i.pid),
            None => "?".to_string(),
        });

    p.connection = if p.bus == Bus::Bluetooth {
        Connection::Bluetooth
    } else {
        let strings = p
            .interfaces
            .iter()
            .flat_map(|i| [i.product.as_str(), i.manufacturer.as_str()])
            .chain(p.audio.iter().map(|a| a.name.as_str()))
            .collect::<Vec<_>>()
            .join(" ");
        if contains_any(&strings, DONGLE_WORDS) { Connection::Dongle } else { Connection::Wired }
    };
}
