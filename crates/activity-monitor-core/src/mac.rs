//! macOS platform backend (v1.4). Consent-first by platform design: macOS has
//! no silent screen capture (Screen Recording is a TCC permission and any
//! active capture shows the mandatory menu-bar indicator), so the default
//! backend here is text-only: frontmost app + window title + focused-element
//! text via the Accessibility API. No screenshot is ever taken on this path.
//!
//! - Idle: CoreGraphics CGEventSourceSecondsSinceLastEventType, no permission.
//! - Focus/context: Accessibility API via the axuielement crate. Needs the
//!   Accessibility permission (System Settings > Privacy & Security); without
//!   it the AX calls fail and focus degrades to None (idle-only events).

use crate::models::FocusInfo;
use jiff::{Span, Timestamp};

// CGEventSourceState / CGEventType are C enums (u32/i32 repr). HID system
// state = 1; kCGAnyInputEventType = 0xFFFFFFFF ("any input event type").
const K_CG_EVENT_SOURCE_STATE_HID_SYSTEM_STATE: u32 = 1;
const K_CG_ANY_INPUT_EVENT_TYPE: u32 = u32::MAX;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventSourceSecondsSinceLastEventType(state: u32, event_type: u32) -> f64;
}

#[link(name = "System", kind = "dylib")]
unsafe extern "C" {
    fn proc_pidpath(pid: i32, buffer: *mut core::ffi::c_char, buffersize: u32) -> i32;
}

/// Seconds since the last HID input event, as a Span (zero on failure, like
/// the Windows path returning TimeSpan.Zero).
pub fn idle_time() -> Span {
    let secs = unsafe {
        CGEventSourceSecondsSinceLastEventType(
            K_CG_EVENT_SOURCE_STATE_HID_SYSTEM_STATE,
            K_CG_ANY_INPUT_EVENT_TYPE,
        )
    };
    if secs.is_finite() && secs >= 0.0 {
        Span::new()
            .seconds(secs as i64)
            .milliseconds((secs.fract() * 1000.0) as i64)
    } else {
        Span::new()
    }
}

/// Process name for a pid. proc_pidpath gives the executable path; for .app
/// bundles we report the bundle name ("Visual Studio Code"), else the basename.
pub fn process_name(pid: i32) -> String {
    let mut buf = vec![0i8; 4096]; // PROC_PIDPATHINFO_MAXSIZE
    let n = unsafe { proc_pidpath(pid, buf.as_mut_ptr(), buf.len() as u32) };
    if n <= 0 {
        return "Unknown".to_string();
    }
    let path = unsafe { core::ffi::CStr::from_ptr(buf.as_ptr()) }
        .to_string_lossy()
        .into_owned();
    app_name_from_exe_path(&path)
}

fn app_name_from_exe_path(path: &str) -> String {
    let parts: Vec<&str> = path.split('/').collect();
    for (i, p) in parts.iter().enumerate() {
        if let Some(name) = p.strip_suffix(".app") {
            if parts.len() > i + 2 && parts[i + 1] == "Contents" && parts[i + 2] == "MacOS" {
                return name.to_string();
            }
        }
    }
    path.rsplit('/').next().unwrap_or(path).to_string()
}

/// Cross-process AX reads are synchronous; a frozen target app would block
/// the service loop. Cap every element we read at one second.
const AX_TIMEOUT_SECS: f32 = 1.0;

fn cap_timeout(el: &axuielement::AXUIElement) {
    let _ = el.set_timeout(AX_TIMEOUT_SECS);
}

fn ax_hint() {
    tracing::error!(
        "Accessibility permission missing or AX API disabled. Grant it in \
         System Settings > Privacy & Security > Accessibility for this binary, \
         then restart the service. Without it, only idle events are recorded."
    );
}

/// Port of FocusTracker.GetCurrentFocus on macOS: the AX focused application
/// (pid + name) plus the focused window title. None when AX is unavailable.
pub fn current_focus() -> Option<FocusInfo> {
    if !axuielement::api_enabled() || !axuielement::is_process_trusted() {
        ax_hint();
        return None;
    }
    let system = axuielement::system_wide()?;
    let app = system.focused_application().ok()??;
    cap_timeout(&app);
    let pid = app.pid().ok()?;
    if pid <= 0 {
        return None;
    }
    let window_title = system
        .focused_window()
        .ok()
        .flatten()
        .and_then(|w| {
            cap_timeout(&w);
            w.string_attribute(axuielement::ax_attribute::AX_TITLE_ATTRIBUTE)
                .ok()
                .flatten()
        })
        .unwrap_or_default();
    Some(FocusInfo {
        window_handle: pid as usize, // no HWND on macOS; pid doubles as the key
        process_id: pid as i64,
        process_name: process_name(pid),
        window_title,
        timestamp: Timestamp::now(),
    })
}

/// Text-only capture context for the axtext backend: app, window title and the
/// focused element's value (or selected text), truncated to 4000 chars. This
/// is what goes to the model on macOS; no screen image is involved.
pub fn focus_text() -> Option<String> {
    if !axuielement::api_enabled() || !axuielement::is_process_trusted() {
        ax_hint();
        return None;
    }
    let system = axuielement::system_wide()?;
    let app = system.focused_application().ok()??;
    cap_timeout(&app);
    let pid = app.pid().ok()?;
    if pid <= 0 {
        return None;
    }
    let app_name = process_name(pid);
    let title = system
        .focused_window()
        .ok()
        .flatten()
        .and_then(|w| {
            cap_timeout(&w);
            w.string_attribute(axuielement::ax_attribute::AX_TITLE_ATTRIBUTE)
                .ok()
                .flatten()
        })
        .unwrap_or_default();
    let mut text = String::new();
    if let Some(focused) = system.focused_ui_element().ok().flatten() {
        cap_timeout(&focused);
        // One round trip for both attributes instead of two.
        if let Ok(vals) = focused.copy_multiple_attribute_values(
            &[
                axuielement::ax_attribute::AX_VALUE_ATTRIBUTE,
                axuielement::ax_attribute::AX_SELECTED_TEXT_ATTRIBUTE,
            ],
            axuielement::AXCopyMultipleAttributeOptions::NONE,
        ) {
            if let Some(v) = vals.first().and_then(|v| v.as_string()) {
                text = v;
            }
            if text.is_empty() {
                if let Some(s) = vals.get(1).and_then(|v| v.as_string()) {
                    text = s;
                }
            }
        }
    }
    let text: String = text.chars().take(4000).collect();
    let mut out = format!("Application: {app_name}\nWindow title: {title}\n");
    if !text.is_empty() {
        out.push_str("Focused element text:\n");
        out.push_str(&text);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_name_from_bundle_path() {
        assert_eq!(
            app_name_from_exe_path("/Applications/Visual Studio Code.app/Contents/MacOS/Electron"),
            "Visual Studio Code"
        );
        assert_eq!(app_name_from_exe_path("/usr/bin/ssh"), "ssh");
        assert_eq!(app_name_from_exe_path("/opt/homebrew/bin/ollama"), "ollama");
    }

    #[test]
    fn live_smoke_does_not_panic() {
        // CI runner has no Accessibility grant: calls must fail cleanly, not panic.
        let _ = idle_time().get_milliseconds();
        let _ = current_focus();
        let _ = focus_text();
    }
}
