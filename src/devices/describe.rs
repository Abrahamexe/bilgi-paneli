//! Human-readable facts about physical devices, shared by `probe list/dump` and
//! the test report (`crate::report`).

use std::fmt::Write as _;

use windows::Win32::System::SystemInformation::GetTickCount64;

use super::cm::Bus;
use super::enumerate::{self, Physical};
use super::{DeviceState, HzSource, drivers, hid, scan, usb_desc};

pub fn opt<T: std::fmt::Display>(v: Option<T>) -> String {
    v.map_or_else(|| "-".to_string(), |x| x.to_string())
}

pub fn bus_name(b: Bus) -> &'static str {
    match b {
        Bus::Usb => "USB",
        Bus::Bluetooth => "Bluetooth",
    }
}

fn hex_opt(v: Option<u8>) -> String {
    v.map_or_else(|| "--".to_string(), |x| format!("{x:02X}"))
}

/// What [`describe_device`] learned beyond the text it wrote.
pub struct Described {
    /// Everything the panel would show for this device (paired devices included).
    pub states: Vec<DeviceState>,
    /// Name of the first driver that matched an interface.
    pub driver: Option<&'static str>,
}

/// One block of facts per physical device. `md` switches to Markdown.
/// Runs the matching drivers once (this is the device read).
pub fn describe_device(out: &mut String, n: usize, p: &Physical, md: bool) -> Described {
    let now = unsafe { GetTickCount64() };
    let mut all = scan::read_physical_all(p, now);
    let states = all.clone();
    let st = all.remove(0);
    let ep = match (&p.usb_device_id, p.primary) {
        (Some(usb), Some(i)) => usb_desc::endpoint_info(usb, p.interfaces[i].interface),
        _ => None,
    };
    let hz = match (st.polling_hz, st.hz_source) {
        (Some(h), HzSource::Driver) => format!("{h} Hz (driver)"),
        (Some(h), HzSource::UsbDescriptor) => match &ep {
            Some(e) => format!(
                "{h} Hz (USB descriptor: {:?} speed, bInterval {}, ep 0x{:02X}, if {})",
                e.speed, e.b_interval, e.endpoint_address, e.interface
            ),
            None => format!("{h} Hz (USB descriptor)"),
        },
        _ => "-".to_string(),
    };
    let driver = p.interfaces.iter().find_map(|i| drivers::find(i).map(|d| d.name()));
    let bt_batt = p.bt.as_ref().and_then(|b| b.battery);

    let title = format!("{} [{} / {}]", st.name, st.kind.label(), st.connection.label());
    if md {
        let _ = writeln!(out, "### {n}. {title}\n");
    } else {
        let _ = writeln!(out, "[{n}] {title}");
    }
    let facts: [(&str, String); 10] = [
        ("bus", bus_name(p.bus).to_string()),
        ("container", opt(p.container.as_deref())),
        ("usb device", opt(p.usb_device_id.as_deref())),
        ("battery", st.battery.map_or("-".into(), |b| format!("{b}%"))),
        ("bluetooth battery (Windows)", opt(bt_batt.map(|b| format!("{b}%")))),
        ("bluetooth connected", opt(p.bt.as_ref().and_then(|b| b.connected))),
        ("charging", opt(st.charging)),
        ("polling rate", hz),
        (
            "usb endpoint (raw)",
            ep.as_ref().map_or("-".into(), |e| {
                format!("{:?} speed, if {}, ep 0x{:02X}, bInterval {} => {} Hz", e.speed, e.interface, e.endpoint_address, e.b_interval, e.hz)
            }),
        ),
        ("driver", driver.unwrap_or("none").to_string()),
    ];
    for (k, v) in facts {
        if md {
            let _ = writeln!(out, "- **{k}**: `{v}`");
        } else {
            let _ = writeln!(out, "    {k:<28} {v}");
        }
    }
    for d in std::iter::once(&st).chain(all.iter()).filter(|d| d.id.contains('#')) {
        let _ = writeln!(
            out,
            "{}paired device: {} [{}] battery {} charging {} hz {}",
            if md { "- " } else { "    " },
            d.name,
            d.kind.label(),
            opt(d.battery.map(|b| format!("{b}%"))),
            opt(d.charging),
            opt(d.polling_hz)
        );
    }
    if let Some(name) = p.bt.as_ref().and_then(|b| b.name.as_ref()) {
        let _ = writeln!(out, "{}bluetooth name: {name}", if md { "- " } else { "    " });
    }
    for a in &p.audio {
        let _ = writeln!(
            out,
            "{}audio render endpoint: {} ({})",
            if md { "- " } else { "    " },
            a.name,
            bus_name(a.bus)
        );
    }
    if md {
        let _ = writeln!(out, "\n| VID:PID | usage page/usage | mi | col | in/out/feat | product | manufacturer | serial |");
        let _ = writeln!(out, "|---|---|---|---|---|---|---|---|");
    }
    for i in &p.interfaces {
        let vp = format!("{:04X}:{:04X}", i.vid, i.pid);
        let usage = format!("{:04X}/{:04X}", i.usage_page, i.usage);
        let lens = format!("{}/{}/{}", i.input_len, i.output_len, i.feature_len);
        let ids = report_ids_text(&i.path);
        if md {
            let _ = writeln!(
                out,
                "| {vp} | {usage} | {} | {} | {lens} (ids {ids}) | {} | {} | {} |",
                hex_opt(i.interface),
                hex_opt(i.collection),
                i.product,
                i.manufacturer,
                i.serial
            );
        } else {
            let _ = writeln!(
                out,
                "    HID {vp}  usage {usage}  mi={} col={}  in/out/feat={lens}  report ids in/out/feat={ids}\n        product={:?} manufacturer={:?} serial={:?}\n        path={}",
                hex_opt(i.interface),
                hex_opt(i.collection),
                i.product,
                i.manufacturer,
                i.serial,
                i.path
            );
        }
    }
    let _ = writeln!(out);
    Described { states, driver }
}

fn report_ids_text(path: &str) -> String {
    let [i, o, f] = hid::report_ids(path);
    let fmt = |v: Vec<u8>| v.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(",");
    format!("{}/{}/{}", fmt(i), fmt(o), fmt(f))
}

/// The whole `probe list` (plain) / `probe dump` (Markdown) text.
pub fn all_devices(md: bool) -> String {
    let phys = enumerate::enumerate();
    let mut out = String::new();
    if md {
        let _ = writeln!(out, "# probe dump\n");
    }
    let _ = writeln!(
        out,
        "{} physical device(s), {} HID interface(s)\n",
        phys.len(),
        phys.iter().map(|p| p.interfaces.len()).sum::<usize>()
    );
    for (i, p) in phys.iter().enumerate() {
        describe_device(&mut out, i + 1, p, md);
    }
    out
}
