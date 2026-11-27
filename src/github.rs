//! Direct report sending: GitHub App user-to-server token (OAuth Device Flow)
//! kept in Windows Credential Manager, and the issue POST over WinHTTP.
//!
//! Nothing here runs unless the user presses "Gönder"; every function blocks
//! and belongs on a worker thread. The WinHTTP session lives for one request.
//! No secrets are compiled in: the Client ID is public by design.

use std::fmt::Write as _;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use windows::Win32::Foundation::{HANDLE, WAIT_OBJECT_0};
use windows::Win32::Networking::WinHttp::{
    INTERNET_DEFAULT_HTTPS_PORT, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE, WINHTTP_QUERY_FLAG_NUMBER,
    WINHTTP_QUERY_STATUS_CODE, WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpReadData,
    WinHttpReceiveResponse, WinHttpQueryHeaders, WinHttpSendRequest, WinHttpSetTimeouts,
};
use windows::Win32::Security::Credentials::{
    CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CredDeleteW, CredFree, CredReadW, CredWriteW,
};
use windows::Win32::System::Threading::{CreateEventW, SetEvent, WaitForSingleObject};
use windows::core::{HSTRING, PCWSTR, PWSTR};

use crate::report::url_encode;

/// GitHub App -> Client ID (public; Settings > Developer settings > GitHub Apps).
/// While empty, "Rapor gönder" falls back to the browser issue form.
pub const GITHUB_CLIENT_ID: &str = "Iv23liG3fMmOP9ljlBzS";

/// Public repo the reports go to.
pub const REPO: &str = "Abrahamexe/bilgi-paneli-raporlar";
/// Credential Manager target name.
const CRED_TARGET: &str = "BilgiPaneli/github";
/// GitHub allows 65 536 characters per issue body; stay below.
pub const MAX_BODY_CHARS: usize = 60_000;
const MAX_RESPONSE: usize = 512 * 1024;
const TIMEOUT_MS: i32 = 15_000;

// ------------------------------------------------------------------ JSON (minimal)

/// Escapes `s` for use inside a JSON string literal (without the quotes).
pub fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 16);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}

/// Parses a JSON string literal starting at `b[i] == '"'`; returns it and the index after it.
fn parse_json_string(b: &[u8], mut i: usize) -> Option<(String, usize)> {
    let mut out: Vec<u8> = Vec::new();
    i += 1;
    while i < b.len() {
        match b[i] {
            b'"' => return Some((String::from_utf8_lossy(&out).into_owned(), i + 1)),
            b'\\' => {
                i += 1;
                match *b.get(i)? {
                    b'n' => out.push(b'\n'),
                    b'r' => out.push(b'\r'),
                    b't' => out.push(b'\t'),
                    b'b' => out.push(8),
                    b'f' => out.push(12),
                    b'u' => {
                        let hex4 = |at: usize| -> Option<u32> {
                            u32::from_str_radix(std::str::from_utf8(b.get(at..at + 4)?).ok()?, 16).ok()
                        };
                        let mut cp = hex4(i + 1)?;
                        i += 4;
                        if (0xD800..0xDC00).contains(&cp)
                            && b.get(i + 1) == Some(&b'\\')
                            && b.get(i + 2) == Some(&b'u')
                            && let Some(lo) = hex4(i + 3).filter(|l| (0xDC00..0xE000).contains(l))
                        {
                            cp = 0x10000 + ((cp - 0xD800) << 10) + (lo - 0xDC00);
                            i += 6;
                        }
                        let ch = char::from_u32(cp).unwrap_or('\u{FFFD}');
                        let mut tmp = [0u8; 4];
                        out.extend_from_slice(ch.encode_utf8(&mut tmp).as_bytes());
                    }
                    other => out.push(other), // \" \\ \/
                }
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    None
}

/// Value of a top-level key of a JSON object: strings unescaped, numbers /
/// booleans / null as their raw text. Nested objects and arrays are skipped.
pub fn json_get(body: &str, key: &str) -> Option<String> {
    let b = body.as_bytes();
    let mut depth = 0usize;
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'{' | b'[' => {
                depth += 1;
                i += 1;
            }
            b'}' | b']' => {
                depth = depth.saturating_sub(1);
                i += 1;
            }
            b'"' => {
                let (s, next) = parse_json_string(b, i)?;
                let mut j = next;
                while b.get(j).is_some_and(u8::is_ascii_whitespace) {
                    j += 1;
                }
                if depth == 1 && s == key && b.get(j) == Some(&b':') {
                    j += 1;
                    while b.get(j).is_some_and(u8::is_ascii_whitespace) {
                        j += 1;
                    }
                    return match b.get(j)? {
                        b'"' => parse_json_string(b, j).map(|(v, _)| v),
                        b'{' | b'[' => None,
                        _ => {
                            let end = (j..b.len()).find(|&k| matches!(b[k], b',' | b'}' | b']' | b' ' | b'\n' | b'\r'))?;
                            Some(body[j..end].to_string())
                        }
                    };
                }
                i = next;
            }
            _ => i += 1,
        }
    }
    None
}

// ------------------------------------------------------------------ WinHTTP

pub struct Response {
    pub status: u16,
    pub body: String,
}

struct Inet(*mut core::ffi::c_void);
impl Drop for Inet {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = WinHttpCloseHandle(self.0);
            }
        }
    }
}

fn net_err(what: &str, e: windows::core::Error) -> String {
    format!("{what} ({:#010X})", e.code().0 as u32)
}

/// One HTTPS POST to `host` (port 443). Session, connection and request are
/// created here and closed on return.
pub fn post(host: &str, path: &str, headers: &[(&str, &str)], body: &[u8]) -> Result<Response, String> {
    unsafe {
        let agent = HSTRING::from(format!("BilgiPaneli/{}", env!("CARGO_PKG_VERSION")));
        let session = Inet(WinHttpOpen(&agent, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, PCWSTR::null(), PCWSTR::null(), 0));
        if session.0.is_null() {
            return Err("İnternet bağlantısı başlatılamadı".into());
        }
        WinHttpSetTimeouts(session.0, TIMEOUT_MS, TIMEOUT_MS, TIMEOUT_MS, TIMEOUT_MS).map_err(|e| net_err("Zaman aşımı ayarlanamadı", e))?;
        let conn = Inet(WinHttpConnect(session.0, &HSTRING::from(host), INTERNET_DEFAULT_HTTPS_PORT, 0));
        if conn.0.is_null() {
            return Err("Sunucuya bağlanılamadı".into());
        }
        let req = Inet(WinHttpOpenRequest(
            conn.0,
            windows::core::w!("POST"),
            &HSTRING::from(path),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            WINHTTP_FLAG_SECURE,
        ));
        if req.0.is_null() {
            return Err("İstek oluşturulamadı".into());
        }
        let mut hdr = String::new();
        for (k, v) in headers {
            let _ = write!(hdr, "{k}: {v}\r\n");
        }
        let hdr_w: Vec<u16> = hdr.encode_utf16().collect();
        let len = body.len() as u32;
        let opt = if body.is_empty() { None } else { Some(body.as_ptr().cast()) };
        WinHttpSendRequest(req.0, Some(&hdr_w), opt, len, len, 0).map_err(|e| net_err("İstek gönderilemedi (bağlantıyı kontrol edin)", e))?;
        WinHttpReceiveResponse(req.0, std::ptr::null_mut()).map_err(|e| net_err("Yanıt alınamadı", e))?;

        let mut status = 0u32;
        let mut size = 4u32;
        WinHttpQueryHeaders(
            req.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some(&mut status as *mut u32 as *mut _),
            &mut size,
            std::ptr::null_mut(),
        )
        .map_err(|e| net_err("Durum kodu okunamadı", e))?;

        let mut data: Vec<u8> = Vec::new();
        let mut buf = [0u8; 8192];
        while data.len() < MAX_RESPONSE {
            let mut got = 0u32;
            if WinHttpReadData(req.0, buf.as_mut_ptr().cast(), buf.len() as u32, &mut got).is_err() || got == 0 {
                break;
            }
            data.extend_from_slice(&buf[..got as usize]);
        }
        Ok(Response { status: status as u16, body: String::from_utf8_lossy(&data).into_owned() })
    }
}

fn post_form(host: &str, path: &str, form: &str) -> Result<Response, String> {
    post(
        host,
        path,
        &[("Accept", "application/json"), ("Content-Type", "application/x-www-form-urlencoded")],
        form.as_bytes(),
    )
}

// ------------------------------------------------------------------ cancel event

/// Manual-reset event: wakes sleeping workers when the dialog closes.
pub struct Cancel(isize);

impl Cancel {
    pub fn new() -> Arc<Cancel> {
        let h = unsafe { CreateEventW(None, true, false, PCWSTR::null()) }.map(|h| h.0 as isize).unwrap_or(0);
        Arc::new(Cancel(h))
    }

    pub fn cancel(&self) {
        if self.0 != 0 {
            unsafe {
                let _ = SetEvent(HANDLE(self.0 as *mut _));
            }
        }
    }

    pub fn is_set(&self) -> bool {
        self.wait(0)
    }

    /// Sleeps up to `ms`; true if cancelled meanwhile.
    pub fn wait(&self, ms: u64) -> bool {
        if self.0 == 0 {
            std::thread::sleep(Duration::from_millis(ms));
            return false;
        }
        unsafe { WaitForSingleObject(HANDLE(self.0 as *mut _), ms.min(u32::MAX as u64 - 1) as u32) == WAIT_OBJECT_0 }
    }
}

impl Drop for Cancel {
    fn drop(&mut self) {
        if self.0 != 0 {
            unsafe {
                let _ = windows::Win32::Foundation::CloseHandle(HANDLE(self.0 as *mut _));
            }
        }
    }
}

// ------------------------------------------------------------------ tokens + Credential Manager

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tokens {
    pub access: String,
    pub refresh: Option<String>,
    /// Unix seconds; 0 = does not expire.
    pub expires_at: u64,
}

impl Tokens {
    pub fn valid_at(&self, now: u64) -> bool {
        !self.access.is_empty() && (self.expires_at == 0 || now + 60 < self.expires_at)
    }

    fn encode(&self) -> String {
        format!("v1\n{}\n{}\n{}", self.access, self.refresh.as_deref().unwrap_or(""), self.expires_at)
    }

    fn decode(s: &str) -> Option<Tokens> {
        let mut l = s.split('\n');
        if l.next()? != "v1" {
            return None;
        }
        let access = l.next()?.to_string();
        let refresh = Some(l.next()?.to_string()).filter(|r| !r.is_empty());
        let expires_at = l.next()?.trim().parse().ok()?;
        (!access.is_empty()).then_some(Tokens { access, refresh, expires_at })
    }
}

fn now_unix() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

pub fn load_tokens() -> Option<Tokens> {
    let target = HSTRING::from(CRED_TARGET);
    let mut p: *mut CREDENTIALW = std::ptr::null_mut();
    unsafe {
        CredReadW(&target, CRED_TYPE_GENERIC, None, &mut p).ok()?;
        let c = &*p;
        let blob = if c.CredentialBlob.is_null() { &[][..] } else { std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize) };
        let t = Tokens::decode(&String::from_utf8_lossy(blob));
        CredFree(p.cast());
        t
    }
}

pub fn has_credential() -> bool {
    let target = HSTRING::from(CRED_TARGET);
    let mut p: *mut CREDENTIALW = std::ptr::null_mut();
    unsafe {
        let ok = CredReadW(&target, CRED_TYPE_GENERIC, None, &mut p).is_ok();
        if ok {
            CredFree(p.cast());
        }
        ok
    }
}

fn store_tokens(t: &Tokens) {
    let target = HSTRING::from(CRED_TARGET);
    let mut blob = t.encode().into_bytes();
    let cred = CREDENTIALW {
        Type: CRED_TYPE_GENERIC,
        TargetName: PWSTR(target.as_ptr() as *mut u16),
        CredentialBlobSize: blob.len() as u32,
        CredentialBlob: blob.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        ..Default::default()
    };
    unsafe {
        let _ = CredWriteW(&cred, 0);
    }
}

pub fn delete_tokens() {
    unsafe {
        let _ = CredDeleteW(&HSTRING::from(CRED_TARGET), CRED_TYPE_GENERIC, None);
    }
}

// ------------------------------------------------------------------ device flow

#[derive(Debug)]
pub enum Fail {
    Cancelled,
    /// 401, or GitHub rejected the (refresh) token.
    Unauthorized,
    /// The GitHub App is not installed on the repo.
    NotInstalled,
    Other(String),
}

#[derive(Debug, PartialEq, Eq)]
pub struct DeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub interval: u64,
    pub expires_in: u64,
}

fn json_error_text(body: &str) -> String {
    let e = json_get(body, "error").or_else(|| json_get(body, "message")).unwrap_or_default();
    match json_get(body, "error_description") {
        Some(d) if !d.is_empty() => format!("{e}: {d}"),
        _ => e,
    }
}

pub fn parse_device_code(body: &str) -> Result<DeviceCode, String> {
    let get = |k: &str| json_get(body, k).filter(|v| !v.is_empty());
    match (get("device_code"), get("user_code"), get("verification_uri")) {
        (Some(device_code), Some(user_code), Some(verification_uri)) => Ok(DeviceCode {
            device_code,
            user_code,
            verification_uri,
            interval: get("interval").and_then(|v| v.parse().ok()).unwrap_or(5),
            expires_in: get("expires_in").and_then(|v| v.parse().ok()).unwrap_or(900),
        }),
        _ => Err(json_error_text(body)),
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Poll {
    Token(Tokens),
    Pending,
    SlowDown(u64),
    Failed(String),
}

pub fn parse_token_response(body: &str, now: u64) -> Poll {
    if let Some(access) = json_get(body, "access_token").filter(|a| !a.is_empty()) {
        let expires_at = json_get(body, "expires_in").and_then(|v| v.parse::<u64>().ok()).map_or(0, |s| now + s);
        return Poll::Token(Tokens { access, refresh: json_get(body, "refresh_token").filter(|r| !r.is_empty()), expires_at });
    }
    match json_get(body, "error").as_deref() {
        Some("authorization_pending") => Poll::Pending,
        Some("slow_down") => Poll::SlowDown(json_get(body, "interval").and_then(|v| v.parse().ok()).unwrap_or(0)),
        Some("expired_token") => Poll::Failed("Kodun süresi doldu. Lütfen yeniden deneyin.".into()),
        Some("access_denied") => Poll::Failed("GitHub'da izin verilmedi.".into()),
        Some("device_flow_disabled") => Poll::Failed("GitHub uygulamasında cihaz akışı etkin değil.".into()),
        _ => Poll::Failed(format!("GitHub hatası: {}", json_error_text(body))),
    }
}

/// UI hooks for a running flow.
pub struct Flow<'a> {
    pub cancel: &'a Cancel,
    /// Called once with the code to show (the caller copies it and opens the page).
    pub on_code: &'a dyn Fn(&DeviceCode),
}

fn request_device_code() -> Result<DeviceCode, Fail> {
    let r = post_form("github.com", "/login/device/code", &format!("client_id={}", url_encode(GITHUB_CLIENT_ID)))
        .map_err(|e| Fail::Other(format!("GitHub'a ulaşılamadı: {e}")))?;
    parse_device_code(&r.body).map_err(|e| Fail::Other(format!("GitHub kod vermedi ({}): {e}", r.status)))
}

fn poll_for_token(dc: &DeviceCode, cancel: &Cancel) -> Result<Tokens, Fail> {
    let deadline = Instant::now() + Duration::from_secs(dc.expires_in.clamp(60, 1800));
    let mut interval = dc.interval.max(1);
    let mut net_errors = 0;
    let form = format!(
        "client_id={}&device_code={}&grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code",
        url_encode(GITHUB_CLIENT_ID),
        url_encode(&dc.device_code)
    );
    loop {
        if cancel.wait(interval * 1000) {
            return Err(Fail::Cancelled);
        }
        if Instant::now() > deadline {
            return Err(Fail::Other("Kodun süresi doldu. Lütfen yeniden deneyin.".into()));
        }
        match post_form("github.com", "/login/oauth/access_token", &form) {
            Err(e) => {
                net_errors += 1;
                if net_errors >= 3 {
                    return Err(Fail::Other(format!("GitHub'a ulaşılamadı: {e}")));
                }
            }
            Ok(r) => {
                net_errors = 0;
                match parse_token_response(&r.body, now_unix()) {
                    Poll::Token(t) => return Ok(t),
                    Poll::Pending => {}
                    Poll::SlowDown(n) => interval = if n > interval { n } else { interval + 5 },
                    Poll::Failed(m) => return Err(Fail::Other(m)),
                }
            }
        }
    }
}

fn device_flow(flow: &Flow) -> Result<Tokens, Fail> {
    let dc = request_device_code()?;
    (flow.on_code)(&dc);
    poll_for_token(&dc, flow.cancel)
}

/// Exchanges the refresh token. `Unauthorized` = GitHub refused it.
fn refresh(t: &Tokens) -> Result<Tokens, Fail> {
    let Some(rt) = &t.refresh else { return Err(Fail::Unauthorized) };
    let form = format!(
        "client_id={}&grant_type=refresh_token&refresh_token={}",
        url_encode(GITHUB_CLIENT_ID),
        url_encode(rt)
    );
    let r = post_form("github.com", "/login/oauth/access_token", &form)
        .map_err(|e| Fail::Other(format!("GitHub'a ulaşılamadı: {e}")))?;
    match parse_token_response(&r.body, now_unix()) {
        Poll::Token(n) => Ok(n),
        _ => Err(Fail::Unauthorized),
    }
}

/// A usable access token: stored, refreshed, or from a fresh device flow.
pub fn ensure_token(flow: &Flow) -> Result<Tokens, Fail> {
    if let Some(t) = load_tokens() {
        if t.valid_at(now_unix()) {
            return Ok(t);
        }
        match refresh(&t) {
            Ok(n) => {
                store_tokens(&n);
                return Ok(n);
            }
            Err(Fail::Unauthorized) => delete_tokens(),
            Err(e) => return Err(e),
        }
    }
    let t = device_flow(flow)?;
    store_tokens(&t);
    Ok(t)
}

// ------------------------------------------------------------------ issue

/// POSTs the issue; returns its `html_url`.
pub fn create_issue(token: &str, title: &str, body: &str) -> Result<String, Fail> {
    let json = format!("{{\"title\":\"{}\",\"body\":\"{}\"}}", json_escape(title), json_escape(body));
    let auth = format!("Bearer {token}");
    let r = post(
        "api.github.com",
        &format!("/repos/{REPO}/issues"),
        &[
            ("Accept", "application/vnd.github+json"),
            ("X-GitHub-Api-Version", "2022-11-28"),
            ("Authorization", &auth),
            ("Content-Type", "application/json; charset=utf-8"),
        ],
        json.as_bytes(),
    )
    .map_err(|e| Fail::Other(format!("GitHub'a ulaşılamadı: {e}")))?;
    classify_issue_response(r.status, &r.body)
}

pub fn classify_issue_response(status: u16, body: &str) -> Result<String, Fail> {
    let message = json_get(body, "message").unwrap_or_default();
    match status {
        200..=299 => json_get(body, "html_url").ok_or_else(|| Fail::Other("GitHub yanıtında bağlantı yok".into())),
        401 => Err(Fail::Unauthorized),
        403 | 404 if message.contains("Resource not accessible by integration") => Err(Fail::NotInstalled),
        _ => {
            let short: String = message.chars().take(160).collect();
            Err(Fail::Other(format!("GitHub {status}: {short}")))
        }
    }
}

// ------------------------------------------------------------------ issue text

/// What the user typed plus what the panel showed, all already anonymized.
#[derive(Default, Clone, Debug)]
pub struct IssueFields {
    pub device: String,
    pub vendor_battery: String,
    pub vendor_charging: String,
    pub vendor_hz: String,
    pub note: String,
    pub panel_battery: String,
    pub panel_charging: String,
    pub panel_hz: String,
}

fn cell(s: &str) -> String {
    let one: String = s.split_whitespace().collect::<Vec<_>>().join(" ").replace('|', "\\|");
    let one: String = one.chars().take(80).collect();
    if one.is_empty() { "-".into() } else { one }
}

pub fn issue_title(device: &str) -> String {
    let name: String = device.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(60).collect();
    format!("[Cihaz] {name}")
}

pub fn issue_header(f: &IssueFields) -> String {
    let mut s = String::from("## Kullanıcı bildirimi\n\n");
    let _ = writeln!(s, "Cihaz: **{}**\n", cell(&f.device));
    let _ = writeln!(s, "| | Üretici yazılımında | Bilgi Paneli'nde |\n|---|---|---|");
    let _ = writeln!(s, "| Pil | {} | {} |", cell(&f.vendor_battery), cell(&f.panel_battery));
    let _ = writeln!(s, "| Şarj | {} | {} |", cell(&f.vendor_charging), cell(&f.panel_charging));
    let _ = writeln!(s, "| Hz | {} | {} |", cell(&f.vendor_hz), cell(&f.panel_hz));
    let note = f.note.trim();
    if !note.is_empty() {
        let _ = writeln!(s, "\n**Not:** {}", note.replace("\r\n", "\n"));
    }
    s
}

/// The report without its empty "Kullanıcı notları" template (the header replaces it).
fn without_user_notes(report: &str) -> &str {
    report.find("## Kullanıcı notları").map_or(report, |i| report[..i].trim_end())
}

/// Replaces every "#### Pasif dinleme" section with a short note.
fn drop_listen(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let (mut skipping, mut fence) = (false, false);
    for line in text.split_inclusive('\n') {
        let t = line.trim_end();
        if t.starts_with("```") {
            fence = !fence;
        }
        if !fence && t.starts_with("#### Pasif dinleme") {
            skipping = true;
            out.push_str("#### Pasif dinleme\n\n(kısaltıldı: boyut sınırı nedeniyle çıkarıldı)\n\n");
            continue;
        }
        if skipping {
            if !fence && t.starts_with('#') {
                skipping = false;
            } else {
                continue;
            }
        }
        out.push_str(line);
    }
    out
}

/// Cuts every fenced block to `max_lines` lines.
fn truncate_blocks(text: &str, max_lines: usize) -> String {
    let mut out = String::with_capacity(text.len());
    let (mut fence, mut n, mut skipped) = (false, 0usize, 0usize);
    for line in text.split_inclusive('\n') {
        if line.trim_end().starts_with("```") {
            if fence && skipped > 0 {
                let _ = writeln!(out, "(kısaltıldı: {skipped} satır daha)");
            }
            fence = !fence;
            (n, skipped) = (0, 0);
            out.push_str(line);
        } else if fence {
            n += 1;
            if n <= max_lines {
                out.push_str(line);
            } else {
                skipped += 1;
            }
        } else {
            out.push_str(line);
        }
    }
    out
}

/// Issue body: header + report, at most `cap` characters. Too long: passive
/// listen dumps go first, then hex blocks are shortened, last the tail is cut.
/// The header and the identifiers (device lines, VID:PID) are never dropped.
pub fn build_body(f: &IssueFields, report: &str, cap: usize) -> String {
    let header = issue_header(f);
    let join = |r: &str| format!("{header}\n---\n\n{r}\n");
    let fits = |s: &str| s.chars().count() <= cap;
    let mut r = without_user_notes(report).to_string();
    let mut body = join(&r);
    if fits(&body) {
        return body;
    }
    r = drop_listen(&r);
    body = join(&r);
    for lines in [150, 80, 40, 20, 10, 5] {
        if fits(&body) {
            return body;
        }
        r = truncate_blocks(&r, lines);
        body = join(&r);
    }
    if fits(&body) {
        return body;
    }
    let mut cut: String = body.chars().take(cap.saturating_sub(40)).collect();
    cut.push_str("\n\n(kısaltıldı: boyut sınırı)\n");
    cut
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_json() {
        assert_eq!(json_escape("a\"b\\c\nd\re\tf\u{1}ş"), "a\\\"b\\\\c\\nd\\re\\tf\\u0001ş");
        assert_eq!(json_escape(""), "");
    }

    #[test]
    fn extracts_json_fields() {
        let b = r#"{"device_code":"3584d83530557fdd","user_code":"WDJB-MJHT","verification_uri":"https://github.com/login/device","expires_in":900,"interval":5}"#;
        assert_eq!(json_get(b, "user_code").as_deref(), Some("WDJB-MJHT"));
        assert_eq!(json_get(b, "interval").as_deref(), Some("5"));
        assert_eq!(json_get(b, "expires_in").as_deref(), Some("900"));
        assert_eq!(json_get(b, "nope"), None);
        // Nested keys and key-like string values do not match; escapes decode.
        let n = r#"{ "user": {"html_url": "inner"}, "x": "html_url", "html_url" : "https:\/\/x\/1ç😀\"q", "n": null }"#;
        assert_eq!(json_get(n, "html_url").as_deref(), Some("https://x/1ç😀\"q"));
        assert_eq!(json_get(n, "n").as_deref(), Some("null"));
        assert_eq!(json_get("not json", "a"), None);
        assert_eq!(json_get(r#"{"a":"unterminated"#, "a"), None);
    }

    #[test]
    fn json_roundtrip() {
        let s = "Satır 1\n\"tırnak\" \\ ışık\t😀 \u{2}";
        let doc = format!("{{\"k\":\"{}\"}}", json_escape(s));
        assert_eq!(json_get(&doc, "k").as_deref(), Some(s));
    }

    #[test]
    fn device_code_and_errors() {
        let ok = parse_device_code(r#"{"device_code":"d","user_code":"AB-CD","verification_uri":"u","expires_in":899,"interval":7}"#).unwrap();
        assert_eq!((ok.user_code.as_str(), ok.interval, ok.expires_in), ("AB-CD", 7, 899));
        let err = parse_device_code(r#"{"error":"Not Found"}"#).unwrap_err();
        assert_eq!(err, "Not Found");
        let err = parse_device_code(r#"{"error":"unauthorized_client","error_description":"bad id"}"#).unwrap_err();
        assert_eq!(err, "unauthorized_client: bad id");
    }

    #[test]
    fn token_responses() {
        let t = parse_token_response(r#"{"access_token":"ghu_x","expires_in":28800,"refresh_token":"ghr_y","token_type":"bearer"}"#, 1000);
        assert_eq!(t, Poll::Token(Tokens { access: "ghu_x".into(), refresh: Some("ghr_y".into()), expires_at: 29800 }));
        assert_eq!(parse_token_response(r#"{"error":"authorization_pending"}"#, 0), Poll::Pending);
        assert_eq!(parse_token_response(r#"{"error":"slow_down","interval":10}"#, 0), Poll::SlowDown(10));
        assert!(matches!(parse_token_response(r#"{"error":"expired_token"}"#, 0), Poll::Failed(m) if m.contains("süresi")));
        assert!(matches!(parse_token_response(r#"{"error":"access_denied"}"#, 0), Poll::Failed(m) if m.contains("izin")));
    }

    #[test]
    fn token_storage_format() {
        let t = Tokens { access: "a".into(), refresh: Some("r".into()), expires_at: 77 };
        assert_eq!(Tokens::decode(&t.encode()), Some(t.clone()));
        let t2 = Tokens { access: "a".into(), refresh: None, expires_at: 0 };
        assert_eq!(Tokens::decode(&t2.encode()), Some(t2.clone()));
        assert_eq!(Tokens::decode("garbage"), None);
        assert!(t.valid_at(10) && !t.valid_at(17) && t2.valid_at(u64::MAX / 2));
    }

    #[test]
    fn issue_responses() {
        assert_eq!(classify_issue_response(201, r#"{"id":1,"html_url":"https://github.com/o/r/issues/3"}"#).unwrap(), "https://github.com/o/r/issues/3");
        assert!(matches!(classify_issue_response(401, "{}"), Err(Fail::Unauthorized)));
        assert!(matches!(classify_issue_response(403, r#"{"message":"Resource not accessible by integration"}"#), Err(Fail::NotInstalled)));
        assert!(matches!(classify_issue_response(404, r#"{"message":"Resource not accessible by integration"}"#), Err(Fail::NotInstalled)));
        assert!(matches!(classify_issue_response(422, r#"{"message":"Validation Failed"}"#), Err(Fail::Other(m)) if m == "GitHub 422: Validation Failed"));
    }

    fn fields() -> IssueFields {
        IssueFields {
            device: "Lamzu | Atlantis".into(),
            vendor_battery: "85".into(),
            vendor_charging: "Hayır".into(),
            vendor_hz: "1000".into(),
            note: "ilk\nikinci".into(),
            panel_battery: "%83".into(),
            panel_charging: "-".into(),
            panel_hz: "1000 Hz (sürücü)".into(),
        }
    }

    #[test]
    fn header_table_and_title() {
        let h = issue_header(&fields());
        assert!(h.contains("| Pil | 85 | %83 |") && h.contains("| Hz | 1000 | 1000 Hz (sürücü) |"), "{h}");
        assert!(h.contains("Lamzu \\| Atlantis") && h.contains("**Not:** ilk\nikinci"));
        assert_eq!(issue_title("  Foo \n Mouse "), "[Cihaz] Foo Mouse");
        assert_eq!(issue_title(&"x".repeat(100)).chars().count(), 8 + 60);
    }

    fn big_report() -> String {
        let hex: String = (0..3000).map(|i| format!("{i:04X}: 00 11 22 33 44 55 66 77\n")).collect();
        format!(
            "# Rapor\n\n##### mi=00 — 046D:C54D\n\nkimlik\n\n#### USB\n\n```\n{hex}```\n\n#### Pasif dinleme (5 sn)\n\n```\n{hex}```\n\n#### Bluetooth\n\nson satır\n\n## Kullanıcı notları\n\n- boş şablon\n"
        )
    }

    #[test]
    fn body_unchanged_when_small() {
        let b = build_body(&fields(), "# Rapor\n\nküçük\n\n## Kullanıcı notları\n\n- x\n", MAX_BODY_CHARS);
        assert!(b.contains("küçük") && !b.contains("boş şablon") && !b.contains("- x") && !b.contains("kısaltıldı"));
    }

    #[test]
    fn body_is_capped_and_keeps_identifiers() {
        let r = big_report();
        assert!(r.chars().count() > 150_000);
        let b = build_body(&fields(), &r, MAX_BODY_CHARS);
        assert!(b.chars().count() <= MAX_BODY_CHARS, "{}", b.chars().count());
        assert!(b.starts_with("## Kullanıcı bildirimi") && b.contains("| Pil | 85 | %83 |"));
        assert!(b.contains("046D:C54D") && b.contains("son satır") && b.contains("(kısaltıldı"));
        assert!(b.contains("#### Pasif dinleme\n\n(kısaltıldı: boyut sınırı nedeniyle çıkarıldı)"));
        assert!(b.contains("#### Bluetooth") && !b.contains("boş şablon"));
        // Hex block shortened, not dropped.
        assert!(b.contains("0000: 00 11 22") && b.contains("satır daha)"));
    }

    #[test]
    fn body_hard_cut_as_last_resort() {
        let r = format!("# Rapor\n\n{}", "uzun satır ışık\n".repeat(10_000));
        let b = build_body(&fields(), &r, 5_000);
        assert!(b.chars().count() <= 5_000 && b.ends_with("(kısaltıldı: boyut sınırı)\n") && b.starts_with("## Kullanıcı"));
    }
}
