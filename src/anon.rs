//! Anonymizer for test reports (they get posted publicly).
//!
//! Serial numbers, Bluetooth / MAC addresses and ContainerIds become a short
//! stable hash (FNV-1a 64, first 8 hex digits) so the same device still matches
//! across reports. User-set Bluetooth names, the user name, the machine name and
//! the profile path are replaced outright. The scrubber works on the finished
//! text: first exact strings it was told about, then MAC-shaped patterns.

/// First 8 hex digits of FNV-1a 64 over the normalized (uppercase, trimmed) text.
pub fn hash8(s: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.trim().to_uppercase().bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")[..8].to_string()
}

fn mac_hash(hex12: &str) -> String {
    format!("mac_{}", hash8(&hex12.replace([':', '-'], "")))
}

#[derive(Default)]
pub struct Scrubber {
    /// (needle, replacement), matched ASCII-case-insensitively, longest needle first.
    subs: Vec<(String, String)>,
}

impl Scrubber {
    pub fn new() -> Scrubber {
        Scrubber::default()
    }

    /// Replaces `needle` (ignored when shorter than 3 chars: too likely to hit other words).
    pub fn add_literal(&mut self, needle: &str, replacement: &str) {
        let needle = needle.trim();
        if needle.len() >= 3 && !self.subs.iter().any(|(n, _)| n.eq_ignore_ascii_case(needle)) {
            self.subs.push((needle.to_string(), replacement.to_string()));
        }
    }

    /// Serial number: hashed in text, and masked where it shows up as spaced hex bytes.
    pub fn add_serial(&mut self, serial: &str) {
        let s = serial.trim();
        if s.len() < 3 {
            return;
        }
        self.add_literal(s, &format!("seri_{}", hash8(s)));
        let spaced: Vec<String> = s.bytes().map(|b| format!("{b:02X}")).collect();
        let mask = vec!["XX"; spaced.len()].join(" ");
        self.add_literal(&spaced.join(" "), &mask);
    }

    /// ContainerId (any case, with or without braces).
    pub fn add_container(&mut self, id: &str) {
        let bare = id.trim().trim_start_matches('{').trim_end_matches('}');
        self.add_literal(bare, &format!("cont_{}", hash8(bare)));
    }

    /// Replaces `text` with `replacement` (user-set names, user / machine names, profile path).
    pub fn add_replace(&mut self, text: &str, replacement: &str) {
        self.add_literal(text, replacement);
    }

    /// Applies everything to `text`.
    pub fn apply(&self, text: &str) -> String {
        let mut subs: Vec<&(String, String)> = self.subs.iter().collect();
        subs.sort_by_key(|(n, _)| std::cmp::Reverse(n.len()));
        let mut out = text.to_string();
        for (needle, repl) in subs {
            out = replace_ci(&out, needle, repl);
        }
        scrub_macs(&out)
    }
}

/// ASCII-case-insensitive replace (byte offsets stay valid: ASCII lowering keeps lengths).
fn replace_ci(text: &str, needle: &str, repl: &str) -> String {
    let (lt, ln) = (text.to_ascii_lowercase(), needle.to_ascii_lowercase());
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while let Some(i) = lt[at..].find(&ln) {
        out.push_str(&text[at..at + i]);
        out.push_str(repl);
        at += i + ln.len();
    }
    out.push_str(&text[at..]);
    out
}

/// Hashes `AA:BB:CC:DD:EE:FF` / `AA-BB-...` and bare 12-hex-digit runs (as in
/// `BTHENUM\DEV_A0B1C2D3E4F5`). Runs touching a `-` are left alone: they are
/// the last group of a GUID, not an address.
fn scrub_macs(text: &str) -> String {
    let b = text.as_bytes();
    let hexd = |i: usize| b.get(i).is_some_and(u8::is_ascii_hexdigit);
    let alnum = |i: usize| b.get(i).is_some_and(u8::is_ascii_alphanumeric);
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < b.len() {
        if hexd(i) && (i == 0 || !alnum(i - 1)) {
            // Separated form: 6 pairs joined by ':' or '-'.
            let sep = b.get(i + 2).copied();
            if matches!(sep, Some(b':' | b'-'))
                && (0..6).all(|k| hexd(i + k * 3) && hexd(i + k * 3 + 1) && (k == 5 || b.get(i + k * 3 + 2).copied() == sep))
                && !alnum(i + 17)
            {
                out.push_str(&mac_hash(&text[i..i + 17]));
                i += 17;
                continue;
            }
            // Bare form: exactly 12 hex digits as a whole alphanumeric run.
            let mut j = i;
            while alnum(j) {
                j += 1;
            }
            let prev_dash = i > 0 && b[i - 1] == b'-';
            let next_dash = b.get(j) == Some(&b'-');
            if j - i == 12 && (i..j).all(hexd) && !prev_dash && !next_dash {
                out.push_str(&mac_hash(&text[i..j]));
                i = j;
                continue;
            }
        }
        // Copy one char (keeps UTF-8 intact).
        let ch_len = text[i..].chars().next().map_or(1, char::len_utf8);
        out.push_str(&text[i..i + ch_len]);
        i += ch_len;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_stable_and_case_insensitive() {
        assert_eq!(hash8("abc123"), hash8(" ABC123 "));
        assert_eq!(hash8("abc123").len(), 8);
        assert_ne!(hash8("abc123"), hash8("abc124"));
    }

    #[test]
    fn serial_container_and_names_are_replaced() {
        let mut s = Scrubber::new();
        s.add_serial("SN0042XYZ");
        s.add_container("{12345678-90AB-CDEF-1234-567890ABCDEF}");
        s.add_replace("Abraham's AirPods", "<BT cihaz>");
        s.add_replace(r"C:\Users\abraham", "%USERPROFILE%");
        s.add_replace("abraham", "<kullanici>");
        let t = "serial SN0042xyz usb\\VID_1&PID_2\\sn0042XYZ cont {12345678-90ab-cdef-1234-567890abcdef} \
                 Headphones (Abraham's AirPods - Stereo) C:\\Users\\Abraham\\Desktop user abraham bytes 53 4E 30 30 34 32 58 59 5A";
        let out = s.apply(t);
        let hs = hash8("SN0042XYZ");
        assert!(out.contains(&format!("serial seri_{hs} usb\\VID_1&PID_2\\seri_{hs}")), "{out}");
        assert!(out.contains("cont {cont_"), "{out}");
        assert!(out.contains("Headphones (<BT cihaz> - Stereo)"), "{out}");
        assert!(out.contains("%USERPROFILE%\\Desktop"), "{out}");
        assert!(out.contains("user <kullanici>"), "{out}");
        assert!(out.contains("bytes XX XX XX XX XX XX XX XX XX"), "{out}");
        for secret in ["sn0042", "abraham", "airpods", "12345678-90"] {
            assert!(!out.to_lowercase().contains(secret), "{secret} leaked: {out}");
        }
    }

    #[test]
    fn mac_shapes_hash_alike() {
        let s = Scrubber::new();
        let out = s.apply(
            r"mac A0:B1:C2:D3:E4:F5 / a0-b1-c2-d3-e4-f5 / BTHENUM\DEV_A0B1C2D3E4F5\7&1A2B3C4D&0&BLUETOOTHDEVICE_A0B1C2D3E4F5",
        );
        let h = mac_hash("A0B1C2D3E4F5");
        assert_eq!(out.matches(&h).count(), 4, "{out}");
        assert!(!out.to_uppercase().contains("A0B1C2D3E4F5"));
        assert!(!out.to_uppercase().contains("A0:B1"));
        assert!(out.contains("7&1A2B3C4D&0")); // short Windows suffixes stay
    }

    #[test]
    fn guid_tails_and_hex_dumps_are_left_alone() {
        let s = Scrubber::new();
        let t = "{0000110B-0000-1000-8000-00805F9B34FB} 12 34 AB CD EF 01 hash seri_deadbeef 0123456789AB-1";
        assert_eq!(s.apply(t), t);
        assert_eq!(
            s.apply("türkçe ışık A0B1C2D3E4F5."),
            format!("türkçe ışık {}.", mac_hash("A0B1C2D3E4F5"))
        );
    }
}
