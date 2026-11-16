//! Report v2.0 (schema "2.0") — port of ExportActivityReportAsync from Program.cs.
//! Segment accumulation, clamping, usage builders and insight indices replicate
//! the C# logic exactly; serde field order matches the C# property declaration
//! order so the JSON can be canonical-diffed against the .NET output.

use activity_monitor_core::db::{ActivityDatabase, EventSnapshotRow};
use activity_monitor_core::timefmt::{format_o, now_o, parse_o};
use jiff::Timestamp;
use serde::Serialize;
use std::collections::HashSet;

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct ActivityReport {
    pub schema_version: String,
    pub generated_at_utc: String,
    pub range_start_utc: String,
    pub range_end_utc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub check_in_time_utc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub check_out_time_utc: Option<String>,
    pub total_tracked_seconds: i64,
    pub total_active_seconds: i64,
    pub total_idle_seconds: i64,
    pub focus_events_analyzed: i64,
    pub applications: Vec<ApplicationUsage>,
    pub detailed_activities: Vec<ActivityInsight>,
    pub content_type_breakdown: Vec<ContentTypeSummary>,
    pub segments: Vec<TimelineSegment>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct ApplicationUsage {
    pub process_name: String,
    pub total_active_seconds: i64,
    pub windows: Vec<WindowUsage>,
    pub insight_indices: Vec<i64>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct WindowUsage {
    pub title: String,
    pub active_seconds: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct TimelineSegment {
    pub end_utc: String,
    pub duration_seconds: i64,
    pub is_idle: bool,
    pub process_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_title: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct ActivityInsight {
    pub timestamp_utc: String,
    pub activity_label: String,
    pub application: String,
    pub content_type: String,
    pub topic: String,
    pub action: String,
    pub summary: String,
    pub visible_text: String,
    pub confidence: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct ContentTypeSummary {
    pub content_type: String,
    pub activity_count: i64,
    pub distinct_topics: Vec<String>,
    pub distinct_applications: Vec<String>,
}

struct UsageBuilder {
    total: i64,
    // Vec keeps insertion order so stable sort ties match C# dictionary order
    windows: Vec<(String, i64)>,
}

impl UsageBuilder {
    fn add_duration(&mut self, seconds: i64, window_title: Option<&str>) {
        if seconds <= 0 {
            return;
        }
        self.total += seconds;
        let title = match window_title {
            Some(t) if !t.trim().is_empty() => t.to_string(),
            _ => "Unknown".to_string(),
        };
        match self.windows.iter_mut().find(|(t, _)| t.eq_ignore_ascii_case(&title)) {
            Some((_, secs)) => *secs += seconds,
            None => self.windows.push((title, seconds)),
        }
    }
}

struct ContentBuilder {
    count: i64,
    // C# HashSet(StringComparer.OrdinalIgnoreCase): dedup case-insensitively, keep first casing
    topics: Vec<String>,
    applications: Vec<String>,
}

pub fn add_unique_ci(list: &mut Vec<String>, v: String) {
    if !list.iter().any(|x| x.eq_ignore_ascii_case(&v)) {
        list.push(v);
    }
}

fn blank_to_unknown(v: Option<&str>) -> String {
    match v {
        Some(s) if !s.trim().is_empty() => s.to_string(),
        _ => "Unknown".to_string(),
    }
}

/// C# NormalizeKey: keep letters/digits, lowercase.
pub fn normalize_key(text: &str) -> String {
    text.chars().filter(|c| c.is_alphanumeric()).map(|c| c.to_lowercase().to_string()).collect()
}

/// C# MatchInsightIndices: normalized containment match, dedup, ascending sort.
pub fn match_insight_indices(process_name: &str, index_by_app: &[(String, Vec<i64>)]) -> Vec<i64> {
    let norm_proc = normalize_key(process_name);
    if norm_proc.is_empty() {
        return vec![];
    }
    let mut seen = HashSet::new();
    let mut results: Vec<i64> = vec![];
    for (app, indices) in index_by_app {
        let norm_app = normalize_key(app);
        if norm_app.is_empty() {
            continue;
        }
        if norm_app == norm_proc || norm_app.contains(&norm_proc) || norm_proc.contains(&norm_app) {
            for i in indices {
                if seen.insert(*i) {
                    results.push(*i);
                }
            }
        }
    }
    results.sort_unstable();
    results
}

/// C# FormatDuration (Program.cs): "Nd hh:mm:ss" / "hh:mm:ss" / "mm:ss", truncated.
pub fn format_duration(seconds: i64) -> String {
    let s = seconds.max(0);
    let days = s / 86_400;
    let rem = s % 86_400;
    let h = rem / 3_600;
    let m = (rem % 3_600) / 60;
    let sec = rem % 60;
    if s >= 86_400 {
        format!("{days}d {h:02}:{m:02}:{sec:02}")
    } else if s >= 3_600 {
        format!("{h:02}:{m:02}:{sec:02}")
    } else {
        format!("{m:02}:{sec:02}")
    }
}

fn ts(s: &str) -> Timestamp {
    parse_o(s).expect("stored timestamps are always valid O format")
}

pub fn build_report(
    db: &ActivityDatabase,
    from_o: &str,
    to_exclusive_o: &str,
) -> rusqlite::Result<ActivityReport> {
    let from = ts(from_o);
    let to_excl = ts(to_exclusive_o);

    let previous = db.previous_event(from_o)?;
    let events = db.event_snapshots(from_o, to_exclusive_o)?;
    let insight_rows = db.insight_rows(from_o, to_exclusive_o)?;

    let check_in = events.first().map(|e| ts(&e.timestamp));
    let check_out = events.last().map(|e| ts(&e.timestamp));
    let duration_total = match (check_in, check_out) {
        (Some(a), Some(b)) => b.duration_since(a).as_secs().max(0),
        _ => 0,
    };

    let mut segment_start = check_in.unwrap_or(from);
    let mut current: Option<EventSnapshotRow> = if events.is_empty() {
        Some(previous.unwrap_or(EventSnapshotRow {
            timestamp: from_o.to_string(),
            event_type: "idle".into(),
            process_name: None,
            window_title: None,
            is_idle: true,
        }))
    } else {
        None
    };

    let mut segments: Vec<TimelineSegment> = vec![];
    let mut usage: Vec<(String, UsageBuilder)> = vec![]; // insertion order preserved
    let mut total_active: i64 = 0;
    let mut total_idle: i64 = 0;

    fn accumulate(
        segments: &mut Vec<TimelineSegment>,
        usage: &mut Vec<(String, UsageBuilder)>,
        total_active: &mut i64,
        total_idle: &mut i64,
        snapshot: &EventSnapshotRow,
        start: Timestamp,
        end: Timestamp,
    ) {
        if end <= start {
            return;
        }
        // truncate toward zero, like (long)TotalSeconds
        let dur = end.duration_since(start).as_secs();
        if dur <= 0 {
            return;
        }
        let (process_name, window_title) = if snapshot.is_idle {
            ("Idle".to_string(), None)
        } else {
            let name = snapshot.process_name.as_deref();
            let title = snapshot.window_title.as_deref();
            (
                match name {
                    Some(n) if !n.trim().is_empty() => n.to_string(),
                    _ => "Unknown".to_string(),
                },
                match title {
                    Some(t) if !t.trim().is_empty() => Some(t.to_string()),
                    _ => None,
                },
            )
        };
        segments.push(TimelineSegment {
            end_utc: format_o(&end),
            duration_seconds: dur,
            is_idle: snapshot.is_idle,
            process_name: process_name.clone(),
            window_title,
        });
        if snapshot.is_idle {
            *total_idle += dur;
            return;
        }
        let idx = usage.iter().position(|(k, _)| k.eq_ignore_ascii_case(&process_name));
        match idx {
            Some(i) => usage[i].1.add_duration(dur, snapshot.window_title.as_deref()),
            None => {
                let mut b = UsageBuilder { total: 0, windows: vec![] };
                b.add_duration(dur, snapshot.window_title.as_deref());
                usage.push((process_name, b));
            }
        }
        *total_active += dur;
    }

    for evt in &events {
        let mut t = ts(&evt.timestamp);
        if let Some(ci) = check_in {
            if t < ci {
                t = ci;
            }
        }
        if let Some(co) = check_out {
            if t > co {
                t = co;
            }
        }
        if let Some(cur) = &current {
            accumulate(&mut segments, &mut usage, &mut total_active, &mut total_idle, cur, segment_start, t);
        }
        current = Some(evt.clone());
        segment_start = t;
    }

    // Remaining tracked time counts as idle (C#: totalIdle = max(0, tracked - active))
    if duration_total > 0 {
        total_idle = (duration_total - total_active).max(0);
    }

    // Insights (C# normalizes application/contentType, truncates VisibleText to 200)
    let mut insights: Vec<ActivityInsight> = vec![];
    let mut index_by_app: Vec<(String, Vec<i64>)> = vec![];
    let mut content_builders: Vec<(String, ContentBuilder)> = vec![];
    for row in &insight_rows {
        let application = blank_to_unknown(row.application.as_deref());
        let content_type = blank_to_unknown(row.content_type.as_deref());
        let mut visible = row.visible_text.clone().unwrap_or_default();
        if visible.chars().count() > 200 {
            visible = visible.chars().take(197).collect::<String>() + "...";
        }
        let insight = ActivityInsight {
            timestamp_utc: format_o(&ts(&row.processed_at)),
            activity_label: row.activity_label.clone().unwrap_or_default(),
            application: application.clone(),
            content_type: content_type.clone(),
            topic: row.topic.clone().unwrap_or_default(),
            action: row.action.clone().unwrap_or_default(),
            summary: row.summary.clone().unwrap_or_default(),
            visible_text: visible,
            confidence: row.confidence,
        };
        let idx = insights.len() as i64;
        insights.push(insight);
        match index_by_app.iter_mut().find(|(k, _)| k.eq_ignore_ascii_case(&application)) {
            Some((_, v)) => v.push(idx),
            None => index_by_app.push((application.clone(), vec![idx])),
        }
        match content_builders.iter_mut().find(|(k, _)| k.eq_ignore_ascii_case(&content_type)) {
            Some((_, b)) => {
                b.count += 1;
                if let Some(t) = &row.topic {
                    if !t.trim().is_empty() {
                        add_unique_ci(&mut b.topics, t.clone());
                    }
                }
                if !application.trim().is_empty() {
                    add_unique_ci(&mut b.applications, application.clone());
                }
            }
            None => {
                let mut b = ContentBuilder { count: 1, topics: vec![], applications: vec![] };
                if let Some(t) = &row.topic {
                    if !t.trim().is_empty() {
                        add_unique_ci(&mut b.topics, t.clone());
                    }
                }
                add_unique_ci(&mut b.applications, application.clone());
                content_builders.push((content_type, b));
            }
        }
    }

    let mut content_breakdown: Vec<ContentTypeSummary> = content_builders
        .into_iter()
        .map(|(ct, b)| ContentTypeSummary {
            content_type: ct,
            activity_count: b.count,
            // C# uses _topics.OrderBy(t => t): culture-aware, not ordinal.
            distinct_topics: { let mut v = b.topics; crate::culture::culture_sort(&mut v); v },
            distinct_applications: { let mut v = b.applications; crate::culture::culture_sort(&mut v); v },
        })
        .collect();
    content_breakdown.sort_by(|a, b| b.activity_count.cmp(&a.activity_count)); // stable, like OrderByDescending

    let mut applications: Vec<ApplicationUsage> = usage
        .into_iter()
        .map(|(name, b)| {
            let mut windows: Vec<WindowUsage> = b
                .windows
                .into_iter()
                .map(|(title, secs)| WindowUsage { title, active_seconds: secs })
                .collect();
            windows.sort_by(|a, b| b.active_seconds.cmp(&a.active_seconds)); // stable
            ApplicationUsage {
                process_name: name.clone(),
                total_active_seconds: b.total,
                windows,
                insight_indices: match_insight_indices(&name, &index_by_app),
            }
        })
        .collect();
    applications.sort_by(|a, b| b.total_active_seconds.cmp(&a.total_active_seconds)); // stable

    Ok(ActivityReport {
        schema_version: "2.0".into(),
        generated_at_utc: now_o(),
        range_start_utc: format_o(&from),
        range_end_utc: format_o(&(to_excl - std::time::Duration::from_nanos(100))), // AddTicks(-1)
        check_in_time_utc: check_in.map(|t| format_o(&t)),
        check_out_time_utc: check_out.map(|t| format_o(&t)),
        total_tracked_seconds: duration_total,
        total_active_seconds: total_active,
        total_idle_seconds: total_idle,
        focus_events_analyzed: events.len() as i64,
        applications,
        detailed_activities: insights,
        content_type_breakdown: content_breakdown,
        segments,
    })
}

// EventSnapshotRow needs Clone for the accumulate loop
#[cfg(test)]
mod tests {
    use super::*;
    use activity_monitor_core::db::ActivityDatabase;
    use activity_monitor_core::models::{ActivityEvent, InferenceResult};
    use activity_monitor_core::timefmt::parse_o;

    #[test]
    fn duration_formats() {
        assert_eq!(format_duration(0), "00:00");
        assert_eq!(format_duration(65), "01:05");
        assert_eq!(format_duration(3600), "01:00:00");
        assert_eq!(format_duration(3661), "01:01:01");
        assert_eq!(format_duration(90061), "1d 01:01:01");
        assert_eq!(format_duration(-5), "00:00");
    }

    #[test]
    fn normalize_and_match() {
        assert_eq!(normalize_key("Google Chrome!"), "googlechrome");
        assert_eq!(normalize_key("  "), "");
        let index_by_app = vec![
            ("code".to_string(), vec![0, 2]),
            ("Google Chrome".to_string(), vec![1]),
        ];
        assert_eq!(match_insight_indices("code", &index_by_app), vec![0, 2]);
        assert_eq!(match_insight_indices("chrome", &index_by_app), vec![1]);
        assert_eq!(match_insight_indices("CODE", &index_by_app), vec![0, 2]);
        assert!(match_insight_indices("notepad", &index_by_app).is_empty());
        assert!(match_insight_indices("  ", &index_by_app).is_empty());
    }

    fn ev(ts: &str, proc: Option<&str>, title: Option<&str>, idle: bool) -> ActivityEvent {
        ActivityEvent {
            id: 0,
            timestamp: parse_o(ts).unwrap(),
            event_type: "FocusChange".into(),
            process_id: Some(1),
            process_name: proc.map(String::from),
            window_title: title.map(String::from),
            is_idle: idle,
            idle_duration_seconds: None,
            metadata: None,
        }
    }

    #[test]
    fn report_segments_and_totals() {
        let dir = std::env::temp_dir().join(format!("am-report-test-{}", uuid::Uuid::new_v4()));
        let db = ActivityDatabase::open(&dir.join("t.db")).unwrap();
        db.initialize().unwrap();
        // previous event before the range (drives the idle seed when range is empty only)
        db.save_activity_event(&ev("2026-11-12T09:00:00.0000000Z", Some("old"), None, false)).unwrap();
        db.save_activity_event(&ev("2026-11-13T10:00:00.0000000Z", Some("code"), Some("main.rs"), false)).unwrap();
        db.save_activity_event(&ev("2026-11-13T10:05:00.0000000Z", None, None, true)).unwrap();
        db.save_activity_event(&ev("2026-11-13T10:10:00.0000000Z", Some("code"), Some("main.rs"), false)).unwrap();
        db.save_inference_result(&InferenceResult {
            request_id: uuid::Uuid::new_v4(),
            processed_at: parse_o("2026-11-13T10:01:00.0000000Z").unwrap(),
            activity_label: "coding".into(),
            application: "code".into(),
            content_type: "development".into(),
            topic: "rust".into(),
            action: "writing".into(),
            summary: "s".into(),
            visible_text: "v".into(),
            detected_objects: None,
            confidence: 0.9,
            raw_response: None,
        })
        .unwrap();

        let r = build_report(
            &db,
            "2026-11-13T00:00:00.0000000Z",
            "2026-11-14T00:00:00.0000000Z",
        )
        .unwrap();

        assert_eq!(r.schema_version, "2.0");
        assert_eq!(r.focus_events_analyzed, 3);
        assert_eq!(r.check_in_time_utc.as_deref(), Some("2026-11-13T10:00:00.0000000Z"));
        assert_eq!(r.check_out_time_utc.as_deref(), Some("2026-11-13T10:10:00.0000000Z"));
        assert_eq!(r.total_tracked_seconds, 600);
        assert_eq!(r.total_active_seconds, 300);
        assert_eq!(r.total_idle_seconds, 300);
        assert_eq!(r.range_end_utc, "2026-11-13T23:59:59.9999999Z");
        // segments: code 10:00-10:05 (300), idle 10:05-10:10 (300)
        assert_eq!(r.segments.len(), 2);
        assert_eq!(r.segments[0].process_name, "code");
        assert_eq!(r.segments[0].duration_seconds, 300);
        assert_eq!(r.segments[0].end_utc, "2026-11-13T10:05:00.0000000Z");
        assert!(r.segments[1].is_idle);
        assert_eq!(r.segments[1].process_name, "Idle");
        assert!(r.segments[1].window_title.is_none());
        // one application: code, 300s, insight index 0
        assert_eq!(r.applications.len(), 1);
        assert_eq!(r.applications[0].process_name, "code");
        assert_eq!(r.applications[0].total_active_seconds, 300);
        assert_eq!(r.applications[0].insight_indices, vec![0]);
        assert_eq!(r.applications[0].windows[0].title, "main.rs");
        // content breakdown
        assert_eq!(r.content_type_breakdown.len(), 1);
        assert_eq!(r.content_type_breakdown[0].content_type, "development");
        assert_eq!(r.content_type_breakdown[0].activity_count, 1);
        assert_eq!(r.content_type_breakdown[0].distinct_topics, vec!["rust".to_string()]);
        assert_eq!(r.detailed_activities.len(), 1);
        assert_eq!(r.detailed_activities[0].application, "code");

        // DistinctTopics/DistinctApplications use C# OrderBy (culture-aware),
        // not ordinal: "apple" must sort before "Zebra".
        assert_ne!(
            { let mut v = vec!["Zebra".to_string(), "apple".to_string()]; v.sort(); v },
            vec!["apple".to_string(), "Zebra".to_string()],
            "ordinal sort must differ here, otherwise this test proves nothing"
        );
    }

    #[test]
    fn report_topics_use_culture_order() {
        let dir = std::env::temp_dir().join(format!("am-report-culture-{}", uuid::Uuid::new_v4()));
        let db = ActivityDatabase::open(&dir.join("t.db")).unwrap();
        db.initialize().unwrap();
        db.save_activity_event(&ev("2026-11-13T10:00:00.0000000Z", Some("code"), Some("a"), false)).unwrap();
        for (i, topic) in ["Zebra", "apple"].iter().enumerate() {
            db.save_inference_result(&InferenceResult {
                request_id: uuid::Uuid::new_v4(),
                processed_at: parse_o(&format!("2026-11-13T10:0{i}:00.0000000Z")).unwrap(),
                activity_label: "coding".into(),
                application: "code".into(),
                content_type: "development".into(),
                topic: topic.to_string(),
                action: "writing".into(),
                summary: "s".into(),
                visible_text: "v".into(),
                detected_objects: None,
                confidence: 0.9,
                raw_response: None,
            })
            .unwrap();
        }
        let r = build_report(&db, "2026-11-13T00:00:00.0000000Z", "2026-11-14T00:00:00.0000000Z").unwrap();
        assert_eq!(
            r.content_type_breakdown[0].distinct_topics,
            vec!["apple".to_string(), "Zebra".to_string()],
            "must match C# OrderBy (CurrentCulture), not ordinal"
        );

        // JSON is pretty-printed with PascalCase keys and null CheckIn omitted when empty
        let json = serde_json::to_string_pretty(&r).unwrap();
        assert!(json.contains("\"SchemaVersion\": \"2.0\""));
        let empty = ActivityReport {
            schema_version: "2.0".into(),
            generated_at_utc: now_o(),
            range_start_utc: "x".into(),
            range_end_utc: "y".into(),
            check_in_time_utc: None,
            check_out_time_utc: None,
            total_tracked_seconds: 0,
            total_active_seconds: 0,
            total_idle_seconds: 0,
            focus_events_analyzed: 0,
            applications: vec![],
            detailed_activities: vec![],
            content_type_breakdown: vec![],
            segments: vec![],
        };
        let j2 = serde_json::to_string_pretty(&empty).unwrap();
        assert!(!j2.contains("CheckInTimeUtc"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
