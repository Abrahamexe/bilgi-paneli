//! Raw Win32 HID access: interface description and an RAII handle.
//!
//! Handles are opened with FILE_SHARE_READ|WRITE so vendor software can keep
//! running, use overlapped I/O with short timeouts, and close on drop.

use std::ffi::c_void;

use windows::Win32::Devices::HumanInterfaceDevice::*;
use windows::Win32::Foundation::{
    CloseHandle, GENERIC_READ, GENERIC_WRITE, HANDLE, WAIT_OBJECT_0,
};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_OVERLAPPED, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ,
    FILE_SHARE_WRITE, OPEN_EXISTING, ReadFile, WriteFile,
};
use windows::Win32::System::IO::{CancelIoEx, DeviceIoControl, GetOverlappedResult, OVERLAPPED};
use windows::Win32::System::Threading::{CreateEventW, ResetEvent, WaitForSingleObject};
use windows::core::{HRESULT, PCWSTR};

use super::cm::{self, Bus};
use super::trace::{self, Dir};

/// IOCTL_HID_GET_FEATURE (HID_OUT_CTL_CODE(100)).
const IOCTL_HID_GET_FEATURE: u32 = 0x000B_0192;
/// IOCTL_HID_GET_INPUT_REPORT (HID_OUT_CTL_CODE(104)).
const IOCTL_HID_GET_INPUT_REPORT: u32 = 0x000B_01A2;
const ERROR_IO_PENDING_HR: HRESULT = HRESULT(0x8007_03E5_u32 as i32);

/// Default I/O timeout for report transfers.
pub const IO_TIMEOUT_MS: u32 = 150;

/// Everything known about one HID top-level collection (one device interface).
#[derive(Clone, Debug)]
pub struct HidInfo {
    pub path: String,
    pub vid: u16,
    pub pid: u16,
    pub version: u16,
    pub usage_page: u16,
    pub usage: u16,
    pub input_len: u16,
    pub output_len: u16,
    pub feature_len: u16,
    pub product: String,
    pub manufacturer: String,
    pub serial: String,
    /// `&mi_XX` interface number for composite USB devices.
    pub interface: Option<u8>,
    /// `&col_XX` collection number.
    pub collection: Option<u8>,
    pub bus: Bus,
    pub container: Option<String>,
    /// Instance id of the HID devnode.
    pub instance_id: String,
    /// USB device instance id (USB bus only).
    pub usb_device_id: Option<String>,
}

impl HidInfo {
    pub fn is_mouse(&self) -> bool {
        self.usage_page == 0x01 && self.usage == 0x02
    }
    pub fn is_keyboard(&self) -> bool {
        self.usage_page == 0x01 && self.usage == 0x06
    }
    /// Vendor-defined usage page (0xFF00..=0xFFFF).
    pub fn is_vendor(&self) -> bool {
        self.usage_page >= 0xFF00
    }
}

/// Parses `&mi_XX` / `&col_XX` style hex suffixes out of a device path.
pub fn path_field(path: &str, tag: &str) -> Option<u8> {
    let lower = path.to_ascii_lowercase();
    let i = lower.find(tag)? + tag.len();
    u8::from_str_radix(lower.get(i..i + 2)?, 16).ok()
}

struct Owned(HANDLE);
impl Drop for Owned {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

fn open_path(path: &str, access: u32, flags: FILE_FLAGS_AND_ATTRIBUTES) -> Option<Owned> {
    let w = cm::to_wide(path);
    unsafe {
        CreateFileW(
            PCWSTR(w.as_ptr()),
            access,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            flags,
            None,
        )
    }
    .ok()
    .map(Owned)
}

fn hid_string(h: HANDLE, f: unsafe fn(HANDLE, *mut c_void, u32) -> bool) -> String {
    let mut buf = [0u16; 256];
    unsafe {
        if f(h, buf.as_mut_ptr().cast(), (buf.len() * 2) as u32) {
            cm::from_wide(&buf).trim().to_string()
        } else {
            String::new()
        }
    }
}

/// Describes one HID interface. Opens with access 0 (works for the system-owned
/// mouse/keyboard collections too). `None` for non-USB/BT or unreadable devices.
pub fn describe(path: &str) -> Option<HidInfo> {
    let instance_id = cm::interface_instance_id(path)?;
    let devnode = cm::locate(&instance_id)?;
    let bus = cm::find_bus(devnode)?; // internal / virtual devices are skipped here
    let container = cm::container_id(devnode);

    let h = open_path(path, 0, FILE_FLAGS_AND_ATTRIBUTES(0))?;
    let mut attr = HIDD_ATTRIBUTES { Size: size_of::<HIDD_ATTRIBUTES>() as u32, ..Default::default() };
    let mut caps = HIDP_CAPS::default();
    unsafe {
        if !HidD_GetAttributes(h.0, &mut attr) {
            return None;
        }
        let mut pp = PHIDP_PREPARSED_DATA::default();
        if HidD_GetPreparsedData(h.0, &mut pp) {
            let st = HidP_GetCaps(pp, &mut caps);
            let _ = HidD_FreePreparsedData(pp);
            if st != HIDP_STATUS_SUCCESS {
                caps = HIDP_CAPS::default();
            }
        }
    }
    Some(HidInfo {
        path: path.to_string(),
        vid: attr.VendorID,
        pid: attr.ProductID,
        version: attr.VersionNumber,
        usage_page: caps.UsagePage,
        usage: caps.Usage,
        input_len: caps.InputReportByteLength,
        output_len: caps.OutputReportByteLength,
        feature_len: caps.FeatureReportByteLength,
        product: hid_string(h.0, HidD_GetProductString),
        manufacturer: hid_string(h.0, HidD_GetManufacturerString),
        serial: hid_string(h.0, HidD_GetSerialNumberString),
        interface: path_field(path, "&mi_"),
        collection: path_field(path, "&col"),
        bus: bus.bus,
        container,
        instance_id,
        usb_device_id: bus.usb_device_id,
    })
}

/// Open HID interface. Closed on drop.
pub struct HidHandle {
    h: Owned,
    ev: Owned,
    /// False when only a 0-access handle could be opened (system mouse/keyboard
    /// collections): feature reports work, input/output reports do not.
    pub rw: bool,
    pub input_len: usize,
    pub output_len: usize,
    pub feature_len: usize,
    /// Only used to label trace entries.
    usage_page: u16,
    usage: u16,
    mi: Option<u8>,
}

impl HidHandle {
    pub fn open(info: &HidInfo) -> Option<HidHandle> {
        let ov = FILE_FLAG_OVERLAPPED;
        let (h, rw) = match open_path(&info.path, GENERIC_READ.0 | GENERIC_WRITE.0, ov) {
            Some(h) => (h, true),
            None => (open_path(&info.path, 0, ov)?, false),
        };
        let ev = unsafe { CreateEventW(None, true, false, PCWSTR::null()) }.ok().map(Owned)?;
        Some(HidHandle {
            h,
            ev,
            rw,
            input_len: info.input_len as usize,
            output_len: info.output_len as usize,
            feature_len: info.feature_len as usize,
            usage_page: info.usage_page,
            usage: info.usage,
            mi: info.interface,
        })
    }

    fn trace(&self, dir: Dir, kind: &'static str, ok: bool, bytes: &[u8]) {
        let mi = self.mi.map_or("--".to_string(), |m| format!("{m:02X}"));
        trace::record(dir, kind, format!("mi={mi} page {:04X}/{:04X}", self.usage_page, self.usage), ok, bytes);
    }

    /// Runs one overlapped operation with a timeout. Returns bytes transferred.
    /// `start` must issue the I/O with the given OVERLAPPED; buffers it uses must
    /// outlive this call (they do: callers own them across it).
    fn overlapped(
        &self,
        timeout_ms: u32,
        start: impl FnOnce(*mut OVERLAPPED) -> windows::core::Result<()>,
    ) -> Option<u32> {
        unsafe {
            let _ = ResetEvent(self.ev.0);
            let mut ov = OVERLAPPED { hEvent: self.ev.0, ..Default::default() };
            match start(&mut ov) {
                Ok(()) => {}
                Err(e) if e.code() == ERROR_IO_PENDING_HR => {}
                Err(_) => return None,
            }
            if WaitForSingleObject(self.ev.0, timeout_ms) != WAIT_OBJECT_0 {
                let _ = CancelIoEx(self.h.0, Some(&ov));
            }
            let mut n = 0u32;
            // After a cancel this waits for the (aborted) operation to retire.
            GetOverlappedResult(self.h.0, &ov, &mut n, true).ok()?;
            Some(n)
        }
    }

    /// GET_REPORT(feature). `len` is raised to the descriptor's feature length.
    /// Result starts with the report id byte.
    pub fn get_feature(&self, report_id: u8, len: usize) -> Option<Vec<u8>> {
        self.get_report(IOCTL_HID_GET_FEATURE, report_id, len.max(self.feature_len))
    }

    /// GET_REPORT(input): polls the current value of an input report (no wait
    /// for an interrupt transfer). Result starts with the report id byte.
    pub fn get_input_report(&self, report_id: u8, len: usize) -> Option<Vec<u8>> {
        self.get_report(IOCTL_HID_GET_INPUT_REPORT, report_id, len.max(self.input_len))
    }

    fn get_report(&self, ioctl: u32, report_id: u8, len: usize) -> Option<Vec<u8>> {
        let len = len.max(2);
        let mut buf = vec![0u8; len];
        buf[0] = report_id;
        let p = buf.as_mut_ptr();
        let n = self.overlapped(IO_TIMEOUT_MS * 4, |ov| unsafe {
            DeviceIoControl(self.h.0, ioctl, Some(p as *const c_void), len as u32,
                Some(p as *mut c_void), len as u32, None, Some(ov))
        });
        let kind = if ioctl == IOCTL_HID_GET_FEATURE { "GET_FEATURE" } else { "GET_INPUT" };
        let Some(n) = n else {
            if trace::enabled() {
                self.trace(Dir::In, kind, false, &[report_id]);
            }
            return None;
        };
        // The driver does not count the report id byte in the returned size.
        buf.truncate((n as usize + 1).min(len));
        if trace::enabled() {
            self.trace(Dir::In, kind, true, &buf);
        }
        Some(buf)
    }

    /// SET_REPORT(feature). `data[0]` is the report id. Padded to the feature length.
    pub fn set_feature(&self, data: &[u8]) -> bool {
        let mut buf = data.to_vec();
        if buf.len() < self.feature_len {
            buf.resize(self.feature_len, 0);
        }
        let ok = unsafe { HidD_SetFeature(self.h.0, buf.as_ptr().cast(), buf.len() as u32) };
        if trace::enabled() {
            self.trace(Dir::Out, "SET_FEATURE", ok, &buf);
        }
        ok
    }

    /// SET_REPORT(output) over the control pipe (`HidD_SetOutputReport`), for
    /// devices without an interrupt OUT endpoint. `data[0]` is the report id;
    /// padded to the descriptor's output length.
    pub fn set_output(&self, data: &[u8]) -> bool {
        if !self.rw {
            return false;
        }
        let mut buf = data.to_vec();
        if buf.len() < self.output_len {
            buf.resize(self.output_len, 0);
        }
        let ok = unsafe { HidD_SetOutputReport(self.h.0, buf.as_ptr().cast(), buf.len() as u32) };
        if trace::enabled() {
            self.trace(Dir::Out, "SET_OUTPUT", ok, &buf);
        }
        ok
    }

    /// Output report via the interrupt/OUT pipe. `data[0]` is the report id.
    /// Padded to the descriptor's output length.
    pub fn write(&self, data: &[u8]) -> bool {
        if !self.rw {
            return false;
        }
        let mut buf = data.to_vec();
        if buf.len() < self.output_len {
            buf.resize(self.output_len, 0);
        }
        let b = &buf[..];
        let ok = self
            .overlapped(IO_TIMEOUT_MS, |ov| unsafe { WriteFile(self.h.0, Some(b), None, Some(ov)) })
            .is_some();
        if trace::enabled() {
            self.trace(Dir::Out, "OUTPUT", ok, &buf);
        }
        ok
    }

    /// Waits up to `timeout_ms` for one input report (first byte = report id).
    pub fn read_timeout(&self, timeout_ms: u32) -> Option<Vec<u8>> {
        if !self.rw || self.input_len == 0 {
            return None;
        }
        let mut buf = vec![0u8; self.input_len];
        let p = buf.as_mut_ptr();
        let len = buf.len();
        let n = self.overlapped(timeout_ms, |ov| unsafe {
            ReadFile(self.h.0, Some(std::slice::from_raw_parts_mut(p, len)), None, Some(ov))
        })?;
        buf.truncate(n as usize);
        if n > 0 && trace::enabled() {
            self.trace(Dir::In, "INPUT", true, &buf);
        }
        (n > 0).then_some(buf)
    }
}

/// Report ids used by the interface's descriptor: `[input, output, feature]`
/// (0 = unnumbered). For `probe list`, to know which ids can be requested.
pub fn report_ids(path: &str) -> [Vec<u8>; 3] {
    let mut out: [Vec<u8>; 3] = Default::default();
    let Some(h) = open_path(path, 0, FILE_FLAGS_AND_ATTRIBUTES(0)) else { return out };
    unsafe {
        let mut pp = PHIDP_PREPARSED_DATA::default();
        if !HidD_GetPreparsedData(h.0, &mut pp) {
            return out;
        }
        let mut caps = HIDP_CAPS::default();
        if HidP_GetCaps(pp, &mut caps) == HIDP_STATUS_SUCCESS {
            let kinds = [
                (HidP_Input, caps.NumberInputButtonCaps, caps.NumberInputValueCaps),
                (HidP_Output, caps.NumberOutputButtonCaps, caps.NumberOutputValueCaps),
                (HidP_Feature, caps.NumberFeatureButtonCaps, caps.NumberFeatureValueCaps),
            ];
            for (slot, (ty, nb, nv)) in kinds.into_iter().enumerate() {
                let mut ids: Vec<u8> = Vec::new();
                if nb > 0 {
                    let mut v = vec![HIDP_BUTTON_CAPS::default(); nb as usize];
                    let mut n = nb;
                    if HidP_GetButtonCaps(ty, v.as_mut_ptr(), &mut n, pp) == HIDP_STATUS_SUCCESS {
                        ids.extend(v[..n as usize].iter().map(|c| c.ReportID));
                    }
                }
                if nv > 0 {
                    let mut v = vec![HIDP_VALUE_CAPS::default(); nv as usize];
                    let mut n = nv;
                    if HidP_GetValueCaps(ty, v.as_mut_ptr(), &mut n, pp) == HIDP_STATUS_SUCCESS {
                        ids.extend(v[..n as usize].iter().map(|c| c.ReportID));
                    }
                }
                ids.sort_unstable();
                ids.dedup();
                out[slot] = ids;
            }
        }
        let _ = HidD_FreePreparsedData(pp);
    }
    out
}

/// One button or value capability of a report (enough to rebuild the descriptor).
#[derive(Clone, Debug, Default)]
pub struct CapRow {
    /// "Input", "Output" or "Feature".
    pub kind: &'static str,
    pub is_button: bool,
    pub report_id: u8,
    pub usage_page: u16,
    pub usage_min: u16,
    pub usage_max: u16,
    pub is_range: bool,
    pub bit_size: u16,
    pub count: u16,
    pub logical_min: i32,
    pub logical_max: i32,
    pub physical_min: i32,
    pub physical_max: i32,
    pub link: u16,
    pub is_absolute: bool,
    pub has_null: bool,
    pub is_alias: bool,
    pub bitfield: u16,
    pub units: u32,
    pub units_exp: u32,
}

#[derive(Clone, Debug, Default)]
pub struct LinkNode {
    pub usage_page: u16,
    pub usage: u16,
    pub parent: u16,
    pub children: u16,
    /// HID collection type (1 application, 2 logical, ...).
    pub collection_type: u8,
    pub is_alias: bool,
}

/// Everything `HidP_*` reveals about one top-level collection.
#[derive(Clone, Debug, Default)]
pub struct CapsDump {
    pub usage_page: u16,
    pub usage: u16,
    pub input_len: u16,
    pub output_len: u16,
    pub feature_len: u16,
    pub rows: Vec<CapRow>,
    pub links: Vec<LinkNode>,
}

/// Full HidP capabilities of the interface at `path` (opened with access 0).
pub fn caps_dump(path: &str) -> Option<CapsDump> {
    let h = open_path(path, 0, FILE_FLAGS_AND_ATTRIBUTES(0))?;
    let mut out = CapsDump::default();
    unsafe {
        let mut pp = PHIDP_PREPARSED_DATA::default();
        if !HidD_GetPreparsedData(h.0, &mut pp) {
            return None;
        }
        let mut caps = HIDP_CAPS::default();
        if HidP_GetCaps(pp, &mut caps) != HIDP_STATUS_SUCCESS {
            let _ = HidD_FreePreparsedData(pp);
            return None;
        }
        out.usage_page = caps.UsagePage;
        out.usage = caps.Usage;
        out.input_len = caps.InputReportByteLength;
        out.output_len = caps.OutputReportByteLength;
        out.feature_len = caps.FeatureReportByteLength;

        let kinds = [
            ("Input", HidP_Input, caps.NumberInputButtonCaps, caps.NumberInputValueCaps),
            ("Output", HidP_Output, caps.NumberOutputButtonCaps, caps.NumberOutputValueCaps),
            ("Feature", HidP_Feature, caps.NumberFeatureButtonCaps, caps.NumberFeatureValueCaps),
        ];
        for (name, ty, nb, nv) in kinds {
            if nb > 0 {
                let mut v = vec![HIDP_BUTTON_CAPS::default(); nb as usize];
                let mut n = nb;
                if HidP_GetButtonCaps(ty, v.as_mut_ptr(), &mut n, pp) == HIDP_STATUS_SUCCESS {
                    for c in &v[..n as usize] {
                        let (lo, hi) = if c.IsRange {
                            (c.Anonymous.Range.UsageMin, c.Anonymous.Range.UsageMax)
                        } else {
                            (c.Anonymous.NotRange.Usage, c.Anonymous.NotRange.Usage)
                        };
                        out.rows.push(CapRow {
                            kind: name,
                            is_button: true,
                            report_id: c.ReportID,
                            usage_page: c.UsagePage,
                            usage_min: lo,
                            usage_max: hi,
                            is_range: c.IsRange,
                            bit_size: 1,
                            count: hi.saturating_sub(lo) + 1,
                            logical_min: 0,
                            logical_max: 1,
                            link: c.LinkCollection,
                            is_absolute: c.IsAbsolute,
                            is_alias: c.IsAlias,
                            bitfield: c.BitField,
                            ..Default::default()
                        });
                    }
                }
            }
            if nv > 0 {
                let mut v = vec![HIDP_VALUE_CAPS::default(); nv as usize];
                let mut n = nv;
                if HidP_GetValueCaps(ty, v.as_mut_ptr(), &mut n, pp) == HIDP_STATUS_SUCCESS {
                    for c in &v[..n as usize] {
                        let (lo, hi) = if c.IsRange {
                            (c.Anonymous.Range.UsageMin, c.Anonymous.Range.UsageMax)
                        } else {
                            (c.Anonymous.NotRange.Usage, c.Anonymous.NotRange.Usage)
                        };
                        out.rows.push(CapRow {
                            kind: name,
                            is_button: false,
                            report_id: c.ReportID,
                            usage_page: c.UsagePage,
                            usage_min: lo,
                            usage_max: hi,
                            is_range: c.IsRange,
                            bit_size: c.BitSize,
                            count: c.ReportCount,
                            logical_min: c.LogicalMin,
                            logical_max: c.LogicalMax,
                            physical_min: c.PhysicalMin,
                            physical_max: c.PhysicalMax,
                            link: c.LinkCollection,
                            is_absolute: c.IsAbsolute,
                            has_null: c.HasNull,
                            is_alias: c.IsAlias,
                            bitfield: c.BitField,
                            units: c.Units,
                            units_exp: c.UnitsExp,
                        });
                    }
                }
            }
        }
        let nl = caps.NumberLinkCollectionNodes as u32;
        if nl > 0 {
            let mut v = vec![HIDP_LINK_COLLECTION_NODE::default(); nl as usize];
            let mut n = nl;
            if HidP_GetLinkCollectionNodes(v.as_mut_ptr(), &mut n, pp) == HIDP_STATUS_SUCCESS {
                out.links = v[..n as usize]
                    .iter()
                    .map(|l| LinkNode {
                        usage_page: l.LinkUsagePage,
                        usage: l.LinkUsage,
                        parent: l.Parent,
                        children: l.NumberOfChildren,
                        collection_type: (l._bitfield & 0xFF) as u8,
                        is_alias: l._bitfield & 0x100 != 0,
                    })
                    .collect();
            }
        }
        let _ = HidD_FreePreparsedData(pp);
    }
    // Stable order: kind, report id, usage page, first usage.
    let rank = |k: &str| match k {
        "Input" => 0,
        "Output" => 1,
        _ => 2,
    };
    out.rows.sort_by_key(|r| (rank(r.kind), r.report_id, r.link, r.usage_page, r.usage_min));
    Some(out)
}
