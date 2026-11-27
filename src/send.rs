//! "Rapor gönder": a small modeless dialog (plain Win32 controls) that builds the
//! report in the background and posts it as a GitHub issue.
//!
//! Everything blocking (report, WinHTTP, device-flow polling) runs on short-lived
//! worker threads that publish a [`Phase`] and post `WM_UPDATE` to the dialog.
//! Nothing exists while the dialog is closed.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering::SeqCst};
use std::sync::{Arc, Mutex};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, COLOR_BTNFACE, CreateFontW, DEFAULT_CHARSET, DeleteObject, GetMonitorInfoW,
    HFONT, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint, OUT_DEFAULT_PRECIS,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow};
use windows::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{HSTRING, PCWSTR, w};

use crate::devices::{DeviceState, Kind};
use crate::github::{self, Cancel, Fail, Flow, IssueFields};
use crate::report;

const CLASS: PCWSTR = w!("BilgiPaneliSend");
/// Worker -> dialog: wparam = generation. State is read from [`SHARED`].
const WM_UPDATE: u32 = WM_APP + 20;

const ID_DEV: i32 = 101;
const ID_PIL: i32 = 102;
const ID_CHG: i32 = 103;
const ID_HZ: i32 = 104;
const ID_NOTE: i32 = 105;
const ID_NOTICE: i32 = 106;
const ID_STATUS: i32 = 107;
const ID_CODE: i32 = 108;
const ID_SHOW: i32 = 111;
const ID_EXTRA: i32 = 112;
const ID_SEND: i32 = 1; // IDOK: Enter
const ID_CLOSE: i32 = 2; // IDCANCEL: Esc

/// Client width in device-independent pixels.
const W: i32 = 460;

static DIALOG: AtomicIsize = AtomicIsize::new(0);
static EPOCH: AtomicU32 = AtomicU32::new(0);
static CLASS_REGISTERED: AtomicBool = AtomicBool::new(false);

/// What the dialog currently shows.
#[derive(Clone, Debug)]
enum Phase {
    Preparing,
    Ready,
    Busy(String),
    Code { code: String },
    Done { url: String },
    Failed(String),
    Info(String),
}

/// Prepared report plus its %TEMP% copy (for "Raporu göster").
struct Prep {
    r: report::Prepared,
    path: Option<PathBuf>,
}

struct Shared {
    epoch: u32,
    phase: Phase,
    prep: Option<Arc<Prep>>,
}

static SHARED: Mutex<Shared> = Mutex::new(Shared { epoch: 0, phase: Phase::Preparing, prep: None });
static CANCEL: Mutex<Option<Arc<Cancel>>> = Mutex::new(None);

/// Control handles and metrics (UI thread only).
#[derive(Clone, Copy)]
struct Ctl {
    hwnd: HWND,
    owner: HWND,
    epoch: u32,
    dpi: u32,
    font: HFONT,
    big: HFONT,
    code_shown: bool,
    labels: [HWND; 5],
    dev: HWND,
    pil: HWND,
    chg: HWND,
    hz: HWND,
    note: HWND,
    notice: HWND,
    status: HWND,
    code: HWND,
    send: HWND,
    show: HWND,
    close: HWND,
    extra: HWND,
}

thread_local! {
    static CTL: Cell<Option<Ctl>> = const { Cell::new(None) };
    static DEVICES: RefCell<Vec<DeviceState>> = const { RefCell::new(Vec::new()) };
}

fn ctl() -> Option<Ctl> {
    CTL.with(Cell::get)
}

/// Lets the dialog handle Tab / Enter / Esc. Call from the main message loop.
pub fn dispatch_dialog_message(msg: &MSG) -> bool {
    let h = DIALOG.load(SeqCst);
    h != 0 && unsafe { IsDialogMessageW(HWND(h as *mut _), msg) }.as_bool()
}

// ------------------------------------------------------------------ open

/// Opens the dialog (or brings the open one to the front). `preferred` = name
/// of the device to preselect.
pub fn open(owner: HWND, preferred: Option<&str>) {
    if let Some(c) = ctl() {
        unsafe {
            let _ = ShowWindow(c.hwnd, SW_RESTORE);
            let _ = SetForegroundWindow(c.hwnd);
        }
        return;
    }
    if report::BUSY.compare_exchange(false, true, SeqCst, SeqCst).is_err() {
        return;
    }
    let devices = crate::scheduler::snapshot_all();
    let default = preferred
        .and_then(|n| devices.iter().position(|d| d.name == n))
        .or_else(|| devices.iter().position(|d| d.kind == Kind::Mouse))
        .unwrap_or(0);
    let epoch = EPOCH.fetch_add(1, SeqCst) + 1;
    let Some(c) = create(owner, epoch, &devices, default) else {
        report::BUSY.store(false, SeqCst);
        return;
    };
    DEVICES.with(|d| *d.borrow_mut() = devices);
    CTL.with(|x| x.set(Some(c)));
    DIALOG.store(c.hwnd.0 as isize, SeqCst);
    if let Ok(mut s) = SHARED.lock() {
        *s = Shared { epoch, phase: Phase::Preparing, prep: None };
    }
    if let Ok(mut x) = CANCEL.lock() {
        *x = Some(Cancel::new());
    }
    layout(&c, false);
    apply_phase(&c, &Phase::Preparing);
    position(&c);
    unsafe {
        let _ = ShowWindow(c.hwnd, SW_SHOW);
        let _ = SetForegroundWindow(c.hwnd);
        let _ = SetFocus(Some(c.dev));
    }

    let hwnd = c.hwnd.0 as isize;
    let spawned = std::thread::Builder::new().name("report".into()).stack_size(1024 * 1024).spawn(move || {
        let r = report::prepare();
        let path = report::save_temp(&r.text);
        report::BUSY.store(false, SeqCst);
        let prep = Arc::new(Prep { r, path });
        let fresh = match SHARED.lock() {
            Ok(mut s) if s.epoch == epoch => {
                s.prep = Some(prep.clone());
                s.phase = Phase::Ready;
                true
            }
            _ => false,
        };
        if fresh {
            post_update(hwnd, epoch);
        } else if let Some(p) = &prep.path {
            let _ = std::fs::remove_file(p);
        }
    });
    if spawned.is_err() {
        report::BUSY.store(false, SeqCst);
        set_phase(epoch, hwnd, Phase::Failed("Rapor hazırlanamadı.".into()));
    }
}

fn post_update(hwnd: isize, epoch: u32) {
    unsafe {
        let _ = PostMessageW(Some(HWND(hwnd as *mut _)), WM_UPDATE, WPARAM(epoch as usize), LPARAM(0));
    }
}

/// Publishes a new phase if the dialog of generation `epoch` still exists.
fn set_phase(epoch: u32, hwnd: isize, phase: Phase) {
    if let Ok(mut s) = SHARED.lock() {
        if s.epoch != epoch {
            return;
        }
        s.phase = phase;
    }
    post_update(hwnd, epoch);
}

// ------------------------------------------------------------------ window + controls

fn font(dpi: u32, pt: i32, weight: i32) -> HFONT {
    unsafe {
        CreateFontW(
            -((pt * dpi as i32 + 36) / 72),
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            0,
            w!("Segoe UI"),
        )
    }
}

fn child(parent: HWND, class: PCWSTR, text: &str, style: u32, ex: WINDOW_EX_STYLE, id: i32) -> HWND {
    unsafe {
        let hinst = GetModuleHandleW(None).unwrap_or_default();
        CreateWindowExW(
            ex,
            class,
            &HSTRING::from(text),
            WS_CHILD | WS_VISIBLE | WINDOW_STYLE(style),
            0,
            0,
            0,
            0,
            Some(parent),
            Some(HMENU(id as isize as *mut _)),
            Some(hinst.into()),
            None,
        )
        .unwrap_or_default()
    }
}

fn create(owner: HWND, epoch: u32, devices: &[DeviceState], default: usize) -> Option<Ctl> {
    unsafe {
        let hinst = GetModuleHandleW(None).ok()?;
        if !CLASS_REGISTERED.swap(true, SeqCst) {
            let wc = WNDCLASSEXW {
                cbSize: size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(wnd_proc),
                hInstance: hinst.into(),
                lpszClassName: CLASS,
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                hIcon: LoadIconW(None, IDI_APPLICATION).unwrap_or_default(),
                hbrBackground: windows::Win32::Graphics::Gdi::HBRUSH((COLOR_BTNFACE.0 + 1) as usize as *mut _),
                ..Default::default()
            };
            RegisterClassExW(&wc);
        }
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            CLASS,
            w!("Cihaz raporu gönder"),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN,
            0,
            0,
            300,
            300,
            None,
            None,
            Some(hinst.into()),
            None,
        )
        .ok()?;
        let dpi = GetDpiForWindow(hwnd).max(96);
        let (f, big) = (font(dpi, 9, 400), font(dpi, 20, 700));

        let (st, tab) = (0u32, WS_TABSTOP.0);
        let label = |t: &str| child(hwnd, w!("STATIC"), t, st, WINDOW_EX_STYLE(0), -1);
        let labels = [
            label("Cihaz"),
            label("Üretici yazılımında pil (%)"),
            label("Şarj oluyor mu?"),
            label("Üretici yazılımında Hz"),
            label("Not (isteğe bağlı)"),
        ];
        let edit = WS_EX_CLIENTEDGE;
        let dev = child(hwnd, w!("COMBOBOX"), "", CBS_DROPDOWNLIST as u32 | WS_VSCROLL.0 | tab, WINDOW_EX_STYLE(0), ID_DEV);
        let pil = child(hwnd, w!("EDIT"), "", ES_AUTOHSCROLL as u32 | tab, edit, ID_PIL);
        let chg = child(hwnd, w!("COMBOBOX"), "", CBS_DROPDOWNLIST as u32 | WS_VSCROLL.0 | tab, WINDOW_EX_STYLE(0), ID_CHG);
        let hz = child(hwnd, w!("EDIT"), "", ES_AUTOHSCROLL as u32 | tab, edit, ID_HZ);
        let note = child(
            hwnd,
            w!("EDIT"),
            "",
            (ES_MULTILINE | ES_WANTRETURN | ES_AUTOVSCROLL) as u32 | WS_VSCROLL.0 | tab,
            edit,
            ID_NOTE,
        );
        let notice = child(
            hwnd,
            w!("STATIC"),
            "Rapor anonimdir: seri numarası, kullanıcı adı ve bilgisayar adı gönderilmez.",
            st,
            WINDOW_EX_STYLE(0),
            ID_NOTICE,
        );
        let status = child(hwnd, w!("STATIC"), "", st, WINDOW_EX_STYLE(0), ID_STATUS);
        let code = child(hwnd, w!("STATIC"), "", st, WINDOW_EX_STYLE(0), ID_CODE);
        let send = child(hwnd, w!("BUTTON"), "Gönder", BS_DEFPUSHBUTTON as u32 | tab, WINDOW_EX_STYLE(0), ID_SEND);
        let show = child(hwnd, w!("BUTTON"), "Raporu göster", BS_PUSHBUTTON as u32 | tab, WINDOW_EX_STYLE(0), ID_SHOW);
        let close = child(hwnd, w!("BUTTON"), "İptal", BS_PUSHBUTTON as u32 | tab, WINDOW_EX_STYLE(0), ID_CLOSE);
        let extra = child(hwnd, w!("BUTTON"), "", BS_PUSHBUTTON as u32 | tab, WINDOW_EX_STYLE(0), ID_EXTRA);

        // Combo contents.
        let add = |cb: HWND, text: &str| {
            let t = HSTRING::from(text);
            SendMessageW(cb, CB_ADDSTRING, None, Some(LPARAM(t.as_ptr() as isize)));
        };
        if devices.is_empty() {
            add(dev, "(cihaz bulunamadı)");
        }
        for d in devices {
            add(dev, &d.name);
        }
        SendMessageW(dev, CB_SETCURSEL, Some(WPARAM(default)), None);
        for t in ["Bilinmiyor", "Evet", "Hayır"] {
            add(chg, t);
        }
        SendMessageW(chg, CB_SETCURSEL, Some(WPARAM(0)), None);
        SendMessageW(pil, EM_SETLIMITTEXT, Some(WPARAM(40)), None);
        SendMessageW(hz, EM_SETLIMITTEXT, Some(WPARAM(40)), None);
        SendMessageW(note, EM_SETLIMITTEXT, Some(WPARAM(2000)), None);

        let c = Ctl { hwnd, owner, epoch, dpi, font: f, big, code_shown: false, labels, dev, pil, chg, hz, note, notice, status, code, send, show, close, extra };
        apply_fonts(&c);
        Some(c)
    }
}

const EM_SETLIMITTEXT: u32 = 0xC5;

fn apply_fonts(c: &Ctl) {
    let all = [c.dev, c.pil, c.chg, c.hz, c.note, c.notice, c.status, c.send, c.show, c.close, c.extra];
    unsafe {
        for h in all.iter().chain(c.labels.iter()) {
            SendMessageW(*h, WM_SETFONT, Some(WPARAM(c.font.0 as usize)), Some(LPARAM(1)));
        }
        SendMessageW(c.code, WM_SETFONT, Some(WPARAM(c.big.0 as usize)), Some(LPARAM(1)));
    }
}

/// Places every control for the current DPI and sizes the window to fit.
fn layout(c: &Ctl, show_code: bool) {
    let s = |v: i32| v * c.dpi as i32 / 96;
    let put = |h: HWND, x: i32, y: i32, w: i32, hh: i32| unsafe {
        let _ = MoveWindow(h, s(x), s(y), s(w), s(hh), true);
    };
    put(c.labels[0], 16, 14, 428, 18);
    put(c.dev, 16, 34, 428, 24 + 150);
    put(c.labels[1], 16, 70, 205, 18);
    put(c.pil, 16, 88, 205, 24);
    put(c.labels[2], 239, 70, 205, 18);
    put(c.chg, 239, 88, 205, 24 + 90);
    put(c.labels[3], 16, 124, 205, 18);
    put(c.hz, 16, 142, 205, 24);
    put(c.labels[4], 16, 178, 428, 18);
    put(c.note, 16, 196, 428, 60);
    put(c.notice, 16, 266, 428, 34);
    put(c.status, 16, 306, 428, 48);
    let mut y = 358;
    put(c.code, 16, y, 428, 40);
    if show_code {
        y += 46;
    }
    put(c.extra, 16, y, 150, 28);
    put(c.send, 172, y, 84, 28);
    put(c.show, 262, y, 106, 28);
    put(c.close, 374, y, 70, 28);
    let client_h = y + 28 + 16;
    let mut r = RECT { left: 0, top: 0, right: s(W), bottom: s(client_h) };
    unsafe {
        let style = WINDOW_STYLE(GetWindowLongW(c.hwnd, GWL_STYLE) as u32);
        let _ = AdjustWindowRectExForDpi(&mut r, style, false, WINDOW_EX_STYLE(0), c.dpi);
        let _ = SetWindowPos(c.hwnd, None, 0, 0, r.right - r.left, r.bottom - r.top, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE);
    }
}

/// Centers the dialog on the work area of the monitor under the cursor.
fn position(c: &Ctl) {
    unsafe {
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let mon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        if GetMonitorInfoW(mon, &mut mi).as_bool() {
            let mut r = RECT::default();
            let _ = GetWindowRect(c.hwnd, &mut r);
            let (w, h) = (r.right - r.left, r.bottom - r.top);
            let wa = mi.rcWork;
            let x = wa.left + ((wa.right - wa.left - w) / 2).max(0);
            let y = wa.top + ((wa.bottom - wa.top - h) / 3).max(0);
            let _ = SetWindowPos(c.hwnd, None, x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
        }
    }
}

fn set_text(h: HWND, t: &str) {
    unsafe {
        let _ = SetWindowTextW(h, &HSTRING::from(t));
    }
}

fn text_of(h: HWND) -> String {
    unsafe {
        let n = GetWindowTextLengthW(h).max(0) as usize;
        let mut b = vec![0u16; n + 1];
        let k = GetWindowTextW(h, &mut b).max(0) as usize;
        String::from_utf16_lossy(&b[..k.min(n)])
    }
}

fn apply_phase(c: &Ctl, phase: &Phase) {
    let (status, inputs, send, show, code, extra, close): (&str, bool, bool, bool, Option<&str>, Option<&str>, &str) = match phase {
        Phase::Preparing => ("Rapor hazırlanıyor… (cihazı oynatın)", true, false, false, None, None, "İptal"),
        Phase::Ready => ("Rapor hazır. Bilgileri girip Gönder'e basın.", true, true, true, None, None, "İptal"),
        Phase::Busy(m) => (m, false, false, true, None, None, "İptal"),
        Phase::Code { code } => (
            "Tarayıcıda açılan GitHub sayfasında bu kodu girip onaylayın (kod panoya kopyalandı). Bekleniyor…",
            false,
            false,
            true,
            Some(code),
            Some("Kodu kopyala"),
            "İptal",
        ),
        Phase::Done { .. } => ("Gönderildi! Teşekkürler.", false, false, true, None, Some("Raporu aç"), "Kapat"),
        Phase::Failed(m) => (m, true, true, true, None, Some("Raporu sadece kaydet"), "İptal"),
        Phase::Info(m) => (m, true, true, true, None, None, "İptal"),
    };
    set_text(c.status, status);
    set_text(c.close, close);
    set_text(c.code, code.unwrap_or(""));
    if let Some(e) = extra {
        set_text(c.extra, e);
    }
    unsafe {
        for h in [c.dev, c.pil, c.chg, c.hz, c.note] {
            let _ = EnableWindow(h, inputs);
        }
        let _ = EnableWindow(c.send, send);
        let _ = EnableWindow(c.show, show);
        let _ = ShowWindow(c.extra, if extra.is_some() { SW_SHOW } else { SW_HIDE });
        let _ = ShowWindow(c.code, if code.is_some() { SW_SHOW } else { SW_HIDE });
    }
    if c.code_shown != code.is_some() {
        let n = Ctl { code_shown: code.is_some(), ..*c };
        CTL.with(|x| x.set(Some(n)));
        layout(&n, n.code_shown);
    }
}

// ------------------------------------------------------------------ actions

fn clip(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

fn panel_values(d: Option<&DeviceState>) -> (String, String, String) {
    let Some(d) = d else { return ("-".into(), "-".into(), "-".into()) };
    let stale = if d.stale { " (eski değer)" } else { "" };
    (
        format!("{}{stale}", d.battery.map_or("-".to_string(), |b| format!("%{b}"))),
        d.charging.map_or("-", |c| if c { "evet" } else { "hayır" }).to_string(),
        d.polling_hz.map_or("-".to_string(), |h| {
            format!(
                "{h} Hz ({})",
                match d.hz_source {
                    crate::devices::HzSource::Driver => "sürücü",
                    crate::devices::HzSource::UsbDescriptor => "USB tanımlayıcısı",
                }
            )
        }),
    )
}

fn current_prep() -> Option<Arc<Prep>> {
    SHARED.lock().ok().and_then(|s| s.prep.clone())
}

fn on_send(c: Ctl) {
    let Some(prep) = current_prep() else { return };
    let sel = unsafe { SendMessageW(c.dev, CB_GETCURSEL, None, None) }.0;
    let dev = DEVICES.with(|d| usize::try_from(sel).ok().and_then(|i| d.borrow().get(i).cloned()));
    let sc = &prep.r.scrubber;
    let name = dev.as_ref().map_or_else(|| prep.r.name.clone(), |d| sc.apply(&d.name));
    let (panel_battery, panel_charging, panel_hz) = panel_values(dev.as_ref());
    let pil = clip(text_of(c.pil).trim(), 40);
    let charging = match unsafe { SendMessageW(c.chg, CB_GETCURSEL, None, None) }.0 {
        1 => "Evet",
        2 => "Hayır",
        _ => "Bilinmiyor",
    };
    let fields = IssueFields {
        device: name,
        vendor_battery: if !pil.is_empty() && pil.chars().all(|ch| ch.is_ascii_digit()) { format!("%{pil}") } else { sc.apply(&pil) },
        vendor_charging: charging.into(),
        vendor_hz: sc.apply(&clip(text_of(c.hz).trim(), 40)),
        note: sc.apply(&clip(&text_of(c.note), 2000)),
        panel_battery,
        panel_charging,
        panel_hz,
    };
    let (epoch, hwnd, owner) = (c.epoch, c.hwnd.0 as isize, c.owner.0 as isize);
    let cancel = CANCEL.lock().ok().and_then(|x| x.clone());
    let first = if github::GITHUB_CLIENT_ID.is_empty() { "Form açılıyor…" } else { "GitHub'a bağlanılıyor…" };
    if let Ok(mut s) = SHARED.lock() {
        s.phase = Phase::Busy(first.into());
    }
    apply_phase(&c, &Phase::Busy(first.into()));
    let spawned = std::thread::Builder::new().name("send".into()).stack_size(512 * 1024).spawn(move || {
        send_job(epoch, hwnd, owner, prep, fields, cancel.unwrap_or_else(Cancel::new));
    });
    if spawned.is_err() {
        set_phase(epoch, hwnd, Phase::Failed("Gönderme başlatılamadı.".into()));
    }
}

fn send_job(epoch: u32, hwnd: isize, owner: isize, prep: Arc<Prep>, fields: IssueFields, cancel: Arc<Cancel>) {
    let post = |p: Phase| set_phase(epoch, hwnd, p);
    if github::GITHUB_CLIENT_ID.is_empty() {
        report::deliver_form(&prep.r.text, &fields.device);
        post(Phase::Info("Doğrudan gönderme henüz etkin değil; form tarayıcıda açıldı, Ctrl+V ile yapıştırın.".into()));
        return;
    }
    let title = github::issue_title(&fields.device);
    let body = github::build_body(&fields, &prep.r.text, github::MAX_BODY_CHARS);
    let on_code = |dc: &github::DeviceCode| {
        let _ = report::copy_to_clipboard(&dc.user_code);
        report::open_url(&dc.verification_uri);
        post(Phase::Code { code: dc.user_code.clone() });
    };
    let flow = Flow { cancel: &cancel, on_code: &on_code };
    let mut retried = false;
    loop {
        post(Phase::Busy("GitHub'a bağlanılıyor…".into()));
        let tokens = match github::ensure_token(&flow) {
            Ok(t) => t,
            Err(Fail::Cancelled) => return,
            Err(e) => return post(Phase::Failed(fail_text(&e))),
        };
        post(Phase::Busy("Rapor gönderiliyor…".into()));
        match github::create_issue(&tokens.access, &title, &body) {
            Ok(url) => {
                crate::tray::info_balloon(HWND(owner as *mut _), "Rapor gönderildi", "Teşekkürler!");
                return post(Phase::Done { url });
            }
            Err(Fail::Unauthorized) if !retried => {
                retried = true;
                github::delete_tokens();
            }
            Err(e) => return post(Phase::Failed(fail_text(&e))),
        }
    }
}

fn fail_text(e: &Fail) -> String {
    match e {
        Fail::Cancelled => String::new(),
        Fail::Unauthorized => "GitHub oturumu doğrulanamadı. Yeniden deneyin.".into(),
        Fail::NotInstalled => "GitHub uygulaması bu repoya kurulmamış.".into(),
        Fail::Other(m) => format!("{m}\nRapor gönderilemedi; isterseniz raporu kaydedebilirsiniz."),
    }
}

fn on_extra(c: Ctl) {
    let phase = SHARED.lock().map(|s| s.phase.clone()).unwrap_or(Phase::Ready);
    match phase {
        Phase::Code { code } => {
            let _ = report::copy_to_clipboard(&code);
        }
        Phase::Done { url } => report::open_url(&url),
        Phase::Failed(_) => {
            let Some(prep) = current_prep() else { return };
            let (epoch, hwnd) = (c.epoch, c.hwnd.0 as isize);
            let _ = std::thread::Builder::new().name("save".into()).spawn(move || {
                report::save_and_copy(&prep.r.text);
                set_phase(epoch, hwnd, Phase::Info("Rapor kaydedildi ve panoya kopyalandı.".into()));
            });
        }
        _ => {}
    }
}

// ------------------------------------------------------------------ window procedure

fn destroy(hwnd: HWND) {
    let Some(c) = ctl() else { return };
    if c.hwnd != hwnd {
        return;
    }
    CTL.with(|x| x.set(None));
    DIALOG.store(0, SeqCst);
    DEVICES.with(|d| *d.borrow_mut() = Vec::new());
    if let Ok(mut x) = CANCEL.lock()
        && let Some(cancel) = x.take()
    {
        cancel.cancel();
    }
    if let Ok(mut s) = SHARED.lock() {
        let old = std::mem::replace(&mut *s, Shared { epoch: 0, phase: Phase::Preparing, prep: None });
        if let Some(p) = old.prep.and_then(|p| p.path.clone()) {
            let _ = std::fs::remove_file(p);
        }
    }
    unsafe {
        let _ = DeleteObject(c.font.into());
        let _ = DeleteObject(c.big.into());
    }
    crate::power::trim_working_set();
}

extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_COMMAND => {
            let Some(c) = ctl() else { return LRESULT(0) };
            let enabled = |h: HWND| unsafe { windows::Win32::UI::Input::KeyboardAndMouse::IsWindowEnabled(h) }.as_bool();
            match (wparam.0 & 0xFFFF) as i32 {
                ID_SEND if enabled(c.send) => on_send(c),
                ID_SHOW if enabled(c.show) => {
                    if let Some(p) = current_prep().and_then(|p| p.path.clone()) {
                        report::open_in_notepad(&p);
                    }
                }
                ID_EXTRA => on_extra(c),
                ID_CLOSE => unsafe {
                    let _ = DestroyWindow(hwnd);
                },
                _ => {}
            }
            LRESULT(0)
        }
        WM_UPDATE => {
            if let Some(c) = ctl()
                && c.epoch == wparam.0 as u32
            {
                let phase = SHARED.lock().map(|s| s.phase.clone());
                if let Ok(p) = phase {
                    apply_phase(&c, &p);
                }
            }
            LRESULT(0)
        }
        WM_DPICHANGED => {
            if let Some(c) = ctl() {
                let dpi = (wparam.0 & 0xFFFF) as u32;
                let r = unsafe { &*(lparam.0 as *const RECT) };
                unsafe {
                    let _ = SetWindowPos(hwnd, None, r.left, r.top, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
                    let _ = DeleteObject(c.font.into());
                    let _ = DeleteObject(c.big.into());
                }
                let n = Ctl { dpi, font: font(dpi, 9, 400), big: font(dpi, 20, 700), ..c };
                CTL.with(|x| x.set(Some(n)));
                apply_fonts(&n);
                layout(&n, n.code_shown);
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            destroy(hwnd);
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}
