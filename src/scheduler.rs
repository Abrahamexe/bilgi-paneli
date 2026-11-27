//! Refresh orchestration. No polling loops: refreshes are triggered by panel
//! open, the tray menu, device-change notifications (debounced by a one-shot
//! timer) and a coalescable 10 minute timer. A refresh runs on ONE short-lived
//! worker thread that posts its result to the owner window and exits.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::SeqCst};

use windows::Win32::Foundation::{HANDLE, HWND, LPARAM, WPARAM};
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::devices::enumerate::KSCATEGORY_RENDER;
use crate::devices::{DeviceState, scan};
use crate::{config, panel, settings};
use windows::Win32::Devices::HumanInterfaceDevice::GUID_DEVINTERFACE_HID;
use windows::core::GUID;

/// Worker -> owner window: lparam = `Box<Vec<DeviceState>>` raw pointer.
pub const WM_APP_DEVICES: u32 = WM_APP + 2;

const TIMER_DEBOUNCE: usize = 1;
const TIMER_PERIODIC: usize = 2;
const DEBOUNCE_MS: u32 = 1500;
const PERIOD_TOLERANCE_MS: u32 = 30_000;
const NO_TIME: u32 = u32::MAX;

static RUNNING: AtomicBool = AtomicBool::new(false);
static DIRTY: AtomicBool = AtomicBool::new(false);
/// Minutes since midnight of the last completed refresh (`NO_TIME` = never).
static UPDATED_AT: AtomicU32 = AtomicU32::new(NO_TIME);
static CACHE: Mutex<Vec<DeviceState>> = Mutex::new(Vec::new());
static CONFIG: Mutex<Option<config::Config>> = Mutex::new(None);
/// Minutes of the armed periodic timer (0 = none).
static PERIOD_MIN: AtomicU32 = AtomicU32::new(0);
static NOTIFY: Mutex<[isize; 2]> = Mutex::new([0, 0]);

/// Last known devices, plus the minute-of-day they were collected (`None` before the first refresh).
pub fn snapshot() -> (Vec<DeviceState>, Option<(u32, u32)>) {
    let mut devs = CACHE.lock().map(|c| c.clone()).unwrap_or_default();
    if let Ok(c) = CONFIG.lock()
        && let Some(cfg) = c.as_ref()
    {
        cfg.apply(&mut devs);
    }
    let t = UPDATED_AT.load(SeqCst);
    (devs, (t != NO_TIME).then_some((t / 60, t % 60)))
}

/// Every known device, ignoring the user's hide/rename/order settings.
pub fn snapshot_all() -> Vec<DeviceState> {
    CACHE.lock().map(|c| c.clone()).unwrap_or_default()
}

pub fn is_refreshing() -> bool {
    RUNNING.load(SeqCst)
}

/// Registers device notifications and the periodic timer, then starts the first refresh.
pub fn init(hwnd: HWND) {
    unsafe {
        let reg = |class: GUID| {
            let filter = DEV_BROADCAST_DEVICEINTERFACE_W {
                dbcc_size: size_of::<DEV_BROADCAST_DEVICEINTERFACE_W>() as u32,
                dbcc_devicetype: DBT_DEVTYP_DEVICEINTERFACE.0,
                dbcc_classguid: class,
                ..Default::default()
            };
            RegisterDeviceNotificationW(
                HANDLE(hwnd.0),
                &filter as *const _ as *const _,
                DEVICE_NOTIFY_WINDOW_HANDLE,
            )
            .map(|h| h.0 as isize)
            .unwrap_or(0)
        };
        if let Ok(mut n) = NOTIFY.lock() {
            *n = [reg(GUID_DEVINTERFACE_HID), reg(KSCATEGORY_RENDER)];
        }
    }
    let cfg = config::load();
    arm_periodic(hwnd, cfg.refresh_minutes);
    store_config(cfg);
    request_refresh(hwnd);
}

fn store_config(cfg: config::Config) {
    if let Ok(mut c) = CONFIG.lock() {
        *c = Some(cfg);
    }
}

/// (Re)arms the periodic timer; 0 minutes = no background refresh.
fn arm_periodic(hwnd: HWND, minutes: u32) {
    if PERIOD_MIN.swap(minutes, SeqCst) == minutes && minutes != 0 {
        return;
    }
    unsafe {
        if minutes == 0 {
            let _ = KillTimer(Some(hwnd), TIMER_PERIODIC);
        } else {
            SetCoalescableTimer(Some(hwnd), TIMER_PERIODIC, minutes * 60_000, None, PERIOD_TOLERANCE_MS);
        }
    }
}

pub fn shutdown(hwnd: HWND) {
    unsafe {
        let _ = KillTimer(Some(hwnd), TIMER_PERIODIC);
        let _ = KillTimer(Some(hwnd), TIMER_DEBOUNCE);
        if let Ok(n) = NOTIFY.lock() {
            for h in n.iter().filter(|&&h| h != 0) {
                let _ = UnregisterDeviceNotification(HDEVNOTIFY(*h as *mut _));
            }
        }
    }
}

/// WM_DEVICECHANGE: (re)arm the one-shot debounce timer.
pub fn on_device_change(hwnd: HWND, wparam: WPARAM) {
    let ev = wparam.0 as u32;
    if matches!(ev, DBT_DEVICEARRIVAL | DBT_DEVICEREMOVECOMPLETE | DBT_DEVNODES_CHANGED) {
        // Same id => SetTimer restarts it; a burst of events yields one refresh.
        unsafe { SetTimer(Some(hwnd), TIMER_DEBOUNCE, DEBOUNCE_MS, None) };
    }
}

/// WM_TIMER. Returns true if the timer was one of ours.
pub fn on_timer(hwnd: HWND, id: usize) -> bool {
    match id {
        TIMER_DEBOUNCE => {
            unsafe {
                let _ = KillTimer(Some(hwnd), TIMER_DEBOUNCE);
            }
            request_refresh(hwnd);
            true
        }
        TIMER_PERIODIC => {
            start_refresh(hwnd, true);
            true
        }
        _ => false,
    }
}

/// Starts a refresh, or marks one more run if one is in flight.
pub fn request_refresh(hwnd: HWND) {
    start_refresh(hwnd, false);
}

fn start_refresh(hwnd: HWND, background: bool) {
    if !background {
        // A user/hotplug request upgrades a pending background run.
        crate::devices::BACKGROUND.store(false, SeqCst);
    }
    if RUNNING.compare_exchange(false, true, SeqCst, SeqCst).is_err() {
        DIRTY.store(true, SeqCst);
        return;
    }
    crate::devices::BACKGROUND.store(background, SeqCst);
    let owner = hwnd.0 as isize;
    let spawned = std::thread::Builder::new().name("refresh".into()).stack_size(512 * 1024).spawn(move || {
        let hwnd = HWND(owner as *mut _);
        // Config is re-read on every refresh (cheap; no file watcher).
        store_config(config::load());
        let prev = CACHE.lock().map(|c| c.clone()).unwrap_or_default();
        let mut devs = scan::scan();
        scan::apply_cache(&mut devs, &prev);
        let raw = Box::into_raw(Box::new(devs));
        let posted = unsafe { PostMessageW(Some(hwnd), WM_APP_DEVICES, WPARAM(0), LPARAM(raw as isize)) };
        if posted.is_err() {
            // Window is gone: reclaim the result.
            drop(unsafe { Box::from_raw(raw) });
            RUNNING.store(false, SeqCst);
        }
    });
    if spawned.is_err() {
        RUNNING.store(false, SeqCst);
    }
}

/// One balloon for wanted (test-list) devices never announced before.
fn announce_wanted(hwnd: HWND, devs: &[DeviceState]) {
    if !devs.iter().any(|d| d.wanted.is_some()) || !settings::test_notify_enabled() {
        return;
    }
    let fresh: Vec<&DeviceState> =
        devs.iter().filter(|d| d.wanted.is_some() && !settings::was_notified(&d.id)).collect();
    let Some(first) = fresh.first() else { return };
    for d in &fresh {
        settings::mark_notified(&d.id);
    }
    let text = match fresh.len() {
        1 => format!("{} — rapor oluşturmak için tıklayın.", first.name),
        n => format!("{} ve {} cihaz daha — rapor oluşturmak için tıklayın.", first.name, n - 1),
    };
    crate::tray::notify_wanted(hwnd, &first.name, &text);
}

/// WM_APP_DEVICES on the UI thread.
pub fn on_result(hwnd: HWND, lparam: LPARAM) {
    let devs = unsafe { Box::from_raw(lparam.0 as *mut Vec<DeviceState>) };
    announce_wanted(hwnd, &devs);
    if let Ok(mut c) = CACHE.lock() {
        *c = *devs;
    }
    let t = unsafe { GetLocalTime() };
    UPDATED_AT.store(t.wHour as u32 * 60 + t.wMinute as u32, SeqCst);
    RUNNING.store(false, SeqCst);
    let minutes = CONFIG.lock().ok().and_then(|c| c.as_ref().map(|c| c.refresh_minutes));
    if let Some(m) = minutes {
        arm_periodic(hwnd, m);
    }
    panel::on_devices_updated();
    if DIRTY.swap(false, SeqCst) {
        request_refresh(hwnd);
    }
}
