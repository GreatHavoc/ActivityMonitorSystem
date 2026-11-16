//! Console + daily rolling-file logging. The caller decides the directory
//! (executable dir / Logs) — the C# hardcoded C:\ActivityMonitor\publish\Logs.

use std::path::Path;

use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, Registry};

/// Keeps the non-blocking file writer alive; drop flushes pending lines.
pub struct LogGuard(#[allow(dead_code)] tracing_appender::non_blocking::WorkerGuard);

pub fn init(log_dir: &Path) -> std::io::Result<LogGuard> {
    std::fs::create_dir_all(log_dir)?;
    let file: RollingFileAppender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("activity-monitor")
        .max_log_files(30)
        .build(log_dir)
        .map_err(std::io::Error::other)?;
    let (writer, guard) = tracing_appender::non_blocking(file);
    Registry::default()
        .with(fmt::layer()) // console
        .with(fmt::layer().with_writer(writer).with_ansi(false)) // file
        .init();
    Ok(LogGuard(guard))
}
