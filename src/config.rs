//! `%APPDATA%\BilgiPaneli\config.toml`: tiny hand-rolled reader for the small
//! TOML subset we need (a `toml`/`serde` dependency would cost far more binary
//! size than this whole file). Read on every refresh; no file watcher.
//!
//! ```toml
//! refresh_minutes = 10            # 0 = no background refresh
//! order = ["id1", "id2"]
//! [devices."id1"]
//! name = "My mouse"
//! hide = true
//! ```

use std::path::PathBuf;

use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{HSTRING, PCWSTR, w};

use crate::devices::DeviceState;

pub const DEFAULT_REFRESH_MINUTES: u32 = 10;
const MAX_REFRESH_MINUTES: u32 = 24 * 60;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeviceCfg {
    pub id: String,
    pub name: Option<String>,
    pub hide: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    /// 0 = background refresh disabled.
    pub refresh_minutes: u32,
    pub order: Vec<String>,
    pub devices: Vec<DeviceCfg>,
}

impl Default for Config {
    fn default() -> Self {
        Config { refresh_minutes: DEFAULT_REFRESH_MINUTES, order: Vec::new(), devices: Vec::new() }
    }
}

impl Config {
    /// Renames, hides and orders (stable; unlisted devices keep their place after the listed ones).
    pub fn apply(&self, devs: &mut Vec<DeviceState>) {
        for d in devs.iter_mut() {
            if let Some(c) = self.devices.iter().find(|c| c.id == d.id)
                && let Some(n) = &c.name
            {
                d.name = n.clone();
            }
        }
        devs.retain(|d| !self.devices.iter().any(|c| c.hide && c.id == d.id));
        if !self.order.is_empty() {
            devs.sort_by_key(|d| self.order.iter().position(|o| *o == d.id).unwrap_or(usize::MAX));
        }
    }
}

pub fn path() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")?;
    Some(PathBuf::from(base).join("BilgiPaneli").join("config.toml"))
}

/// Missing or unreadable file = defaults.
pub fn load() -> Config {
    path().and_then(|p| std::fs::read_to_string(p).ok()).map(|t| parse(&t)).unwrap_or_default()
}

// ---------------------------------------------------------------- parser

/// Removes a trailing `# comment` (not inside a string).
fn strip_comment(line: &str) -> &str {
    let (mut in_dq, mut in_sq, mut esc) = (false, false, false);
    for (i, c) in line.char_indices() {
        match c {
            _ if esc => esc = false,
            '\\' if in_dq => esc = true,
            '"' if !in_sq => in_dq = !in_dq,
            '\'' if !in_dq => in_sq = !in_sq,
            '#' if !in_dq && !in_sq => return &line[..i],
            _ => {}
        }
    }
    line
}

/// Parses a `"basic"` or `'literal'` string at the start of `s`; returns it and the rest.
fn parse_string(s: &str) -> Option<(String, &str)> {
    let s = s.trim_start();
    let quote = s.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let mut out = String::new();
    let mut chars = s[1..].char_indices();
    while let Some((i, c)) = chars.next() {
        if c == quote {
            return Some((out, &s[1 + i + 1..]));
        }
        if c == '\\' && quote == '"' {
            match chars.next()?.1 {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                o => out.push(o), // \\ and \" (anything else kept verbatim)
            }
        } else {
            out.push(c);
        }
    }
    None
}

fn parse_string_array(s: &str) -> Option<Vec<String>> {
    let mut rest = s.trim().strip_prefix('[')?;
    let mut out = Vec::new();
    loop {
        rest = rest.trim_start();
        if rest.starts_with(']') {
            return Some(out);
        }
        let (v, r) = parse_string(rest)?;
        out.push(v);
        rest = r.trim_start();
        rest = rest.strip_prefix(',').unwrap_or(rest);
    }
}

pub fn parse(text: &str) -> Config {
    let mut cfg = Config::default();
    let mut current: Option<usize> = None; // index into cfg.devices
    let mut lines = text.lines();
    while let Some(raw) = lines.next() {
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if let Some(inner) = line.strip_prefix('[') {
            // Section header: [devices."id"] or [devices.id]
            current = None;
            if let Some(key) = inner.trim().strip_prefix("devices.") {
                let key = key.trim_end_matches(']').trim();
                let id = parse_string(key).map(|(s, _)| s).unwrap_or_else(|| key.to_string());
                if !id.is_empty() {
                    cfg.devices.push(DeviceCfg { id, ..Default::default() });
                    current = Some(cfg.devices.len() - 1);
                }
            }
            continue; // other sections are ignored (current = None)
        }
        let Some((key, val)) = line.split_once('=') else { continue };
        let (key, mut val) = (key.trim(), val.trim().to_string());
        // Multi-line array: keep joining until the bracket closes.
        if val.starts_with('[') && !val.contains(']') {
            for more in lines.by_ref() {
                val.push(' ');
                val.push_str(strip_comment(more).trim());
                if val.contains(']') {
                    break;
                }
            }
        }
        match (current, key) {
            (None, "refresh_minutes") => {
                if let Ok(n) = val.parse::<u32>() {
                    cfg.refresh_minutes = n.min(MAX_REFRESH_MINUTES);
                }
            }
            (None, "order") => {
                if let Some(v) = parse_string_array(&val) {
                    cfg.order = v;
                }
            }
            (Some(i), "name") => cfg.devices[i].name = parse_string(&val).map(|(s, _)| s).filter(|s| !s.is_empty()),
            (Some(i), "hide") => cfg.devices[i].hide = val == "true",
            _ => {}
        }
    }
    cfg
}

// ---------------------------------------------------------------- settings file

fn template(devices: &[(String, String)]) -> String {
    let mut t = String::from(
        "# Bilgi Paneli ayarları\n\
         # Değişiklikler bir sonraki yenilemede uygulanır (paneli açınca veya sağ tık > Yenile).\n\
         # Kullanmak istediğin satırın başındaki # işaretini sil.\n\n\
         # Arka planda yenileme aralığı (dakika). 0 = arka planda yenileme yok.\n\
         # refresh_minutes = 10\n\n\
         # Cihaz sırası (aşağıdaki kimliklerden). Listede olmayanlar sonra gelir.\n\
         # order = [\"kimlik1\", \"kimlik2\"]\n\n\
         # Cihaza özel ad ve gizleme:\n\
         # [devices.\"kimlik\"]\n\
         # name = \"Yeni ad\"\n\
         # hide = true\n\n\
         # Şu an bulunan cihazlar (kimlik  # ad):\n",
    );
    for (id, name) in devices {
        t.push_str(&format!("#   \"{id}\"   # {name}\n"));
    }
    t
}

/// Opens the config file in the default editor, creating a commented template
/// (listing the current device ids) if it does not exist yet.
pub fn open_settings(devices: &[(String, String)]) {
    let Some(path) = path() else { return };
    if !path.exists() {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if std::fs::write(&path, template(devices)).is_err() {
            return;
        }
    }
    let file = HSTRING::from(path.as_os_str());
    unsafe {
        let r = ShellExecuteW(None, w!("open"), &file, PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
        if (r.0 as isize) <= 32 {
            // No association for .toml: fall back to Notepad.
            let _ = ShellExecuteW(None, w!("open"), w!("notepad.exe"), &file, PCWSTR::null(), SW_SHOWNORMAL);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::devices::{Connection, HzSource, Kind};

    fn dev(id: &str, name: &str) -> DeviceState {
        DeviceState {
            id: id.into(),
            name: name.into(),
            kind: Kind::Mouse,
            connection: Connection::Wired,
            battery: None,
            charging: None,
            polling_hz: None,
            hz_source: HzSource::Driver,
            last_read: 0,
            stale: false,
            serial: String::new(),
            driver: None,
            wanted: None,
        }
    }

    #[test]
    fn parses_all_keys() {
        let c = parse(
            "# hi\nrefresh_minutes = 0 # none\norder = [\n \"b\", # first\n 'a'\n]\n\
             [devices.\"{X-1} #1\"]\nname = \"My \\\"Mouse\\\"\"\nhide = true\n\
             [devices.\"c\"]\nname = \"C\"\n[other]\nhide = true\n",
        );
        assert_eq!(c.refresh_minutes, 0);
        assert_eq!(c.order, ["b", "a"]);
        assert_eq!(c.devices.len(), 2);
        assert_eq!(c.devices[0].id, "{X-1} #1");
        assert_eq!(c.devices[0].name.as_deref(), Some("My \"Mouse\""));
        assert!(c.devices[0].hide);
        assert!(!c.devices[1].hide);
    }

    #[test]
    fn defaults_and_garbage() {
        assert_eq!(parse(""), Config::default());
        assert_eq!(parse("refresh_minutes = abc\n= = =\n[[x\n").refresh_minutes, 10);
        assert_eq!(parse("refresh_minutes = 99999").refresh_minutes, MAX_REFRESH_MINUTES);
    }

    #[test]
    fn apply_renames_hides_orders() {
        let c = parse("order = [\"c\", \"a\"]\n[devices.\"a\"]\nname = \"A2\"\n[devices.\"b\"]\nhide = true\n");
        let mut v = vec![dev("a", "A"), dev("b", "B"), dev("d", "D"), dev("c", "C")];
        c.apply(&mut v);
        let ids: Vec<_> = v.iter().map(|d| (d.id.as_str(), d.name.as_str())).collect();
        assert_eq!(ids, [("c", "C"), ("a", "A2"), ("d", "D")]);
    }
}
