//! Test report for device collectors: one anonymized Markdown file with
//! everything needed to verify or reverse-engineer a device protocol.
//!
//! Everything stays local. [`generate_for`] writes the file to the Desktop,
//! copies the text to the clipboard, selects the file in Explorer and opens the
//! pre-filled GitHub issue form; the user attaches the file themselves.
//! It does blocking work (device reads, a 5 s passive listen) and must run on a
//! worker thread, never on the UI thread.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows::Win32::System::SystemInformation::{GetLocalTime, OSVERSIONINFOW};
use windows::Win32::UI::Shell::{SHGetFolderPathW, ShellExecuteW};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{HSTRING, PCWSTR, w};

use crate::anon::Scrubber;
use crate::devices::cm::Bus;
use crate::devices::enumerate::{self, Physical};
use crate::devices::trace::{self, Dir};
use crate::devices::{DeviceState, HidHandle, HidInfo, bt, describe, hid, usb_desc};

/// Issue form the report is meant for.
pub const ISSUE_URL: &str = "https://github.com/Abrahamexe/bilgi-paneli-raporlar/issues/new?template=cihaz-raporu.yml";
/// Page that explains how to send a report (placeholder until the final repo exists).
pub const HOWTO_URL: &str = "https://github.com/Abrahamexe/bilgi-paneli-raporlar#readme";

/// How long vendor collections are listened to (all of them in parallel).
const LISTEN_MS: u64 = 5000;
const MAX_LISTEN_INTERFACES: usize = 24;
const MAX_LISTEN_REPORTS: usize = 150;

// ------------------------------------------------------------------ entry points

/// Builds the report for every connected device. See [`generate_for`].
pub fn generate() -> PathBuf {
    generate_for(None)
}

/// Set while any report is being built (the driver trace is global, so only one at a time).
pub static BUSY: AtomicBool = AtomicBool::new(false);

/// A finished, anonymized report plus what is needed to anonymize more text with it.
pub struct Prepared {
    pub text: String,
    /// Device name for the issue title (already anonymized).
    pub name: String,
    pub scrubber: Scrubber,
}

/// Builds the report (blocking, ~5 s). Writes nothing and opens nothing.
pub fn prepare() -> Prepared {
    let (text, name, scrubber) = build();
    Prepared { text, name, scrubber }
}

/// Builds the report, saves it (Desktop, else %TEMP%), copies it to the
/// clipboard and opens Explorer plus the issue form. `title_hint` = device name
/// for the issue title (default: the first wanted device). Returns the file path.
pub fn generate_for(title_hint: Option<&str>) -> PathBuf {
    let p = prepare();
    deliver_form(&p.text, title_hint.unwrap_or(&p.name))
}

/// Saves + copies + selects the file (no browser). Returns the file path.
pub fn save_and_copy(text: &str) -> PathBuf {
    let path = save(text);
    let _ = copy_to_clipboard(text);
    if !no_open() {
        explorer_select(&path.to_string_lossy());
    }
    path
}

/// [`save_and_copy`] plus the pre-filled issue form in the browser.
pub fn deliver_form(text: &str, name: &str) -> PathBuf {
    let path = save_and_copy(text);
    open_url(&issue_url(name));
    path
}

/// Writes the report to %TEMP% (for "Raporu göster"); `None` if that fails.
pub fn save_temp(text: &str) -> Option<PathBuf> {
    let path = std::env::temp_dir().join(file_name());
    std::fs::write(&path, text).ok().map(|_| path)
}

/// Opens a local file in Notepad.
pub fn open_in_notepad(path: &std::path::Path) {
    if no_open() {
        return;
    }
    let file = HSTRING::from(format!("\"{}\"", path.display()));
    unsafe {
        let _ = ShellExecuteW(None, w!("open"), w!("notepad.exe"), &file, PCWSTR::null(), SW_SHOWNORMAL);
    }
}

fn no_open() -> bool {
    std::env::var_os("BILGI_NO_OPEN").is_some()
}

/// Opens `url` in the default browser (skipped when `BILGI_NO_OPEN` is set).
pub fn open_url(url: &str) {
    if !no_open() {
        shell_open(url);
    }
}

fn explorer_select(path: &str) {
    let args = HSTRING::from(format!("/select,\"{path}\""));
    unsafe {
        let _ = ShellExecuteW(None, w!("open"), w!("explorer.exe"), &args, PCWSTR::null(), SW_SHOWNORMAL);
    }
}

fn shell_open(target: &str) {
    unsafe {
        let _ = ShellExecuteW(None, w!("open"), &HSTRING::from(target), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
    }
}

// ------------------------------------------------------------------ files

fn desktop_dir() -> Option<PathBuf> {
    let mut buf = [0u16; 260];
    // CSIDL_DESKTOPDIRECTORY follows OneDrive-redirected desktops.
    let ok = unsafe { SHGetFolderPathW(None, 0x10, None, 0, &mut buf) }.is_ok();
    let from_api = ok.then(|| PathBuf::from(crate::devices::cm::from_wide(&buf))).filter(|p| p.is_dir());
    from_api.or_else(|| {
        std::env::var_os("USERPROFILE").map(|p| PathBuf::from(p).join("Desktop")).filter(|p| p.is_dir())
    })
}

fn file_name() -> String {
    let t = unsafe { GetLocalTime() };
    format!(
        "BilgiPaneli-rapor-{:04}{:02}{:02}-{:02}{:02}{:02}.md",
        t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond
    )
}

/// Desktop first, %TEMP% when the Desktop is missing or not writable.
fn save(text: &str) -> PathBuf {
    let name = file_name();
    let temp = std::env::temp_dir();
    for dir in desktop_dir().into_iter().chain(std::iter::once(temp.clone())) {
        let path = dir.join(&name);
        if std::fs::write(&path, text).is_ok() {
            return path;
        }
    }
    temp.join(name)
}

/// Puts `text` on the clipboard as Unicode text.
pub fn copy_to_clipboard(text: &str) -> Result<(), String> {
    let w: Vec<u16> = text.replace('\n', "\r\n").encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        OpenClipboard(None).map_err(|e| e.to_string())?;
        let res = (|| {
            EmptyClipboard().map_err(|e| e.to_string())?;
            let hmem = GlobalAlloc(GMEM_MOVEABLE, w.len() * 2).map_err(|e| e.to_string())?;
            let p = GlobalLock(hmem) as *mut u16;
            if p.is_null() {
                return Err("GlobalLock failed".to_string());
            }
            std::ptr::copy_nonoverlapping(w.as_ptr(), p, w.len());
            let _ = GlobalUnlock(hmem);
            SetClipboardData(13, Some(HANDLE(hmem.0))).map_err(|e| e.to_string())?; // CF_UNICODETEXT
            Ok(())
        })();
        let _ = CloseClipboard();
        res
    }
}

// ------------------------------------------------------------------ formatting helpers

/// Percent-encodes `s` (UTF-8) for a URL query value.
pub fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}

/// Issue-form URL with the title pre-filled: `[Cihaz] <name>`.
pub fn issue_url(device_name: &str) -> String {
    let name: String = device_name.chars().take(60).collect();
    format!("{ISSUE_URL}&title={}", url_encode(&format!("[Cihaz] {name}")))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ")
}

/// 16 bytes per line with offsets, for a code block.
fn hex_block(bytes: &[u8]) -> String {
    let mut s = String::new();
    for (i, c) in bytes.chunks(16).enumerate() {
        let _ = writeln!(s, "{:04X}: {}", i * 16, hex(c));
    }
    s
}

fn usage_text(r: &hid::CapRow) -> String {
    if r.is_range && r.usage_min != r.usage_max {
        format!("{:04X}-{:04X}", r.usage_min, r.usage_max)
    } else {
        format!("{:04X}", r.usage_min)
    }
}

/// HidP capabilities as Markdown tables (enough to rebuild the report descriptor).
fn fmt_caps(c: &hid::CapsDump) -> String {
    let mut s = String::new();
    let _ = writeln!(
        s,
        "Giriş / çıkış / özellik rapor uzunluğu (rapor kimliği baytı dahil): `{}` / `{}` / `{}`  ",
        c.input_len, c.output_len, c.feature_len
    );
    let _ = writeln!(s, "Üst düzey koleksiyon: kullanım sayfası `{:04X}`, kullanım `{:04X}`\n", c.usage_page, c.usage);
    if c.rows.is_empty() {
        let _ = writeln!(s, "(Düğme / değer yeteneği yok)\n");
    } else {
        let _ = writeln!(s, "| Rapor | Kimlik | Tür | Kullanım sayfası | Kullanım | Bit | Sayı | Mantıksal min..maks | Fiziksel min..maks | Koleksiyon | Bayraklar |");
        let _ = writeln!(s, "|---|---|---|---|---|---|---|---|---|---|---|");
        for r in &c.rows {
            let mut flags = vec![if r.is_absolute { "mutlak" } else { "göreli" }];
            if r.has_null {
                flags.push("null");
            }
            if r.is_alias {
                flags.push("takma ad");
            }
            let units = if r.units != 0 { format!(", birim {:#X} üs {:#X}", r.units, r.units_exp) } else { String::new() };
            let _ = writeln!(
                s,
                "| {} | {:02X} | {} | {:04X} | {} | {} | {} | {}..{} | {}..{} | {} | {} (bitfield {:#06X}{units}) |",
                r.kind,
                r.report_id,
                if r.is_button { "düğme" } else { "değer" },
                r.usage_page,
                usage_text(r),
                r.bit_size,
                r.count,
                r.logical_min,
                r.logical_max,
                r.physical_min,
                r.physical_max,
                r.link,
                flags.join(", "),
                r.bitfield
            );
        }
        let _ = writeln!(s);
    }
    if !c.links.is_empty() {
        let _ = writeln!(s, "| Koleksiyon # | Kullanım sayfası/kullanım | Koleksiyon türü | Ebeveyn | Alt sayısı |");
        let _ = writeln!(s, "|---|---|---|---|---|");
        for (i, l) in c.links.iter().enumerate() {
            let _ = writeln!(
                s,
                "| {i} | {:04X}/{:04X} | {}{} | {} | {} |",
                l.usage_page,
                l.usage,
                l.collection_type,
                if l.is_alias { " (takma ad)" } else { "" },
                l.parent,
                l.children
            );
        }
        let _ = writeln!(s);
    }
    s
}

fn fmt_trace(entries: &[trace::Entry], dropped: usize) -> String {
    let mut s = String::new();
    for e in entries {
        let _ = writeln!(
            s,
            "+{:>5} ms  {:<3} {:<11} {:<26} {}  {}",
            e.t_ms,
            if e.dir == Dir::Out { "OUT" } else { "IN" },
            e.kind,
            e.iface,
            if e.ok { "ok  " } else { "FAIL" },
            hex(&e.bytes)
        );
    }
    if dropped > 0 {
        let _ = writeln!(s, "... {dropped} kayıt daha (üst sınır aşıldı)");
    }
    s
}

// ------------------------------------------------------------------ system facts

#[link(name = "ntdll")]
unsafe extern "system" {
    fn RtlGetVersion(info: *mut OSVERSIONINFOW) -> i32;
}

fn windows_version() -> String {
    let mut v = OSVERSIONINFOW { dwOSVersionInfoSize: size_of::<OSVERSIONINFOW>() as u32, ..Default::default() };
    if unsafe { RtlGetVersion(&mut v) } == 0 {
        format!("{}.{}.{} (derleme {})", v.dwMajorVersion, v.dwMinorVersion, v.dwBuildNumber, v.dwBuildNumber)
    } else {
        "?".to_string()
    }
}

fn now_text() -> String {
    let t = unsafe { GetLocalTime() };
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond)
}

// ------------------------------------------------------------------ passive listen

#[derive(Default)]
struct Listened {
    /// (ms since the start of the listen, report bytes), capped.
    reports: Vec<(u32, Vec<u8>)>,
    total: usize,
    /// Why nothing could be listened to.
    error: Option<&'static str>,
}

/// Reads input reports of every given interface in parallel for [`LISTEN_MS`].
/// READ ONLY: opens handles and calls `read_timeout`, nothing is ever written.
fn listen(infos: &[HidInfo]) -> HashMap<String, Listened> {
    let start = Instant::now();
    let deadline = start + Duration::from_millis(LISTEN_MS);
    let mut out = HashMap::new();
    std::thread::scope(|scope| {
        let handles: Vec<_> = infos
            .iter()
            .filter_map(|info| {
                let h = std::thread::Builder::new()
                    .name("listen".into())
                    .stack_size(256 * 1024)
                    .spawn_scoped(scope, move || {
                        let mut l = Listened::default();
                        let Some(dev) = HidHandle::open(info) else {
                            l.error = Some("arayüz açılamadı");
                            return l;
                        };
                        if !dev.rw {
                            l.error = Some("yalnızca salt-okunur tanıtıcı açılabildi (sistem tarafından kullanılıyor)");
                            return l;
                        }
                        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
                            let ms = left.as_millis().clamp(1, 1000) as u32;
                            if let Some(rep) = dev.read_timeout(ms) {
                                l.total += 1;
                                if l.reports.len() < MAX_LISTEN_REPORTS {
                                    l.reports.push((start.elapsed().as_millis() as u32, rep));
                                }
                            }
                        }
                        l
                    })
                    .ok()?;
                Some((info.path.clone(), h))
            })
            .collect();
        for (path, h) in handles {
            if let Ok(l) = h.join() {
                out.insert(path, l);
            }
        }
    });
    out
}

fn fmt_listen(infos: &[&HidInfo], results: &HashMap<String, Listened>, skipped: bool) -> String {
    let mut s = String::new();
    for i in infos {
        let _ = writeln!(
            s,
            "**mi={} col={} kullanım sayfası {:04X}/{:04X}** (giriş raporu {} bayt)",
            i.interface.map_or("--".into(), |m| format!("{m:02X}")),
            i.collection.map_or("--".into(), |m| format!("{m:02X}")),
            i.usage_page,
            i.usage,
            i.input_len
        );
        match results.get(&i.path) {
            None if skipped => {
                let _ = writeln!(s, "\nAtlandı (çok fazla arayüz).\n");
            }
            None => {
                let _ = writeln!(s, "\nDinlenemedi.\n");
            }
            Some(l) if l.error.is_some() => {
                let _ = writeln!(s, "\nDinlenemedi: {}.\n", l.error.unwrap_or_default());
            }
            Some(l) if l.total == 0 => {
                let _ = writeln!(s, "\n{} sn içinde rapor gelmedi.\n", LISTEN_MS / 1000);
            }
            Some(l) => {
                let _ = writeln!(s, "\n{} rapor geldi{}:\n\n```", l.total, if l.total > l.reports.len() { " (ilk kısmı gösteriliyor)" } else { "" });
                for (t, rep) in &l.reports {
                    let _ = writeln!(s, "+{t:>5} ms  {}", hex(rep));
                }
                let _ = writeln!(s, "```\n");
            }
        }
    }
    s
}

// ------------------------------------------------------------------ the report

fn state_lines(states: &[DeviceState]) -> String {
    let mut s = String::new();
    for d in states {
        let _ = writeln!(
            s,
            "- **{}** [{} / {}]: pil {}, şarj {}, {}{}",
            d.name,
            d.kind.label(),
            d.connection.label(),
            d.battery.map_or("-".into(), |b| format!("%{b}")),
            d.charging.map_or("-".into(), |c| if c { "evet".to_string() } else { "hayır".to_string() }),
            d.polling_hz.map_or("Hz yok".into(), |h| format!(
                "{h} Hz ({})",
                match d.hz_source {
                    crate::devices::HzSource::Driver => "sürücü",
                    crate::devices::HzSource::UsbDescriptor => "USB tanımlayıcısı",
                }
            )),
            if d.stale { ", değerler eski (okuma başarısız)" } else { "" }
        );
    }
    s
}

pub fn scrubber_for(phys: &[Physical]) -> Scrubber {
    let mut sc = Scrubber::new();
    for (var, repl) in [("USERNAME", "<kullanici>"), ("COMPUTERNAME", "<bilgisayar>")] {
        if let Ok(v) = std::env::var(var) {
            sc.add_replace(&v, repl);
        }
    }
    if let Ok(p) = std::env::var("USERPROFILE") {
        sc.add_replace(&p, "%USERPROFILE%");
        sc.add_replace(&p.replace('\\', "/"), "%USERPROFILE%");
    }
    for p in phys {
        sc.add_serial(&p.serial);
        for i in &p.interfaces {
            sc.add_serial(&i.serial);
        }
        if let Some(c) = &p.container {
            sc.add_container(c);
        }
        // User-set names: pairing name -> a product string if there is one.
        let product = p.interfaces.iter().map(|i| i.product.as_str()).find(|s| !s.is_empty());
        for name in p.bt.iter().filter_map(|b| b.name.as_deref()) {
            if Some(name) != product {
                sc.add_replace(name, product.unwrap_or("<BT cihaz>"));
            }
        }
    }
    sc
}

/// Returns (report text, device name for the issue title, the scrubber used).
fn build() -> (String, String, Scrubber) {
    let phys = enumerate::enumerate();
    let bt_nodes = bt::nodes();
    let mut sc = scrubber_for(&phys);
    let (connected, away): (Vec<&Physical>, Vec<&Physical>) = phys.iter().partition(|p| p.is_connected());

    // Passive listen over every readable vendor collection, once for all devices.
    let mut listen_infos: Vec<HidInfo> = connected
        .iter()
        .flat_map(|p| p.interfaces.iter())
        .filter(|i| i.is_vendor() && i.input_len > 0)
        .cloned()
        .collect();
    let skipped = listen_infos.len() > MAX_LISTEN_INTERFACES;
    listen_infos.truncate(MAX_LISTEN_INTERFACES);
    // The device reads first (they may wake a device), the listen follows.
    let mut title_name: Option<String> = None;
    let mut fallback_name: Option<String> = None;
    let mut per_device: Vec<(usize, String, String, String)> = Vec::new(); // (n, text, trace, bt)
    for (n, p) in connected.iter().enumerate() {
        let mut head = String::new();
        trace::begin();
        let d = describe::describe_device(&mut head, n + 1, p, true);
        let (entries, dropped) = trace::end();

        let mut text = head;
        let _ = writeln!(text, "#### Panelde görünen ve test listesi\n");
        text.push_str(&state_lines(&d.states));
        let wanted = d.states.iter().find_map(|s| s.wanted);
        match wanted {
            Some(w) => {
                let _ = writeln!(text, "\nTest listesinde mi: {}", w.explain());
                if title_name.is_none() {
                    title_name = d.states.iter().find(|s| s.wanted.is_some()).map(|s| s.name.clone());
                }
            }
            None => {
                let _ = writeln!(text, "\nTest listesinde mi: Hayır.");
            }
        }
        let _ = writeln!(text, "Eşleşen sürücü: `{}`\n", d.driver.unwrap_or("yok"));
        if fallback_name.is_none() && d.states.first().is_some_and(|s| s.kind != crate::devices::Kind::Other) {
            fallback_name = d.states.first().map(|s| s.name.clone());
        }

        let _ = writeln!(text, "#### HID arayüzleri (HidP yetenekleri)\n");
        for i in &p.interfaces {
            let _ = writeln!(
                text,
                "##### mi={} col={} — {:04X}:{:04X}, kullanım sayfası {:04X}/{:04X}\n",
                i.interface.map_or("--".into(), |m| format!("{m:02X}")),
                i.collection.map_or("--".into(), |m| format!("{m:02X}")),
                i.vid,
                i.pid,
                i.usage_page,
                i.usage
            );
            match hid::caps_dump(&i.path) {
                Some(c) => text.push_str(&fmt_caps(&c)),
                None => text.push_str("(Yetenekler okunamadı)\n\n"),
            }
        }

        if let Some(usb) = &p.usb_device_id {
            let _ = writeln!(text, "#### USB\n");
            match usb_desc::dump(usb) {
                Some(u) => {
                    sc.add_serial(u.serial.as_deref().unwrap_or(""));
                    let _ = writeln!(text, "- hız: `{:?}`", u.speed);
                    let _ = writeln!(text, "- üretici: `{}`", u.manufacturer.as_deref().unwrap_or("-"));
                    let _ = writeln!(text, "- ürün: `{}`", u.product.as_deref().unwrap_or("-"));
                    let _ = writeln!(text, "- seri no: `{}`", u.serial.as_deref().unwrap_or("-"));
                    let _ = writeln!(text, "\nAygıt tanımlayıcısı (18 bayt):\n\n```\n{}```\n", hex_block(&u.device));
                    let _ = writeln!(text, "Yapılandırma tanımlayıcısı ({} bayt):\n\n```\n{}```\n", u.config.len(), hex_block(&u.config));
                    for (iface, desc) in &u.report_descriptors {
                        let _ = writeln!(text, "HID rapor tanımlayıcısı, arayüz {iface} ({} bayt):\n\n```\n{}```\n", desc.len(), hex_block(desc));
                    }
                    if u.report_descriptors.is_empty() {
                        let _ = writeln!(text, "(HID rapor tanımlayıcısı hub üzerinden okunamadı; HidP tablolarından kurulabilir.)\n");
                    }
                }
                None => {
                    let _ = writeln!(text, "(USB tanımlayıcıları okunamadı)\n");
                }
            }
        }

        let tr = if entries.is_empty() && dropped == 0 {
            "Sürücü bu okumada cihazla hiç rapor alışverişi yapmadı.\n".to_string()
        } else {
            format!("```\n{}```\n", fmt_trace(&entries, dropped))
        };
        let nodes: Vec<&bt::BtNode> = bt_nodes.iter().filter(|nd| nd.container.is_some() && nd.container == p.container).collect();
        let mut btt = String::new();
        if p.bus == Bus::Bluetooth || p.bt.as_ref().is_some_and(|b| b.name.is_some() || b.battery.is_some()) {
            if let Some(b) = &p.bt {
                let _ = writeln!(
                    btt,
                    "- Windows'un birleştirdiği değer: pil {}, bağlı {}, ad `{}`",
                    b.battery.map_or("-".into(), |v| format!("%{v}")),
                    b.connected.map_or("-".into(), |v| v.to_string()),
                    b.name.as_deref().unwrap_or("-")
                );
            }
            for nd in &nodes {
                let _ = writeln!(
                    btt,
                    "- `{}`: pil {}, bağlı {}, ad `{}`",
                    nd.instance_id,
                    nd.battery.map_or("-".into(), |v| format!("%{v}")),
                    nd.connected.map_or("-".into(), |v| v.to_string()),
                    nd.name.as_deref().unwrap_or("-")
                );
            }
        }
        per_device.push((n, text, tr, btt));
    }

    // Passive listen, then assemble.
    let results = listen(&listen_infos);
    let mut out = String::new();
    let _ = writeln!(out, "# Bilgi Paneli cihaz raporu\n");
    let _ = writeln!(out, "- Uygulama sürümü: `{}`", env!("CARGO_PKG_VERSION"));
    let _ = writeln!(out, "- Windows: `{}`", windows_version());
    let _ = writeln!(out, "- Tarih: `{}`", now_text());
    let _ = writeln!(
        out,
        "- Gizlilik: seri numaraları, Bluetooth/MAC adresleri ve ContainerId değerleri kısa bir özetle (hash) değiştirildi; kullanıcı/bilgisayar adları ve Bluetooth eşleştirme adları çıkarıldı. Dosya yalnızca bu bilgisayarda oluşturuldu."
    );
    let _ = writeln!(out, "- Bağlı cihaz sayısı: {} (bağlı olmayan {} Bluetooth cihazı atlandı)\n", connected.len(), away.len());
    for (n, text, tr, btt) in per_device {
        out.push_str(&text);
        let _ = writeln!(out, "#### Sürücü izi (bu okumada gidip gelen baytlar)\n");
        out.push_str(&tr);
        out.push('\n');
        let p = connected[n];
        let vend: Vec<&HidInfo> = p.interfaces.iter().filter(|i| i.is_vendor() && i.input_len > 0).collect();
        if !vend.is_empty() {
            let _ = writeln!(out, "#### Pasif dinleme ({} sn, yalnızca okuma)\n", LISTEN_MS / 1000);
            out.push_str(&fmt_listen(&vend, &results, skipped));
        }
        if !btt.is_empty() {
            let _ = writeln!(out, "#### Bluetooth\n");
            out.push_str(&btt);
            out.push('\n');
        }
    }
    let _ = writeln!(out, "## Kullanıcı notları\n");
    let _ = writeln!(out, "Üretici yazılımında (G Hub, Synapse, web sürücüsü vb.) görünen değerleri yazın:\n");
    let _ = writeln!(out, "- Üretici yazılımındaki pil %:\n- Şarj durumu (şarj oluyor / olmuyor):\n- Hz:\n- Dinleme sırasında yaptığım şey (örn. fareyi oynattım, tuşlara bastım):\n");

    let name = title_name.or(fallback_name).unwrap_or_else(|| "Cihaz".to_string());
    let (out, name) = (sc.apply(&out), sc.apply(&name));
    (out, name, sc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_encoding() {
        assert_eq!(url_encode("abc-._~XYZ019"), "abc-._~XYZ019");
        assert_eq!(url_encode("[Cihaz] Lamzu Atlantis"), "%5BCihaz%5D%20Lamzu%20Atlantis");
        assert_eq!(url_encode("Işık&x=1"), "I%C5%9F%C4%B1k%26x%3D1");
    }

    #[test]
    fn issue_url_has_encoded_title() {
        let u = issue_url("Foo Mouse");
        assert!(u.starts_with("https://github.com/Abrahamexe/bilgi-paneli-raporlar/issues/new?template=cihaz-raporu.yml&title="));
        assert!(u.ends_with("title=%5BCihaz%5D%20Foo%20Mouse"));
        // Long names are cut.
        assert!(issue_url(&"x".repeat(200)).len() < ISSUE_URL.len() + 120);
    }

    #[test]
    fn hex_formats() {
        assert_eq!(hex(&[0, 0xAB, 0x0F]), "00 AB 0F");
        let b: Vec<u8> = (0..20).collect();
        assert_eq!(hex_block(&b), "0000: 00 01 02 03 04 05 06 07 08 09 0A 0B 0C 0D 0E 0F\n0010: 10 11 12 13\n");
    }

    #[test]
    fn trace_lines_are_one_per_entry() {
        let e = trace::Entry { t_ms: 12, dir: Dir::Out, kind: "SET_FEATURE", iface: "mi=01 page FF00/0001".into(), ok: true, bytes: vec![1, 2] };
        let s = fmt_trace(&[e], 3);
        assert_eq!(s.lines().count(), 2);
        assert!(s.contains("OUT SET_FEATURE") && s.contains("01 02") && s.contains("3 kayıt"));
    }

    #[test]
    fn caps_table_lists_every_row() {
        let c = hid::CapsDump {
            usage_page: 0xFF00,
            usage: 1,
            input_len: 65,
            rows: vec![hid::CapRow { kind: "Input", report_id: 1, usage_page: 0xFF00, usage_min: 1, usage_max: 1, bit_size: 8, count: 64, logical_max: 255, is_absolute: true, ..Default::default() }],
            links: vec![hid::LinkNode { usage_page: 0xFF00, usage: 1, collection_type: 1, ..Default::default() }],
            ..Default::default()
        };
        let s = fmt_caps(&c);
        assert!(s.contains("| Input | 01 | değer | FF00 | 0001 | 8 | 64 | 0..255 |"), "{s}");
        assert!(s.contains("| 0 | FF00/0001 | 1 |"));
    }
}
