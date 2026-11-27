//! `probe`: console tool for exploring HID devices and sharing unsupported ones.
//!
//!   probe list
//!   probe get  <vid:pid> <usagepage> <reportid> <len> [--mi N]
//!   probe send <vid:pid> <usagepage> <hexbytes...> [--feature] [--read <ms>] [--mi N]
//!   probe dump
//!   probe report
//!
//! Numbers: vid/pid/usagepage are hex; reportid/len are decimal (or 0x-hex).

use std::process::ExitCode;
use std::time::{Duration, Instant};

use bilgi_paneli::devices::{HidHandle, HidInfo, describe, enumerate};
use bilgi_paneli::report;
use windows::Win32::System::Console::SetConsoleOutputCP;

const WRITE_WARNING: &str = "WARNING: writing arbitrary bytes to a device can change its settings \
(DPI, polling rate, RGB, profiles) or even brick its firmware. Only send bytes you understand.";

fn main() -> ExitCode {
    unsafe {
        let _ = SetConsoleOutputCP(65001);
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("list") => {
            print!("{}", describe::all_devices(false));
            Ok(())
        }
        Some("dump") => dump(),
        Some("report") => make_report(),
        Some("get") => cmd_get(&args[1..]),
        Some("send") => cmd_send(&args[1..]),
        _ => {
            eprintln!(
                "usage:\n  probe list\n  probe get <vid:pid> <usagepage> <reportid> <len> [--mi N]\n  \
                 probe send <vid:pid> <usagepage> <hexbytes...> [--feature] [--read <ms>] [--mi N]\n  probe dump\n  probe report"
            );
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

// ------------------------------------------------------------------ dump / report

fn dump() -> Result<(), String> {
    // Anonymized like the report: dumps get pasted into public issues.
    let phys = bilgi_paneli::devices::enumerate::enumerate();
    let text = report::scrubber_for(&phys).apply(&describe::all_devices(true));
    print!("{text}");
    match report::copy_to_clipboard(&text) {
        Ok(()) => eprintln!("(copied to clipboard)"),
        Err(e) => eprintln!("(clipboard copy failed: {e})"),
    }
    Ok(())
}

/// Same as the tray menu item "Cihaz raporu oluştur" (takes about 10 seconds).
fn make_report() -> Result<(), String> {
    eprintln!("Building the report (about 10 seconds; move / use your devices while it listens)...");
    let path = report::generate();
    println!("{}", path.display());
    eprintln!("(saved, copied to clipboard)");
    Ok(())
}

// ------------------------------------------------------------------ get / send

fn parse_hex(s: &str) -> Result<u32, String> {
    let t = s.trim_start_matches("0x").trim_start_matches("0X");
    u32::from_str_radix(t, 16).map_err(|_| format!("bad hex number: {s}"))
}

fn parse_num(s: &str) -> Result<u32, String> {
    match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(h) => u32::from_str_radix(h, 16),
        None => s.parse(),
    }
    .map_err(|_| format!("bad number: {s}"))
}

fn parse_vidpid(s: &str) -> Result<(u16, u16), String> {
    let (v, p) = s.split_once(':').ok_or_else(|| format!("expected vid:pid, got {s}"))?;
    Ok((parse_hex(v)? as u16, parse_hex(p)? as u16))
}

fn parse_bytes(parts: &[&str]) -> Result<Vec<u8>, String> {
    let joined: String = parts.join(" ");
    let cleaned: String = joined
        .replace("0x", " ")
        .replace("0X", " ")
        .replace([',', ';'], " ");
    let mut out = Vec::new();
    for tok in cleaned.split_whitespace() {
        if tok.len() % 2 != 0 {
            return Err(format!("odd-length hex token: {tok}"));
        }
        for i in (0..tok.len()).step_by(2) {
            out.push(u8::from_str_radix(&tok[i..i + 2], 16).map_err(|_| format!("bad hex: {tok}"))?);
        }
    }
    if out.is_empty() {
        return Err("no bytes given".into());
    }
    Ok(out)
}

fn find_interface(vp: (u16, u16), usage_page: u16, mi: Option<u8>) -> Result<HidInfo, String> {
    enumerate::list_hid()
        .into_iter()
        .find(|i| {
            (i.vid, i.pid) == vp
                && i.usage_page == usage_page
                && mi.is_none_or(|m| i.interface == Some(m))
        })
        .ok_or_else(|| format!("no HID interface {:04X}:{:04X} with usage page {usage_page:04X} (see `probe list`)", vp.0, vp.1))
}

fn take_flag(args: &mut Vec<String>, flag: &str) -> bool {
    match args.iter().position(|a| a == flag) {
        Some(i) => {
            args.remove(i);
            true
        }
        None => false,
    }
}

fn take_value(args: &mut Vec<String>, flag: &str) -> Result<Option<String>, String> {
    match args.iter().position(|a| a == flag) {
        Some(i) => {
            args.remove(i);
            if i < args.len() {
                Ok(Some(args.remove(i)))
            } else {
                Err(format!("{flag} needs a value"))
            }
        }
        None => Ok(None),
    }
}

fn hexdump(data: &[u8]) {
    for (i, chunk) in data.chunks(16).enumerate() {
        let hex: Vec<String> = chunk.iter().map(|b| format!("{b:02X}")).collect();
        println!("  {:04X}: {}", i * 16, hex.join(" "));
    }
}

fn cmd_get(args: &[String]) -> Result<(), String> {
    let mut a = args.to_vec();
    let mi = take_value(&mut a, "--mi")?.map(|s| parse_hex(&s)).transpose()?.map(|v| v as u8);
    if a.len() != 4 {
        return Err("usage: probe get <vid:pid> <usagepage> <reportid> <len> [--mi N]".into());
    }
    let vp = parse_vidpid(&a[0])?;
    let page = parse_hex(&a[1])? as u16;
    let rid = parse_num(&a[2])? as u8;
    let len = parse_num(&a[3])? as usize;
    let info = find_interface(vp, page, mi)?;
    let dev = HidHandle::open(&info).ok_or("cannot open the interface")?;
    println!("feature report id {rid} on {} (feature length {})", info.path, info.feature_len);
    let data = dev.get_feature(rid, len).ok_or("GET_FEATURE failed or timed out")?;
    println!("{} bytes:", data.len());
    hexdump(&data);
    Ok(())
}

fn cmd_send(args: &[String]) -> Result<(), String> {
    let mut a = args.to_vec();
    let feature = take_flag(&mut a, "--feature");
    let read_ms = take_value(&mut a, "--read")?.map(|s| parse_num(&s)).transpose()?;
    let mi = take_value(&mut a, "--mi")?.map(|s| parse_hex(&s)).transpose()?.map(|v| v as u8);
    if a.len() < 3 {
        return Err("usage: probe send <vid:pid> <usagepage> <hexbytes...> [--feature] [--read <ms>] [--mi N]".into());
    }
    let vp = parse_vidpid(&a[0])?;
    let page = parse_hex(&a[1])? as u16;
    // "-" = send nothing, only listen (--read).
    let listen_only = a[2] == "-" && read_ms.is_some();
    let bytes = if listen_only {
        Vec::new()
    } else {
        parse_bytes(&a[2..].iter().map(String::as_str).collect::<Vec<_>>())?
    };
    let info = find_interface(vp, page, mi)?;
    eprintln!("{WRITE_WARNING}\n");
    let dev = HidHandle::open(&info).ok_or("cannot open the interface")?;
    if !dev.rw && !feature {
        return Err("interface is system-owned (mouse/keyboard); only --feature works here".into());
    }
    if !listen_only {
        println!("{} {} bytes to {}", if feature { "SET_FEATURE" } else { "OUTPUT" }, bytes.len(), info.path);
        hexdump(&bytes);
        let ok = if feature { dev.set_feature(&bytes) } else { dev.write(&bytes) };
        println!("{}", if ok { "sent" } else { "FAILED" });
        if !ok {
            return Err("write failed".into());
        }
    }
    if let Some(ms) = read_ms {
        let start = Instant::now();
        let deadline = Duration::from_millis(ms as u64);
        let mut n = 0;
        while let Some(left) = deadline.checked_sub(start.elapsed()) {
            if let Some(rep) = dev.read_timeout(left.as_millis().max(1) as u32) {
                n += 1;
                println!("+{:>5} ms  input report ({} bytes):", start.elapsed().as_millis(), rep.len());
                hexdump(&rep);
            }
        }
        println!("{n} input report(s) in {ms} ms");
    }
    Ok(())
}
