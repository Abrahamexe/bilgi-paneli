#![cfg_attr(feature = "demo", allow(unused))]
//! Glue: enumeration -> drivers -> generic fallbacks -> dongle/cable merge.

use windows::Win32::System::SystemInformation::GetTickCount64;

use super::cm::Bus;
use super::enumerate::{self, Physical};
use super::{Connection, DeviceState, HidHandle, HzSource, Kind, Reading, drivers, usb_desc};

/// Result of running the drivers over one physical device.
pub struct Driven {
    /// Merged values of the device itself (readings without a `key`).
    pub main: Reading,
    /// Devices a driver found behind it (receiver slots), one per `key`.
    pub subs: Vec<Reading>,
    /// Name of the first driver that matched.
    pub driver: Option<&'static str>,
}

/// Runs matching drivers over all interfaces of a device (first value wins).
pub fn run_drivers(p: &Physical) -> Driven {
    let mut out = Driven { main: Reading::default(), subs: Vec::new(), driver: None };
    for info in &p.interfaces {
        let Some(d) = drivers::find(info) else { continue };
        out.driver.get_or_insert(d.name());
        let Some(dev) = HidHandle::open(info) else { continue };
        for r in d.read_all(&dev, info, &p.interfaces) {
            let Some(k) = r.key.clone() else {
                out.main.fill_from(r);
                continue;
            };
            match out.subs.iter_mut().find(|s| s.key.as_deref() == Some(k.as_str())) {
                Some(s) => s.fill_from(r),
                None => out.subs.push(r),
            }
        }
    }
    out
}

fn build_state(p: &Physical, r: Reading, driver: Option<&'static str>, now: u64) -> DeviceState {
    let sub = r.key.is_some();
    let mut st = DeviceState {
        id: match &r.key {
            Some(k) => format!("{}#{}", p.id, k),
            None => p.id.clone(),
        },
        name: r.name.clone().unwrap_or_else(|| p.name.clone()),
        kind: r.kind.unwrap_or(p.kind),
        connection: r.connection.unwrap_or(p.connection),
        battery: r.battery,
        charging: r.charging,
        polling_hz: r.polling_hz,
        hz_source: HzSource::Driver,
        last_read: now,
        stale: false,
        serial: r.serial.clone().unwrap_or_else(|| if sub { String::new() } else { p.serial.clone() }),
        driver,
        wanted: super::wanted::for_physical(p, r.kind.unwrap_or(p.kind), driver),
    };
    if !sub && st.battery.is_none() {
        st.battery = p.bt.as_ref().and_then(|b| b.battery);
    }
    // A receiver's own USB rate stands in for a paired mouse only (not keyboards).
    let hz_kind = if sub { st.kind == Kind::Mouse } else { matches!(st.kind, Kind::Mouse | Kind::Keyboard) };
    if st.polling_hz.is_none() && hz_kind && p.bus == Bus::Usb {
        let iface = p.primary.map(|i| &p.interfaces[i]);
        if let (Some(usb), Some(iface)) = (&p.usb_device_id, iface)
            && let Some(hz) = usb_desc::polling_hz(usb, iface.interface)
        {
            st.polling_hz = Some(hz);
            st.hz_source = HzSource::UsbDescriptor;
        }
    }
    st
}

/// Builds the states of one physical device: driver values, then generic
/// fallbacks (USB descriptor Hz, Windows BT battery). A receiver whose driver
/// found paired devices yields one state per paired device instead of its own.
pub fn read_physical_all(p: &Physical, now: u64) -> Vec<DeviceState> {
    let d = run_drivers(p);
    if d.subs.is_empty() {
        return vec![build_state(p, d.main, d.driver, now)];
    }
    d.subs.into_iter().map(|r| build_state(p, r, d.driver, now)).collect()
}

/// First state of [`read_physical_all`] (what `probe` shows per interface group).
pub fn read_physical(p: &Physical, now: u64) -> DeviceState {
    read_physical_all(p, now).swap_remove(0)
}

/// Names compared for merging: lowercase, noise words and punctuation removed.
pub fn normalize_name(name: &str) -> String {
    const NOISE: &[&str] = &["wireless", "receiver", "dongle", "2.4g", "usb", "gaming", "device"];
    name.to_lowercase()
        .split(|c: char| !(c.is_alphanumeric() || c == '.'))
        .filter(|w| !w.is_empty() && !NOISE.contains(w))
        .collect::<Vec<_>>()
        .join(" ")
}

fn serial_usable(s: &str) -> bool {
    let s = s.trim();
    s.len() >= 6 && s.chars().any(|c| c != s.chars().next().unwrap_or('0')) && s != "0123456789ABCDEF"
}

fn same_model(a: &DeviceState, b: &DeviceState) -> bool {
    if serial_usable(&a.serial) && a.serial == b.serial {
        return true;
    }
    let (na, nb) = (normalize_name(&a.name), normalize_name(&b.name));
    !na.is_empty() && na == nb
}

/// One row when the same model shows up both wired and via dongle/Bluetooth:
/// connection = Wired, charging = true unless a driver said otherwise.
pub fn merge_cable_and_dongle(mut devs: Vec<DeviceState>) -> Vec<DeviceState> {
    let mut i = 0;
    while i < devs.len() {
        let mut merged = false;
        if matches!(devs[i].kind, Kind::Mouse | Kind::Keyboard) {
            for j in (0..devs.len()).filter(|&j| j != i) {
                let (a, b) = (&devs[i], &devs[j]);
                if a.kind == b.kind
                    && a.connection == Connection::Wired
                    && b.connection != Connection::Wired
                    && same_model(a, b)
                {
                    let wireless = devs.remove(j);
                    let i2 = if j < i { i - 1 } else { i };
                    let w = &mut devs[i2];
                    w.battery = w.battery.or(wireless.battery);
                    w.charging = w.charging.or(Some(true));
                    // Driver-sourced Hz beats the cable's descriptor rate.
                    if wireless.polling_hz.is_some()
                        && (w.polling_hz.is_none()
                            || (wireless.hz_source == HzSource::Driver && w.hz_source != HzSource::Driver))
                    {
                        w.polling_hz = wireless.polling_hz;
                        w.hz_source = wireless.hz_source;
                    }
                    w.driver = w.driver.or(wireless.driver);
                    w.wanted = w.wanted.or(wireless.wanted);
                    if w.serial.is_empty() {
                        w.serial = wireless.serial;
                    }
                    merged = true;
                    break;
                }
            }
        }
        if !merged {
            i += 1;
        } else {
            i = 0; // indices shifted; rescan (lists are tiny)
        }
    }
    devs
}

/// Full scan: what the tray app shows (kind Other filtered out).
pub fn scan() -> Vec<DeviceState> {
    #[cfg(feature = "demo")]
    return demo();
    #[cfg(not(feature = "demo"))]
    real_scan()
}

#[cfg(feature = "demo")]
fn demo() -> Vec<DeviceState> {
    let now = unsafe { GetTickCount64() };
    let mk = |name: &str, kind, connection, battery, charging, hz: Option<u32>, stale_min: u64| DeviceState {
        id: name.to_string(),
        name: name.to_string(),
        kind,
        connection,
        battery,
        charging,
        polling_hz: hz,
        hz_source: HzSource::Driver,
        last_read: now - stale_min * 60_000,
        stale: stale_min > 0,
        serial: String::new(),
        driver: None,
        wanted: None,
    };
    vec![
        mk("WLMouse Beast X", Kind::Mouse, Connection::Dongle, Some(78), None, Some(4000), 0),
        mk("Logitech G Pro X Superlight 2 with a very long name", Kind::Mouse, Connection::Wired, Some(41), Some(true), Some(2000), 0),
        mk("Lamzu Atlantis", Kind::Mouse, Connection::Dongle, Some(55), None, Some(1000), 12),
        mk("Pulsar X2", Kind::Mouse, Connection::Dongle, Some(9), None, None, 0),
        mk("Akko 5075B Plus", Kind::Keyboard, Connection::Dongle, Some(63), None, Some(1000), 0),
        mk("Luminkey Magger68", Kind::Keyboard, Connection::Wired, None, None, Some(8000), 0),
        mk("Arctis Nova 7", Kind::Headset, Connection::Bluetooth, Some(100), None, None, 0),
        mk("IEM (USB DAC)", Kind::Headset, Connection::Wired, None, None, None, 0),
    ]
}

#[cfg(not(feature = "demo"))]
fn real_scan() -> Vec<DeviceState> {
    let now = unsafe { GetTickCount64() };
    let phys = enumerate::enumerate();
    let states: Vec<DeviceState> =
        phys.iter().filter(|p| p.is_connected()).flat_map(|p| read_physical_all(p, now)).collect();
    let mut merged = merge_cable_and_dongle(states);
    merged.retain(|d| d.kind != Kind::Other);
    merged.sort_by_key(|d| (d.kind, d.name.to_lowercase()));
    merged
}

/// Carries last-known live values over when a driver failed this time.
pub fn apply_cache(new: &mut [DeviceState], prev: &[DeviceState]) {
    for d in new.iter_mut() {
        let Some(old) = prev.iter().find(|o| o.id == d.id) else { continue };
        // A driver that reports a charging state without a level (voltage-based
        // headsets) must not be overwritten by the last level.
        if d.driver.is_some() && d.battery.is_none() && d.charging.is_none() && old.battery.is_some() {
            d.battery = old.battery;
            d.charging = old.charging;
            d.last_read = old.last_read;
            d.stale = true;
            if (d.polling_hz.is_none() || d.hz_source != HzSource::Driver)
                && old.hz_source == HzSource::Driver
                && old.polling_hz.is_some()
            {
                d.polling_hz = old.polling_hz;
                d.hz_source = HzSource::Driver;
            }
        }
    }
}

#[cfg(all(test, not(feature = "demo")))]
mod tests {
    use super::*;

    fn dev(id: &str, name: &str, conn: Connection) -> DeviceState {
        DeviceState {
            id: id.into(),
            name: name.into(),
            kind: Kind::Mouse,
            connection: conn,
            battery: None,
            charging: None,
            polling_hz: None,
            hz_source: HzSource::UsbDescriptor,
            last_read: 0,
            stale: false,
            serial: String::new(),
            driver: None,
            wanted: None,
        }
    }

    #[test]
    fn merges_wired_with_dongle_by_name() {
        let mut dongle = dev("a", "Foo Mouse 2.4G Wireless Receiver", Connection::Dongle);
        dongle.battery = Some(70);
        dongle.polling_hz = Some(4000);
        dongle.hz_source = HzSource::Driver;
        let mut wired = dev("b", "Foo Mouse", Connection::Wired);
        wired.polling_hz = Some(1000);
        let out = merge_cable_and_dongle(vec![dongle, wired]);
        assert_eq!(out.len(), 1);
        let m = &out[0];
        assert_eq!(m.connection, Connection::Wired);
        assert_eq!(m.charging, Some(true));
        assert_eq!(m.battery, Some(70));
        assert_eq!((m.polling_hz, m.hz_source), (Some(4000), HzSource::Driver));
    }

    #[test]
    fn does_not_merge_two_wired_or_different_models() {
        let out = merge_cable_and_dongle(vec![dev("a", "Foo", Connection::Wired), dev("b", "Foo", Connection::Wired)]);
        assert_eq!(out.len(), 2);
        let out = merge_cable_and_dongle(vec![dev("a", "Foo", Connection::Wired), dev("b", "Bar", Connection::Dongle)]);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn driver_charging_value_is_kept() {
        let mut w = dev("b", "Foo", Connection::Wired);
        w.charging = Some(false);
        let out = merge_cable_and_dongle(vec![w, dev("a", "Foo Wireless", Connection::Dongle)]);
        assert_eq!(out[0].charging, Some(false));
    }

    #[test]
    fn stale_values_are_kept_when_driver_fails() {
        let mut old = dev("a", "Foo", Connection::Dongle);
        old.battery = Some(55);
        old.last_read = 1234;
        let mut new = dev("a", "Foo", Connection::Dongle);
        new.driver = Some("x");
        let mut v = vec![new];
        apply_cache(&mut v, &[old]);
        assert_eq!((v[0].battery, v[0].stale, v[0].last_read), (Some(55), true, 1234));
    }
}
