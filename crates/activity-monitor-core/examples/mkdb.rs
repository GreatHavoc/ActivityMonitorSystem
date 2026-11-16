//! Golden-test helper: create a database and insert one row of each type.
//! Usage: mkdb <db-path>   (used by the Phase 1 differential test vs the .NET CLI)

use activity_monitor_core::db::ActivityDatabase;
use activity_monitor_core::models::ActivityEvent;
use activity_monitor_core::timefmt::parse_o;

fn main() {
    let path = std::env::args().nth(1).expect("usage: mkdb <db-path>");
    let db = ActivityDatabase::open(std::path::Path::new(&path)).expect("open db");
    db.initialize().expect("initialize");
    db.save_activity_event(&ActivityEvent {
        id: 0,
        timestamp: parse_o("2026-11-13T08:05:00.0000000Z").unwrap(),
        event_type: "FocusChange".into(),
        process_id: Some(101),
        process_name: Some("code".into()),
        window_title: Some("main.rs - rust-version".into()),
        is_idle: false,
        idle_duration_seconds: None,
        metadata: None,
    })
    .expect("save event");
    println!("OK {path}");
}
