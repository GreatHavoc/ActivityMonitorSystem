//! Phase 6 gate probe: capture one frame, write it to disk, print size.
//! Run: cargo run -p activity-monitor-core --example capture_probe -- <out.jpg>

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| "cap-rs.jpg".to_string());
    let cfg = activity_monitor_core::config::CaptureSettings::default();
    let frame = activity_monitor_core::capture::capture_screen(&cfg).expect("capture failed");
    std::fs::write(&out, &frame).unwrap();
    // decode to report dimensions
    let img = image::load_from_memory(&frame).expect("decode");
    println!("CAPTURE file={out} bytes={} width={} height={}", frame.len(), img.width(), img.height());
}
