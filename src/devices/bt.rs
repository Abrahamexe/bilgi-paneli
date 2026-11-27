//! Bluetooth battery from the value Windows caches on the BT device nodes.
//!
//! `DEVPKEY_Bluetooth_Battery` {104EA319-6EE2-4701-BD47-8DDBF425BBE5} pid 2
//! (DEVPROP_TYPE_BYTE, percent) lives on `BTHENUM\DEV_<mac>` for classic
//! devices (HFP headsets publish it) and on `BTHLE\DEV_<mac>` for BLE devices
//! whose battery service Windows already reads (the value can sit on any node
//! of the device, so all of them are read). All nodes of one paired device
//! share the ContainerId (or at least the MAC), so results are keyed by
//! container and apply to the HID interfaces and audio endpoints alike. The
//! battery only counts while the device reports `IsConnected`.
//!
//! TODO: BLE devices Windows does not cache a value for need a GATT Battery
//! Service (0x180F) read. That requires WinRT (Windows.Devices.Bluetooth) and
//! is intentionally not done here (heavy runtime, no GATT sessions).

use std::collections::HashMap;

use windows::Win32::Devices::Properties::{DEVPKEY_Device_FriendlyName, DEVPKEY_Device_DeviceDesc};

use super::cm;

#[derive(Clone, Debug, Default)]
pub struct BtInfo {
    /// Pairing / friendly name from the `DEV_<mac>` node.
    pub name: Option<String>,
    pub battery: Option<u8>,
    /// `DEVPKEY_Device_IsConnected` of the device node. Paired-but-away
    /// devices keep their nodes (and audio endpoints) present, so this is
    /// what tells "connected now" from "merely paired".
    pub connected: Option<bool>,
}

/// 12-hex Bluetooth address of a BTHENUM / BTHLE instance id: `DEV_<mac>` on the
/// device node, `...&<mac>_C00000000` on classic service nodes.
fn mac_of(id: &str) -> Option<String> {
    let up = id.to_ascii_uppercase();
    let hex12 = |s: &str| (s.len() >= 12 && s.is_char_boundary(12) && s[..12].bytes().all(|b| b.is_ascii_hexdigit())).then(|| s[..12].to_string());
    match up.find("DEV_") {
        Some(i) => hex12(&up[i + 4..]),
        None => up.rsplit('\\').next()?.rsplit('&').next().and_then(hex12),
    }
}

/// One pass over all present BTHENUM + BTHLE nodes. Cheap (a few property reads per node).
///
/// The battery property is read on every node of the device (the classic
/// `BTHENUM` node, the HFP / service nodes and the `BTHLE` node: the value can
/// sit on any of them) and counts only while the device is connected: the
/// property can outlive a connection, a paired-but-away device must not show it.
pub fn scan() -> HashMap<String, BtInfo> {
    struct Node {
        container: Option<String>,
        mac: Option<String>,
        battery: Option<u8>,
        is_dev: bool,
        connected: Option<bool>,
        name: Option<String>,
    }
    let mut nodes: Vec<Node> = Vec::new();
    for enumerator in ["BTHENUM", "BTHLE"] {
        for id in cm::device_ids_of_enumerator(enumerator) {
            let Some(dn) = cm::locate(&id) else { continue };
            // Only the device node itself carries the real name / connection state;
            // service nodes ("{0000110B-...}") are named after the profile.
            let is_dev = id.to_ascii_uppercase().contains("\\DEV_");
            nodes.push(Node {
                battery: cm::prop_u8(dn, &cm::DEVPKEY_BLUETOOTH_BATTERY).filter(|b| *b <= 100),
                container: cm::container_id(dn),
                mac: mac_of(&id),
                connected: if is_dev { cm::prop_bool(dn, &cm::DEVPKEY_DEVICE_ISCONNECTED) } else { None },
                name: if is_dev {
                    cm::prop_string(dn, &DEVPKEY_Device_FriendlyName).or_else(|| cm::prop_string(dn, &DEVPKEY_Device_DeviceDesc))
                } else {
                    None
                },
                is_dev,
            });
        }
    }
    // Nodes without a ContainerId join their device through the shared MAC.
    let by_mac: HashMap<String, String> =
        nodes.iter().filter_map(|n| Some((n.mac.clone()?, n.container.clone()?))).collect();
    let mut map: HashMap<String, BtInfo> = HashMap::new();
    for n in &nodes {
        let Some(key) = n.container.clone().or_else(|| by_mac.get(n.mac.as_ref()?).cloned()) else { continue };
        let e = map.entry(key).or_default();
        e.battery = e.battery.or(n.battery);
        if n.is_dev {
            // A dual-mode device has a BTHENUM and a BTHLE node: connected if either is.
            e.connected = if e.connected == Some(true) || n.connected == Some(true) {
                Some(true)
            } else {
                e.connected.or(n.connected)
            };
            e.name = e.name.take().or_else(|| n.name.clone());
        }
    }
    for e in map.values_mut() {
        if e.connected == Some(false) {
            e.battery = None;
        }
    }
    map
}

/// One Bluetooth devnode with every property we read from it (for the test report).
#[derive(Clone, Debug)]
pub struct BtNode {
    pub instance_id: String,
    pub container: Option<String>,
    /// `DEVPKEY_Bluetooth_Battery` as stored on this node (before the "connected" filter).
    pub battery: Option<u8>,
    /// `DEVPKEY_Device_IsConnected` (only meaningful on `DEV_` nodes).
    pub connected: Option<bool>,
    pub name: Option<String>,
}

/// All present BTHENUM / BTHLE nodes, unfiltered.
pub fn nodes() -> Vec<BtNode> {
    let mut out = Vec::new();
    for enumerator in ["BTHENUM", "BTHLE"] {
        for id in cm::device_ids_of_enumerator(enumerator) {
            let Some(dn) = cm::locate(&id) else { continue };
            out.push(BtNode {
                battery: cm::prop_u8(dn, &cm::DEVPKEY_BLUETOOTH_BATTERY),
                container: cm::container_id(dn),
                connected: cm::prop_bool(dn, &cm::DEVPKEY_DEVICE_ISCONNECTED),
                name: cm::prop_string(dn, &DEVPKEY_Device_FriendlyName).or_else(|| cm::prop_string(dn, &DEVPKEY_Device_DeviceDesc)),
                instance_id: id,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::mac_of;

    #[test]
    fn mac_from_device_and_service_nodes() {
        let dev = r"BTHENUM\DEV_A0B1C2D3E4F5\7&1A2B3C4D&0&BLUETOOTHDEVICE_A0B1C2D3E4F5";
        assert_eq!(mac_of(dev).as_deref(), Some("A0B1C2D3E4F5"));
        let le = r"BTHLE\DEV_a0b1c2d3e4f5\8&2B3C4D5E&0&a0b1c2d3e4f5";
        assert_eq!(mac_of(le).as_deref(), Some("A0B1C2D3E4F5"));
        let svc = r"BTHENUM\{0000110B-0000-1000-8000-00805F9B34FB}_VID&0001004C_PID&2019\7&1A2B3C4D&0&A0B1C2D3E4F5_C00000000";
        assert_eq!(mac_of(svc).as_deref(), Some("A0B1C2D3E4F5"));
        assert_eq!(mac_of(r"BTHENUM\DEV_XYZ").as_deref(), None);
    }
}
