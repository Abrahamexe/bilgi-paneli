//! "Start with Windows" via HKCU\...\Run, plus first-run marker.

use windows::Win32::Foundation::{ERROR_SUCCESS, MAX_PATH};
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_DWORD, REG_SZ, RRF_RT_REG_DWORD, RRF_RT_REG_SZ, RegDeleteKeyValueW,
    RegGetValueW, RegSetKeyValueW,
};
use windows::core::{PCWSTR, w};

const RUN_KEY: PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");
const RUN_VALUE: PCWSTR = w!("BilgiPaneli");
const APP_KEY: PCWSTR = w!(r"Software\BilgiPaneli");
const FIRST_RUN_VALUE: PCWSTR = w!("FirstRunDone");

pub fn is_enabled() -> bool {
    unsafe {
        RegGetValueW(HKEY_CURRENT_USER, RUN_KEY, RUN_VALUE, RRF_RT_REG_SZ, None, None, None)
            == ERROR_SUCCESS
    }
}

/// Writes the Run value as the quoted path of the current exe.
pub fn enable() {
    let mut buf = [0u16; MAX_PATH as usize * 2];
    let len = unsafe { GetModuleFileNameW(None, &mut buf) } as usize;
    if len == 0 || len >= buf.len() - 3 {
        return;
    }
    let mut value: Vec<u16> = Vec::with_capacity(len + 3);
    value.push(b'"' as u16);
    value.extend_from_slice(&buf[..len]);
    value.push(b'"' as u16);
    value.push(0);
    unsafe {
        let _ = RegSetKeyValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            RUN_VALUE,
            REG_SZ.0,
            Some(value.as_ptr().cast()),
            (value.len() * 2) as u32,
        );
    }
}

pub fn disable() {
    unsafe {
        let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, RUN_VALUE);
    }
}

pub fn toggle() {
    if is_enabled() {
        disable();
    } else {
        enable();
    }
}

/// On the very first run autostart is switched on once; afterwards the user's
/// choice is respected. If already enabled, the path is refreshed (exe moved).
pub fn first_run_setup() {
    unsafe {
        let done = RegGetValueW(
            HKEY_CURRENT_USER,
            APP_KEY,
            FIRST_RUN_VALUE,
            RRF_RT_REG_DWORD,
            None,
            None,
            None,
        ) == ERROR_SUCCESS;
        if !done {
            enable();
            let one = 1u32.to_ne_bytes();
            let _ = RegSetKeyValueW(
                HKEY_CURRENT_USER,
                APP_KEY,
                FIRST_RUN_VALUE,
                REG_DWORD.0,
                Some(one.as_ptr().cast()),
                4,
            );
        } else if is_enabled() {
            enable();
        }
    }
}
