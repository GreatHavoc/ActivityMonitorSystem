use std::process::ExitCode;

mod culture;
mod llm;
mod report;

use activity_monitor_core::db::{resolve_cli_db_path, ActivityDatabase};
use activity_monitor_core::timefmt;
use clap::{CommandFactory, Parser, Subcommand};
use owo_colors::OwoColorize;
use tabled::{Table, Tabled};

#[derive(Parser)]
#[command(name = "activity-monitor-cli", about = "Activity Monitor CLI (Rust port)", version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Query activity events (raw data)
    Query {
        #[arg(long)]
        from: Option<String>,
        #[arg(long)]
        to: Option<String>,
        #[arg(long, default_value_t = 100)]
        limit: i64,
    },
    /// Show activity statistics
    Stats,
    /// Show comprehensive timeline of all activities
    Timeline {
        #[arg(long)]
        from: Option<String>,
        #[arg(long)]
        to: Option<String>,
    },
    /// Show detailed activity logs with AI analysis
    Detailed {
        #[arg(long)]
        from: Option<String>,
        #[arg(long)]
        to: Option<String>,
        #[arg(long, default_value_t = 100)]
        limit: i64,
    },
    /// Show daily summary of activities by category
    Summary {
        #[arg(long)]
        date: Option<String>,
    },
    /// Export an activity report (JSON schema 2.0)
    Report {
        #[arg(long)]
        from: Option<String>,
        #[arg(long)]
        to: Option<String>,
        #[arg(long)]
        output: Option<String>,
        #[arg(long, default_value = "json")]
        format: String,
        /// LLM-optimized report (schema 3.0-llm) â€” Phase 4, not yet ported
        #[arg(long)]
        llm: bool,
        /// v1.3: weekly digest as a Markdown file instead of the JSON report
        #[arg(long)]
        week: bool,
    },
    /// Export raw events and AI summaries as JSON or Markdown (data ownership, v1.3)
    Export {
        #[arg(long)]
        from: Option<String>,
        #[arg(long)]
        to: Option<String>,
        #[arg(long)]
        output: Option<String>,
        #[arg(long, default_value = "json")]
        format: String,
    },
}

#[derive(Tabled)]
struct QueryRow {
    #[tabled(rename = "Timestamp")]
    timestamp: String,
    #[tabled(rename = "Event Type")]
    event_type: String,
    #[tabled(rename = "Process")]
    process: String,
    #[tabled(rename = "Window Title")]
    window_title: String,
    #[tabled(rename = "Status")]
    status: String,
}

#[derive(Tabled)]
struct MetricRow {
    #[tabled(rename = "Metric")]
    metric: String,
    #[tabled(rename = "Value")]
    value: String,
}

#[derive(Tabled)]
struct ProcessRow {
    #[tabled(rename = "Process")]
    process: String,
    #[tabled(rename = "Event Count")]
    count: String,
}

#[derive(Tabled)]
struct CategoryRow {
    #[tabled(rename = "Category")]
    category: String,
    #[tabled(rename = "Count")]
    count: String,
    #[tabled(rename = "Topics Covered")]
    topics: String,
}

#[derive(Tabled)]
struct AppRow {
    #[tabled(rename = "Application")]
    app: String,
    #[tabled(rename = "Inference Count")]
    count: String,
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Some(Commands::Query { from, to, limit }) => run_query(from, to, limit),
        Some(Commands::Stats) => run_stats(),
        Some(Commands::Timeline { from, to }) => run_timeline(from, to),
        Some(Commands::Detailed { from, to, limit }) => run_detailed(from, to, limit),
        Some(Commands::Summary { date }) => run_summary(date),
        Some(Commands::Report { from, to, output, format, llm, week }) => run_report(from, to, output, format, llm, week),
        Some(Commands::Export { from, to, output, format }) => run_export(from, to, output, format),
        None => {
            let mut cmd = Cli::command();
            let _ = cmd.print_help();
            println!();
            ExitCode::SUCCESS
        }
    }
}

/// Same resolution order as the C# CLI; missing DB exits 1 (plan contract).
fn resolve_db() -> Result<std::path::PathBuf, ExitCode> {
    match resolve_cli_db_path() {
        Some(path) => Ok(path),
        None => {
            eprintln!("{}", "Database not found!".red());
            eprintln!("{}", "Make sure the service is running first!".yellow());
            Err(ExitCode::from(1))
        }
    }
}

fn open_db() -> Result<ActivityDatabase, ExitCode> {
    let path = match resolve_db() {
        Ok(p) => p,
        Err(c) => return Err(c),
    };
    println!("{}", format!("Using database: {}", path.display()).green());
    ActivityDatabase::open(&path).map_err(|e| {
        eprintln!("Error: {e}");
        ExitCode::from(1)
    })
}

/// The C# report command prints no "Using database" line.
fn open_db_quiet() -> Result<ActivityDatabase, ExitCode> {
    let path = match resolve_db() {
        Ok(p) => p,
        Err(c) => return Err(c),
    };
    ActivityDatabase::open(&path).map_err(|e| {
        eprintln!("Error: {e}");
        ExitCode::from(1)
    })
}

fn fail<E: std::fmt::Display>(e: E) -> ExitCode {
    eprintln!("Error: {e}");
    ExitCode::from(1)
}

/// --from/--to bound: local date/datetime converted to UTC "O" (tz-bug fix).
/// Defaults match the C# command: from = local midnight today, to = now.
fn date_bound(arg: Option<&str>, default: String) -> Result<String, ExitCode> {
    match arg {
        None => Ok(default),
        Some(s) => timefmt::local_date_to_utc_o(s).map_err(|e| {
            eprintln!("Error: invalid date '{s}': {e}");
            ExitCode::from(1)
        }),
    }
}

/// Header date (yyyy-MM-dd) for an optional --from/--to/--date argument; default today (local).
fn header_date(arg: Option<&str>) -> Result<String, ExitCode> {
    match arg {
        None => Ok(timefmt::today_local_date().strftime("%Y-%m-%d").to_string()),
        Some(s) => timefmt::parse_local_date(s)
            .map(|d| d.strftime("%Y-%m-%d").to_string())
            .map_err(|e| {
                eprintln!("Error: invalid date '{s}': {e}");
                ExitCode::from(1)
            }),
    }
}

/// "to" bound for day-inclusive commands: local midnight of the day after the
/// given date (or after today), as UTC "O" â€” replaces AddDays(1).ToUniversalTime().
fn next_day_bound(arg: Option<&str>) -> Result<String, ExitCode> {
    let today = timefmt::today_local_date().strftime("%Y-%m-%d").to_string();
    let s = arg.unwrap_or(&today);
    timefmt::next_day_local_midnight_utc_o(s).map_err(|e| {
        eprintln!("Error: invalid date '{s}': {e}");
        ExitCode::from(1)
    })
}

fn local_display_o(ts_o: &str, fmt: &str) -> String {
    timefmt::parse_o(ts_o)
        .map(|t| t.to_zoned_datetime(jiff::tz::TimeZone::system()).strftime(fmt).to_string())
        .unwrap_or_else(|_| ts_o.to_string())
}

fn run_query(from: Option<String>, to: Option<String>, limit: i64) -> ExitCode {
    let db = match open_db() {
        Ok(d) => d,
        Err(c) => return c,
    };
    let from_o = match date_bound(from.as_deref(), timefmt::today_start_utc_o()) {
        Ok(v) => v,
        Err(c) => return c,
    };
    let to_o = match date_bound(to.as_deref(), timefmt::now_o()) {
        Ok(v) => v,
        Err(c) => return c,
    };
    let rows = match db.query_event_rows(&from_o, &to_o, limit) {
        Ok(r) => r,
        Err(e) => return fail(e),
    };
    let table_rows: Vec<QueryRow> = rows
        .iter()
        .map(|r| QueryRow {
            timestamp: local_display_o(&r.timestamp, "%Y-%m-%d %H:%M:%S"),
            event_type: r.event_type.clone(),
            process: r.process_name.clone().unwrap_or_else(|| "-".into()),
            window_title: trunc(&r.window_title.clone().unwrap_or_else(|| "-".into()), 50),
            status: if r.is_idle {
                "Idle".yellow().to_string()
            } else {
                "Active".green().to_string()
            },
        })
        .collect();
    println!("{}", Table::new(table_rows));
    ExitCode::SUCCESS
}

fn run_stats() -> ExitCode {
    let db = match open_db() {
        Ok(d) => d,
        Err(c) => return c,
    };
    // C# interpolated DateTime.Today:O into the events-today SQL; the port uses
    // the UTC form (tz-bug fix) via a bound parameter.
    let total_events = match db.count("SELECT COUNT(*) FROM ActivityEvents") {
        Ok(v) => v,
        Err(e) => return fail(e),
    };
    let total_inferences = match db.count("SELECT COUNT(*) FROM InferenceResults") {
        Ok(v) => v,
        Err(e) => return fail(e),
    };
    let events_today = match db.count_events_since(&timefmt::today_start_utc_o()) {
        Ok(v) => v,
        Err(e) => return fail(e),
    };
    let top = match db.top_processes(&timefmt::days_ago_o(7)) {
        Ok(v) => v,
        Err(e) => return fail(e),
    };

    println!("{}", "Activity Monitor Statistics".yellow().bold());
    println!();
    println!(
        "{}",
        Table::new(vec![
            MetricRow { metric: "Total Events".into(), value: format_n0(total_events) },
            MetricRow { metric: "Total Inferences".into(), value: format_n0(total_inferences) },
            MetricRow { metric: "Events Today".into(), value: format_n0(events_today) },
        ])
    );
    println!();
    println!("{}", "Top Processes (Last 7 Days)".yellow().bold());
    println!();
    let rows: Vec<ProcessRow> = top
        .into_iter()
        .map(|(p, c)| ProcessRow { process: p, count: format_n0(c) })
        .collect();
    println!("{}", Table::new(rows));
    ExitCode::SUCCESS
}

fn run_timeline(from: Option<String>, to: Option<String>) -> ExitCode {
    let db = match open_db() {
        Ok(d) => d,
        Err(c) => return c,
    };
    let from_h = match header_date(from.as_deref()) {
        Ok(v) => v,
        Err(c) => return c,
    };
    let to_h = match header_date(to.as_deref()) {
        Ok(v) => v,
        Err(c) => return c,
    };
    let from_o = match date_bound(from.as_deref(), timefmt::today_start_utc_o()) {
        Ok(v) => v,
        Err(c) => return c,
    };
    let to_o = match date_bound(to.as_deref(), timefmt::now_o()) {
        Ok(v) => v,
        Err(c) => return c,
    };
    let mut entries = match db.timeline_entries(&from_o, &to_o) {
        Ok(r) => r,
        Err(e) => return fail(e),
    };
    entries.reverse(); // SQL is DESC; the C# renders ascending

    println!("{}", format!("Activity Timeline: {from_h} to {to_h}").yellow());
    println!();
    for e in &entries {
        let time = local_display_o(&e.timestamp, "%H:%M:%S");
        if e.is_idle {
            println!("{} {}", time.dimmed(), "â— IDLE".yellow());
        } else if let Some(label) = &e.activity_label {
            if !label.is_empty() {
                println!("{} {}", time.dimmed(), format!("â— {label}").cyan());
                if let Some(app) = &e.application {
                    if !app.is_empty() {
                        println!("         {} {}", "App:".dimmed(), app);
                    }
                }
                if let Some(topic) = &e.topic {
                    if !topic.is_empty() {
                        println!("         {} {}", "Topic:".dimmed(), topic);
                    }
                }
                if let Some(action) = &e.action {
                    if !action.is_empty() {
                        println!("         {} {}", "Action:".dimmed(), action);
                    }
                }
                if let Some(summary) = &e.summary {
                    if !summary.is_empty() {
                        println!("         {} {}", "Summary:".dimmed(), trunc(summary, 100));
                    }
                }
                if let Some(text) = &e.visible_text {
                    if !text.is_empty() {
                        println!("         {} {}", "Text:".dimmed(), trunc(text, 80).dimmed());
                    }
                }
            } else {
                print_process_line(&time, &e.process_name, &e.window_title);
            }
        } else {
            print_process_line(&time, &e.process_name, &e.window_title);
        }
        println!();
    }
    println!("{}", format!("Total activities: {}", entries.len()).green());
    ExitCode::SUCCESS
}

fn print_process_line(time: &str, process: &Option<String>, window: &Option<String>) {
    let process_info = process.clone().unwrap_or_else(|| "Unknown".into());
    let window_info = window.clone().map(|w| trunc(&w, 50)).unwrap_or_default();
    println!("{} {} {}", time.dimmed(), format!("â— {process_info}").green(), window_info.dimmed());
}

fn run_detailed(from: Option<String>, to: Option<String>, limit: i64) -> ExitCode {
    let db = match open_db() {
        Ok(d) => d,
        Err(c) => return c,
    };
    let from_h = match header_date(from.as_deref()) {
        Ok(v) => v,
        Err(c) => return c,
    };
    let to_h = match header_date(to.as_deref()) {
        Ok(v) => v,
        Err(c) => return c,
    };
    let from_o = match date_bound(from.as_deref(), timefmt::today_start_utc_o()) {
        Ok(v) => v,
        Err(c) => return c,
    };
    // C# treats --to as inclusive by advancing one day (ToUniversalTime is applied there)
    let to_o = match next_day_bound(to.as_deref()) {
        Ok(v) => v,
        Err(c) => return c,
    };
    let rows = match db.detailed_rows(&from_o, &to_o, limit) {
        Ok(r) => r,
        Err(e) => return fail(e),
    };

    println!("{}", format!("Detailed Activity Analysis: {from_h} to {to_h}").yellow());
    println!();
    let count = rows.len();
    for r in &rows {
        let ts = local_display_o(&r.processed_at, "%Y-%m-%d %H:%M:%S");
        let label = r.activity_label.clone().unwrap_or_else(|| "Unknown activity".into());
        let or_na = |v: &Option<String>| v.clone().unwrap_or_else(|| "N/A".into());
        println!("{}", ts.bold());
        println!();
        println!("{}", label.cyan().bold());
        println!();
        println!("{} {}", "Application:".dimmed(), or_na(&r.application));
        println!("{} {}", "Content Type:".dimmed(), or_na(&r.content_type));
        println!("{} {}", "Topic:".dimmed(), or_na(&r.topic));
        println!("{} {}", "Action:".dimmed(), or_na(&r.action));
        println!();
        println!("{}", "Summary:".yellow());
        println!("{}", or_na(&r.summary));
        println!();
        println!("{}", "Visible Text:".dimmed());
        println!("{}", or_na(&r.visible_text).dimmed());
        println!();
        // C# {confidence:P0} â€” percent, 0 decimals
        println!("{} {}%", "Confidence:".dimmed(), (r.confidence * 100.0).round() as i64);
        println!();
    }
    if count == 0 {
        println!("{}", "No detailed activity data found for this period.".yellow());
        println!("{}", "Make sure Ollama is running and the service has been capturing screens.".dimmed());
    } else {
        println!("{}", format!("Showing {count} detailed activities").green());
    }
    ExitCode::SUCCESS
}

fn run_summary(date: Option<String>) -> ExitCode {
    let db = match open_db() {
        Ok(d) => d,
        Err(c) => return c,
    };
    let target_h = match header_date(date.as_deref()) {
        Ok(v) => v,
        Err(c) => return c,
    };
    // summary converts the local day bounds to UTC (C# does ToUniversalTime here)
    let target = date.clone().unwrap_or_else(|| target_h.clone());
    let from_o = match timefmt::local_date_to_utc_o(&target) {
        Ok(v) => v,
        Err(e) => return fail(e),
    };
    let to_o = match timefmt::next_day_local_midnight_utc_o(&target) {
        Ok(v) => v,
        Err(e) => return fail(e),
    };

    println!("{}", format!("Daily Summary: {target_h}").yellow());
    println!();

    let categories = match db.summary_categories(&from_o, &to_o) {
        Ok(r) => r,
        Err(e) => return fail(e),
    };
    println!("{}", "Activity by Content Type".cyan());
    let cat_rows: Vec<CategoryRow> = categories
        .into_iter()
        .map(|(cat, count, topics)| CategoryRow {
            category: cat,
            count: count.to_string(),
            topics: trunc(&topics.unwrap_or_default(), 60),
        })
        .collect();
    println!("{}", Table::new(cat_rows));
    println!();

    let apps = match db.summary_top_apps(&from_o, &to_o) {
        Ok(r) => r,
        Err(e) => return fail(e),
    };
    println!("{}", "Top Applications by Inference Count".cyan());
    let app_rows: Vec<AppRow> = apps
        .into_iter()
        .map(|(app, count)| AppRow { app, count: count.to_string() })
        .collect();
    println!("{}", Table::new(app_rows));
    println!();

    let (total, coding, web, docs, videos, comm) = match db.summary_distribution(&from_o, &to_o) {
        Ok(v) => v,
        Err(e) => return fail(e),
    };
    if total > 0 {
        println!("{}", "Activity Distribution by Inference Count".green().bold());
        let items = [
            ("Coding", coding),
            ("Web Browsing", web),
            ("Documents", docs),
            ("Videos", videos),
            ("Communication", comm),
        ];
        let max = items.iter().map(|(_, c)| *c).max().unwrap_or(0).max(1);
        for (label, count) in items {
            let bar = "â–ˆ".repeat((count * 60 / max) as usize);
            println!("{label:<14} |{bar} {count}");
        }
    }
    ExitCode::SUCCESS
}

/// v1.3: `report --week` writes the weekly digest as a Markdown file.
fn run_week(db: &ActivityDatabase, from: Option<String>, to: Option<String>, output: Option<String>) -> ExitCode {
    let from_o = match date_bound(from.as_deref(), timefmt::days_ago_o(7)) {
        Ok(v) => v,
        Err(c) => return c,
    };
    let to_o = match date_bound(to.as_deref(), timefmt::now_o()) {
        Ok(v) => v,
        Err(c) => return c,
    };
    let stats = match activity_monitor_core::digest::weekly_stats(db, &from_o, &to_o) {
        Ok(s) => s,
        Err(e) => return fail(e),
    };
    let md = activity_monitor_core::digest::render_markdown(&stats);
    let output_path = output.unwrap_or_else(|| {
        format!(
            "activity-digest-{}.md",
            from_o.get(..10).unwrap_or("period")
        )
    });
    if let Some(parent) = std::path::Path::new(&output_path).parent() {
        if !parent.as_os_str().is_empty() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                return fail(e);
            }
        }
    }
    if let Err(e) = std::fs::write(&output_path, md) {
        return fail(e);
    }
    println!(
        "{}",
        format!(
            "Weekly digest exported to {output_path} ({} events, {} AI summaries)",
            stats.total_events, stats.inference_count
        )
        .green()
    );
    ExitCode::SUCCESS
}

/// v1.3: `export` writes the raw records (events + AI summaries) as JSON or
/// Markdown, so the user's history is a file they own. v1.5 adds `--format
/// sqlite`: a whole-database snapshot via VACUUM INTO (from/to do not apply).
fn run_export(from: Option<String>, to: Option<String>, output: Option<String>, format: String) -> ExitCode {
    let db = match open_db_quiet() {
        Ok(d) => d,
        Err(c) => return c,
    };
    let format_norm = if format.trim().is_empty() {
        "json".to_string()
    } else {
        format.trim().to_lowercase()
    };
    if format_norm == "sqlite" {
        return export_sqlite(&db, output);
    }
    let from_o = match date_bound(from.as_deref(), timefmt::days_ago_o(30)) {
        Ok(v) => v,
        Err(c) => return c,
    };
    let to_o = match date_bound(to.as_deref(), timefmt::now_o()) {
        Ok(v) => v,
        Err(c) => return c,
    };
    let events = match db.query_event_rows(&from_o, &to_o, 1_000_000) {
        Ok(r) => r,
        Err(e) => return fail(e),
    };
    let inferences = match db.detailed_rows(&from_o, &to_o, 1_000_000) {
        Ok(r) => r,
        Err(e) => return fail(e),
    };
    let content = match format_norm.as_str() {
        "md" | "markdown" => {
            let mut md = String::new();
            md.push_str(&format!("# Activity export\n\nFrom: {from_o}\nTo: {to_o}\n\n"));
            md.push_str(&format!("## Events ({})\n\n", events.len()));
            for e in &events {
                md.push_str(&format!(
                    "- {} | {} | {} | {} | {}\n",
                    e.timestamp,
                    e.event_type,
                    e.process_name.as_deref().unwrap_or("-"),
                    trunc(&e.window_title.clone().unwrap_or_default(), 60),
                    if e.is_idle { "Idle" } else { "Active" }
                ));
            }
            md.push_str(&format!("\n## AI summaries ({})\n\n", inferences.len()));
            for r in &inferences {
                md.push_str(&format!(
                    "### {} ({})\n\nApp: {} | Topic: {} | Confidence: {}%\n\n{}\n\n",
                    r.activity_label.as_deref().unwrap_or("Unknown"),
                    r.processed_at,
                    r.application.as_deref().unwrap_or("N/A"),
                    r.topic.as_deref().unwrap_or("N/A"),
                    (r.confidence * 100.0).round() as i64,
                    r.summary.as_deref().unwrap_or("")
                ));
            }
            md
        }
        _ => {
            let value = serde_json::json!({
                "exportedAt": timefmt::now_o(),
                "from": from_o,
                "to": to_o,
                "events": events.iter().map(|e| serde_json::json!({
                    "timestamp": e.timestamp, "eventType": e.event_type,
                    "processName": e.process_name, "windowTitle": e.window_title,
                    "isIdle": e.is_idle
                })).collect::<Vec<_>>(),
                "inferences": inferences.iter().map(|r| serde_json::json!({
                    "processedAt": r.processed_at, "activityLabel": r.activity_label,
                    "application": r.application, "contentType": r.content_type,
                    "topic": r.topic, "action": r.action, "summary": r.summary,
                    "visibleText": r.visible_text, "confidence": r.confidence
                })).collect::<Vec<_>>(),
            });
            match serde_json::to_string_pretty(&value) {
                Ok(j) => j,
                Err(e) => return fail(e),
            }
        }
    };
    match output {
        Some(p) => {
            if let Some(parent) = std::path::Path::new(&p).parent() {
                if !parent.as_os_str().is_empty() {
                    if let Err(e) = std::fs::create_dir_all(parent) {
                        return fail(e);
                    }
                }
            }
            if let Err(e) = std::fs::write(&p, &content) {
                return fail(e);
            }
            println!("{}", format!("Export written to {p}").green());
        }
        None => println!("{content}"),
    }
    ExitCode::SUCCESS
}

/// v1.5: whole-database snapshot via VACUUM INTO, one consistent file for
/// backup/portability. Needs an explicit --output and refuses to overwrite.
fn export_sqlite(db: &ActivityDatabase, output: Option<String>) -> ExitCode {
    let out = match output {
        Some(p) => p,
        None => {
            eprintln!("{}", "Error: --format sqlite needs --output <file>".red());
            return ExitCode::from(1);
        }
    };
    let target = std::path::Path::new(&out);
    if target.exists() {
        eprintln!("{}", format!("Error: {out} already exists; remove it first").red());
        return ExitCode::from(1);
    }
    if let Some(parent) = target.parent() {
        if !parent.as_os_str().is_empty() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                return fail(e);
            }
        }
    }
    if let Err(e) = db.conn().execute("VACUUM INTO ?1", rusqlite::params![out]) {
        return fail(e);
    }
    println!("{}", format!("Database snapshot written to {out}").green());
    ExitCode::SUCCESS
}

/// Port of HandleReportCommand: report v2.0 JSON export (LLM 3.0-llm is Phase 4).
fn run_report(from: Option<String>, to: Option<String>, output: Option<String>, format: String, llm: bool, week: bool) -> ExitCode {
    let db = match open_db_quiet() {
        Ok(d) => d,
        Err(c) => return c,
    };
    if week {
        return run_week(&db, from, to, output);
    }
    let parse_date = |s: Option<&str>, default: jiff::civil::Date| -> Result<jiff::civil::Date, ExitCode> {
        match s {
            None => Ok(default),
            Some(v) => timefmt::parse_local_date(v).map_err(|e| {
                eprintln!("Error: invalid date '{v}': {e}");
                ExitCode::from(1)
            }),
        }
    };
    let mut from_d = match parse_date(from.as_deref(), timefmt::today_local_date()) {
        Ok(v) => v,
        Err(c) => return c,
    };
    let mut to_d = match parse_date(to.as_deref(), from_d) {
        Ok(v) => v,
        Err(c) => return c,
    };
    if to_d < from_d {
        std::mem::swap(&mut from_d, &mut to_d);
    }

    let format_norm = if format.trim().is_empty() { "json".to_string() } else { format.trim().to_lowercase() };
    if format_norm != "json" {
        println!("{}", "Only JSON export is currently supported. Defaulting to JSON.".yellow());
    }

    let output_path = match output {
        Some(p) => p,
        None => {
            let mut name = String::from("activity-report-");
            if llm {
                name.push_str("llm-");
            }
            name.push_str(&from_d.strftime("%Y%m%d").to_string());
            if to_d != from_d {
                name.push('-');
                name.push_str(&to_d.strftime("%Y%m%d").to_string());
            }
            name.push('-');
            name.push_str(&jiff::Timestamp::now().to_zoned_datetime(jiff::tz::TimeZone::system()).strftime("%H%M%S").to_string());
            name.push('.');
            name.push_str("json"); // normalizedFormat after the warning above
            name
        }
    };

    let from_o = timefmt::date_midnight_utc_o(&from_d);
    let to_str = to_d.strftime("%Y-%m-%d").to_string();
    let to_excl_o = match timefmt::next_day_local_midnight_utc_o(&to_str) {
        Ok(v) => v,
        Err(e) => return fail(e),
    };

    if let Some(parent) = std::path::Path::new(&output_path).parent() {
        if !parent.as_os_str().is_empty() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                return fail(e);
            }
        }
    }

    if llm {
        let rep = match llm::build_llm_report(&db, &from_o, &to_excl_o, &from_d) {
            Ok(r) => r,
            Err(e) => return fail(e),
        };
        // C# File.WriteAllText(json, UTF8) â€” no trailing newline; BOM drift accepted (plan)
        let json = match serde_json::to_string_pretty(&rep) {
            Ok(j) => j,
            Err(e) => return fail(e),
        };
        if let Err(e) = std::fs::write(&output_path, json) {
            return fail(e);
        }
        println!("{}", format!("LLM-optimized report exported to {output_path}").green());
        println!("{} {}", "Schema:".cyan(), rep.schema_version);
        println!("{} {}", "Summary:".cyan(), rep.executive_summary.one_liner);
        println!(
            "{}",
            format!(
                "Projects: {} | Insights: {} | Moments: {}",
                rep.projects.len(),
                rep.executive_summary.total_insights,
                rep.significant_moments.len()
            )
            .dimmed()
        );
        return ExitCode::SUCCESS;
    }

    let report = match report::build_report(&db, &from_o, &to_excl_o) {
        Ok(r) => r,
        Err(e) => return fail(e),
    };
    let json = match serde_json::to_string_pretty(&report) {
        Ok(j) => j,
        Err(e) => return fail(e),
    };
    if let Err(e) = std::fs::write(&output_path, json) {
        return fail(e);
    }
    println!("{}", format!("Report exported to {output_path}").green());

    match (&report.check_in_time_utc, &report.check_out_time_utc) {
        (Some(ci), Some(co)) => println!(
            "{} {}  {} {}",
            "Check-in:".cyan(),
            local_display_o(ci, "%H:%M:%S"),
            "Check-out:".cyan(),
            local_display_o(co, "%H:%M:%S")
        ),
        _ => println!("{}", "No activity events found in the specified date range.".yellow()),
    }
    println!(
        "{}",
        format!(
            "Tracked time: {} (Active: {}, Idle: {})",
            report::format_duration(report.total_tracked_seconds),
            report::format_duration(report.total_active_seconds),
            report::format_duration(report.total_idle_seconds)
        )
        .dimmed()
    );
    ExitCode::SUCCESS
}

/// C# Substring(0, max-3) + "..." when longer than max chars.
fn trunc(s: &str, max: usize) -> String {
    if s.chars().count() > max {
        let mut t: String = s.chars().take(max - 3).collect();
        t.push_str("...");
        t
    } else {
        s.to_string()
    }
}

/// .NET "N0" format: integer with comma group separators.
fn format_n0(v: i64) -> String {
    let neg = v < 0;
    let digits = v.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    if neg {
        out.insert(0, '-');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn n0_grouping() {
        assert_eq!(format_n0(0), "0");
        assert_eq!(format_n0(999), "999");
        assert_eq!(format_n0(1234), "1,234");
        assert_eq!(format_n0(1234567), "1,234,567");
        assert_eq!(format_n0(-12345), "-12,345");
    }

    #[test]
    fn truncate_matches_csharp() {
        let long: String = "x".repeat(51);
        let t = trunc(&long, 50);
        assert_eq!(t.chars().count(), 50);
        assert!(t.ends_with("..."));
        assert_eq!(trunc("short", 50), "short");
        let exact: String = "y".repeat(50);
        assert_eq!(trunc(&exact, 50).chars().count(), 50);
        // summary/visible/topics limits
        let s100: String = "s".repeat(101);
        assert_eq!(trunc(&s100, 100).chars().count(), 100);
        let s80: String = "t".repeat(81);
        assert_eq!(trunc(&s80, 80).chars().count(), 80);
    }

    #[test]
    fn sqlite_export_round_trip() {
        let dir = std::env::temp_dir().join(format!("am-cli-export-{}", uuid::Uuid::new_v4()));
        let src = dir.join("src.db");
        let dst = dir.join("out.db");
        let db = ActivityDatabase::open(&src).unwrap();
        db.initialize().unwrap();
        assert_eq!(export_sqlite(&db, Some(dst.to_string_lossy().into_owned())), ExitCode::SUCCESS);
        // The snapshot opens read-only and has the schema; refusing to
        // overwrite an existing file is a hard stop, not a clobber.
        let snap = ActivityDatabase::open_read_only(&dst).unwrap();
        let n: i64 = snap
            .conn()
            .query_row("SELECT count(*) FROM sqlite_master WHERE name = 'ActivityEvents'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
        assert_eq!(export_sqlite(&db, Some(dst.to_string_lossy().into_owned())), ExitCode::from(1));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
