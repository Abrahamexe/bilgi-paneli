//! Hidden owner window, tray icon and right-click menu.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, AtomicU8, Ordering::Relaxed};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_INFO, NIM_ADD, NIM_DELETE, NIM_MODIFY, NIM_SETVERSION,
    NOTIFYICON_VERSION_4, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{Result, w};

use crate::{autostart, github, panel, report, scheduler, send, settings};

pub const ICON_ID: u32 = 1;
const MAIN_CLASS: windows::core::PCWSTR = w!("BilgiPaneliMain");
const WM_TRAY: u32 = WM_APP + 1;
/// NIN_SELECT (WM_USER) | NINF_KEY: icon activated from the keyboard.
const NIN_KEYSELECT: u32 = 0x401;
const TIP: &str = "Bilgi Paneli";

const ID_REFRESH: usize = 1;
const ID_AUTOSTART: usize = 2;
const ID_EXIT: usize = 3;
const ID_SETTINGS: usize = 4;
const ID_REPORT_SEND: usize = 5;
const ID_TEST_NOTIFY: usize = 6;
const ID_HOWTO: usize = 7;
const ID_REPORT_SAVE: usize = 8;
const ID_LOGOUT: usize = 9;

/// Report worker -> owner window: the report is finished.
const WM_APP_REPORT: u32 = WM_APP + 3;
/// NIN_BALLOONUSERCLICK (WM_USER + 5): the user clicked a balloon.
const NIN_BALLOONUSERCLICK: u32 = 0x405;
const TIP_REPORT: &str = "Rapor hazırlanıyor…";

/// What a click on the current balloon does.
const CLICK_NONE: u8 = 0;
const CLICK_REPORT: u8 = 1;

static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);
static CLICK_ACTION: AtomicU8 = AtomicU8::new(CLICK_NONE);
/// Device the last "wanted" balloon was about (preselected in the report dialog).
static BALLOON_DEVICE: Mutex<String> = Mutex::new(String::new());

pub fn run() -> Result<()> {
    unsafe {
        let hinst = GetModuleHandleW(None)?.into();
        let wc = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinst,
            lpszClassName: MAIN_CLASS,
            ..Default::default()
        };
        RegisterClassExW(&wc);
        TASKBAR_CREATED.store(RegisterWindowMessageW(w!("TaskbarCreated")), Relaxed);

        // A normal (never shown) top-level window: message-only windows do not
        // receive the TaskbarCreated broadcast.
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            MAIN_CLASS,
            w!("Bilgi Paneli"),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(hinst),
            None,
        )?;
        add_icon(hwnd);
        scheduler::init(hwnd);
        crate::power::trim_working_set(); // drop startup pages before idling

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            if !send::dispatch_dialog_message(&msg) {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
    Ok(())
}

fn icon_data(hwnd: HWND) -> NOTIFYICONDATAW {
    NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: ICON_ID,
        ..Default::default()
    }
}

/// Copies `src` into a fixed UTF-16 buffer, always leaving a terminating zero.
fn put(dst: &mut [u16], src: &str) {
    let max = dst.len() - 1;
    for (d, s) in dst.iter_mut().zip(src.encode_utf16().take(max)) {
        *d = s;
    }
}

/// Tooltip of the tray icon.
fn set_tip(hwnd: HWND, text: &str) {
    let mut nid = icon_data(hwnd);
    nid.uFlags = NIF_TIP;
    put(&mut nid.szTip, text);
    unsafe {
        let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
    }
}

/// Balloon / toast next to the tray icon. `click` = what clicking it does.
fn balloon(hwnd: HWND, title: &str, text: &str, click: u8) {
    CLICK_ACTION.store(click, Relaxed);
    let mut nid = icon_data(hwnd);
    nid.uFlags = NIF_INFO;
    nid.dwInfoFlags = NIIF_INFO;
    put(&mut nid.szInfoTitle, title);
    put(&mut nid.szInfo, text);
    unsafe {
        let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
    }
}

/// A device from the test list showed up: offer a report (click = "Rapor gönder").
pub fn notify_wanted(hwnd: HWND, device: &str, text: &str) {
    if let Ok(mut d) = BALLOON_DEVICE.lock() {
        *d = device.to_string();
    }
    balloon(hwnd, "Test listesindeki cihaz bulundu", text, CLICK_REPORT);
}

/// Plain balloon without a click action (callable from worker threads).
pub fn info_balloon(hwnd: HWND, title: &str, text: &str) {
    balloon(hwnd, title, text, CLICK_NONE);
}

/// Builds the report and saves it (Desktop + clipboard, no browser) on a short-lived worker thread.
fn start_save(hwnd: HWND) {
    if report::BUSY.compare_exchange(false, true, Relaxed, Relaxed).is_err() {
        return;
    }
    set_tip(hwnd, TIP_REPORT);
    let owner = hwnd.0 as isize;
    let spawned = std::thread::Builder::new().name("report".into()).stack_size(1024 * 1024).spawn(move || {
        let p = report::prepare();
        report::save_and_copy(&p.text);
        report::BUSY.store(false, Relaxed);
        let hwnd = HWND(owner as *mut _);
        unsafe {
            let _ = PostMessageW(Some(hwnd), WM_APP_REPORT, WPARAM(0), LPARAM(0));
        }
    });
    if spawned.is_err() {
        report::BUSY.store(false, Relaxed);
        set_tip(hwnd, TIP);
    }
}

fn add_icon(hwnd: HWND) {
    unsafe {
        let mut nid = icon_data(hwnd);
        nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        nid.uCallbackMessage = WM_TRAY;
        nid.hIcon = LoadIconW(None, IDI_APPLICATION).unwrap_or_default();
        put(&mut nid.szTip, if report::BUSY.load(Relaxed) { TIP_REPORT } else { TIP });
        if Shell_NotifyIconW(NIM_ADD, &nid).as_bool() {
            nid.Anonymous.uVersion = NOTIFYICON_VERSION_4;
            let _ = Shell_NotifyIconW(NIM_SETVERSION, &nid);
        }
    }
}

fn remove_icon(hwnd: HWND) {
    unsafe {
        let _ = Shell_NotifyIconW(NIM_DELETE, &icon_data(hwnd));
    }
}

fn show_menu(hwnd: HWND, at: POINT) {
    unsafe {
        let Ok(menu) = CreatePopupMenu() else { return };
        let check = if autostart::is_enabled() { MF_CHECKED } else { MF_UNCHECKED };
        let _ = AppendMenuW(menu, MF_STRING, ID_REFRESH, w!("Yenile"));
        let _ = AppendMenuW(menu, MF_STRING, ID_SETTINGS, w!("Ayarlar"));
        let _ = AppendMenuW(menu, MF_STRING | check, ID_AUTOSTART, w!("Windows ile başlat"));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
        let notify = if settings::test_notify_enabled() { MF_CHECKED } else { MF_UNCHECKED };
        let busy = if report::BUSY.load(Relaxed) { MF_GRAYED } else { MF_ENABLED };
        if let Ok(sub) = CreatePopupMenu() {
            let _ = AppendMenuW(sub, MF_STRING | busy, ID_REPORT_SEND, w!("Rapor gönder"));
            let _ = AppendMenuW(sub, MF_STRING | busy, ID_REPORT_SAVE, w!("Raporu sadece kaydet"));
            let _ = AppendMenuW(sub, MF_STRING, ID_HOWTO, w!("Rapor nasıl gönderilir?"));
            if github::has_credential() {
                let _ = AppendMenuW(sub, MF_SEPARATOR, 0, None);
                let _ = AppendMenuW(sub, MF_STRING, ID_LOGOUT, w!("Çıkış yap (GitHub)"));
            }
            let _ = AppendMenuW(menu, MF_POPUP, sub.0 as usize, w!("Rapor"));
        }
        let _ = AppendMenuW(menu, MF_STRING | notify, ID_TEST_NOTIFY, w!("Test bildirimleri"));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
        let _ = AppendMenuW(menu, MF_STRING, ID_EXIT, w!("Çıkış"));

        // Required so the menu closes when the user clicks elsewhere.
        let _ = SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            at.x,
            at.y,
            None,
            hwnd,
            None,
        );
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);

        match cmd.0 as usize {
            ID_REFRESH => scheduler::request_refresh(hwnd),
            ID_SETTINGS => {
                let ids: Vec<(String, String)> =
                    scheduler::snapshot_all().into_iter().map(|d| (d.id, d.name)).collect();
                crate::config::open_settings(&ids);
            }
            ID_AUTOSTART => autostart::toggle(),
            ID_REPORT_SEND => send::open(hwnd, None),
            ID_REPORT_SAVE => start_save(hwnd),
            ID_LOGOUT => github::delete_tokens(),
            ID_TEST_NOTIFY => settings::toggle_test_notify(),
            ID_HOWTO => report::open_url(report::HOWTO_URL),
            ID_EXIT => {
                let _ = DestroyWindow(hwnd);
            }
            _ => {}
        }
    }
}

extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg != 0 && msg == TASKBAR_CREATED.load(Relaxed) {
        add_icon(hwnd);
        return LRESULT(0);
    }
    match msg {
        WM_TRAY => {
            // NOTIFYICON_VERSION_4: low word = event, wParam = anchor point.
            let event = (lparam.0 & 0xFFFF) as u32;
            let anchor = POINT {
                x: (wparam.0 & 0xFFFF) as u16 as i16 as i32,
                y: ((wparam.0 >> 16) & 0xFFFF) as u16 as i16 as i32,
            };
            match event {
                WM_LBUTTONUP | NIN_KEYSELECT => panel::toggle(hwnd, anchor),
                WM_CONTEXTMENU => show_menu(hwnd, anchor),
                NIN_BALLOONUSERCLICK if CLICK_ACTION.swap(CLICK_NONE, Relaxed) == CLICK_REPORT => {
                    let device = BALLOON_DEVICE.lock().ok().map(|d| d.clone()).filter(|d| !d.is_empty());
                    send::open(hwnd, device.as_deref());
                }
                _ => {}
            }
            LRESULT(0)
        }
        // Menu commands posted by automation (the popup menu itself uses TPM_RETURCMD).
        WM_COMMAND => {
            if wparam.0 & 0xFFFF == ID_REPORT_SEND {
                send::open(hwnd, None);
            }
            LRESULT(0)
        }
        WM_APP_REPORT => {
            set_tip(hwnd, TIP);
            balloon(hwnd, "Rapor hazır", "Masaüstüne kaydedildi ve panoya kopyalandı.", CLICK_NONE);
            LRESULT(0)
        }
        WM_DEVICECHANGE => {
            scheduler::on_device_change(hwnd, wparam);
            LRESULT(1) // TRUE: grant the request
        }
        WM_TIMER => {
            scheduler::on_timer(hwnd, wparam.0);
            LRESULT(0)
        }
        scheduler::WM_APP_DEVICES => {
            scheduler::on_result(hwnd, lparam);
            LRESULT(0)
        }
        WM_DESTROY => {
            scheduler::shutdown(hwnd);
            panel::close();
            remove_icon(hwnd);
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}
