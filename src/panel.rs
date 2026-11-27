//! Popup panel: created on demand, destroyed when it loses focus.
//! Draws the device list with Direct2D + DirectWrite; every graphics object
//! lives only while the panel is open.

use std::cell::RefCell;
use std::mem::transmute;
use std::ffi::c_void;
use std::ptr::null_mut;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicPtr, AtomicU64, Ordering::Relaxed};

use windows::Win32::Foundation::{FreeLibrary, HMODULE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT, D2D_RECT_F, D2D_SIZE_U,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_ANTIALIAS_MODE_ALIASED, D2D1_DRAW_TEXT_OPTIONS_CLIP, D2D1_FACTORY_TYPE_SINGLE_THREADED,
    D2D1_FEATURE_LEVEL_DEFAULT, D2D1_HWND_RENDER_TARGET_PROPERTIES, D2D1_PRESENT_OPTIONS_NONE,
    D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_SOFTWARE, D2D1_RENDER_TARGET_USAGE_NONE,
    D2D1_FACTORY_OPTIONS, D2D1_FACTORY_TYPE, D2D1_ROUNDED_RECT, D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE,
    ID2D1Factory,
    ID2D1HwndRenderTarget, ID2D1SolidColorBrush,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_WEIGHT, DWRITE_MEASURING_MODE_NATURAL, DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
    DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_TEXT_METRICS,
    DWRITE_TRIMMING, DWRITE_TRIMMING_GRANULARITY_CHARACTER, DWRITE_WORD_WRAPPING_NO_WRAP,
    DWRITE_FACTORY_TYPE, IDWriteFactory, IDWriteTextFormat,
};
use windows::Win32::Graphics::Dwm::{
    DWMSBT_MAINWINDOW, DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_USE_IMMERSIVE_DARK_MODE,
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmExtendFrameIntoClientArea,
    DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::{
    BeginPaint, EndPaint, GetMonitorInfoW, InvalidateRect, MONITOR_DEFAULTTONEAREST, MONITORINFO,
    MonitorFromPoint, PAINTSTRUCT,
};
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress, LoadLibraryW};
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows::Win32::System::SystemInformation::GetTickCount64;
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, GetDpiForWindow, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE;
use windows::Win32::UI::Shell::{NOTIFYICONIDENTIFIER, Shell_NotifyIconGetRect};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{GUID, HRESULT, Interface, PCWSTR, s as pcstr, w};

use crate::devices::{Connection, DeviceState, Kind};
use crate::{power, scheduler, tray};

// Layout in device-independent pixels (96 dpi).
const WIDTH: i32 = 340;
const PAD: i32 = 16;
const CARD_INSET: i32 = 8;
const CARD_RADIUS: f32 = 8.0;
const GAP: i32 = 12;
const TOP_PAD: i32 = 10;
const HEADER_H: i32 = 26;
const ROW_H: i32 = 46;
const FOOTER_H: i32 = 30;
const EMPTY_H: i32 = 84;
const COL_GAP: i32 = 10;

const WA_INACTIVE: usize = 0;
/// Clicking the tray icon while the panel is open first deactivates (closes)
/// the panel; ignore the click that follows within this window.
const REOPEN_GUARD_MS: u64 = 300;

static PANEL: AtomicPtr<c_void> = AtomicPtr::new(null_mut());
static CLOSED_AT: AtomicU64 = AtomicU64::new(0);
static MICA: AtomicBool = AtomicBool::new(false);
static DARK: AtomicBool = AtomicBool::new(false);
/// Vertical scroll offset in DIPs (content taller than the work area).
static SCROLL: AtomicI32 = AtomicI32::new(0);
const WHEEL_STEP: i32 = 48;
static PLACE: Mutex<Option<Place>> = Mutex::new(None);

/// Where the panel sits relative to the tray icon (kept for re-layout on data changes).
#[derive(Clone, Copy)]
struct Place {
    icon: RECT,
    monitor: RECT,
    work: RECT,
    dpi: i32,
}

fn panel_hwnd() -> Option<HWND> {
    let p = PANEL.load(Relaxed);
    (!p.is_null()).then_some(HWND(p))
}

pub fn is_open() -> bool {
    panel_hwnd().is_some()
}

pub fn toggle(owner: HWND, anchor: POINT) {
    if panel_hwnd().is_some() {
        close();
    } else if unsafe { GetTickCount64() } - CLOSED_AT.load(Relaxed) > REOPEN_GUARD_MS {
        open(owner, anchor);
    }
}

pub fn close() {
    if let Some(hwnd) = panel_hwnd() {
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
    }
}

/// Scheduler delivered fresh data: re-layout and repaint if the panel is showing.
pub fn on_devices_updated() {
    if let Some(hwnd) = panel_hwnd() {
        relayout(hwnd);
        unsafe {
            let _ = InvalidateRect(Some(hwnd), None, false);
        }
    } else {
        power::trim_working_set();
    }
}

// ---------------------------------------------------------------- content model

enum Item<'a> {
    Header(&'static str),
    Dev(&'a DeviceState),
}

fn build_items(devs: &[DeviceState]) -> Vec<Item<'_>> {
    let mut items = Vec::new();
    for (kind, title) in [
        (Kind::Mouse, "FARELER"),
        (Kind::Keyboard, "KLAVYELER"),
        (Kind::Headset, "KULAKLIKLAR"),
    ] {
        let mut group = devs.iter().filter(|d| d.kind == kind).peekable();
        if group.peek().is_some() {
            items.push(Item::Header(title));
            items.extend(group.map(Item::Dev));
        }
    }
    items
}

/// Full content height in DIPs (96 dpi).
fn content_dip(items: &[Item]) -> i32 {
    if items.is_empty() {
        return EMPTY_H;
    }
    let body: i32 = items
        .iter()
        .map(|i| match i {
            Item::Header(_) => HEADER_H,
            Item::Dev(_) => ROW_H,
        })
        .sum();
    TOP_PAD + body + FOOTER_H
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

fn battery_text(d: &DeviceState) -> String {
    d.battery.map_or_else(|| "—".to_string(), |b| format!("{b}%"))
}

fn hz_text(d: &DeviceState) -> String {
    d.polling_hz.map_or_else(|| "—".to_string(), |h| format!("{h} Hz"))
}

fn sub_text(d: &DeviceState, now: u64) -> String {
    let mut t = String::from(match d.connection {
        Connection::Dongle => "Dongle",
        Connection::Wired => "Kablolu",
        Connection::Bluetooth => "Bluetooth",
    });
    if d.charging == Some(true) {
        t.push_str(" · şarj oluyor");
    }
    if d.stale {
        let mins = now.saturating_sub(d.last_read) / 60_000;
        t.push_str(&format!(" · uykuda ({mins} dk önce)"));
    }
    t
}

// ---------------------------------------------------------------- window creation / placement

fn compute_pos(pl: &Place, w: i32, h: i32, gap: i32) -> (i32, i32) {
    let icon = pl.icon;
    let center = POINT { x: (icon.left + icon.right) / 2, y: (icon.top + icon.bottom) / 2 };
    // Taskbar edge = monitor edge nearest to the icon.
    let m = pl.monitor;
    let d_left = center.x - m.left;
    let d_right = m.right - center.x;
    let d_top = center.y - m.top;
    let d_bottom = m.bottom - center.y;
    let min = d_left.min(d_right).min(d_top).min(d_bottom);
    let (mut x, mut y);
    if min == d_bottom {
        x = center.x - w / 2;
        y = icon.top - h - gap;
    } else if min == d_top {
        x = center.x - w / 2;
        y = icon.bottom + gap;
    } else if min == d_left {
        x = icon.right + gap;
        y = center.y - h / 2;
    } else {
        x = icon.left - w - gap;
        y = center.y - h / 2;
    }
    let wa = pl.work;
    x = x.clamp(wa.left, (wa.right - w).max(wa.left));
    y = y.clamp(wa.top, (wa.bottom - h).max(wa.top));
    (x, y)
}

/// Size + position for the current data, capped to the monitor work area.
fn geometry(pl: &Place, devs: &[DeviceState]) -> (i32, i32, i32, i32) {
    let s = |v: i32| v * pl.dpi / 96;
    let items = build_items(devs);
    let gap = s(GAP);
    let w = s(WIDTH);
    let max_h = (pl.work.bottom - pl.work.top - 2 * gap).max(s(EMPTY_H));
    let h = s(content_dip(&items)).min(max_h);
    let (x, y) = compute_pos(pl, w, h, gap);
    (x, y, w, h)
}

fn relayout(hwnd: HWND) {
    let Some(pl) = PLACE.lock().ok().and_then(|p| *p) else { return };
    let (devs, _) = scheduler::snapshot();
    let (x, y, w, h) = geometry(&pl, &devs);
    unsafe {
        let _ = SetWindowPos(hwnd, None, x, y, w, h, SWP_NOZORDER | SWP_NOACTIVATE);
    }
    scroll_by(hwnd, 0);
}

/// Max scroll offset in DIPs for the current data and window height.
fn scroll_max(hwnd: HWND) -> i32 {
    let mut rc = RECT::default();
    let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96) as i32;
    unsafe {
        let _ = GetClientRect(hwnd, &mut rc);
    }
    let view = (rc.bottom - rc.top) * 96 / dpi;
    let (devs, _) = scheduler::snapshot();
    (content_dip(&build_items(&devs)) - view).max(0)
}

fn scroll_by(hwnd: HWND, delta: i32) {
    let old = SCROLL.load(Relaxed);
    let new = (old + delta).clamp(0, scroll_max(hwnd));
    SCROLL.store(new, Relaxed);
    if new != old {
        unsafe {
            let _ = InvalidateRect(Some(hwnd), None, false);
        }
    }
}

fn open(owner: HWND, anchor: POINT) {
    unsafe {
        // Icon rectangle (falls back to the click point if unavailable).
        let id = NOTIFYICONIDENTIFIER {
            cbSize: size_of::<NOTIFYICONIDENTIFIER>() as u32,
            hWnd: owner,
            uID: tray::ICON_ID,
            ..Default::default()
        };
        let icon = Shell_NotifyIconGetRect(&id).unwrap_or(RECT {
            left: anchor.x,
            top: anchor.y,
            right: anchor.x,
            bottom: anchor.y,
        });
        let center = POINT { x: (icon.left + icon.right) / 2, y: (icon.top + icon.bottom) / 2 };
        let mon = MonitorFromPoint(center, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        let _ = GetMonitorInfoW(mon, &mut mi);
        let (mut dx, mut dy) = (96u32, 96u32);
        let _ = GetDpiForMonitor(mon, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);

        let pl = Place { icon, monitor: mi.rcMonitor, work: mi.rcWork, dpi: dx as i32 };
        if let Ok(mut p) = PLACE.lock() {
            *p = Some(pl);
        }
        // Show cached data immediately; a refresh is started below.
        let (devs, _) = scheduler::snapshot();
        let (x, y, w, h) = geometry(&pl, &devs);

        let Ok(hmod) = GetModuleHandleW(None) else { return };
        let hinst = hmod.into();
        let class = w!("BilgiPaneliPanel");
        let wc = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinst,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            lpszClassName: class,
            ..Default::default()
        };
        RegisterClassExW(&wc); // fails harmlessly if already registered

        let Ok(hwnd) = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            class,
            w!("Bilgi Paneli"),
            WS_POPUP,
            x,
            y,
            w,
            h,
            None,
            None,
            Some(hinst),
            None,
        ) else {
            return;
        };
        PANEL.store(hwnd.0, Relaxed);
        SCROLL.store(0, Relaxed);
        apply_theme(hwnd, true);
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
    }
    scheduler::request_refresh(owner);
}

fn system_uses_dark() -> bool {
    let mut v = 1u32;
    let mut size = 4u32;
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("AppsUseLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut v as *mut u32 as *mut c_void),
            Some(&mut size),
        )
        .is_ok()
            && v == 0
    }
}

/// Rounded corners + Mica on Windows 11; every call may fail on Windows 10.
fn apply_theme(hwnd: HWND, first: bool) {
    unsafe {
        let dark = system_uses_dark();
        DARK.store(dark, Relaxed);
        let set = |attr, val: i32| {
            DwmSetWindowAttribute(hwnd, attr, &val as *const i32 as *const c_void, 4).is_ok()
        };
        set(DWMWA_USE_IMMERSIVE_DARK_MODE, dark as i32);
        set(DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND.0);
        if first {
            let mica = set(DWMWA_SYSTEMBACKDROP_TYPE, DWMSBT_MAINWINDOW.0);
            if mica {
                let m = MARGINS { cxLeftWidth: -1, cxRightWidth: -1, cyTopHeight: -1, cyBottomHeight: -1 };
                let _ = DwmExtendFrameIntoClientArea(hwnd, &m);
            }
            MICA.store(mica, Relaxed);
        }
    }
}

// ---------------------------------------------------------------- painting (Direct2D / DirectWrite)

struct Formats {
    name: IDWriteTextFormat,
    value: IDWriteTextFormat,
    sub: IDWriteTextFormat,
    header: IDWriteTextFormat,
    symbol: IDWriteTextFormat,
    center: IDWriteTextFormat,
}

/// d2d1.dll / dwrite.dll are loaded on demand (not linked) and unloaded again
/// with the panel, which returns their heaps and threads to the OS.
struct Libs(HMODULE, HMODULE);

impl Drop for Libs {
    fn drop(&mut self) {
        unsafe {
            let _ = FreeLibrary(self.1);
            let _ = FreeLibrary(self.0);
        }
    }
}

type CreateD2d = unsafe extern "system" fn(D2D1_FACTORY_TYPE, *const GUID, *const D2D1_FACTORY_OPTIONS, *mut *mut c_void) -> HRESULT;
type CreateDw = unsafe extern "system" fn(DWRITE_FACTORY_TYPE, *const GUID, *mut *mut c_void) -> HRESULT;

/// All graphics resources; created on the first paint, dropped with the window.
/// `_libs` is the last field so the DLLs are unloaded after every COM object is released.
struct Gfx {
    rt: ID2D1HwndRenderTarget,
    brush: ID2D1SolidColorBrush,
    dw: IDWriteFactory,
    fmt: Formats,
    _libs: Libs,
}

thread_local! {
    static GFX: RefCell<Option<Gfx>> = const { RefCell::new(None) };
}

fn make_format(
    dw: &IDWriteFactory,
    family: PCWSTR,
    size: f32,
    weight: i32,
    center: bool,
) -> windows::core::Result<IDWriteTextFormat> {
    unsafe {
        let f = dw.CreateTextFormat(
            family,
            None,
            DWRITE_FONT_WEIGHT(weight),
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            size,
            w!("tr-TR"),
        )?;
        f.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
        f.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
        f.SetTextAlignment(if center { DWRITE_TEXT_ALIGNMENT_CENTER } else { DWRITE_TEXT_ALIGNMENT_LEADING })?;
        let trimming = DWRITE_TRIMMING {
            granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
            delimiter: 0,
            delimiterCount: 0,
        };
        let sign = dw.CreateEllipsisTrimmingSign(&f)?;
        f.SetTrimming(&trimming, &sign)?;
        Ok(f)
    }
}

impl Gfx {
    fn new(hwnd: HWND) -> windows::core::Result<Gfx> {
        unsafe {
            let mut rc = RECT::default();
            GetClientRect(hwnd, &mut rc)?;
            let dpi = GetDpiForWindow(hwnd).max(96) as f32;
            let d2d = LoadLibraryW(w!("d2d1.dll"))?;
            let dwl = match LoadLibraryW(w!("dwrite.dll")) {
                Ok(m) => m,
                Err(e) => {
                    let _ = FreeLibrary(d2d);
                    return Err(e);
                }
            };
            let libs = Libs(d2d, dwl);
            let create_d2d = GetProcAddress(d2d, pcstr!("D2D1CreateFactory")).ok_or_else(windows::core::Error::from_thread)?;
            let create_dw = GetProcAddress(dwl, pcstr!("DWriteCreateFactory")).ok_or_else(windows::core::Error::from_thread)?;
            #[allow(clippy::missing_transmute_annotations)] // target types are given by the let pattern
            let (create_d2d, create_dw): (CreateD2d, CreateDw) = (transmute(create_d2d), transmute(create_dw));
            let mut raw = null_mut();
            create_d2d(D2D1_FACTORY_TYPE_SINGLE_THREADED, &ID2D1Factory::IID, std::ptr::null(), &mut raw).ok()?;
            let factory = ID2D1Factory::from_raw(raw);
            let props = D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: dpi,
                dpiY: dpi,
                usage: D2D1_RENDER_TARGET_USAGE_NONE,
                minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
            };
            let hprops = D2D1_HWND_RENDER_TARGET_PROPERTIES {
                hwnd,
                pixelSize: D2D_SIZE_U { width: (rc.right - rc.left) as u32, height: (rc.bottom - rc.top) as u32 },
                presentOptions: D2D1_PRESENT_OPTIONS_NONE,
            };
            let rt = factory.CreateHwndRenderTarget(&props, &hprops)?;
            rt.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            let brush = rt.CreateSolidColorBrush(&rgba(0, 1.0), None)?;

            let mut raw = null_mut();
            create_dw(DWRITE_FACTORY_TYPE_SHARED, &IDWriteFactory::IID, &mut raw).ok()?;
            let dw = IDWriteFactory::from_raw(raw);
            // Windows 11 UI font if present, else Segoe UI.
            let mut coll = None;
            let variable = dw.GetSystemFontCollection(&mut coll, false).is_ok()
                && coll.is_some_and(|c| {
                    let (mut idx, mut exists) = (0u32, false.into());
                    c.FindFamilyName(w!("Segoe UI Variable Text"), &mut idx, &mut exists).is_ok() && exists.as_bool()
                });
            let family = if variable { w!("Segoe UI Variable Text") } else { w!("Segoe UI") };
            let fmt = Formats {
                name: make_format(&dw, family, 15.0, 600, false)?,
                value: make_format(&dw, family, 15.0, 400, false)?,
                sub: make_format(&dw, family, 12.0, 400, false)?,
                header: make_format(&dw, family, 11.0, 700, false)?,
                symbol: make_format(&dw, w!("Segoe UI Symbol"), 14.0, 400, false)?,
                center: make_format(&dw, family, 15.0, 400, true)?,
            };
            Ok(Gfx { rt, brush, dw, fmt, _libs: libs })
        }
    }

    fn color(&self, c: D2D1_COLOR_F) {
        unsafe { self.brush.SetColor(&c) };
    }

    fn fill_round(&self, r: D2D_RECT_F, radius: f32, c: D2D1_COLOR_F) {
        self.color(c);
        unsafe {
            self.rt.FillRoundedRectangle(&D2D1_ROUNDED_RECT { rect: r, radiusX: radius, radiusY: radius }, &self.brush);
        }
    }

    fn line(&self, x0: f32, y0: f32, x1: f32, y1: f32, w: f32, c: D2D1_COLOR_F) {
        self.color(c);
        unsafe {
            self.rt.DrawLine(
                windows_numerics::Vector2 { X: x0, Y: y0 },
                windows_numerics::Vector2 { X: x1, Y: y1 },
                &self.brush,
                w,
                None,
            );
        }
    }

    fn stroke_round(&self, r: D2D_RECT_F, radius: f32, c: D2D1_COLOR_F) {
        self.color(c);
        unsafe {
            self.rt.DrawRoundedRectangle(
                &D2D1_ROUNDED_RECT { rect: r, radiusX: radius, radiusY: radius },
                &self.brush,
                1.0,
                None,
            );
        }
    }

    fn text(&self, s: &str, f: &IDWriteTextFormat, r: D2D_RECT_F, c: D2D1_COLOR_F) {
        self.color(c);
        unsafe {
            self.rt.DrawText(&wide(s), f, &r, &self.brush, D2D1_DRAW_TEXT_OPTIONS_CLIP, DWRITE_MEASURING_MODE_NATURAL);
        }
    }

    fn width(&self, s: &str, f: &IDWriteTextFormat) -> f32 {
        unsafe {
            let Ok(layout) = self.dw.CreateTextLayout(&wide(s), f, 10_000.0, 100.0) else { return 0.0 };
            let mut m = DWRITE_TEXT_METRICS::default();
            let _ = layout.GetMetrics(&mut m);
            m.widthIncludingTrailingWhitespace
        }
    }
}

fn rgba(rgb: u32, a: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: ((rgb >> 16) & 0xFF) as f32 / 255.0,
        g: ((rgb >> 8) & 0xFF) as f32 / 255.0,
        b: (rgb & 0xFF) as f32 / 255.0,
        a,
    }
}

fn rect(l: f32, t: f32, r: f32, b: f32) -> D2D_RECT_F {
    D2D_RECT_F { left: l, top: t, right: r, bottom: b }
}

struct Palette {
    bg: D2D1_COLOR_F,
    card: D2D1_COLOR_F,
    card_edge: D2D1_COLOR_F,
    text: D2D1_COLOR_F,
    grey: D2D1_COLOR_F,
    good: D2D1_COLOR_F,
    mid: D2D1_COLOR_F,
    low: D2D1_COLOR_F,
    sleep: D2D1_COLOR_F,
}

fn palette(dark: bool) -> Palette {
    if dark {
        Palette {
            bg: rgba(0x202020, 1.0),
            card: rgba(0xFFFFFF, 0.05),
            card_edge: rgba(0xFFFFFF, 0.06),
            text: rgba(0xFFFFFF, 1.0),
            grey: rgba(0xFFFFFF, 0.62),
            good: rgba(0x6CCB5F, 1.0),
            mid: rgba(0xFCB900, 1.0),
            low: rgba(0xFF99A4, 1.0),
            sleep: rgba(0xA9B8FF, 1.0),
        }
    } else {
        Palette {
            bg: rgba(0xF3F3F3, 1.0),
            card: rgba(0xFFFFFF, 0.6),
            card_edge: rgba(0x000000, 0.06),
            text: rgba(0x1A1A1A, 1.0),
            grey: rgba(0x000000, 0.55),
            good: rgba(0x0F7B0F, 1.0),
            mid: rgba(0x9D5D00, 1.0),
            low: rgba(0xC42B1C, 1.0),
            sleep: rgba(0x5B6BC9, 1.0),
        }
    }
}

fn paint(hwnd: HWND) {
    unsafe {
        let mut ps = PAINTSTRUCT::default();
        BeginPaint(hwnd, &mut ps);
        GFX.with(|cell| {
            let mut slot = cell.borrow_mut();
            if slot.is_none() {
                *slot = Gfx::new(hwnd).ok();
            }
            if let Some(g) = slot.as_ref() {
                g.rt.BeginDraw();
                draw_content(hwnd, g);
                // Device loss (D2DERR_RECREATE_TARGET) etc.: rebuild on the next paint.
                if g.rt.EndDraw(None, None).is_err() {
                    *slot = None;
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
            }
        });
        let _ = EndPaint(hwnd, &ps);
    }
}

fn draw_bat_icon(g: &Gfx, x: f32, cy: f32, pct: u8, color: D2D1_COLOR_F, outline: D2D1_COLOR_F) {
    const W: f32 = 22.0;
    const H: f32 = 11.0;
    let (t, b) = (cy - H / 2.0, cy + H / 2.0);
    g.stroke_round(rect(x + 0.5, t + 0.5, x + W - 0.5, b - 0.5), 3.0, outline);
    g.fill_round(rect(x + W + 1.0, cy - 2.5, x + W + 3.0, cy + 2.5), 1.0, outline);
    let inner = (W - 5.0) * f32::from(pct.min(100)) / 100.0;
    g.fill_round(rect(x + 2.5, t + 2.5, x + 2.5 + inner.max(1.5), b - 2.5), 1.5, color);
}

/// Width of the sleep mark drawn by `draw_zzz`.
const ZZZ_W: f32 = 17.0;

/// Sleep mark: three "z" letters growing left to right and rising, drawn as
/// lines so they stay crisp at any DPI without another font. `x` is the left
/// edge, `cy` the row's vertical centre.
fn draw_zzz(g: &Gfx, x: f32, cy: f32, color: D2D1_COLOR_F) {
    // (left offset, size, baseline offset from cy, stroke width)
    const ZS: [(f32, f32, f32, f32); 3] = [(0.0, 4.0, 5.0, 1.1), (5.0, 5.0, 3.0, 1.25), (11.0, 6.0, 0.5, 1.4)];
    for (dx, s, base, w) in ZS {
        let (l, r) = (x + dx, x + dx + s);
        let (b, t) = (cy + base, cy + base - s);
        g.line(l, t, r, t, w, color);
        g.line(r, t, l, b, w, color);
        g.line(l, b, r, b, w, color);
    }
}

fn draw_content(hwnd: HWND, g: &Gfx) {
    unsafe {
        let dark = DARK.load(Relaxed);
        let pal = palette(dark);
        let mut rc = RECT::default();
        let _ = GetClientRect(hwnd, &mut rc);
        let dpi = GetDpiForWindow(hwnd).max(96) as i32;
        let view_w = (rc.right - rc.left) as f32 * 96.0 / dpi as f32;
        let view_h = (rc.bottom - rc.top) as f32 * 96.0 / dpi as f32;

        // Mica: transparent so the backdrop shows through; otherwise a solid fill.
        g.rt.Clear(Some(&if MICA.load(Relaxed) { rgba(0, 0.0) } else { pal.bg }));

        let (devs, updated) = scheduler::snapshot();
        let items = build_items(&devs);
        let now = GetTickCount64();

        if items.is_empty() {
            let msg = if updated.is_none() && scheduler::is_refreshing() {
                "Güncelleniyor…"
            } else {
                "Cihaz bulunamadı"
            };
            g.text(msg, &g.fmt.center, rect(0.0, 0.0, view_w, view_h), pal.grey);
            return;
        }

        // Right-hand columns are shared by all rows so they line up.
        let f = &g.fmt;
        let hz_w = g.width("8000 Hz", &f.value);
        let pct_w = g.width("100%", &f.value);
        let bolt_w = g.width("⚡", &f.symbol) + 2.0;
        const BAR_SLOT: f32 = 22.0 + 3.0 + 6.0; // body + nub + gap
        let left = PAD as f32;
        let right = view_w - PAD as f32;
        let col_gap = COL_GAP as f32;

        // Scrolling items; the footer stays fixed.
        let list_bottom = view_h - FOOTER_H as f32;
        g.rt.PushAxisAlignedClip(&rect(0.0, 0.0, view_w, list_bottom), D2D1_ANTIALIAS_MODE_ALIASED);
        let mut y = TOP_PAD as f32 - SCROLL.load(Relaxed) as f32;

        let mut idx = 0;
        while idx < items.len() {
            let d = match &items[idx] {
                Item::Header(title) => {
                    g.text(title, &f.header, rect(left, y, right, y + HEADER_H as f32), pal.grey);
                    y += HEADER_H as f32;
                    idx += 1;
                    // One rounded card per section behind its rows.
                    let rows = items[idx..].iter().take_while(|i| matches!(i, Item::Dev(_))).count();
                    let card = rect(CARD_INSET as f32, y, view_w - CARD_INSET as f32, y + (rows as i32 * ROW_H) as f32);
                    g.fill_round(card, CARD_RADIUS, pal.card);
                    g.stroke_round(
                        rect(card.left + 0.5, card.top + 0.5, card.right - 0.5, card.bottom - 0.5),
                        CARD_RADIUS,
                        pal.card_edge,
                    );
                    continue;
                }
                Item::Dev(d) => d,
            };
            idx += 1;
            let (l1t, l1b) = (y + 3.0, y + 25.0);
            let line1 = |l: f32, r: f32| rect(l, l1t, r, l1b);
            let line2 = rect(left, y + 25.0, right, y + 43.0);

            let mut cur = right;
            if d.kind != Kind::Headset {
                let t = hz_text(d);
                let x = right - g.width(&t, &f.value);
                g.text(&t, &f.value, line1(x, right + 2.0), pal.text);
                cur = right - hz_w - col_gap;
            }

            // Percentage column (right-aligned) with the bar icon on its left.
            let bat = battery_text(d);
            let charging = d.charging == Some(true);
            let low = d.battery.is_some_and(|b| b <= 15) && !charging;
            let bw = g.width(&bat, &f.value);
            g.text(&bat, &f.value, line1(cur - bw, cur + 2.0), if low { pal.low } else { pal.text });
            cur -= pct_w + col_gap;
            if let Some(b) = d.battery {
                let color = if low {
                    pal.low
                } else if charging || b > 40 {
                    pal.good
                } else {
                    pal.mid
                };
                draw_bat_icon(g, cur - BAR_SLOT + 6.0, (l1t + l1b) / 2.0, b, color, pal.grey);
            }
            cur -= BAR_SLOT;
            let mut name_right = cur - col_gap;
            if d.stale {
                cur -= ZZZ_W + 3.0;
                draw_zzz(g, cur, (l1t + l1b) / 2.0, pal.sleep);
                name_right = cur - col_gap;
            }
            if charging {
                g.text("⚡", &f.symbol, line1(cur - bolt_w, cur), pal.text);
                name_right = cur - bolt_w - col_gap;
            }
            g.text(&d.name, &f.name, line1(left, name_right), pal.text);
            g.text(&sub_text(d, now), &f.sub, line2, pal.grey);
            y += ROW_H as f32;
        }
        g.rt.PopAxisAlignedClip();

        // Scroll indicator.
        let total = content_dip(&items) as f32;
        if total > view_h {
            let track = list_bottom - 8.0;
            let thumb = (track * view_h / total).max(20.0);
            let top = 4.0 + (track - thumb) * SCROLL.load(Relaxed) as f32 / (total - view_h);
            g.fill_round(rect(view_w - 5.0, top, view_w - 2.0, top + thumb), 1.5, pal.grey);
        }

        let text = match updated {
            Some((h, m)) => format!("Güncellendi {h:02}:{m:02}"),
            None => "Güncelleniyor…".to_string(),
        };
        let tw = g.width(&text, &f.sub);
        g.text(&text, &f.sub, rect(right - tw, list_bottom, right + 2.0, view_h), pal.grey);
    }
}

extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            paint(hwnd);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_SIZE => {
            let (cw, ch) = ((lparam.0 & 0xFFFF) as u32, ((lparam.0 >> 16) & 0xFFFF) as u32);
            GFX.with(|c| {
                if let Some(g) = c.borrow().as_ref() {
                    let _ = unsafe { g.rt.Resize(&D2D_SIZE_U { width: cw, height: ch }) };
                }
            });
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            let notches = ((wparam.0 >> 16) as u16 as i16) as i32;
            scroll_by(hwnd, -notches * WHEEL_STEP / 120);
            LRESULT(0)
        }
        WM_SETTINGCHANGE => {
            // Light/dark switch while the panel is open.
            let is_theme = lparam.0 != 0
                && unsafe { PCWSTR(lparam.0 as *const u16).to_string().is_ok_and(|s| s == "ImmersiveColorSet") };
            if is_theme {
                apply_theme(hwnd, false);
                unsafe {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
            }
            LRESULT(0)
        }
        WM_ACTIVATE => {
            if wparam.0 & 0xFFFF == WA_INACTIVE {
                // Defer destruction: destroying inside WM_ACTIVATE can disturb
                // the activation sequence.
                unsafe {
                    CLOSED_AT.store(GetTickCount64(), Relaxed);
                    let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                }
            }
            LRESULT(0)
        }
        WM_KEYDOWN if wparam.0 == VK_ESCAPE.0 as usize => {
            close();
            LRESULT(0)
        }
        WM_DESTROY => {
            GFX.with(|c| *c.borrow_mut() = None); // release all Direct2D/DirectWrite objects
            PANEL.store(null_mut(), Relaxed);
            LRESULT(0)
        }
        WM_NCDESTROY => {
            let r = unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
            power::trim_working_set();
            r
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}
