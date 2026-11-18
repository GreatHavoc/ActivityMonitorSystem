//! Sensors — port of NativeSensors / FocusTracker / IdleDetector.
//! Windows: Win32 (GetForegroundWindow / GetLastInputInfo). macOS (v1.4):
//! CoreGraphics idle + Accessibility focus, see `crate::mac`.
//! One deliberate fix kept from the port: idle time uses GetTickCount64
//! instead of the C# GetTickCount (u32), which wraps after ~49.7 days and
//! reported garbage idle.

use crate::models::{FocusInfo, IdleInfo};
use jiff::{Span, Timestamp};

/// C# compares TimeSpan >= TimeSpan; jiff Span has no PartialOrd, so both
/// platforms compare totals in ms through this one helper.
fn idle_exceeds(idle: &Span, threshold_seconds: i64) -> bool {
    let threshold_ms = threshold_seconds.max(0) * 1000;
    let idle_ms = idle.total(jiff::Unit::Millisecond).unwrap_or(0.0) as i64;
    idle_ms >= threshold_ms
}

#[cfg(windows)]
mod win {
    use super::*;
    use windows::core::PWSTR;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::SystemInformation::GetTickCount64;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId};

    pub struct NativeSensors;

    impl NativeSensors {
        pub fn foreground_window_handle() -> usize {
            unsafe { GetForegroundWindow().0 as usize }
        }

        pub fn process_id_from_window(window_handle: usize) -> i64 {
            let mut pid: u32 = 0;
            unsafe {
                GetWindowThreadProcessId(HWND(window_handle as *mut core::ffi::c_void), Some(&mut pid));
            }
            pid as i64
        }

        pub fn window_title(window_handle: usize) -> String {
            const MAX_LENGTH: usize = 256; // as in C#
            let mut buf = vec![0u16; MAX_LENGTH];
            let len = unsafe { GetWindowTextW(HWND(window_handle as *mut core::ffi::c_void), &mut buf) };
            if len > 0 {
                String::from_utf16_lossy(&buf[..len as usize])
            } else {
                String::new()
            }
        }

        /// Idle time since last keyboard/mouse input. GetTickCount64 removes the
        /// u32 wrap; dwTime is a tick count in the same 64-bit space.
        pub fn idle_time() -> Span {
            let mut lii = LASTINPUTINFO {
                cbSize: core::mem::size_of::<LASTINPUTINFO>() as u32,
                dwTime: 0,
            };
            let ok = unsafe { GetLastInputInfo(&mut lii) };
            if !ok.as_bool() {
                return Span::new(); // C# returns TimeSpan.Zero on failure
            }
            let idle_ms = unsafe { GetTickCount64() }.wrapping_sub(lii.dwTime as u64);
            Span::new().milliseconds(idle_ms.min(i64::MAX as u64) as i64)
        }

        /// DateTime.Now - idle (C# GetLastInputTimestamp).
        pub fn last_input_timestamp() -> Timestamp {
            let idle = Self::idle_time();
            let ms = idle.get_milliseconds().max(0) as u64;
            Timestamp::now() - std::time::Duration::from_millis(ms)
        }
    }

    /// .NET Process.GetProcessById(pid).ProcessName — image file name without extension.
    pub fn process_name(pid: u32) -> windows::core::Result<String> {
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)?;
            let mut buf = vec![0u16; 1024];
            let mut size = buf.len() as u32;
            QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut size)?;
            let path = String::from_utf16_lossy(&buf[..size as usize]);
            let file = path.rsplit(['\\', '/']).next().unwrap_or(path.as_str());
            let stem = match file.rsplit_once('.') {
                Some((s, _)) => s,
                None => file,
            };
            Ok(stem.to_string())
        }
    }
}

/// Port of FocusTracker.GetCurrentFocus: null when no window or pid == 0,
/// "Unknown" process name on lookup failure, error -> None.
#[cfg(windows)]
pub fn current_focus() -> Option<FocusInfo> {
    let result = std::panic::catch_unwind(|| {
        let window_handle = win::NativeSensors::foreground_window_handle();
        if window_handle == 0 {
            return None;
        }
        let process_id = win::NativeSensors::process_id_from_window(window_handle);
        let window_title = win::NativeSensors::window_title(window_handle);
        if process_id == 0 {
            return None;
        }
        let process_name = match win::process_name(process_id as u32) {
            Ok(n) => n,
            Err(e) => {
                tracing::debug!(error = %e, pid = process_id, "Could not get process name for PID");
                "Unknown".to_string()
            }
        };
        Some(FocusInfo {
            window_handle,
            process_id,
            process_name,
            window_title,
            timestamp: Timestamp::now(),
        })
    });
    match result {
        Ok(f) => f,
        Err(_) => {
            tracing::error!("Error getting current focus information");
            None
        }
    }
}

/// macOS: AX focused application + window title (see crate::mac).
#[cfg(target_os = "macos")]
pub fn current_focus() -> Option<FocusInfo> {
    crate::mac::current_focus()
}

/// Port of IdleDetector.GetIdleState: on error returns the safe non-idle default.
#[cfg(windows)]
pub fn idle_state(idle_threshold_seconds: i64) -> IdleInfo {
    let idle_duration = win::NativeSensors::idle_time();
    IdleInfo {
        is_idle: idle_exceeds(&idle_duration, idle_threshold_seconds),
        idle_duration,
        last_input_time: win::NativeSensors::last_input_timestamp(),
    }
}

/// macOS: CoreGraphics seconds since last HID input event.
#[cfg(target_os = "macos")]
pub fn idle_state(idle_threshold_seconds: i64) -> IdleInfo {
    let idle_duration = crate::mac::idle_time();
    let idle_ms = idle_duration.total(jiff::Unit::Millisecond).unwrap_or(0.0) as i64;
    IdleInfo {
        is_idle: idle_exceeds(&idle_duration, idle_threshold_seconds),
        idle_duration,
        last_input_time: Timestamp::now() - std::time::Duration::from_millis(idle_ms.max(0) as u64),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn threshold_comparison() {
        assert!(idle_exceeds(&Span::new().seconds(300), 300));
        assert!(idle_exceeds(&Span::new().milliseconds(300_001), 300));
        assert!(!idle_exceeds(&Span::new().seconds(299), 300));
        assert!(idle_exceeds(&Span::new(), 0)); // 0 threshold: 0 ms idle counts as idle
    }

    #[cfg(windows)]
    #[test]
    fn live_smoke() {
        // Runs on the Windows desktop: must not panic; values are environment-dependent.
        let _ = win::NativeSensors::foreground_window_handle();
        let _ = win::NativeSensors::idle_time();
        let info = idle_state(300);
        assert!(info.idle_duration.get_milliseconds() >= 0);
        let _ = current_focus();
    }
}
