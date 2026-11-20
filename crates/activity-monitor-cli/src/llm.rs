//! LLM-optimized report (schema "3.0-llm") â€” port of GenerateLlmReportAsync and its
//! helpers (InferProjects, ExtractSignificantMoments, BuildHourlyBreakdown,
//! BuildTopActivities, BuildDayPatterns, BuildExecutiveSummary, ProjectBuilder).
//! Serde field order matches the C# class declaration order for canonical diffs.

use activity_monitor_core::db::{ActivityDatabase, EventSnapshotRow};
use activity_monitor_core::timefmt::{format_o, now_o, parse_o};
use jiff::civil::{Date, DateTime};
use jiff::tz::TimeZone;
use jiff::Timestamp;
use regex::Regex;
use serde::Serialize;

use crate::report::{add_unique_ci, format_duration};

const SYSTEM_PROCESS_FILTER: &[&str] = &[
    "SearchHost", "ShellHost", "ApplicationFrameHost", "StartMenuExperienceHost",
    "ShellExperienceHost", "dwm", "PickerHost", "TextInputHost", "SystemSettings",
    "LockApp", "SearchUI", "Taskmgr", "explorer", "RuntimeBroker", "svchost",
    "csrss", "winlogon", "fontdrvhost", "sihost", "ctfmon",
];

const TECH_TERMS: &[&str] = &[
    "Python", "C#", "JavaScript", "TypeScript", "Java", "Go", "Rust", "Ruby", "PHP",
    "React", "Angular", "Vue", "Node", "Django", "Flask", "ASP.NET", ".NET",
    "Docker", "Kubernetes", "AWS", "Azure", "GCP", "PostgreSQL", "MySQL", "MongoDB",
    "Redis", "GraphQL", "REST", "API", "Git", "GitHub", "VS Code", "Visual Studio",
    "LangChain", "LangGraph", "LangSmith", "OpenAI", "Ollama", "Supabase", "Firebase",
];

fn is_system_process(s: &str) -> bool {
    SYSTEM_PROCESS_FILTER.iter().any(|p| p.eq_ignore_ascii_case(s))
}

fn contains_ci(haystack: &str, needle: &str) -> bool {
    haystack.to_lowercase().contains(&needle.to_lowercase())
}

/// C# Truncate(text, maxLength): Substring(0, max-3) + "..." when longer.
fn truncate(text: &str, max: usize) -> String {
    if text.is_empty() || text.len() <= max {
        text.to_string()
    } else {
        let mut t: String = text.chars().take(max - 3).collect();
        t.push_str("...");
        t
    }
}

fn ts(s: &str) -> Timestamp {
    parse_o(s).expect("stored timestamps are valid O format")
}

fn local_dt(t: &Timestamp) -> DateTime {
    t.to_zoned_datetime(TimeZone::system()).datetime()
}

fn local_hhmm(t: &Timestamp) -> String {
    local_dt(t).strftime("%H:%M").to_string()
}

const MONTHS: &[&str] = &[
    "January", "February", "March", "April", "May", "June",
    "July", "August", "September", "October", "November", "December",
];

/// C# "MMMM d, yyyy" (e.g. "November 13, 2026").
fn fmt_mmmm_d_yyyy(d: &Date) -> String {
    format!("{} {}, {}", MONTHS[(d.month() as usize) - 1], d.day(), d.year())
}

/// C# "h:mm tt" (12-hour, e.g. "1:36 PM").
fn fmt_h_mm_tt(dt: &DateTime) -> String {
    let h = dt.hour();
    let h12 = match h % 12 { 0 => 12, v => v };
    let ampm = if h < 12 { "AM" } else { "PM" };
    format!("{h12}:{:02} {ampm}", dt.minute())
}

/// Group by exact (case-sensitive) key, first-encounter order â€” LINQ GroupBy semantics.
fn group_count(items: &[String]) -> Vec<(String, i64)> {
    let mut out: Vec<(String, i64)> = vec![];
    for it in items {
        match out.iter_mut().find(|(k, _)| k == it) {
            Some((_, c)) => *c += 1,
            None => out.push((it.clone(), 1)),
        }
    }
    out
}

/// Stable descending-by-count sort â€” LINQ OrderByDescending is stable.
fn sort_desc_count(v: &mut Vec<(String, i64)>) {
    v.sort_by(|a, b| b.1.cmp(&a.1));
}

fn distinct(items: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for it in items {
        if !out.contains(&it) {
            out.push(it);
        }
    }
    out
}

// ---------- serde model (C# declaration order) ----------

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct LlmReport {
    pub schema_version: String,
    pub schema_description: LlmSchemaDescription,
    pub generated_at_utc: String,
    pub report_date: String,
    pub working_hours: WorkingHoursInfo,
    pub executive_summary: LlmExecutiveSummary,
    pub hourly_breakdown: Vec<LlmHourlyBreakdown>,
    pub projects: Vec<LlmProjectSummary>,
    pub top_activities: Vec<LlmDetailedActivity>,
    pub significant_moments: Vec<LlmSignificantMoment>,
    pub patterns: LlmDayPatterns,
}

/// Insertion-ordered string map (C# Dictionary preserves insertion order in practice;
/// serde_json::Map would sort alphabetically without the preserve_order feature).
pub struct KeyFields(pub Vec<(String, String)>);

impl Serialize for KeyFields {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut m = serializer.serialize_map(Some(self.0.len()))?;
        for (k, v) in &self.0 {
            m.serialize_entry(k, v)?;
        }
        m.end()
    }
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct LlmSchemaDescription {
    pub purpose: String,
    pub reading_order: Vec<String>,
    pub key_fields: KeyFields,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct WorkingHoursInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<String>,
    pub total_minutes: i32,
    pub total_formatted: String,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct LlmExecutiveSummary {
    pub one_liner: String,
    pub narrative: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accomplishments: Option<String>,
    pub total_insights: i32,
    pub unique_applications: i32,
    pub unique_topics: i32,
    pub primary_applications: Vec<String>,
    pub technologies: Vec<String>,
    pub main_work_categories: Vec<LlmCategoryBreakdown>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct LlmCategoryBreakdown {
    pub category: String,
    pub percentage: i32,
    pub count: i32,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct LlmHourlyBreakdown {
    pub hour: String,
    pub primary_focus: String,
    pub applications: Vec<String>,
    pub topics: Vec<String>,
    pub activity_count: i32,
    pub summary: String,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct LlmProjectSummary {
    pub name: String,
    #[serde(rename = "Type", skip_serializing_if = "Option::is_none")]
    pub project_type: Option<String>,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub work_summary: Option<String>,
    pub time_minutes: i32,
    pub time_formatted: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_files: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topics_worked_on: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activities: Option<Vec<String>>,
    pub insight_count: i32,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct LlmDetailedActivity {
    pub time: String,
    pub application: String,
    pub activity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    pub summary: String,
    pub content_type: String,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct LlmSignificantMoment {
    pub time: String,
    pub event: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_minutes: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub application: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct LlmDayPatterns {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub most_productive_hour: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub longest_focus_session: Option<String>,
    pub context_switches: i32,
    pub frequent_topics: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observations: Option<Vec<String>>,
}

// ---------- internal insight ----------

#[derive(Clone)]
struct Insight {
    timestamp_utc: String,
    activity_label: String,
    application: String,
    content_type: String,
    topic: String,
    action: String,
    summary: String,
    visible_text: String,
    confidence: f64,
}

// ---------- ProjectBuilder ----------

struct ProjectBuilder {
    name: String,
    event_count: i64,
    total_seconds: i64,
    insights: Vec<Insight>,
    files: Vec<String>,
    activities: Vec<String>,
}

impl ProjectBuilder {
    fn new(name: &str) -> Self {
        ProjectBuilder { name: name.to_string(), event_count: 0, total_seconds: 0, insights: vec![], files: vec![], activities: vec![] }
    }

    fn add_event(&mut self, evt: &EventSnapshotRow, file_re: &Regex) {
        self.event_count += 1;
        self.total_seconds += 30; // each event ~30s, as in C#
        if let Some(wt) = &evt.window_title {
            if !wt.is_empty() {
                if let Some(caps) = file_re.captures(wt) {
                    if let Some(m) = caps.get(1) {
                        add_unique_ci(&mut self.files, m.as_str().to_string());
                    }
                }
            }
        }
    }

    fn add_insight(&mut self, insight: &Insight) {
        self.insights.push(insight.clone());
        if !insight.activity_label.is_empty() {
            add_unique_ci(&mut self.activities, insight.activity_label.clone());
        }
        if !insight.action.is_empty() && !insight.topic.is_empty() {
            add_unique_ci(&mut self.activities, format!("{} {}", insight.action, insight.topic).trim().to_string());
        }
    }

    fn to_summary(&self) -> LlmProjectSummary {
        let cts: Vec<String> = self.insights.iter().filter(|i| !i.content_type.is_empty()).map(|i| i.content_type.clone()).collect();
        let mut gc = group_count(&cts);
        sort_desc_count(&mut gc);
        let project_type = gc.first().map(|(k, _)| match k.as_str() {
            "code" => "Software Development".to_string(),
            "document" => "Documentation".to_string(),
            "web" => "Web Research".to_string(),
            "chat" => "Communication".to_string(),
            "video" => "Media".to_string(),
            "image" => "Visual/Design".to_string(),
            other => other.to_string(),
        });

        // top insight: confidence desc, then summary length desc (stable)
        let mut order: Vec<&Insight> = self.insights.iter().collect();
        order.sort_by(|a, b| {
            b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal)
                .then(b.summary.len().cmp(&a.summary.len()))
        });
        let description = match order.first() {
            Some(i) if !i.summary.is_empty() => i.summary.clone(),
            _ => format!("Work related to {}", self.name),
        };
        let description = if description.len() > 200 { truncate(&description, 200) } else { description };

        let work_actions: Vec<String> = distinct(
            self.insights.iter()
                .filter(|i| !i.action.is_empty() && !i.topic.is_empty())
                .map(|i| format!("{} {}", i.action, i.topic).trim().to_string())
                .collect(),
        );
        let work_actions: Vec<String> = work_actions.into_iter().take(5).collect();
        let work_summary = if work_actions.is_empty() { None } else { Some(work_actions.join("; ")) };

        let topics: Vec<String> = distinct(
            self.insights.iter().filter(|i| !i.topic.is_empty()).map(|i| i.topic.clone()).collect(),
        );
        let topics: Vec<String> = topics.into_iter().take(8).collect();

        let time_minutes = (self.total_seconds / 60) as i32;
        LlmProjectSummary {
            name: self.name.clone(),
            project_type,
            description,
            work_summary,
            time_minutes,
            time_formatted: format_duration_static(time_minutes as i64 * 60),
            key_files: if self.files.is_empty() { None } else { Some(self.files.iter().take(10).cloned().collect()) },
            topics_worked_on: if topics.is_empty() { None } else { Some(topics) },
            activities: if self.activities.is_empty() { None } else { Some(self.activities.iter().take(8).cloned().collect()) },
            insight_count: self.insights.len() as i32,
        }
    }
}

/// C# ProjectBuilder.FormatDurationStatic: "{h}h {m}m" when >= 1h else "{m}m".
fn format_duration_static(seconds: i64) -> String {
    let s = seconds.max(0);
    if s >= 3600 {
        format!("{}h {}m", s / 3600, (s % 3600) / 60)
    } else {
        format!("{}m", s / 60)
    }
}

/// C# title.Split(new[] { " - ", " â€“ ", " | " }, RemoveEmptyEntries).
fn split_title(title: &str) -> Vec<String> {
    let re = Regex::new(r" - | â€“ | \| ").unwrap();
    re.split(title).filter(|s| !s.is_empty()).map(|s| s.to_string()).collect()
}

fn infer_projects(events: &[EventSnapshotRow], insights: &[Insight]) -> Vec<LlmProjectSummary> {
    let path_patterns = [
        Regex::new(r"(?i)[/\\]([^/\\]+)[/\\](src|lib|app|bin|obj|node_modules|\.git)[/\\]?").unwrap(),
        Regex::new(r"(?i)[A-Za-z]:[/\\](?:Office|Projects?|Work|Dev|Code|repos?)[/\\]([^/\\]+)").unwrap(),
        Regex::new(r"(?i)[/\\]([^/\\]+)\s*[-â€“]\s*(Visual Studio|VS Code|Code|Antigravity)").unwrap(),
    ];
    let file_re = Regex::new(r"(?i)([a-zA-Z_][a-zA-Z0-9_]*\.(py|cs|js|ts|json|yaml|yml|md|txt|html|css|sql))").unwrap();

    let mut project_data: Vec<(String, ProjectBuilder)> = vec![];
    for evt in events.iter().filter(|e| !e.is_idle && e.window_title.as_deref().map(|t| !t.is_empty()).unwrap_or(false)) {
        let title = evt.window_title.as_deref().unwrap();
        let mut project_name: Option<String> = None;
        for re in &path_patterns {
            if let Some(caps) = re.captures(title) {
                if let Some(m) = caps.get(1) {
                    project_name = Some(m.as_str().trim().to_string());
                    break;
                }
            }
        }
        if project_name.is_none() {
            if let Some(proc) = &evt.process_name {
                if !proc.is_empty() {
                    let parts = split_title(title);
                    if let Some(first) = parts.first() {
                        let f = first.trim();
                        if f.len() > 2 && f.len() < 50 {
                            project_name = Some(format!("{proc}: {}", truncate(f, 30)));
                        }
                    }
                }
            }
        }
        if let Some(name) = project_name {
            if name.is_empty() {
                continue;
            }
            match project_data.iter_mut().find(|(k, _)| k.eq_ignore_ascii_case(&name)) {
                Some((_, b)) => b.add_event(evt, &file_re),
                None => {
                    let mut b = ProjectBuilder::new(&name);
                    b.add_event(evt, &file_re);
                    project_data.push((name, b));
                }
            }
        }
    }

    for ins in insights {
        for (key, b) in project_data.iter_mut() {
            if contains_ci(&ins.application, key)
                || contains_ci(key, &ins.application)
                || (!ins.topic.is_empty() && contains_ci(key, &ins.topic))
            {
                b.add_insight(ins);
                break;
            }
        }
    }

    let mut entries: Vec<(String, ProjectBuilder)> = project_data
        .into_iter()
        .filter(|(_, b)| b.event_count > 0)
        .collect();
    entries.sort_by(|a, b| b.1.total_seconds.cmp(&a.1.total_seconds)); // stable
    entries.into_iter().take(10).map(|(_, b)| b.to_summary()).collect()
}

fn extract_significant_moments(events: &[EventSnapshotRow], insights: &[Insight]) -> Vec<LlmSignificantMoment> {
    let mut moments: Vec<LlmSignificantMoment> = vec![];
    if events.is_empty() {
        return moments;
    }
    let first = &events[0];
    moments.push(LlmSignificantMoment {
        time: local_hhmm(&ts(&first.timestamp)),
        event: "Started work".to_string(),
        context: Some(format!("Began with {}", first.process_name.as_deref().unwrap_or("unknown application"))),
        duration_minutes: None,
        application: first.process_name.clone(),
    });

    let mut last_app: Option<String> = first.process_name.clone();
    let mut last_switch = ts(&first.timestamp);
    let mut consecutive: i32 = 1;

    for evt in &events[1..] {
        if evt.is_idle {
            continue;
        }
        if evt.process_name != last_app {
            let dur_secs = ts(&evt.timestamp).duration_since(last_switch).as_secs();
            let dur_mins = dur_secs as f64 / 60.0;
            if dur_mins >= 5.0 && consecutive >= 3 {
                // insight within 10 minutes of lastSwitchTime, highest confidence (stable)
                let mut relevant: Option<&Insight> = None;
                for i in insights {
                    let diff_min = (ts(&i.timestamp_utc).duration_since(last_switch).as_secs().abs()) as f64 / 60.0;
                    if diff_min < 10.0 {
                        let better = match relevant {
                            None => true,
                            Some(r) => i.confidence > r.confidence,
                        };
                        if better {
                            relevant = Some(i);
                        }
                    }
                }
                let context = match relevant {
                    Some(ri) if !ri.summary.is_empty() => ri.summary.clone(),
                    Some(ri) => format!("{} {}", ri.action, ri.topic).trim().to_string(),
                    None => format!("Working in {}", last_app.as_deref().unwrap_or("")),
                };
                if !context.trim().is_empty() {
                    moments.push(LlmSignificantMoment {
                        time: local_hhmm(&last_switch),
                        event: format!("Focus session: {}", last_app.as_deref().unwrap_or("")),
                        context: Some(truncate(&context, 120)),
                        duration_minutes: Some(dur_mins as i32),
                        application: last_app.clone(),
                    });
                }
            }
            last_app = evt.process_name.clone();
            last_switch = ts(&evt.timestamp);
            consecutive = 1;
        } else {
            consecutive += 1;
        }
    }

    if events.len() > 1 {
        let last = &events[events.len() - 1];
        let last_dur_mins = ts(&last.timestamp).duration_since(last_switch).as_secs() as f64 / 60.0;
        moments.push(LlmSignificantMoment {
            time: local_hhmm(&ts(&last.timestamp)),
            event: "Ended work".to_string(),
            context: Some(format!("Last activity in {}", last.process_name.as_deref().unwrap_or("unknown application"))),
            duration_minutes: if last_dur_mins >= 1.0 { Some(last_dur_mins as i32) } else { None },
            application: last.process_name.clone(),
        });
    }

    moments.truncate(20);
    moments
}

fn build_hourly_breakdown(insights: &[Insight]) -> Vec<LlmHourlyBreakdown> {
    let mut groups: Vec<(i8, Vec<&Insight>)> = vec![];
    for i in insights {
        let h = local_dt(&ts(&i.timestamp_utc)).hour();
        match groups.iter_mut().find(|(gh, _)| *gh == h) {
            Some((_, v)) => v.push(i),
            None => groups.push((h, vec![i])),
        }
    }
    groups.sort_by_key(|(h, _)| *h);

    let mut hourly = vec![];
    for (hour, his) in groups {
        let cts: Vec<String> = his.iter().filter(|i| !i.content_type.is_empty()).map(|i| i.content_type.clone()).collect();
        let mut gc = group_count(&cts);
        sort_desc_count(&mut gc);
        let top_content_type = gc.first().map(|(k, _)| k.clone()).unwrap_or_else(|| "mixed".to_string());

        let apps: Vec<String> = his.iter().filter(|i| !i.application.is_empty()).map(|i| i.application.clone()).collect();
        let mut ga = group_count(&apps);
        sort_desc_count(&mut ga);
        let top_apps: Vec<String> = ga.into_iter().take(3).map(|(k, _)| k).collect();

        let topics: Vec<String> = distinct(his.iter().filter(|i| !i.topic.is_empty()).map(|i| i.topic.clone()).collect())
            .into_iter().take(5).collect();

        // top insight by confidence (stable first-max)
        let mut top: Option<&Insight> = None;
        for i in &his {
            let better = match top { None => true, Some(t) => i.confidence > t.confidence };
            if better { top = Some(i); }
        }
        let summary = match top {
            Some(t) => format!("{} {}", t.action, t.topic).trim().to_string(),
            None => format!("{top_content_type} activities"),
        };

        hourly.push(LlmHourlyBreakdown {
            hour: format!("{hour:02}:00-{hour:02}:59"),
            primary_focus: top_content_type,
            applications: top_apps,
            topics,
            activity_count: his.len() as i32,
            summary,
        });
    }
    hourly
}

fn build_top_activities(insights: &[Insight]) -> Vec<LlmDetailedActivity> {
    let mut sel: Vec<&Insight> = insights
        .iter()
        .filter(|i| !i.summary.is_empty() && i.summary.len() > 20 && i.confidence >= 0.6)
        .collect();
    sel.sort_by(|a, b| {
        b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal)
            .then(b.summary.len().cmp(&a.summary.len()))
    });
    sel.into_iter().take(20)
        .map(|i| LlmDetailedActivity {
            time: local_dt(&ts(&i.timestamp_utc)).strftime("%H:%M").to_string(),
            application: i.application.clone(),
            activity: i.activity_label.clone(),
            topic: if i.topic.is_empty() { None } else { Some(i.topic.clone()) },
            action: if i.action.is_empty() { None } else { Some(i.action.clone()) },
            summary: truncate(&i.summary, 250),
            content_type: i.content_type.clone(),
        })
        .collect()
}

fn build_day_patterns(events: &[EventSnapshotRow], insights: &[Insight]) -> LlmDayPatterns {
    // most productive hour
    let hours: Vec<String> = insights.iter().map(|i| (local_dt(&ts(&i.timestamp_utc)).hour() as i32).to_string()).collect();
    let mut gh = group_count(&hours);
    sort_desc_count(&mut gh);
    let most_productive_hour = gh.first().map(|(k, c)| format!("{k}:00 ({c} activities)"));

    // context switches (ordinal comparison, as C# string !=)
    let mut last_app: Option<String> = None;
    let mut switches: i32 = 0;
    for evt in events.iter().filter(|e| !e.is_idle) {
        if last_app.is_some() && evt.process_name != last_app {
            switches += 1;
        }
        last_app = evt.process_name.clone();
    }

    // longest focus session (no flush after the loop, as in C#)
    let mut focus_app: Option<String> = None;
    let mut focus_start: Option<Timestamp> = None;
    let mut longest_secs: i64 = 0;
    let mut longest_app: Option<String> = None;
    for evt in events.iter().filter(|e| !e.is_idle) {
        if focus_app.is_none() || evt.process_name != focus_app {
            if let (Some(fa), Some(fs)) = (&focus_app, focus_start) {
                let dur = ts(&evt.timestamp).duration_since(fs).as_secs();
                if dur > longest_secs {
                    longest_secs = dur;
                    longest_app = Some(fa.clone());
                }
            }
            focus_app = evt.process_name.clone();
            focus_start = Some(ts(&evt.timestamp));
        }
    }
    let longest_focus_session = if longest_secs as f64 / 60.0 >= 5.0 {
        Some(format!("{} ({} minutes)", longest_app.as_deref().unwrap_or(""), (longest_secs as f64 / 60.0) as i32))
    } else {
        None
    };

    let topics: Vec<String> = insights.iter().filter(|i| !i.topic.is_empty()).map(|i| i.topic.clone()).collect();
    let mut gt = group_count(&topics);
    sort_desc_count(&mut gt);
    let frequent_topics: Vec<String> = gt.into_iter().take(8).map(|(k, c)| format!("{k} ({c}x)")).collect();

    let mut observations: Vec<String> = vec![];
    let n = insights.len() as i32;
    let code_pct = if n > 0 { insights.iter().filter(|i| i.content_type == "code").count() as i32 * 100 / n } else { 0 };
    if code_pct > 50 {
        observations.push(format!("Heavy coding day ({code_pct}% of activities were code-related)"));
    } else if code_pct > 30 {
        observations.push(format!("Balanced day with significant coding ({code_pct}%)"));
    }
    if switches > 50 {
        observations.push(format!("High context switching ({switches} app changes) - may indicate multitasking or interruptions"));
    } else if switches < 20 && events.len() > 50 {
        observations.push("Good focus with minimal context switching".to_string());
    }
    let web_pct = if n > 0 { insights.iter().filter(|i| i.content_type == "web").count() as i32 * 100 / n } else { 0 };
    if web_pct > 40 {
        observations.push(format!("Significant web research/browsing ({web_pct}%)"));
    }

    LlmDayPatterns {
        most_productive_hour,
        longest_focus_session,
        context_switches: switches,
        frequent_topics,
        observations: if observations.is_empty() { None } else { Some(observations) },
    }
}

fn extract_tech_keywords(text: &str, keywords: &mut Vec<String>) {
    if text.is_empty() {
        return;
    }
    for term in TECH_TERMS {
        if contains_ci(text, term) {
            add_unique_ci(keywords, term.to_string());
        }
    }
}

fn build_accomplishments(insights: &[Insight], projects: &[LlmProjectSummary]) -> Option<String> {
    let mut accomplishments: Vec<String> = vec![];

    let code_insights: Vec<&Insight> = insights.iter().filter(|i| i.content_type == "code").collect();
    if !code_insights.is_empty() {
        let files_edited = code_insights
            .iter()
            .filter(|i| contains_ci(&i.action, "edit") || contains_ci(&i.action, "writ"))
            .count();
        if files_edited > 0 {
            accomplishments.push(format!("Edited/wrote code in {files_edited} sessions"));
        }
    }

    let research: Vec<&Insight> = insights
        .iter()
        .filter(|i| i.content_type == "web" || contains_ci(&i.action, "search") || contains_ci(&i.action, "read"))
        .collect();
    if research.len() > 5 {
        let topics: Vec<String> = distinct(research.iter().filter(|i| !i.topic.is_empty()).map(|i| i.topic.clone()).collect())
            .into_iter().take(3).collect();
        if !topics.is_empty() {
            accomplishments.push(format!("Researched: {}", topics.join(", ")));
        }
    }

    for project in projects.iter().take(2) {
        if project.time_minutes > 30 && project.activities.as_ref().map(|a| !a.is_empty()).unwrap_or(false) {
            let acts: Vec<String> = project.activities.clone().unwrap().into_iter().take(3).collect();
            accomplishments.push(format!("Worked on {}: {}", project.name, acts.join(", ")));
        }
    }

    if accomplishments.is_empty() {
        None
    } else {
        Some(format!("{}.", accomplishments.join(". ")))
    }
}

#[allow(clippy::too_many_arguments)]
fn build_narrative(
    from_local: &Date,
    check_in: Option<Timestamp>,
    check_out: Option<Timestamp>,
    total_tracked: i64,
    projects: &[LlmProjectSummary],
    insights: &[Insight],
) -> String {
    let mut sb = String::new();
    if let (Some(ci), Some(co)) = (check_in, check_out) {
        sb.push_str(&format!(
            "On {}, work started at {} and ended at {} ",
            fmt_mmmm_d_yyyy(from_local),
            fmt_h_mm_tt(&local_dt(&ci)),
            fmt_h_mm_tt(&local_dt(&co))
        ));
        sb.push_str(&format!(
            "({} tracked across {} analyzed activities). ",
            format_duration(total_tracked),
            insights.len()
        ));
    }
    if !projects.is_empty() {
        let top = &projects[0];
        sb.push_str(&format!("The primary focus was {}", top.name));
        if let Some(t) = &top.project_type {
            if !t.is_empty() {
                sb.push_str(&format!(" ({t})"));
            }
        }
        if top.time_minutes > 0 {
            sb.push_str(&format!(", accounting for approximately {}", top.time_formatted));
        }
        if top.insight_count > 0 {
            sb.push_str(&format!(" with {} recorded activities", top.insight_count));
        }
        sb.push_str(". ");
        if let Some(topics) = &top.topics_worked_on {
            if !topics.is_empty() {
                let t: Vec<String> = topics.iter().take(4).cloned().collect();
                sb.push_str(&format!("Topics covered: {}. ", t.join(", ")));
            }
        }
        if projects.len() > 1 {
            sb.push_str("Additional work included: ");
            let others: Vec<String> = projects.iter().skip(1).take(3)
                .map(|p| format!("{} ({})", p.name, p.time_formatted))
                .collect();
            sb.push_str(&others.join(", "));
            sb.push_str(". ");
        }
    }
    let cts: Vec<String> = insights.iter().filter(|i| !i.content_type.is_empty()).map(|i| i.content_type.clone()).collect();
    let mut gc = group_count(&cts);
    sort_desc_count(&mut gc);
    let breakdown: Vec<String> = gc.into_iter().take(3).map(|(k, c)| format!("{k} ({c})")).collect();
    if !breakdown.is_empty() {
        sb.push_str(&format!("Activity breakdown by type: {}. ", breakdown.join(", ")));
    }
    let unique_activities: Vec<String> = distinct(
        insights.iter()
            .filter(|i| !i.activity_label.is_empty() && i.confidence >= 0.6)
            .map(|i| i.activity_label.clone())
            .collect(),
    );
    let unique_activities: Vec<String> = unique_activities.into_iter().take(6).collect();
    if !unique_activities.is_empty() {
        sb.push_str(&format!("Notable activities: {}.", unique_activities.join(", ")));
    }
    sb.trim().to_string()
}

#[allow(clippy::too_many_arguments)]
fn build_executive_summary(
    from_local: &Date,
    check_in: Option<Timestamp>,
    check_out: Option<Timestamp>,
    total_tracked: i64,
    insights: &[Insight],
    projects: &[LlmProjectSummary],
    events: &[EventSnapshotRow],
) -> LlmExecutiveSummary {
    let active: Vec<&EventSnapshotRow> = events.iter().filter(|e| !e.is_idle).collect();
    let app_names: Vec<String> = active.iter().filter(|e| e.process_name.as_deref().map(|p| !p.is_empty()).unwrap_or(false)).map(|e| e.process_name.clone().unwrap()).collect();
    let unique_apps = distinct(app_names.clone());
    let unique_topics = distinct(insights.iter().filter(|i| !i.topic.is_empty()).map(|i| i.topic.clone()).collect());
    let mut ga = group_count(&app_names);
    sort_desc_count(&mut ga);
    let top_apps: Vec<String> = ga.into_iter().take(5).map(|(k, _)| k).collect();

    let cts: Vec<String> = insights.iter().filter(|i| !i.content_type.is_empty()).map(|i| i.content_type.clone()).collect();
    let mut gc = group_count(&cts);
    sort_desc_count(&mut gc);
    let n = insights.len();
    let content_breakdown: Vec<LlmCategoryBreakdown> = gc
        .into_iter()
        .take(5)
        .map(|(k, c)| LlmCategoryBreakdown {
            percentage: if n > 0 { (c as f64 * 100.0 / n as f64) as i32 } else { 0 },
            count: c as i32,
            category: k,
        })
        .collect();

    let mut tech_keywords: Vec<String> = vec![];
    for i in insights {
        extract_tech_keywords(&i.topic, &mut tech_keywords);
        extract_tech_keywords(&i.application, &mut tech_keywords);
        extract_tech_keywords(&i.summary, &mut tech_keywords);
    }

    let narrative = build_narrative(from_local, check_in, check_out, total_tracked, projects, insights);
    let accomplishments = build_accomplishments(insights, projects);
    let one_liner = match projects.first() {
        Some(p) => format!(
            "Focused on {} ({}) with {} analyzed activities across {} applications",
            p.name, p.time_formatted, insights.len(), unique_apps.len()
        ),
        None => format!(
            "Worked for {} with {} analyzed activities across {} applications",
            format_duration(total_tracked), insights.len(), unique_apps.len()
        ),
    };

    LlmExecutiveSummary {
        one_liner,
        narrative,
        accomplishments,
        total_insights: n as i32,
        unique_applications: unique_apps.len() as i32,
        unique_topics: unique_topics.len() as i32,
        primary_applications: top_apps,
        technologies: tech_keywords.into_iter().take(10).collect(),
        main_work_categories: content_breakdown,
    }
}

pub fn build_llm_report(
    db: &ActivityDatabase,
    from_o: &str,
    to_exclusive_o: &str,
    from_local: &Date,
) -> rusqlite::Result<LlmReport> {
    let events: Vec<EventSnapshotRow> = db
        .event_snapshots(from_o, to_exclusive_o)?
        .into_iter()
        .filter(|e| match &e.process_name {
            Some(p) if !p.is_empty() => !is_system_process(p),
            _ => true,
        })
        .collect();

    let mut insights: Vec<Insight> = vec![];
    for row in db.insight_rows(from_o, to_exclusive_o)? {
        let application = row.application.clone().unwrap_or_else(|| "Unknown".to_string());
        if is_system_process(&application) {
            continue;
        }
        insights.push(Insight {
            timestamp_utc: parse_o(&row.processed_at).map(|t| format_o(&t)).unwrap_or_else(|_| row.processed_at.clone()),
            activity_label: row.activity_label.clone().unwrap_or_default(),
            application,
            content_type: row.content_type.clone().unwrap_or_else(|| "Unknown".to_string()),
            topic: row.topic.clone().unwrap_or_default(),
            action: row.action.clone().unwrap_or_default(),
            summary: row.summary.clone().unwrap_or_default(),
            visible_text: truncate(row.visible_text.as_deref().unwrap_or(""), 150),
            confidence: row.confidence,
        });
    }

    let check_in = events.first().map(|e| ts(&e.timestamp));
    let check_out = events.last().map(|e| ts(&e.timestamp));
    let total_tracked = match (check_in, check_out) {
        (Some(a), Some(b)) => b.duration_since(a).as_secs().max(0),
        _ => 0,
    };

    let projects = infer_projects(&events, &insights);
    let significant_moments = extract_significant_moments(&events, &insights);
    let hourly_breakdown = build_hourly_breakdown(&insights);
    let top_activities = build_top_activities(&insights);
    let patterns = build_day_patterns(&events, &insights);
    let executive_summary = build_executive_summary(from_local, check_in, check_out, total_tracked, &insights, &projects, &events);

    let key_fields = KeyFields(vec![
        ("ExecutiveSummary".to_string(), "Start here for high-level understanding of the day".to_string()),
        ("HourlyBreakdown".to_string(), "Hour-by-hour view of what the user was working on".to_string()),
        ("Projects".to_string(), "Activity grouped by inferred project or task context".to_string()),
        ("TopActivities".to_string(), "Most significant individual activities with full context".to_string()),
        ("SignificantMoments".to_string(), "Key events and transitions throughout the day".to_string()),
        ("Patterns".to_string(), "Observed patterns, productivity insights, and frequent topics".to_string()),
    ]);

    Ok(LlmReport {
        schema_version: "3.0-llm".to_string(),
        schema_description: LlmSchemaDescription {
            purpose: "Daily activity report optimized for LLM consumption and analysis".to_string(),
            reading_order: vec![
                "ExecutiveSummary".to_string(),
                "HourlyBreakdown".to_string(),
                "Projects".to_string(),
                "TopActivities".to_string(),
                "SignificantMoments".to_string(),
                "Patterns".to_string(),
            ],
            key_fields,
        },
        generated_at_utc: now_o(),
        report_date: from_local.strftime("%Y-%m-%d").to_string(),
        working_hours: WorkingHoursInfo {
            start: check_in.map(|t| local_dt(&t).strftime("%H:%M").to_string()),
            end: check_out.map(|t| local_dt(&t).strftime("%H:%M").to_string()),
            total_minutes: (total_tracked / 60) as i32,
            total_formatted: format_duration(total_tracked),
        },
        executive_summary,
        hourly_breakdown,
        projects,
        top_activities,
        significant_moments,
        patterns,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_matches_csharp() {
        assert_eq!(truncate("", 150), "");
        assert_eq!(truncate("short", 150), "short");
        let long: String = "x".repeat(151);
        assert_eq!(truncate(&long, 150).len(), 150);
        assert!(truncate(&long, 150).ends_with("..."));
        let exact: String = "y".repeat(150);
        assert_eq!(truncate(&exact, 150), exact);
    }

    #[test]
    fn csharp_date_and_time_formats() {
        let d = Date::new(2026, 11, 13).unwrap();
        assert_eq!(fmt_mmmm_d_yyyy(&d), "November 13, 2026");
        let dt = DateTime::new(2026, 11, 13, 13, 36, 0, 0).unwrap();
        assert_eq!(fmt_h_mm_tt(&dt), "1:36 PM");
        let dt0 = DateTime::new(2026, 11, 13, 0, 5, 0, 0).unwrap();
        assert_eq!(fmt_h_mm_tt(&dt0), "12:05 AM");
        let dt12 = DateTime::new(2026, 11, 13, 12, 0, 0, 0).unwrap();
        assert_eq!(fmt_h_mm_tt(&dt12), "12:00 PM");
    }

    #[test]
    fn duration_static() {
        assert_eq!(format_duration_static(0), "0m");
        assert_eq!(format_duration_static(300), "5m");
        assert_eq!(format_duration_static(3600), "1h 0m");
        assert_eq!(format_duration_static(3661), "1h 1m");
    }

    #[test]
    fn project_regexes() {
        let pats = [
            Regex::new(r"(?i)[/\\]([^/\\]+)[/\\](src|lib|app|bin|obj|node_modules|\.git)[/\\]?").unwrap(),
            Regex::new(r"(?i)[A-Za-z]:[/\\](?:Office|Projects?|Work|Dev|Code|repos?)[/\\]([^/\\]+)").unwrap(),
            Regex::new(r"(?i)[/\\]([^/\\]+)\s*[-â€“]\s*(Visual Studio|VS Code|Code|Antigravity)").unwrap(),
        ];
        let m = pats[0].captures("C:\\Users\\x\\rust-version\\src\\main.rs").unwrap();
        assert_eq!(m.get(1).unwrap().as_str(), "rust-version");
        let m = pats[1].captures("C:\\Projects\\MyApp\\README.md").unwrap();
        assert_eq!(m.get(1).unwrap().as_str(), "MyApp");
        let m = pats[2].captures("C:\\dev\\rust-version - Code").unwrap();
        assert_eq!(m.get(1).unwrap().as_str().trim(), "rust-version"); // infer_projects trims the group
        assert!(pats[2].captures("rust-version - Code").is_none()); // pattern 3 needs a path separator
        assert!(pats[0].captures("plain title").is_none());
    }

    #[test]
    fn split_title_matches_csharp() {
        assert_eq!(split_title("main.rs - rust-version"), vec!["main.rs", "rust-version"]);
        assert_eq!(split_title("a | b â€“ c"), vec!["a", "b", "c"]);
        assert_eq!(split_title(" - "), Vec::<String>::new());
    }

    #[test]
    fn group_order_is_stable_desc() {
        let items = vec!["b".to_string(), "a".to_string(), "b".to_string(), "c".to_string(), "a".to_string()];
        let mut g = group_count(&items);
        sort_desc_count(&mut g);
        assert_eq!(g, vec![("b".to_string(), 2), ("a".to_string(), 2), ("c".to_string(), 1)]);
    }
}
