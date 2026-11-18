//! Read-only tools over the Activity Monitor SQLite DB (+ Ollama for the ask tool).

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, MutexGuard};

use jiff::tz::TimeZone;
use jiff::Timestamp;
use serde_json::{json, Value};

use activity_monitor_core::db::ActivityDatabase;
use activity_monitor_core::ollama::OllamaClient;
use activity_monitor_core::timefmt;

/// Mutex because rusqlite's Connection is Send but not Sync, and the HTTP
/// transport spawns one task per connection.
pub struct ToolState {
    pub db: Mutex<Option<ActivityDatabase>>,
    pub ollama: Option<OllamaClient>,
}

fn date_bounds(date: Option<&str>) -> Result<(String, String, String), String> {
    let date = match date {
        Some(d) if !d.is_empty() => d.to_string(),
        _ => timefmt::today_local_date().to_string(),
    };
    timefmt::parse_local_date(&date).map_err(|e| format!("Invalid date '{date}': {e}"))?;
    let from = timefmt::local_date_to_utc_o(&date).map_err(|e| e.to_string())?;
    let to = timefmt::next_day_local_midnight_utc_o(&date).map_err(|e| e.to_string())?;
    Ok((date, from, to))
}

/// Inclusive local-date range [from, to] as UTC "O" bounds; both default today.
fn range_bounds(from: Option<&str>, to: Option<&str>) -> Result<(String, String), String> {
    let today = timefmt::today_local_date().to_string();
    let parse = |d: &str| -> Result<String, String> {
        timefmt::parse_local_date(d).map_err(|e| format!("Invalid date '{d}': {e}"))?;
        Ok(d.to_string())
    };
    let from_d = match from {
        Some(d) if !d.is_empty() => parse(d)?,
        _ => today.clone(),
    };
    let to_d = match to {
        Some(d) if !d.is_empty() => parse(d)?,
        _ => today,
    };
    if from_d > to_d {
        return Err(format!("'from' ({from_d}) must be <= 'to' ({to_d})"));
    }
    let from_o = timefmt::local_date_to_utc_o(&from_d).map_err(|e| e.to_string())?;
    let to_o = timefmt::next_day_local_midnight_utc_o(&to_d).map_err(|e| e.to_string())?;
    Ok((from_o, to_o))
}

fn need_db(state: &ToolState) -> Result<MutexGuard<'_, Option<ActivityDatabase>>, String> {
    let guard = state.db.lock().map_err(|e| e.to_string())?;
    if guard.is_none() {
        return Err("Database not found. Run the service once or pass --db / ACTIVITY_MONITOR_DB.".to_string());
    }
    Ok(guard)
}

pub fn tool_defs() -> Value {
    let date_props = json!({
        "date": {
            "type": "string",
            "description": "Local calendar date YYYY-MM-DD (default: today)"
        }
    });
    let read_only = json!({ "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false });
    let obj = |props: Value, required: Vec<&str>| json!({ "type": "object", "properties": props, "required": required });
    json!([
        {
            "name": "todays_timeline",
            "title": "Today's activity timeline",
            "description": "Every recorded activity event for a day (app switches, idle periods) joined with the AI-written label/summary when one exists.",
            "inputSchema": obj(date_props.clone(), vec![]),
            "outputSchema": obj(json!({ "date": {"type":"string"}, "events": {"type":"array"} }), vec!["date", "events"]),
            "annotations": read_only
        },
        {
            "name": "session_summary",
            "title": "Day summary by app and category",
            "description": "Aggregated view of a day: time per app, per content category, and the AI labels observed.",
            "inputSchema": obj(date_props.clone(), vec![]),
            "outputSchema": obj(json!({ "date": {"type":"string"} }), vec!["date"]),
            "annotations": read_only
        },
        {
            "name": "activity_at",
            "title": "What was on screen at a time",
            "description": "Events in a window around a timestamp, with the nearest event marked. Timestamp accepts UTC \"O\" format or local 'YYYY-MM-DD HH:mm:ss'.",
            "inputSchema": obj({
                let mut p = json!({ "timestamp": { "type": "string", "description": "UTC O-format or local 'YYYY-MM-DD HH:mm:ss'" } }).as_object().unwrap().clone();
                p.insert("windowMinutes".into(), json!({ "type": "number", "description": "Half-window in minutes (default 5)", "default": 5 }));
                Value::Object(p)
            }, vec!["timestamp"]),
            "outputSchema": obj(json!({ "events": {"type":"array"}, "nearest": {"type":["object","null"]} }), vec!["events"]),
            "annotations": read_only
        },
        {
            "name": "daily_report",
            "title": "Structured daily report (JSON)",
            "description": "A compact JSON report for a day: event counts, top apps, category distribution, AI insights. Same data the CLI report is built from.",
            "inputSchema": obj(date_props.clone(), vec![]),
            "outputSchema": obj(json!({ "date": {"type":"string"} }), vec!["date"]),
            "annotations": read_only
        },
        {
            "name": "search_activity",
            "title": "Search recorded activity (ranked + fuzzy)",
            "description": "Hybrid search over past activity days: FTS5 bm25-ranked lexical match over the AI-written fields (app, topic, summary, visible text) plus a trigram-fuzzy leg for typos and shared vocabulary, RRF-merged. Fully local — no ML runtime, no GPU.",
            "inputSchema": obj({
                let p = json!({
                    "query": { "type": "string", "description": "Keywords or a natural-language question, e.g. 'tableau' or 'what was I working on yesterday'" },
                    "from": { "type": "string", "description": "Start local date YYYY-MM-DD (default: today)" },
                    "to": { "type": "string", "description": "End local date YYYY-MM-DD (default: today)" },
                    "limit": { "type": "number", "description": "Max hits (default 20, max 200)", "default": 20 }
                }).as_object().unwrap().clone();
                Value::Object(p)
            }, vec!["query"]),
            "outputSchema": obj(json!({ "query": {"type":"string"}, "count": {"type":"number"}, "matches": {"type":"array"} }), vec!["query", "count", "matches"]),
            "annotations": read_only
        },
        {
            "name": "ask_about_my_day",
            "title": "Ask the local AI about your day",
            "description": "Answers a question about your recorded day using the local Ollama model, grounded only in your own activity data.",
            "inputSchema": obj({
                let mut p = date_props.as_object().unwrap().clone();
                p.insert("question".into(), json!({ "type": "string", "description": "Question about your day, e.g. 'What did I work on this afternoon?'" }));
                Value::Object(p)
            }, vec!["question"]),
            "outputSchema": obj(json!({ "answer": {"type":"string"}, "date": {"type":"string"} }), vec!["answer", "date"]),
            "annotations": read_only
        },
        {
            "name": "weekly_digest",
            "title": "Weekly shape-of-your-work digest (Markdown)",
            "description": "Aggregates a 7-day window (default: last 7 days, or explicit from/to dates) into a Markdown digest: events, context switches, top apps, AI category distribution, top topics, busiest hours. Read-only and fully local; the same digest the CLI writes with report --week.",
            "inputSchema": obj({
                let p = json!({
                    "from": { "type": "string", "description": "Start local date YYYY-MM-DD (default: 7 days ago)" },
                    "to": { "type": "string", "description": "End local date YYYY-MM-DD (default: today)" }
                }).as_object().unwrap().clone();
                Value::Object(p)
            }, vec![]),
            "outputSchema": obj(json!({ "from": {"type":"string"}, "to": {"type":"string"}, "markdown": {"type":"string"}, "stats": {"type":"object"} }), vec!["from", "to", "markdown"]),
            "annotations": read_only
        }
    ])
}

pub async fn call(state: &Arc<ToolState>, name: &str, args: &Value, cancel: &AtomicBool) -> Result<Value, String> {
    match name {
        "todays_timeline" => todays_timeline(state, args),
        "session_summary" => session_summary(state, args),
        "activity_at" => activity_at(state, args),
        "daily_report" => daily_report(state, args),
        "search_activity" => search_activity(state, args),
        "ask_about_my_day" => ask_about_my_day(state, args, cancel).await,
        "weekly_digest" => weekly_digest(state, args),
        _ => Err(format!("Unknown tool: {name}")),
    }
}

fn todays_timeline(state: &ToolState, args: &Value) -> Result<Value, String> {
    let guard = need_db(state)?;
    let db = guard.as_ref().unwrap();
    let (date, from, to) = date_bounds(args.get("date").and_then(|d| d.as_str()))?;
    let rows = db.timeline_entries(&from, &to).map_err(|e| e.to_string())?;
    let events: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "timestamp": r.timestamp,
                "eventType": r.event_type,
                "processName": r.process_name,
                "windowTitle": r.window_title,
                "isIdle": r.is_idle,
                "activityLabel": r.activity_label,
                "application": r.application,
                "contentType": r.content_type,
                "topic": r.topic,
                "action": r.action,
                "summary": r.summary,
                "visibleText": r.visible_text
            })
        })
        .collect();
    Ok(json!({ "date": date, "events": events }))
}

fn session_summary(state: &ToolState, args: &Value) -> Result<Value, String> {
    let guard = need_db(state)?;
    let db = guard.as_ref().unwrap();
    let (date, from, to) = date_bounds(args.get("date").and_then(|d| d.as_str()))?;
    let categories = db.summary_categories(&from, &to).map_err(|e| e.to_string())?;
    let top_apps = db.summary_top_apps(&from, &to).map_err(|e| e.to_string())?;
    let (total, coding, web, docs, videos, comm) = db.summary_distribution(&from, &to).map_err(|e| e.to_string())?;
    let labels = db.detailed_rows(&from, &to, 50).map_err(|e| e.to_string())?;
    Ok(json!({
        "date": date,
        "totalActivities": total,
        "distribution": { "coding": coding, "web": web, "documents": docs, "videos": videos, "communication": comm },
        "categories": categories.iter().map(|(c, n, t)| json!({ "category": c, "count": n, "topics": t })).collect::<Vec<_>>(),
        "topApps": top_apps.iter().map(|(a, n)| json!({ "app": a, "count": n })).collect::<Vec<_>>(),
        "labels": labels.iter().map(|r| json!({
            "at": r.processed_at, "label": r.activity_label, "app": r.application,
            "contentType": r.content_type, "topic": r.topic, "action": r.action,
            "summary": r.summary, "confidence": r.confidence
        })).collect::<Vec<_>>()
    }))
}

fn parse_any_timestamp(s: &str) -> Result<Timestamp, String> {
    if let Ok(ts) = timefmt::parse_o(s) {
        return Ok(ts);
    }
    // local civil datetime (with or without 'T')
    let s2 = s.replace(' ', "T");
    if let Ok(dt) = s2.parse::<jiff::civil::DateTime>() {
        return dt
            .to_zoned(TimeZone::system())
            .map(|z| z.timestamp())
            .map_err(|e| e.to_string());
    }
    if let Ok(d) = s.parse::<jiff::civil::Date>() {
        return d
            .at(0, 0, 0, 0)
            .to_zoned(TimeZone::system())
            .map(|z| z.timestamp())
            .map_err(|e| e.to_string());
    }
    Err(format!("Cannot parse timestamp '{s}' (expected UTC O-format or local 'YYYY-MM-DD HH:mm:ss')"))
}

fn activity_at(state: &ToolState, args: &Value) -> Result<Value, String> {
    let guard = need_db(state)?;
    let db = guard.as_ref().unwrap();
    let ts_raw = args.get("timestamp").and_then(|t| t.as_str()).ok_or("Missing required argument: timestamp")?;
    let center = parse_any_timestamp(ts_raw)?;
    let window_secs = args.get("windowMinutes").and_then(|w| w.as_f64()).unwrap_or(5.0).max(0.0) as i64 * 60;
    let from = timefmt::format_o(&(center - std::time::Duration::from_secs(window_secs as u64)));
    let to = timefmt::format_o(&(center + std::time::Duration::from_secs(window_secs as u64)));
    let rows = db.query_event_rows(&from, &to, 500).map_err(|e| e.to_string())?;
    let events: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "timestamp": r.timestamp, "eventType": r.event_type,
                "processName": r.process_name, "windowTitle": r.window_title, "isIdle": r.is_idle
            })
        })
        .collect();
    let nearest = rows
        .iter()
        .filter_map(|r| {
            timefmt::parse_o(&r.timestamp)
                .ok()
                .map(|t| ((t - center).abs().get_seconds().abs(), r))
        })
        .min_by_key(|(d, _)| *d)
        .map(|(_, r)| {
            json!({
                "timestamp": r.timestamp, "eventType": r.event_type,
                "processName": r.process_name, "windowTitle": r.window_title, "isIdle": r.is_idle
            })
        });
    Ok(json!({ "center": timefmt::format_o(&center), "windowMinutes": window_secs / 60, "events": events, "nearest": nearest }))
}

fn daily_report(state: &ToolState, args: &Value) -> Result<Value, String> {
    let guard = need_db(state)?;
    let db = guard.as_ref().unwrap();
    let (date, from, to) = date_bounds(args.get("date").and_then(|d| d.as_str()))?;
    let total_events = db.count_events_since(&from).map_err(|e| e.to_string())?;
    let top_apps = db.summary_top_apps(&from, &to).map_err(|e| e.to_string())?;
    let categories = db.summary_categories(&from, &to).map_err(|e| e.to_string())?;
    let (total, coding, web, docs, videos, comm) = db.summary_distribution(&from, &to).map_err(|e| e.to_string())?;
    let insights = db.insight_rows(&from, &to).map_err(|e| e.to_string())?;
    let snapshots = db.event_snapshots(&from, &to).map_err(|e| e.to_string())?;
    Ok(json!({
        "date": date,
        "eventCount": total_events,
        "inferenceCount": total,
        "distribution": { "coding": coding, "web": web, "documents": docs, "videos": videos, "communication": comm },
        "topApps": top_apps.iter().map(|(a, n)| json!({ "app": a, "count": n })).collect::<Vec<_>>(),
        "categories": categories.iter().map(|(c, n, t)| json!({ "category": c, "count": n, "topics": t })).collect::<Vec<_>>(),
        "insights": insights.iter().map(|r| json!({
            "processedAt": r.processed_at, "label": r.activity_label, "summary": r.summary, "confidence": r.confidence
        })).collect::<Vec<_>>(),
        "sampleEvents": snapshots.iter().take(100).map(|r| json!({
            "timestamp": r.timestamp, "eventType": r.event_type,
            "processName": r.process_name, "windowTitle": r.window_title, "isIdle": r.is_idle
        })).collect::<Vec<_>>()
    }))
}

/// v1.2 hybrid search: FTS5 bm25 lexical leg + trigram fuzzy leg (see
/// core::rank for what was rejected and why).
fn search_activity(state: &Arc<ToolState>, args: &Value) -> Result<Value, String> {
    let query = args
        .get("query")
        .and_then(|q| q.as_str())
        .map(str::trim)
        .filter(|q| !q.is_empty())
        .ok_or("Missing required argument: query")?;
    let guard = need_db(state)?;
    let db = guard.as_ref().unwrap();
    let (from_o, to_o) = range_bounds(
        args.get("from").and_then(|d| d.as_str()),
        args.get("to").and_then(|d| d.as_str()),
    )?;
    let limit = args
        .get("limit")
        .and_then(|l| l.as_i64())
        .unwrap_or(20)
        .clamp(1, 200);
    let hits = activity_monitor_core::db::search_hybrid(db.conn(), query, &from_o, &to_o, limit)
        .map_err(|e| e.to_string())?;
    let matches: Vec<Value> = hits
        .iter()
        .map(|h| {
            json!({
                "timestamp": h.processed_at,
                "matchType": h.match_type,
                "activityLabel": h.activity_label,
                "application": h.application,
                "topic": h.topic,
                "summary": h.summary,
                "visibleText": h.visible_text,
            })
        })
        .collect();
    Ok(json!({
        "query": query,
        "count": matches.len(),
        "matches": matches,
    }))
}

/// v1.3 weekly digest: same aggregation as `cli report --week`, returned as
/// Markdown plus structured stats. Default window: last 7 days.
fn weekly_digest(state: &ToolState, args: &Value) -> Result<Value, String> {
    let guard = need_db(state)?;
    let db = guard.as_ref().unwrap();
    let today = timefmt::today_local_date().to_string();
    let to_d = args.get("to").and_then(|d| d.as_str()).filter(|s| !s.is_empty()).unwrap_or(&today).to_string();
    timefmt::parse_local_date(&to_d).map_err(|e| format!("Invalid date '{to_d}': {e}"))?;
    let from_d = match args.get("from").and_then(|d| d.as_str()).filter(|s| !s.is_empty()) {
        Some(d) => {
            timefmt::parse_local_date(d).map_err(|e| format!("Invalid date '{d}': {e}"))?;
            d.to_string()
        }
        None => timefmt::days_ago_o(7).get(..10).unwrap_or(&today).to_string(),
    };
    let from_o = timefmt::local_date_to_utc_o(&from_d).map_err(|e| e.to_string())?;
    let to_o = timefmt::next_day_local_midnight_utc_o(&to_d).map_err(|e| e.to_string())?;
    let stats = activity_monitor_core::digest::weekly_stats(db, &from_o, &to_o).map_err(|e| e.to_string())?;
    let markdown = activity_monitor_core::digest::render_markdown(&stats);
    let (total, coding, web, docs, videos, comm) = stats.distribution;
    Ok(json!({
        "from": from_d,
        "to": to_d,
        "markdown": markdown,
        "stats": {
            "totalEvents": stats.total_events,
            "activeEvents": stats.active_events,
            "idleEvents": stats.idle_events,
            "focusChanges": stats.focus_changes,
            "inferenceCount": stats.inference_count,
            "distribution": { "coding": coding, "web": web, "documents": docs, "videos": videos, "communication": comm },
            "topAppsEvents": stats.top_apps_events.iter().map(|(a, c)| json!({ "app": a, "count": c })).collect::<Vec<_>>(),
            "topAppsInference": stats.top_apps_inference.iter().map(|(a, c)| json!({ "app": a, "count": c })).collect::<Vec<_>>(),
            "topTopics": stats.top_topics.iter().map(|(t, c)| json!({ "topic": t, "count": c })).collect::<Vec<_>>(),
            "busiestHoursUtc": stats.busiest_hours.iter().map(|(h, c)| json!({ "hour": h, "count": c })).collect::<Vec<_>>(),
            "total": total
        }
    }))
}

async fn ask_about_my_day(state: &Arc<ToolState>, args: &Value, cancel: &AtomicBool) -> Result<Value, String> {
    let (date, from, to) = date_bounds(args.get("date").and_then(|d| d.as_str()))?;
    let question = args.get("question").and_then(|q| q.as_str()).ok_or("Missing required argument: question")?;
    let ollama = state
        .ollama
        .as_ref()
        .ok_or_else(|| "Ollama is not configured (appsettings.json missing next to the exe).".to_string())?;

    // Build the day context under the lock, then drop it before awaiting so
    // the future stays Send for the HTTP transport.
    let context = {
        let guard = need_db(state)?;
        let db = guard.as_ref().unwrap();
        let timeline = db.timeline_entries(&from, &to).map_err(|e| e.to_string())?;
        let mut context = String::new();
        for r in timeline.iter().rev().take(200) {
            let label = r.activity_label.clone().unwrap_or_else(|| r.event_type.clone());
            let app = r.process_name.clone().unwrap_or_default();
            let title = r.window_title.clone().unwrap_or_default();
            context.push_str(&format!("{} | {} | {} | {}\n", r.timestamp, label, app, title));
            if context.len() > 6000 {
                break;
            }
        }
        let categories = db.summary_categories(&from, &to).map_err(|e| e.to_string())?;
        for (c, n, t) in &categories {
            context.push_str(&format!("category {c}: {n}x topics: {}\n", t.clone().unwrap_or_default()));
        }
        context
    };

    let system = "You are the assistant of Activity Monitor, a privacy-first app that records the user's own screen activity locally. Answer the user's question using ONLY the day log provided. Be concise, quote timestamps when relevant, and say when the log has no answer.";
    let user = format!("Day log for {date}:\n{context}\n\nQuestion: {question}");
    let answer = ollama
        .chat_text(system, &user, cancel)
        .await
        .ok_or_else(|| "Ollama did not answer (server down or model missing). The raw day log is available via todays_timeline.".to_string())?;
    Ok(json!({ "date": date, "answer": answer }))
}
