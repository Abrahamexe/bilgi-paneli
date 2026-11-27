//! Polling rate from the USB endpoint descriptor.
//!
//! Finds the parent hub of the device (hub devnode -> GUID_DEVINTERFACE_USB_HUB),
//! opens the hub interface (no elevation needed), asks for the connection speed
//! and the configuration descriptor, then reads `bInterval` of the HID
//! interface's interrupt IN endpoint:
//! * low/full speed: `1000 / bInterval` Hz (interval in ms)
//! * high/super speed: `8000 / 2^(bInterval-1)` Hz (interval in 125 us units)
//!
//! This is the *link* rate the device asks for. It matches the configured
//! rate on most wired mice/keyboards that implement rate switching through
//! bInterval, but not on devices that fix bInterval=1 and change the rate
//! internally, so callers mark it `HzSource::UsbDescriptor`.

use windows::Win32::Devices::Properties::DEVPKEY_Device_Address;
use windows::Win32::Devices::Usb::{
    GUID_DEVINTERFACE_USB_HUB, IOCTL_USB_GET_DESCRIPTOR_FROM_NODE_CONNECTION,
    IOCTL_USB_GET_NODE_CONNECTION_INFORMATION_EX,
};
use windows::Win32::Foundation::{CloseHandle, GENERIC_WRITE, HANDLE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::IO::DeviceIoControl;
use windows::core::PCWSTR;

use super::cm;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Speed {
    Low,
    Full,
    High,
    Super,
}

#[derive(Clone, Debug)]
pub struct EndpointInfo {
    pub speed: Speed,
    pub interface: u8,
    pub endpoint_address: u8,
    pub b_interval: u8,
    pub hz: u32,
}

struct Hub(HANDLE);
impl Drop for Hub {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

fn open_hub(hub_instance: &str) -> Option<Hub> {
    let path = cm::interface_list(&GUID_DEVINTERFACE_USB_HUB, Some(hub_instance)).into_iter().next()?;
    let w = cm::to_wide(&path);
    let h = unsafe {
        CreateFileW(
            PCWSTR(w.as_ptr()),
            GENERIC_WRITE.0,
            FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_FLAGS_AND_ATTRIBUTES(0),
            None,
        )
    }
    .ok()?;
    Some(Hub(h))
}

fn ioctl(h: &Hub, code: u32, buf: &mut [u8]) -> Option<u32> {
    let mut ret = 0u32;
    unsafe {
        DeviceIoControl(
            h.0,
            code,
            Some(buf.as_ptr().cast()),
            buf.len() as u32,
            Some(buf.as_mut_ptr().cast()),
            buf.len() as u32,
            Some(&mut ret),
            None,
        )
        .ok()?;
    }
    Some(ret)
}

/// Connection speed and the 18-byte device descriptor of the device on `port`.
fn connection_info(h: &Hub, port: u32) -> Option<(Speed, [u8; 18])> {
    // USB_NODE_CONNECTION_INFORMATION_EX (packed): index u32, 18-byte device
    // descriptor, config u8, Speed u8 @23, ... PipeList follows (we ask for none).
    let mut b = vec![0u8; 35 + 32 * 16];
    b[..4].copy_from_slice(&port.to_le_bytes());
    ioctl(h, IOCTL_USB_GET_NODE_CONNECTION_INFORMATION_EX, &mut b)?;
    // ConnectionStatus @31 == DeviceConnected (1)
    if u32::from_le_bytes([b[31], b[32], b[33], b[34]]) != 1 {
        return None;
    }
    let speed = match b[23] {
        0 => Speed::Low,
        1 => Speed::Full,
        2 => Speed::High,
        _ => Speed::Super,
    };
    let mut dev = [0u8; 18];
    dev.copy_from_slice(&b[4..22]);
    Some((speed, dev))
}

fn speed_of(h: &Hub, port: u32) -> Option<Speed> {
    connection_info(h, port).map(|(s, _)| s)
}

/// GET_DESCRIPTOR through the hub (read-only control transfer). `request_type`
/// 0x80 = device, 0x81 = interface; `index` goes in wIndex.
fn get_descriptor(h: &Hub, port: u32, request_type: u8, value: u16, index: u16, want: u16) -> Option<Vec<u8>> {
    // USB_DESCRIPTOR_REQUEST: index u32, setup packet (8), data.
    let mut b = vec![0u8; 12 + want as usize];
    b[..4].copy_from_slice(&port.to_le_bytes());
    b[4] = request_type;
    b[5] = 0x06; // GET_DESCRIPTOR
    b[6..8].copy_from_slice(&value.to_le_bytes());
    b[8..10].copy_from_slice(&index.to_le_bytes());
    b[10..12].copy_from_slice(&want.to_le_bytes());
    let n = ioctl(h, IOCTL_USB_GET_DESCRIPTOR_FROM_NODE_CONNECTION, &mut b)? as usize;
    let got = n.saturating_sub(12);
    b.drain(..12);
    b.truncate(got);
    Some(b)
}

fn config_descriptor(h: &Hub, port: u32) -> Option<Vec<u8>> {
    let head = get_descriptor(h, port, 0x80, 0x0200, 0, 9)?; // CONFIGURATION, index 0
    if head.len() < 9 || head[1] != 2 {
        return None;
    }
    let total = u16::from_le_bytes([head[2], head[3]]).clamp(9, 4096);
    let full = get_descriptor(h, port, 0x80, 0x0200, 0, total)?;
    (full.len() >= 9).then_some(full)
}

/// UTF-16LE string descriptor `index` (US English), or `None`.
fn string_descriptor(h: &Hub, port: u32, index: u8) -> Option<String> {
    if index == 0 {
        return None;
    }
    let d = get_descriptor(h, port, 0x80, 0x0300 | index as u16, 0x0409, 255)?;
    if d.len() < 2 || d[1] != 3 {
        return None;
    }
    let len = (d[0] as usize).min(d.len());
    let w: Vec<u16> = d.get(2..len)?.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect();
    Some(String::from_utf16_lossy(&w))
}

/// (interface number, report descriptor length) of every HID descriptor (0x21) in a configuration descriptor.
fn hid_report_lengths(desc: &[u8]) -> Vec<(u8, u16)> {
    let mut out = Vec::new();
    let (mut i, mut cur_if) = (0, 0u8);
    while i + 2 <= desc.len() {
        let len = desc[i] as usize;
        if len < 2 || i + len > desc.len() {
            break;
        }
        match desc[i + 1] {
            4 if len >= 9 => cur_if = desc[i + 2],
            0x21 if len >= 9 => {
                // bNumDescriptors @5, then (type, wLength) pairs from @6.
                let n = desc[i + 5] as usize;
                for k in 0..n {
                    let at = i + 6 + k * 3;
                    if at + 3 <= i + len && desc[at] == 0x22 {
                        out.push((cur_if, u16::from_le_bytes([desc[at + 1], desc[at + 2]])));
                    }
                }
            }
            _ => {}
        }
        i += len;
    }
    out
}

/// Raw USB descriptors of one device, for the test report.
#[derive(Clone, Debug)]
pub struct UsbDump {
    pub speed: Speed,
    /// 18-byte device descriptor.
    pub device: Vec<u8>,
    /// Full configuration descriptor (all interfaces, endpoints, HID descriptors).
    pub config: Vec<u8>,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
    pub serial: Option<String>,
    /// HID report descriptors by interface number (best effort: the hub may refuse).
    pub report_descriptors: Vec<(u8, Vec<u8>)>,
}

/// Reads the descriptors of the USB device `usb_device_id` (read-only requests).
pub fn dump(usb_device_id: &str) -> Option<UsbDump> {
    let dev = cm::locate(usb_device_id)?;
    let port = cm::prop_u32(dev, &DEVPKEY_Device_Address)?;
    let hub_id = cm::instance_id(cm::parent(dev)?)?;
    let hub = open_hub(&hub_id)?;
    let (speed, device) = connection_info(&hub, port)?;
    let config = config_descriptor(&hub, port).unwrap_or_default();
    let mut report_descriptors = Vec::new();
    for (iface, len) in hid_report_lengths(&config) {
        if len == 0 || len > 4096 {
            continue;
        }
        if let Some(d) = get_descriptor(&hub, port, 0x81, 0x2200, iface as u16, len)
            && d.len() == len as usize
        {
            report_descriptors.push((iface, d));
        }
    }
    Some(UsbDump {
        speed,
        manufacturer: string_descriptor(&hub, port, device[14]),
        product: string_descriptor(&hub, port, device[15]),
        serial: string_descriptor(&hub, port, device[16]),
        device: device.to_vec(),
        config,
        report_descriptors,
    })
}

/// Finds the interrupt-IN endpoint of `interface` (or of the first HID-class
/// interface when `interface` is `None`) in a configuration descriptor.
fn find_endpoint(desc: &[u8], interface: Option<u8>) -> Option<(u8, u8, u8)> {
    let mut i = 0;
    let mut cur_if: Option<(u8, u8, u8)> = None; // (number, alt, class)
    while i + 2 <= desc.len() {
        let len = desc[i] as usize;
        if len < 2 || i + len > desc.len() {
            break;
        }
        match desc[i + 1] {
            4 if len >= 9 => cur_if = Some((desc[i + 2], desc[i + 3], desc[i + 5])),
            5 if len >= 7 => {
                if let Some((num, alt, class)) = cur_if {
                    let wanted = match interface {
                        Some(n) => num == n,
                        None => class == 3,
                    };
                    let (addr, attr, interval) = (desc[i + 2], desc[i + 3], desc[i + 6]);
                    if wanted && alt == 0 && addr & 0x80 != 0 && attr & 3 == 3 {
                        return Some((num, addr, interval));
                    }
                }
            }
            _ => {}
        }
        i += len;
    }
    None
}

pub fn hz_from_interval(speed: Speed, b_interval: u8) -> Option<u32> {
    if b_interval == 0 {
        return None;
    }
    match speed {
        Speed::Low | Speed::Full => Some(1000 / b_interval as u32),
        Speed::High | Speed::Super => {
            let exp = (b_interval - 1).min(15);
            Some((8000u32 >> exp).max(1))
        }
    }
}

/// Endpoint details for the HID interface `interface` (`&mi_XX`; `None` for a
/// non-composite device) of the USB device `usb_device_id`.
pub fn endpoint_info(usb_device_id: &str, interface: Option<u8>) -> Option<EndpointInfo> {
    let dev = cm::locate(usb_device_id)?;
    let port = cm::prop_u32(dev, &DEVPKEY_Device_Address)?; // port number on the parent hub
    let hub_id = cm::instance_id(cm::parent(dev)?)?;
    let hub = open_hub(&hub_id)?;
    let speed = speed_of(&hub, port)?;
    let desc = config_descriptor(&hub, port)?;
    let (num, addr, interval) = find_endpoint(&desc, interface)?;
    Some(EndpointInfo {
        speed,
        interface: num,
        endpoint_address: addr,
        b_interval: interval,
        hz: hz_from_interval(speed, interval)?,
    })
}

pub fn polling_hz(usb_device_id: &str, interface: Option<u8>) -> Option<u32> {
    endpoint_info(usb_device_id, interface).map(|e| e.hz)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hz_math() {
        assert_eq!(hz_from_interval(Speed::Full, 1), Some(1000));
        assert_eq!(hz_from_interval(Speed::Full, 2), Some(500));
        assert_eq!(hz_from_interval(Speed::Low, 10), Some(100));
        assert_eq!(hz_from_interval(Speed::High, 1), Some(8000));
        assert_eq!(hz_from_interval(Speed::High, 4), Some(1000));
        assert_eq!(hz_from_interval(Speed::Super, 3), Some(2000));
        assert_eq!(hz_from_interval(Speed::Full, 0), None);
    }

    #[test]
    fn finds_hid_report_lengths() {
        let mut d = vec![9, 2, 0, 0, 2, 1, 0, 0xA0, 50];
        d.extend([9, 4, 0, 0, 1, 3, 1, 1, 0, 9, 0x21, 0x11, 1, 0, 1, 0x22, 0x34, 0x00]);
        d.extend([7, 5, 0x81, 3, 8, 0, 10]);
        d.extend([9, 4, 1, 0, 1, 3, 1, 2, 0, 9, 0x21, 0x11, 1, 0, 1, 0x22, 0x40, 0x01]);
        assert_eq!(hid_report_lengths(&d), vec![(0, 0x34), (1, 0x140)]);
    }

    #[test]
    fn picks_endpoint_of_requested_interface() {
        // config(9) | if0 HID kbd | ep 0x81 int, bInterval 10 | if1 HID mouse | ep 0x82 int, bInterval 1
        let mut d = vec![9, 2, 0, 0, 2, 1, 0, 0xA0, 50];
        d.extend([9, 4, 0, 0, 1, 3, 1, 1, 0, 9, 0x21, 0, 0, 0, 0, 0, 0, 0]); // if0 + HID desc (9)
        d.extend([7, 5, 0x81, 3, 8, 0, 10]);
        d.extend([9, 4, 1, 0, 1, 3, 1, 2, 0]);
        d.extend([7, 5, 0x82, 3, 8, 0, 1]);
        assert_eq!(find_endpoint(&d, Some(1)), Some((1, 0x82, 1)));
        assert_eq!(find_endpoint(&d, Some(0)), Some((0, 0x81, 10)));
        assert_eq!(find_endpoint(&d, None), Some((0, 0x81, 10))); // first HID interface
        assert_eq!(find_endpoint(&d, Some(2)), None);
    }
}
