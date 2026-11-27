//! Thin helpers over the Configuration Manager (cfgmgr32) API.
//!
//! CM_* is used instead of SetupDi*: no HDEVINFO bookkeeping and it is the
//! cheapest way to list interfaces and read devnode properties.

use windows::Win32::Devices::DeviceAndDriverInstallation::*;
use windows::Win32::Devices::Properties::*;
use windows::Win32::Foundation::DEVPROPKEY;
use windows::core::{GUID, PCWSTR};

// DEVPKEY_Bluetooth_Battery {104EA319-6EE2-4701-BD47-8DDBF425BBE5}, pid 2.
pub const DEVPKEY_BLUETOOTH_BATTERY: DEVPROPKEY = DEVPROPKEY {
    fmtid: GUID::from_u128(0x104ea319_6ee2_4701_bd47_8ddbf425bbe5),
    pid: 2,
};

// DEVPKEY_Device_IsConnected {83DA6326-97A6-4088-9453-A1923F573B29}, pid 15 (BOOLEAN).
pub const DEVPKEY_DEVICE_ISCONNECTED: DEVPROPKEY = DEVPROPKEY {
    fmtid: GUID::from_u128(0x83da6326_97a6_4088_9453_a1923f573b29),
    pid: 15,
};

pub fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn from_wide(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

pub fn fmt_guid(g: &GUID) -> String {
    format!(
        "{{{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
        g.data1, g.data2, g.data3, g.data4[0], g.data4[1], g.data4[2], g.data4[3], g.data4[4],
        g.data4[5], g.data4[6], g.data4[7]
    )
}

/// Paths of all present interfaces of `class`, optionally restricted to one device instance.
pub fn interface_list(class: &GUID, device_id: Option<&str>) -> Vec<String> {
    let wide = device_id.map(to_wide);
    let dev = wide.as_ref().map_or(PCWSTR::null(), |w| PCWSTR(w.as_ptr()));
    unsafe {
        let mut len = 0u32;
        if CM_Get_Device_Interface_List_SizeW(&mut len, class, dev, CM_GET_DEVICE_INTERFACE_LIST_PRESENT)
            != CR_SUCCESS
            || len < 2
        {
            return Vec::new();
        }
        let mut buf = vec![0u16; len as usize];
        if CM_Get_Device_Interface_ListW(class, dev, &mut buf, CM_GET_DEVICE_INTERFACE_LIST_PRESENT)
            != CR_SUCCESS
        {
            return Vec::new();
        }
        split_multi_sz(&buf)
    }
}

fn split_multi_sz(buf: &[u16]) -> Vec<String> {
    buf.split(|&c| c == 0)
        .filter(|s| !s.is_empty())
        .map(String::from_utf16_lossy)
        .collect()
}

/// Present device instance ids below an enumerator (e.g. "BTHENUM", "BTHLE").
pub fn device_ids_of_enumerator(enumerator: &str) -> Vec<String> {
    let f = to_wide(enumerator);
    let flags = CM_GETIDLIST_FILTER_ENUMERATOR | CM_GETIDLIST_FILTER_PRESENT;
    unsafe {
        let mut len = 0u32;
        if CM_Get_Device_ID_List_SizeW(&mut len, PCWSTR(f.as_ptr()), flags) != CR_SUCCESS || len < 2 {
            return Vec::new();
        }
        let mut buf = vec![0u16; len as usize];
        if CM_Get_Device_ID_ListW(PCWSTR(f.as_ptr()), &mut buf, flags) != CR_SUCCESS {
            return Vec::new();
        }
        split_multi_sz(&buf)
    }
}

pub fn locate(instance_id: &str) -> Option<u32> {
    let w = to_wide(instance_id);
    let mut dn = 0u32;
    unsafe {
        (CM_Locate_DevNodeW(&mut dn, PCWSTR(w.as_ptr()), CM_LOCATE_DEVNODE_NORMAL) == CR_SUCCESS)
            .then_some(dn)
    }
}

pub fn instance_id(devnode: u32) -> Option<String> {
    let mut buf = [0u16; 400];
    unsafe {
        (CM_Get_Device_IDW(devnode, &mut buf, 0) == CR_SUCCESS).then(|| from_wide(&buf))
    }
}

pub fn parent(devnode: u32) -> Option<u32> {
    let mut p = 0u32;
    unsafe { (CM_Get_Parent(&mut p, devnode, 0) == CR_SUCCESS).then_some(p) }
}

fn finish_prop(r: CONFIGRET, ty: DEVPROPTYPE, mut buf: Vec<u8>, size: u32) -> Option<(DEVPROPTYPE, Vec<u8>)> {
    (r == CR_SUCCESS).then(|| {
        buf.truncate(size as usize);
        (ty, buf)
    })
}

/// Raw devnode property (type, bytes).
pub fn prop_raw(devnode: u32, key: &DEVPROPKEY) -> Option<(DEVPROPTYPE, Vec<u8>)> {
    unsafe {
        let mut ty = DEVPROPTYPE(0);
        let mut size = 0u32;
        let r = CM_Get_DevNode_PropertyW(devnode, key, &mut ty, None, &mut size, 0);
        if r != CR_BUFFER_SMALL || size == 0 {
            return None;
        }
        let mut buf = vec![0u8; size as usize];
        let r = CM_Get_DevNode_PropertyW(devnode, key, &mut ty, Some(buf.as_mut_ptr()), &mut size, 0);
        finish_prop(r, ty, buf, size)
    }
}

/// Property of a device *interface* (path), e.g. its owning instance id.
pub fn interface_prop_raw(path: &str, key: &DEVPROPKEY) -> Option<(DEVPROPTYPE, Vec<u8>)> {
    let w = to_wide(path);
    unsafe {
        let mut ty = DEVPROPTYPE(0);
        let mut size = 0u32;
        let r = CM_Get_Device_Interface_PropertyW(PCWSTR(w.as_ptr()), key, &mut ty, None, &mut size, 0);
        if r != CR_BUFFER_SMALL || size == 0 {
            return None;
        }
        let mut buf = vec![0u8; size as usize];
        let r = CM_Get_Device_Interface_PropertyW(
            PCWSTR(w.as_ptr()),
            key,
            &mut ty,
            Some(buf.as_mut_ptr()),
            &mut size,
            0,
        );
        finish_prop(r, ty, buf, size)
    }
}

fn bytes_to_string(b: &[u8]) -> String {
    let w: Vec<u16> = b.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect();
    from_wide(&w)
}

pub fn interface_instance_id(path: &str) -> Option<String> {
    let (ty, b) = interface_prop_raw(path, &DEVPKEY_Device_InstanceId)?;
    (ty == DEVPROP_TYPE_STRING).then(|| bytes_to_string(&b))
}

pub fn prop_string(devnode: u32, key: &DEVPROPKEY) -> Option<String> {
    let (ty, b) = prop_raw(devnode, key)?;
    (ty == DEVPROP_TYPE_STRING).then(|| bytes_to_string(&b)).filter(|s| !s.is_empty())
}

pub fn prop_u32(devnode: u32, key: &DEVPROPKEY) -> Option<u32> {
    let (ty, b) = prop_raw(devnode, key)?;
    (ty == DEVPROP_TYPE_UINT32 && b.len() >= 4).then(|| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

pub fn prop_u8(devnode: u32, key: &DEVPROPKEY) -> Option<u8> {
    let (ty, b) = prop_raw(devnode, key)?;
    (ty == DEVPROP_TYPE_BYTE && !b.is_empty()).then(|| b[0])
}

pub fn prop_bool(devnode: u32, key: &DEVPROPKEY) -> Option<bool> {
    let (ty, b) = prop_raw(devnode, key)?;
    (ty == DEVPROP_TYPE_BOOLEAN && !b.is_empty()).then(|| b[0] != 0)
}

pub fn prop_guid(devnode: u32, key: &DEVPROPKEY) -> Option<GUID> {
    let (ty, b) = prop_raw(devnode, key)?;
    if ty != DEVPROP_TYPE_GUID || b.len() < 16 {
        return None;
    }
    Some(GUID {
        data1: u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
        data2: u16::from_le_bytes([b[4], b[5]]),
        data3: u16::from_le_bytes([b[6], b[7]]),
        data4: b[8..16].try_into().ok()?,
    })
}

/// ContainerId as a string; the all-FF "no container" id maps to `None`.
pub fn container_id(devnode: u32) -> Option<String> {
    let g = prop_guid(devnode, &DEVPKEY_Device_ContainerId)?;
    // {00000000-0000-0000-FFFF-FFFFFFFFFFFF} = non-removable / no container.
    if g.data1 == 0 && g.data2 == 0 && g.data3 == 0 && g.data4 == [0, 0, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]
    {
        return None;
    }
    Some(fmt_guid(&g))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bus {
    Usb,
    Bluetooth,
}

/// Bus an instance id belongs to by its enumerator prefix.
pub fn bus_of_instance_id(id: &str) -> Option<Bus> {
    let up = id.to_ascii_uppercase();
    if up.starts_with("USB\\") {
        Some(Bus::Usb)
    } else if up.starts_with("BTHENUM\\") || up.starts_with("BTHLEDEVICE\\") || up.starts_with("BTHLE\\") {
        Some(Bus::Bluetooth)
    } else {
        None
    }
}

#[derive(Clone, Debug)]
pub struct BusInfo {
    pub bus: Bus,
    /// For USB: instance id of the USB *device* (not the `&MI_xx` interface node).
    pub usb_device_id: Option<String>,
}

/// Walks from `devnode` up to the root. Returns the first USB / Bluetooth
/// ancestor, or `None` for internal (ACPI, I2C, PCI), virtual (ROOT, SWD) devices.
pub fn find_bus(devnode: u32) -> Option<BusInfo> {
    let mut cur = devnode;
    let mut bus: Option<Bus> = None;
    for _ in 0..12 {
        let id = instance_id(cur)?;
        let b = bus_of_instance_id(&id);
        if bus.is_none() {
            bus = b;
        }
        if bus == Some(Bus::Bluetooth) {
            return Some(BusInfo { bus: Bus::Bluetooth, usb_device_id: None });
        }
        if bus == Some(Bus::Usb) && !id.to_ascii_uppercase().contains("&MI_") {
            return Some(BusInfo { bus: Bus::Usb, usb_device_id: Some(id) });
        }
        cur = parent(cur)?;
    }
    None
}
