#![windows_subsystem = "windows"]

use bilgi_paneli::{autostart, power, tray};
use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows::core::w;

fn main() {
    unsafe {
        // Single instance: the handle is intentionally kept until process exit.
        let Ok(_mutex) = CreateMutexW(None, false, w!(r"Local\BilgiPaneli")) else {
            return;
        };
        if GetLastError() == ERROR_ALREADY_EXISTS {
            return;
        }

        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        power::enable_eco_qos();
        autostart::first_run_setup();

        let _ = tray::run();
    }
}
