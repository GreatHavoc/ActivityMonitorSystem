//! v1.3 weekly "shape of your work" digest. Read-only aggregation over the
//! existing tables; rendered as Markdown so the output is a plain file the
//! user owns (local-first PKM expectation). Shared by `cli report --week`
//! and the MCP `weekly_digest` tool.

use crate::db::ActivityDatabase;

pub struct WeekStats {
    pub from_o: String,
    pub to_o: String,
    pub total_events: i64,
    pub active_events: i64,
    pub idle_events: i64,
    pub focus_changes: i64,
    pub inference_count: i64,
    pub top_apps_events: Vec<(String, i64)>,
    pub top_apps_inference: Vec<(String, i64)>,
    /// (total, coding, web, documents, videos, communication)
    pub distribution: (i64, i64, i64, i64, i64, i64),
    pub top_topics: Vec<(String, i64)>,
    pub busiest_hours: Vec<(i64, i64)>,
}

pub fn weekly_stats(db: &ActivityDatabase, from_o: &str, to_o: &str) -> rusqlite::Result<WeekStats> {
    let total_events = db
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM ActivityEvents WHERE Timestamp BETWEEN ?1 AND ?2",
            rusqlite::params![from_o, to_o],
            |r| r.get::<_, i64>(0),
        )?;
    let (active_events, idle_events) = db.idle_counts(from_o, to_o)?;
    let focus_changes = db.event_type_count(from_o, to_o, "focus_change")?;
    let inference_count = db
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM InferenceResults WHERE ProcessedAt BETWEEN ?1 AND ?2",
            rusqlite::params![from_o, to_o],
            |r| r.get::<_, i64>(0),
        )?;
    let top_apps_events = {
        let mut stmt = db.conn().prepare(
            "SELECT ProcessName, COUNT(*) AS c FROM ActivityEvents
             WHERE Timestamp BETWEEN ?1 AND ?2 AND ProcessName IS NOT NULL
             GROUP BY ProcessName ORDER BY c DESC LIMIT 10",
        )?;
        let rows = stmt.query_map(rusqlite::params![from_o, to_o], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    let top_apps_inference = db.summary_top_apps(from_o, to_o)?;
    let distribution = db.summary_distribution(from_o, to_o)?;
    let top_topics = db.top_topics(from_o, to_o, 10)?;
    let busiest_hours = db.busiest_hours(from_o, to_o)?;
    Ok(WeekStats {
        from_o: from_o.to_string(),
        to_o: to_o.to_string(),
        total_events,
        active_events,
        idle_events,
        focus_changes,
        inference_count,
        top_apps_events,
        top_apps_inference,
        distribution,
        top_topics,
        busiest_hours,
    })
}

/// Markdown rendering. Plain file, no em dashes, tables where they help.
pub fn render_markdown(s: &WeekStats) -> String {
    let (total, coding, web, docs, videos, comm) = s.distribution;
    let mut out = String::new();
    out.push_str("# Weekly activity digest\n\n");
    out.push_str(&format!("Period (UTC): {} to {}\n\n", s.from_o, s.to_o));
    out.push_str("## Overview\n\n");
    out.push_str(&format!("- Events recorded: {}\n", s.total_events));
    out.push_str(&format!("  - Active: {}\n  - Idle: {}\n", s.active_events, s.idle_events));
    out.push_str(&format!("- Context switches (focus changes): {}\n", s.focus_changes));
    out.push_str(&format!("- AI summaries written: {}\n\n", s.inference_count));

    out.push_str("## Top applications (by events)\n\n");
    if s.top_apps_events.is_empty() {
        out.push_str("No events in this period.\n\n");
    } else {
        out.push_str("| Application | Events |\n|---|---|\n");
        for (app, c) in &s.top_apps_events {
            out.push_str(&format!("| {} | {} |\n", app, c));
        }
        out.push('\n');
    }

    out.push_str("## Top applications (by AI summaries)\n\n");
    if s.top_apps_inference.is_empty() {
        out.push_str("No AI summaries in this period.\n\n");
    } else {
        out.push_str("| Application | Summaries |\n|---|---|\n");
        for (app, c) in &s.top_apps_inference {
            out.push_str(&format!("| {} | {} |\n", app, c));
        }
        out.push('\n');
    }

    if total > 0 {
        out.push_str("## Activity distribution (by AI summaries)\n\n");
        for (label, c) in [
            ("Coding", coding),
            ("Web browsing", web),
            ("Documents", docs),
            ("Videos", videos),
            ("Communication", comm),
        ] {
            out.push_str(&format!("- {}: {}\n", label, c));
        }
        out.push('\n');
    }

    if !s.top_topics.is_empty() {
        out.push_str("## Top topics\n\n");
        for (topic, c) in &s.top_topics {
            out.push_str(&format!("- {} ({})\n", topic, c));
        }
        out.push('\n');
    }

    if !s.busiest_hours.is_empty() {
        out.push_str("## Busiest hours (UTC)\n\n");
        for (h, c) in &s.busiest_hours {
            out.push_str(&format!("- {:02}:00, {} active events\n", h, c));
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::ActivityDatabase;
    use crate::models::{ActivityEvent, InferenceResult};
    use crate::timefmt::format_o;
    use jiff::Timestamp;

    fn seed_db() -> (ActivityDatabase, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("digest-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ActivityData.db");
        let db = ActivityDatabase::open(&path).unwrap();
        db.initialize().unwrap();
        (db, path)
    }

    #[test]
    fn digest_renders_from_seed() {
        let (db, path) = seed_db();
        let now = Timestamp::now();
        for i in 0..5 {
            let ev = ActivityEvent {
                id: 0,
                timestamp: now - std::time::Duration::from_secs(i * 60),
                event_type: if i == 0 { "idle" } else { "focus_change" }.into(),
                process_id: Some(100 + i as i64),
                process_name: Some(if i % 2 == 0 { "Code" } else { "chrome" }.into()),
                window_title: Some("window".into()),
                is_idle: i == 0,
                idle_duration_seconds: None,
                metadata: None,
            };
            db.save_activity_event(&ev).unwrap();
        }
        let inf = InferenceResult {
            request_id: uuid::Uuid::new_v4(),
            processed_at: now,
            activity_label: "Coding".into(),
            application: "Code".into(),
            content_type: "coding".into(),
            topic: "rust".into(),
            action: "editing".into(),
            summary: "Worked on the digest".into(),
            visible_text: String::new(),
            confidence: 0.9,
            detected_objects: None,
            raw_response: None,
        };
        db.save_inference_result(&inf).unwrap();

        let from = format_o(&(now - std::time::Duration::from_secs(3600)));
        let to = format_o(&(now + std::time::Duration::from_secs(60)));
        let stats = weekly_stats(&db, &from, &to).unwrap();
        assert_eq!(stats.total_events, 5);
        assert_eq!(stats.active_events, 4);
        assert_eq!(stats.idle_events, 1);
        assert_eq!(stats.focus_changes, 4);
        assert_eq!(stats.inference_count, 1);
        assert_eq!(stats.top_apps_events[0].0, "Code");
        let md = render_markdown(&stats);
        assert!(md.contains("# Weekly activity digest"));
        assert!(md.contains("Context switches (focus changes): 4"));
        assert!(md.contains("rust (1)"));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn compaction_purges_inference_rows_with_events() {
        let (db, path) = seed_db();
        let old = Timestamp::now() - std::time::Duration::from_secs(100 * 86_400);
        let ev = ActivityEvent {
            id: 0,
            timestamp: old,
            event_type: "focus_change".into(),
            process_id: Some(1),
            process_name: Some("old".into()),
            window_title: None,
            is_idle: false,
            idle_duration_seconds: None,
            metadata: None,
        };
        db.save_activity_event(&ev).unwrap();
        let inf = InferenceResult {
            request_id: uuid::Uuid::new_v4(),
            processed_at: old,
            activity_label: "Old".into(),
            application: String::new(),
            content_type: String::new(),
            topic: String::new(),
            action: String::new(),
            summary: "old".into(),
            visible_text: String::new(),
            confidence: 0.5,
            detected_objects: None,
            raw_response: None,
        };
        db.save_inference_result(&inf).unwrap();
        let deleted = db.compact_old_events(90).unwrap();
        assert_eq!(deleted, 1);
        assert_eq!(db.count("SELECT COUNT(*) FROM InferenceResults").unwrap(), 0);
        assert_eq!(db.count("SELECT COUNT(*) FROM ActivityEvents").unwrap(), 0);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
