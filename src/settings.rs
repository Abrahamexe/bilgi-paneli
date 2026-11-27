//! Tiny registry-backed settings for the test-report feature (HKCU\Software\BilgiPaneli).
//!
//! * `TestNotify` (DWORD, absent = on): balloon when a wanted device shows up.
//! * `Notified\<device id>` (one DWORD value per id): devices already announced,
//!   so each one notifies once ever.

use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_DWORD, RRF_RT_REG_DWORD, RegGetValueW, RegSetKeyValueW,
};
use windows::core::{HSTRING, PCWSTR, w};

const APP_KEY: PCWSTR = w!(r"Software\BilgiPaneli");
const NOTIFIED_KEY: PCWSTR = w!(r"Software\BilgiPaneli\Notified");
const TEST_NOTIFY: PCWSTR = w!("TestNotify");

fn get_dword(key: PCWSTR, name: PCWSTR) -> Option<u32> {
    let mut v = 0u32;
    let mut size = 4u32;
    unsafe {
        (RegGetValueW(
            HKEY_CURRENT_USER,
            key,
            name,
            RRF_RT_REG_DWORD,
            None,
            Some(&mut v as *mut u32 as *mut _),
            Some(&mut size),
        ) == ERROR_SUCCESS)
            .then_some(v)
    }
}

fn set_dword(key: PCWSTR, name: PCWSTR, value: u32) {
    let bytes = value.to_ne_bytes();
    unsafe {
        let _ = RegSetKeyValueW(HKEY_CURRENT_USER, key, name, REG_DWORD.0, Some(bytes.as_ptr().cast()), 4);
    }
}

/// "Test bildirimleri" menu checkbox (default on).
pub fn test_notify_enabled() -> bool {
    get_dword(APP_KEY, TEST_NOTIFY) != Some(0)
}

pub fn toggle_test_notify() {
    set_dword(APP_KEY, TEST_NOTIFY, u32::from(!test_notify_enabled()));
}

pub fn was_notified(id: &str) -> bool {
    get_dword(NOTIFIED_KEY, PCWSTR(HSTRING::from(id).as_ptr())).is_some()
}

pub fn mark_notified(id: &str) {
    set_dword(NOTIFIED_KEY, PCWSTR(HSTRING::from(id).as_ptr()), 1);
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::System::Registry::RegDeleteKeyValueW;

    #[test]
    fn notified_ids_are_remembered_once_set() {
        let id = "{test-0000-notified}#key";
        assert!(!was_notified(id));
        mark_notified(id);
        assert!(was_notified(id));
        unsafe {
            let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, NOTIFIED_KEY, PCWSTR(HSTRING::from(id).as_ptr()));
        }
        assert!(!was_notified(id));
    }
}
