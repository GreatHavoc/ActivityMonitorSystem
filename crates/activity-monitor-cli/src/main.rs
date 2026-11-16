use std::process::ExitCode;

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
        /// LLM-optimized report (schema 3.0-llm) — Phase 4, not yet ported
        #[arg(long)]
        llm: bool,
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
        Some(Commands::Report { from, to, output, format, llm }) => run_report(from, to, output, format, llm),
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
/// given date (or after today), as UTC "O" — replaces AddDays(1).ToUniversalTime().
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
        .map(|t| t.to_zoned(jiff::tz::TimeZone::system()).strftime(fmt).to_string())
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
            println!("{} {}", time.dimmed(), "● IDLE".yellow());
        } else if let Some(label) = &e.activity_label {
            if !label.is_empty() {
                println!("{} {}", time.dimmed(), format!("● {label}").cyan());
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
    println!("{} {} {}", time.dimmed(), format!("● {process_info}").green(), window_info.dimmed());
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
        // C# {confidence:P0} — percent, 0 decimals
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
            let bar = "█".repeat((count * 60 / max) as usize);
            println!("{label:<14} |{bar} {count}");
        }
    }
    ExitCode::SUCCESS
}

/// Port of HandleReportCommand: report v2.0 JSON export (LLM 3.0-llm is Phase 4).
fn run_report(from: Option<String>, to: Option<String>, output: Option<String>, format: String, llm: bool) -> ExitCode {
    let db = match open_db_quiet() {
        Ok(d) => d,
        Err(c) => return c,
    };
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
            name.push_str(&jiff::Timestamp::now().to_zoned(jiff::tz::TimeZone::system()).strftime("%H%M%S").to_string());
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
        // C# File.WriteAllText(json, UTF8) — no trailing newline; BOM drift accepted (plan)
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
}
