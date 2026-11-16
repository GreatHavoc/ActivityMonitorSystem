//! Live sensor probe for the Phase 5 gate: prints the same values the .NET
//! probe prints, so both can be compared on the same desktop.
//! Run: cargo run -p activity-monitor-core --example sensors_probe

fn main() {
    let hwnd = activity_monitor_core::sensors::NativeSensors::foreground_window_handle();
    let pid = activity_monitor_core::sensors::NativeSensors::process_id_from_window(hwnd);
    let title = activity_monitor_core::sensors::NativeSensors::window_title(hwnd);
    println!("FOCUS hwnd={hwnd} pid={pid} title={title}");

    match activity_monitor_core::sensors::current_focus() {
        Some(f) => println!("FOCUSINFO hwnd={} pid={} proc={} title={} ts={}", f.window_handle, f.process_id, f.process_name, f.window_title, activity_monitor_core::timefmt::format_o(&f.timestamp)),
        None => println!("FOCUSINFO none"),
    }

    let idle = activity_monitor_core::sensors::NativeSensors::idle_time();
    println!("IDLE ms={} last_input={}", idle.get_milliseconds(), activity_monitor_core::timefmt::format_o(&activity_monitor_core::sensors::NativeSensors::last_input_timestamp()));

    let state = activity_monitor_core::sensors::idle_state(300);
    println!("IDLESTATE is_idle={} idle_ms={}", state.is_idle, state.idle_duration.get_milliseconds());
}
