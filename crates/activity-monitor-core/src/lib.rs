pub mod config;
#[cfg(windows)]
pub mod capture;
pub mod digest;
#[cfg(windows)]
pub mod dxgi;
#[cfg(target_os = "macos")]
pub mod mac;
pub mod rank;
pub mod db;
pub mod logging;
pub mod models;
pub mod ollama;
pub mod queue;
pub mod sensors;
pub mod timefmt;

/// Serializes tests that use the global capture lock (they would otherwise
/// try-lock each other out when the harness runs them on separate threads).
#[cfg(all(test, windows))]
pub(crate) static CAPTURE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
