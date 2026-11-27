//! Process power/memory behaviour: EcoQoS and working set trimming.

use std::ffi::c_void;

use windows::Win32::System::Memory::{GetProcessHeap, HeapCompact, HEAP_FLAGS};
use windows::Win32::System::ProcessStatus::K32EmptyWorkingSet;
use windows::Win32::System::Threading::{
    GetCurrentProcess, PROCESS_POWER_THROTTLING_CURRENT_VERSION,
    PROCESS_POWER_THROTTLING_EXECUTION_SPEED, PROCESS_POWER_THROTTLING_STATE,
    ProcessPowerThrottling, SetProcessInformation,
};

/// Opt the process into efficiency mode (EcoQoS).
pub fn enable_eco_qos() {
    let state = PROCESS_POWER_THROTTLING_STATE {
        Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
        ControlMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
        StateMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
    };
    unsafe {
        let _ = SetProcessInformation(
            GetCurrentProcess(),
            ProcessPowerThrottling,
            &state as *const _ as *const c_void,
            size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
        );
    }
}

/// Page out everything we can; called after the panel window is destroyed.
pub fn trim_working_set() {
    unsafe {
        // Return freed heap pages (Direct2D/DirectWrite scratch) to the OS first.
        if let Ok(h) = GetProcessHeap() {
            HeapCompact(h, HEAP_FLAGS(0));
        }
        let _ = K32EmptyWorkingSet(GetCurrentProcess());
    }
}
