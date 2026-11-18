//! Live sensor probe: prints focus + idle through the public sensors API,
//! which is cross-platform since v1.4 (Windows: Win32, macOS: CG/AX).
//! Run: cargo run -p activity-monitor-core --example sensors_probe

fn main() {
    match activity_monitor_core::sensors::current_focus() {
        Some(f) => println!("FOCUSINFO hwnd={} pid={} proc={} title={} ts={}", f.window_handle, f.process_id, f.process_name, f.window_title, activity_monitor_core::timefmt::format_o(&f.timestamp)),
        None => println!("FOCUSINFO none"),
    }

    let state = activity_monitor_core::sensors::idle_state(300);
    println!(
        "IDLESTATE is_idle={} idle_ms={} last_input={}",
        state.is_idle,
        state.idle_duration.get_milliseconds(),
        activity_monitor_core::timefmt::format_o(&state.last_input_time)
    );
}
