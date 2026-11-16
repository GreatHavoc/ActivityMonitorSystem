//! appsettings.json parsing. Mirrors ActivityMonitor.Common's settings classes
//! with identical defaults. Unknown keys are tolerated (no deny_unknown_fields),
//! so legacy config files — including the "MaxEventAgeD ays" typo — still load.

use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct AppSettings {
    #[serde(rename = "ActivityMonitor", default)]
    pub activity_monitor: ActivityMonitorSettings,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ActivityMonitorSettings {
    #[serde(default = "d_sampling_interval")]
    pub sampling_interval_seconds: i64,
    #[serde(default = "d_idle_threshold")]
    pub idle_threshold_seconds: i64,
    #[serde(default)]
    pub capture_settings: CaptureSettings,
    #[serde(default = "d_ollama_endpoint")]
    pub ollama_endpoint: String,
    #[serde(default = "d_ollama_model")]
    pub ollama_model: String,
    #[serde(default)]
    pub queue_settings: QueueSettings,
    #[serde(default)]
    pub storage: StorageSettings,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CaptureSettings {
    #[serde(default = "d_frame_rate")]
    pub frame_rate: i64,
    #[serde(default = "d_max_duration")]
    pub max_duration_seconds: i64,
    #[serde(default = "d_true")]
    pub trigger_on_focus_change: bool,
    #[serde(default = "d_true")]
    pub trigger_on_idle_resume: bool,
    // C# class default is 30; the shipped appsettings.json says 1.
    #[serde(default = "d_max_frames")]
    pub max_frames_per_capture: i64,
}

impl Default for CaptureSettings {
    fn default() -> Self {
        serde_json::from_str("{}").expect("empty settings deserialize to defaults")
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct QueueSettings {
    #[serde(default = "d_max_concurrent")]
    pub max_concurrent_tasks: i64,
    #[serde(default = "d_max_queue")]
    pub max_queue_size: i64,
    // Dead in the C# code (never read) — kept for config compatibility, unused.
    #[serde(default = "d_high_priority_threshold")]
    pub high_priority_threshold: i64,
    #[serde(default = "d_processing_timeout")]
    pub processing_timeout_seconds: i64,
}

impl QueueSettings {
    /// The C# code never reads ProcessingTimeoutSeconds and leaves the HTTP
    /// timeout infinite; the port treats <= 0 as "use the 120 s default".
    pub fn effective_timeout(&self) -> std::time::Duration {
        if self.processing_timeout_seconds <= 0 {
            std::time::Duration::from_secs(120)
        } else {
            std::time::Duration::from_secs(self.processing_timeout_seconds as u64)
        }
    }
}

impl Default for QueueSettings {
    fn default() -> Self {
        serde_json::from_str("{}").expect("empty settings deserialize to defaults")
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct StorageSettings {
    #[serde(default = "d_db_path")]
    pub database_path: String,
    // Dead in the C# code — kept for config compatibility, unused.
    #[serde(default)]
    pub retain_frames: bool,
    // Dead in the C# code — kept for config compatibility, unused.
    #[serde(default = "d_compaction_hours")]
    pub compaction_interval_hours: i64,
    // alias covers the typo'd key present in the shipped appsettings.json
    #[serde(default = "d_max_event_age", alias = "MaxEventAgeD ays")]
    pub max_event_age_days: i64,
}

impl Default for StorageSettings {
    fn default() -> Self {
        serde_json::from_str("{}").expect("empty settings deserialize to defaults")
    }
}

impl Default for ActivityMonitorSettings {
    fn default() -> Self {
        serde_json::from_str("{}").expect("empty settings deserialize to defaults")
    }
}

impl ActivityMonitorSettings {
    /// Load from an appsettings.json file. Missing file yields defaults.
    pub fn load_or_default(path: impl AsRef<Path>) -> Self {
        Self::load(path).unwrap_or_default()
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let text = std::fs::read_to_string(path)?;
        let app: AppSettings = serde_json::from_str(&text)?;
        Ok(app.activity_monitor)
    }

    /// appsettings.json next to the running executable.
    pub fn default_config_path() -> PathBuf {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."))
            .join("appsettings.json")
    }
}

fn d_sampling_interval() -> i64 { 5 }
fn d_idle_threshold() -> i64 { 300 }
fn d_frame_rate() -> i64 { 1 }
fn d_max_duration() -> i64 { 30 }
fn d_max_frames() -> i64 { 30 }
fn d_true() -> bool { true }
fn d_ollama_endpoint() -> String { "http://localhost:11434".into() }
fn d_ollama_model() -> String { "qwen3-vl:2b".into() }
fn d_max_concurrent() -> i64 { 4 }
fn d_max_queue() -> i64 { 100 }
fn d_high_priority_threshold() -> i64 { 10 }
fn d_processing_timeout() -> i64 { 120 }
fn d_db_path() -> String { "ActivityData.db".into() }
fn d_compaction_hours() -> i64 { 24 }
fn d_max_event_age() -> i64 { 90 }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_shipped_appsettings_shape() {
        let json = r#"{
          "Serilog": { "MinimumLevel": { "Default": "Information" } },
          "ActivityMonitor": {
            "SamplingIntervalSeconds": 5,
            "IdleThresholdSeconds": 300,
            "CaptureSettings": { "FrameRate": 1, "MaxDurationSeconds": 30,
              "TriggerOnFocusChange": true, "TriggerOnIdleResume": true, "MaxFramesPerCapture": 1 },
            "OllamaEndpoint": "http://localhost:11434",
            "OllamaModel": "qwen3-vl:2b",
            "QueueSettings": { "MaxConcurrentTasks": 4, "MaxQueueSize": 100,
              "HighPriorityThreshold": 10, "ProcessingTimeoutSeconds": 0 },
            "Storage": { "DatabasePath": "C:\\ActivityMonitor\\publish\\Data\\ActivityData.db",
              "RetainFrames": false, "CompactionIntervalHours": 24, "MaxEventAgeD ays": 90 }
          }
        }"#;
        let app: AppSettings = serde_json::from_str(json).unwrap();
        let s = app.activity_monitor;
        assert_eq!(s.sampling_interval_seconds, 5);
        assert_eq!(s.capture_settings.max_frames_per_capture, 1);
        // legacy typo key still lands on max_event_age_days
        assert_eq!(s.storage.max_event_age_days, 90);
        // 0 in the file is the old "infinite" sentinel -> effective timeout is 120 s
        assert_eq!(s.queue_settings.effective_timeout(), std::time::Duration::from_secs(120));
    }

    #[test]
    fn defaults_match_csharp_class_defaults() {
        let s = ActivityMonitorSettings::default();
        assert_eq!(s.sampling_interval_seconds, 5);
        assert_eq!(s.idle_threshold_seconds, 300);
        assert_eq!(s.capture_settings.max_frames_per_capture, 30);
        assert_eq!(s.ollama_endpoint, "http://localhost:11434");
        assert_eq!(s.ollama_model, "qwen3-vl:2b");
        assert_eq!(s.queue_settings.max_concurrent_tasks, 4);
        assert_eq!(s.storage.database_path, "ActivityData.db");
        assert_eq!(s.storage.max_event_age_days, 90);
    }
}
