//! Shared models. Field order matches the C# declaration order so JSON output
//! can be diffed textually; Option fields are omitted when None (nulls skipped).

use jiff::{Span, Timestamp};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ActivityEvent {
    pub id: i64,
    pub timestamp: Timestamp,
    pub event_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_title: Option<String>,
    pub is_idle: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idle_duration_seconds: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct FocusInfo {
    pub window_handle: usize,
    pub process_id: i64,
    pub process_name: String,
    pub window_title: String,
    pub timestamp: Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct IdleInfo {
    pub is_idle: bool,
    pub idle_duration: Span,
    pub last_input_time: Timestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "u8", into = "u8")]
pub enum RequestPriority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

impl From<u8> for RequestPriority {
    fn from(v: u8) -> Self {
        match v {
            0 => Self::Low,
            2 => Self::High,
            3 => Self::Critical,
            _ => Self::Normal,
        }
    }
}

impl From<RequestPriority> for u8 {
    fn from(p: RequestPriority) -> Self {
        p as u8
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct InferenceRequest {
    pub id: Uuid,
    pub timestamp: Timestamp,
    pub trigger_reason: String,
    pub priority: RequestPriority,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub captured_frames: Option<Vec<Vec<u8>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct InferenceResult {
    pub request_id: Uuid,
    pub processed_at: Timestamp,
    pub activity_label: String,
    pub application: String,
    pub content_type: String,
    pub topic: String,
    pub action: String,
    pub summary: String,
    pub visible_text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detected_objects: Option<Vec<DetectedObject>>,
    pub confidence: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_response: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DetectedObject {
    pub label: String,
    pub confidence: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bounding_box: Option<BoundingBox>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct BoundingBox {
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct QueueMetrics {
    pub queued_requests: i64,
    pub processing_requests: i64,
    pub completed_requests: i64,
    pub failed_requests: i64,
    pub average_processing_time: Span,
    pub last_updated: Timestamp,
}
