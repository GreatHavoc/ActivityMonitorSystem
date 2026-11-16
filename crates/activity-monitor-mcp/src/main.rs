//! activity-monitor-mcp — MCP server (spec 2026-07-28, dual-era) exposing
//! read-only views of the local Activity Monitor database to LLM agents.
//!
//! Transports: stdio (default, newline-delimited JSON-RPC, protocol-only on
//! stdout, logs on stderr) or Streamable HTTP on 127.0.0.1 with `--http [port]`.

mod http;
mod rpc;
mod stdio;
mod tools;

use std::path::PathBuf;
use std::sync::Arc;

use activity_monitor_core::config::ActivityMonitorSettings;
use activity_monitor_core::db::{resolve_cli_db_path, ActivityDatabase};
use activity_monitor_core::ollama::OllamaClient;
use tools::ToolState;

const USAGE: &str = "activity-monitor-mcp [--db <path>] [--http [port]]

  --db <path>   Activity DB (default: ACTIVITY_MONITOR_DB env, then ./Data/ActivityData.db,
                then %ProgramData%\\ActivityMonitor\\Data\\ActivityData.db). Opened read-only.
  --http [port] Instead of stdio, serve Streamable HTTP on 127.0.0.1 (default port 8765).
  -h, --help    Show this help.

Logs go to stderr; stdout is protocol-only in stdio mode.";

fn parse_args() -> (Option<PathBuf>, Option<u16>) {
    let mut db = None;
    let mut http = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--db" => db = args.next().map(PathBuf::from),
            "--http" => {
                let port = match args.next() {
                    Some(p) if !p.starts_with("--") => p.parse().ok(),
                    _ => None,
                };
                http = Some(port.unwrap_or(8765));
            }
            "-h" | "--help" => {
                eprintln!("{USAGE}");
                std::process::exit(0);
            }
            other => {
                eprintln!("Unknown argument: {other}\n{USAGE}");
                std::process::exit(2);
            }
        }
    }
    (db, http)
}

fn init_tracing() {
    // stderr only: in stdio mode stdout must carry protocol messages alone.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();
}

#[tokio::main]
async fn main() {
    init_tracing();
    let (db_arg, http_port) = parse_args();

    let db_path = db_arg
        .or_else(|| std::env::var_os("ACTIVITY_MONITOR_DB").map(PathBuf::from))
        .or_else(resolve_cli_db_path);
    let db = match &db_path {
        Some(p) => match ActivityDatabase::open_read_only(p) {
            Ok(d) => {
                tracing::info!(path = %p.display(), "Opened activity DB read-only");
                Some(d)
            }
            Err(e) => {
                tracing::error!(path = %p.display(), error = %e, "Could not open DB read-only; tools will report the error");
                None
            }
        },
        None => {
            tracing::error!("No activity DB found (./Data, %ProgramData%, or --db / ACTIVITY_MONITOR_DB)");
            None
        }
    };

    let settings = ActivityMonitorSettings::load_or_default(ActivityMonitorSettings::default_config_path());
    let ollama = if settings.ollama_endpoint.is_empty() {
        tracing::warn!("No Ollama endpoint configured; ask_about_my_day will report it");
        None
    } else {
        Some(OllamaClient::new(&settings))
    };

    let rpc = Arc::new(rpc::Rpc::new(Arc::new(ToolState {
        db: std::sync::Mutex::new(db),
        ollama,
    })));
    match http_port {
        Some(port) => http::serve(rpc, port).await,
        None => stdio::serve(rpc).await,
    }
}
