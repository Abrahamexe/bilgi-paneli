//! Device model shared by the tray app and `probe`.
//!
//! Layers (all brand-agnostic unless noted):
//! * [`enumerate`]: every HID interface + audio endpoint, grouped per physical device.
//! * [`usb_desc`]: polling rate from the USB endpoint descriptor (wired / dongle fallback).
//! * [`bt`]: Windows-cached Bluetooth battery.
//! * [`drivers`]: pluggable, brand/chipset-specific readers (read-only commands).
//! * [`scan`]: glue that turns the above into `Vec<DeviceState>`.

pub mod bt;
pub mod cm;
pub mod drivers;
pub mod enumerate;
pub mod hid;
pub mod describe;
pub mod scan;
pub mod trace;
pub mod usb_desc;
pub mod wanted;

pub use hid::{HidHandle, HidInfo};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Mouse,
    Keyboard,
    Headset,
    Other,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Mouse => "Mouse",
            Kind::Keyboard => "Keyboard",
            Kind::Headset => "Headset",
            Kind::Other => "Other",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Connection {
    Wired,
    Dongle,
    Bluetooth,
}

impl Connection {
    pub fn label(self) -> &'static str {
        match self {
            Connection::Wired => "Wired",
            Connection::Dongle => "Dongle",
            Connection::Bluetooth => "Bluetooth",
        }
    }
}

/// Where `polling_hz` came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HzSource {
    /// Configured rate read from the device by a driver.
    Driver,
    /// Derived from the USB endpoint `bInterval` (link rate, not necessarily the setting).
    UsbDescriptor,
}

#[derive(Clone, Debug)]
pub struct DeviceState {
    /// Stable key (ContainerId of the physical device).
    pub id: String,
    pub name: String,
    pub kind: Kind,
    pub connection: Connection,
    pub battery: Option<u8>,
    pub charging: Option<bool>,
    pub polling_hz: Option<u32>,
    pub hz_source: HzSource,
    /// `GetTickCount64` of the last successful read of the live values.
    pub last_read: u64,
    /// A driver matched but the read failed now; values are from `last_read`.
    pub stale: bool,
    /// Serial string (used for dongle/cable merging); may be empty.
    pub serial: String,
    /// Name of the driver that produced the values, if any.
    pub driver: Option<&'static str>,
    /// On the test-device list (see `wanted`): a report from this device is welcome.
    pub wanted: Option<wanted::Wanted>,
}

/// What a driver (or generic layer) could learn. `None` = unknown.
#[derive(Clone, Debug, Default)]
pub struct Reading {
    pub name: Option<String>,
    pub kind: Option<Kind>,
    pub connection: Option<Connection>,
    pub battery: Option<u8>,
    pub charging: Option<bool>,
    pub polling_hz: Option<u32>,
    /// Set by drivers that report several devices behind one interface (e.g. a
    /// receiver's paired devices): a stable per-device key (slot). Such readings
    /// each become their own [`DeviceState`] (`id` = physical id + `#` + key).
    pub key: Option<String>,
    /// Serial / unit id of a keyed device (used for cable/dongle merging).
    pub serial: Option<String>,
}

impl Reading {
    /// Fill unset fields from `other` (first reader wins).
    pub fn fill_from(&mut self, other: Reading) {
        self.name = self.name.take().or(other.name);
        self.kind = self.kind.or(other.kind);
        self.connection = self.connection.or(other.connection);
        self.battery = self.battery.or(other.battery);
        self.charging = self.charging.or(other.charging);
        self.polling_hz = self.polling_hz.or(other.polling_hz);
        self.key = self.key.take().or(other.key);
        self.serial = self.serial.take().or(other.serial);
    }
}

/// A pluggable protocol driver. Implementations must only send READ-ONLY
/// commands (battery, charging, report rate queries): never anything that
/// changes a device setting.
///
/// A driver is selected per HID interface by [`Driver::matches`]. It can key on
/// VID/PID lists, on a chipset/firmware family (usage page/usage, report
/// lengths, strings) or both, so one driver can serve many brands that share a
/// protocol under different VIDs.
pub trait Driver: Sync {
    fn name(&self) -> &'static str;
    /// Cheap, side-effect-free check on enumeration data only (no I/O).
    fn matches(&self, info: &HidInfo) -> bool;
    /// Open-handle read. Return `None` when the device does not answer
    /// (asleep, busy) so the cached value can be shown as stale.
    fn read(&self, dev: &HidHandle, info: &HidInfo) -> Option<Reading>;
    /// Like [`Driver::read`] but may return several devices (readings with
    /// `key` set, e.g. everything paired to a receiver). `peers` = all HID
    /// interfaces of the same physical device (to open sibling collections).
    /// Default: the single `read` result.
    fn read_all(&self, dev: &HidHandle, info: &HidInfo, peers: &[HidInfo]) -> Vec<Reading> {
        let _ = peers;
        self.read(dev, info).into_iter().collect()
    }
}

/// True while the current refresh was started by the periodic background timer.
/// Drivers whose read wakes a sleeping device skip their work then.
pub static BACKGROUND: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn is_background() -> bool {
    BACKGROUND.load(std::sync::atomic::Ordering::Relaxed)
}
