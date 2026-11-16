//! Ollama HTTP client — port of OllamaInferenceClient (`/api/chat` + `/api/tags`).
//! Deliberate fix: C# sets `HttpClient.Timeout = Timeout.InfiniteTimeSpan`; this
//! port applies `QueueSettings::effective_timeout()` (120 s when <= 0) instead.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use base64::Engine as _;
use jiff::Timestamp;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::config::ActivityMonitorSettings;
use crate::models::InferenceResult;

const PROMPT: &str = "Analyze this screenshot. Identify the application, what the user is doing, and the topic. Keep all fields concise. For visible_text, include only the window title and 1-2 key lines, nothing more.";

pub struct OllamaClient {
    http: reqwest::Client,
    endpoint: String,
    model: String,
}

impl OllamaClient {
    pub fn new(settings: &ActivityMonitorSettings) -> Self {
        let http = reqwest::Client::builder()
            .timeout(settings.queue_settings.effective_timeout())
            .build()
            .expect("reqwest client build");
        OllamaClient {
            http,
            endpoint: settings.ollama_endpoint.trim_end_matches('/').to_string(),
            model: settings.ollama_model.clone(),
        }
    }

    /// Port of AnalyzeFramesAsync. Returns None on any failure, as in C#.
    /// The returned result has a nil RequestId; the queue assigns it (as in C#).
    pub async fn analyze_frames(&self, frames: &[Vec<u8>], cancel: &AtomicBool) -> Option<InferenceResult> {
        match self.try_analyze(frames, cancel).await {
            Ok(v) => v,
            Err(e) => {
                tracing::error!(error = %e, "Error during Ollama inference");
                None
            }
        }
    }

    async fn try_analyze(
        &self,
        frames: &[Vec<u8>],
        cancel: &AtomicBool,
    ) -> Result<Option<InferenceResult>, Box<dyn std::error::Error>> {
        tracing::info!(frame_count = frames.len(), "Sending frames to Ollama for analysis");
        let images = frames_to_base64(frames);
        let body = chat_request_body(&self.model, images);
        let fut = async {
            let resp = self
                .http
                .post(format!("{}/api/chat", self.endpoint))
                .json(&body)
                .send()
                .await?;
            let status = resp.status();
            let raw = resp.text().await?;
            Ok::<_, reqwest::Error>((status, raw))
        };
        let (status, raw) = select_cancel(fut, cancel).await?.map_err(Box::new)?;
        if !status.is_success() {
            tracing::error!("Ollama request failed: {status} - {raw}");
            return Ok(None);
        }
        tracing::info!(raw_body = %raw, "Raw Ollama response body");
        let chat: Option<ChatResponse> = match serde_json::from_str(&raw) {
            Ok(v) => v,
            Err(e) => {
                tracing::error!(error = %e, raw_body = %raw, "Failed to deserialize Ollama response");
                return Ok(None);
            }
        };
        let content = chat.and_then(|c| c.message).and_then(|m| m.content);
        match content {
            Some(c) if !c.is_empty() => {
                tracing::info!(content = %c, "Ollama response");
                Ok(Some(parse_response(&c)))
            }
            _ => {
                tracing::warn!("Empty response from Ollama");
                Ok(None)
            }
        }
    }

    /// Plain text chat (no images, no structured format) for the MCP server's
    /// ask_about_my_day tool. Returns None on any failure, like analyze_frames.
    pub async fn chat_text(&self, system: &str, user: &str, cancel: &AtomicBool) -> Option<String> {
        match self.try_chat_text(system, user, cancel).await {
            Ok(v) => v,
            Err(e) => {
                tracing::error!(error = %e, "Error during Ollama text chat");
                None
            }
        }
    }

    async fn try_chat_text(
        &self,
        system: &str,
        user: &str,
        cancel: &AtomicBool,
    ) -> Result<Option<String>, Box<dyn std::error::Error>> {
        let body = json!({
            "model": self.model,
            "messages": [
                { "role": "system", "content": system },
                { "role": "user", "content": user }
            ],
            "stream": false,
            "think": false,
            "options": { "temperature": 0.3, "top_p": 0.9, "num_predict": 1024 }
        });
        let fut = async {
            let resp = self
                .http
                .post(format!("{}/api/chat", self.endpoint))
                .json(&body)
                .send()
                .await?;
            let status = resp.status();
            let raw = resp.text().await?;
            Ok::<_, reqwest::Error>((status, raw))
        };
        let (status, raw) = select_cancel(fut, cancel).await?.map_err(Box::new)?;
        if !status.is_success() {
            tracing::error!("Ollama chat failed: {status} - {raw}");
            return Ok(None);
        }
        let chat: Option<ChatResponse> = serde_json::from_str(&raw).ok();
        Ok(chat
            .and_then(|c| c.message)
            .and_then(|m| m.content)
            .filter(|c| !c.is_empty()))
    }

    /// Port of CheckHealthAsync: /api/tags, model name substring match (ordinal ignore case).
    pub async fn check_health(&self, cancel: &AtomicBool) -> bool {
        match self.try_health(cancel).await {
            Ok(b) => b,
            Err(e) => {
                tracing::error!(error = %e, "Error checking Ollama health");
                false
            }
        }
    }

    /// True when the server answers /api/tags at all (the model may still be
    /// missing) — used by the service bootstrap to decide whether to start
    /// `ollama serve` or just `ollama pull`.
    pub async fn server_up(&self, cancel: &AtomicBool) -> bool {
        let fut = async {
            let resp = self.http.get(format!("{}/api/tags", self.endpoint)).send().await?;
            Ok::<_, reqwest::Error>(resp.status().is_success())
        };
        matches!(select_cancel(fut, cancel).await, Ok(Ok(true)))
    }

    async fn try_health(&self, cancel: &AtomicBool) -> Result<bool, Box<dyn std::error::Error>> {
        let fut = async {
            let resp = self.http.get(format!("{}/api/tags", self.endpoint)).send().await?;
            Ok::<_, reqwest::Error>((resp.status(), resp.text().await?))
        };
        let (status, body) = select_cancel(fut, cancel).await?.map_err(Box::new)?;
        if !status.is_success() {
            tracing::warn!("Ollama health check failed: {status}");
            return Ok(false);
        }
        let tags: TagsResponse = match serde_json::from_str(&body) {
            Ok(t) => t,
            Err(_) => return Ok(false), // C# ReadFromJsonAsync throws -> catch -> false
        };
        // C#: tags?.Models == null -> false (covers "null" body and models: null)
        let Some(models) = tags.models else { return Ok(false) };
        let needle = self.model.to_lowercase();
        let found = models.iter().any(|m| m.name.to_lowercase().contains(&needle));
        if !found {
            let names: Vec<&str> = models.iter().map(|m| m.name.as_str()).collect();
            tracing::warn!(model = %self.model, available = %names.join(", "), "Model not found in Ollama");
        }
        Ok(found)
    }
}

#[derive(Deserialize)]
struct ChatResponse {
    message: Option<ChatMessage>,
}

#[derive(Deserialize)]
struct ChatMessage {
    content: Option<String>,
}

#[derive(Deserialize)]
struct TagsResponse {
    models: Option<Vec<TagsModel>>,
}

#[derive(Deserialize)]
struct TagsModel {
    name: String,
}

/// C# ConvertFramesToBase64: only the middle frame, to stay within token limits.
fn frames_to_base64(frames: &[Vec<u8>]) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(f) = frames.get(frames.len() / 2) {
        out.push(base64::engine::general_purpose::STANDARD.encode(f));
    }
    tracing::debug!(original = frames.len(), "Converted frames to 1 sample");
    out
}

/// Exact JSON body POSTed to /api/chat (pub for the Phase 7 gate probe).
pub fn chat_request_body(model: &str, images: Vec<String>) -> Value {
    json!({
        "model": model,
        "messages": [{ "role": "user", "content": PROMPT, "images": images }],
        "stream": false,
        "think": false,
        "format": structured_format(),
        "options": { "temperature": 0.1, "top_p": 0.9, "num_predict": 1024 }
    })
}

/// C# BuildStructuredOutputFormat (anonymous object, @enum serializes as "enum").
fn structured_format() -> Value {
    json!({
        "type": "object",
        "properties": {
            "activity_label": { "type": "string", "description": "Short label for the activity, max 5 words" },
            "application": { "type": "string", "description": "Application name" },
            "content_type": { "type": "string", "enum": ["code", "web", "document", "video", "chat", "email", "image", "other"] },
            "topic": { "type": "string", "description": "Topic in 1-3 words" },
            "action": { "type": "string", "enum": ["reading", "writing", "browsing", "coding", "editing", "watching", "chatting", "searching", "other"] },
            "summary": { "type": "string", "description": "1-2 sentence summary" },
            "visible_text": { "type": "string", "description": "Window title only" },
            "confidence": { "type": "number", "minimum": 0.0, "maximum": 1.0 }
        },
        "required": ["activity_label", "summary", "confidence", "application", "content_type", "topic", "action", "visible_text"]
    })
}

/// C# ParseResponse. Structured path sets RawResponse = content; the fallback
/// (any parse/type error) uses label "Activity", confidence 0.5 and the raw
/// content truncated to 500 chars + "..." as summary.
fn parse_response(content: &str) -> InferenceResult {
    let base = |label: &str, summary: String, confidence: f64| InferenceResult {
        request_id: Uuid::nil(), // queue assigns the real id, as in C#
        processed_at: Timestamp::now(),
        activity_label: label.to_string(),
        application: String::new(),
        content_type: String::new(),
        topic: String::new(),
        action: String::new(),
        summary,
        visible_text: String::new(),
        detected_objects: None,
        confidence,
        raw_response: Some(content.to_string()),
    };
    match parse_structured(content) {
        Some((label, app, ctype, topic, action, summary, visible, confidence)) => InferenceResult {
            activity_label: label,
            application: app,
            content_type: ctype,
            topic,
            action,
            summary,
            visible_text: visible,
            confidence,
            ..base("", String::new(), 0.0)
        },
        None => {
            tracing::error!("Failed to parse structured JSON response");
            let summary = if content.chars().count() > 500 {
                format!("{}...", content.chars().take(500).collect::<String>())
            } else {
                content.to_string()
            };
            base("Activity", summary, 0.5)
        }
    }
}

/// Returns None to signal the fallback path. Required props use C# GetProperty
/// semantics (missing or wrong type -> None); optional props use TryGetProperty.
fn parse_structured(content: &str) -> Option<(String, String, String, String, String, String, String, f64)> {
    let v: Value = serde_json::from_str(content).ok()?;
    let obj = v.as_object()?;
    let req = |key: &str, default: &str| -> Option<String> {
        match obj.get(key) {
            None => None,                       // GetProperty throws -> fallback
            Some(Value::Null) => Some(default.to_string()), // GetString null -> null -> ??
            Some(Value::String(s)) => Some(s.clone()),
            Some(_) => None,                    // GetString throws -> fallback
        }
    };
    let activity_label = req("activity_label", "Unknown")?;
    let application = req("application", "")?;
    let content_type = req("content_type", "")?;
    let topic = req("topic", "")?;
    let action = req("action", "")?;
    let summary = req("summary", "")?;
    let visible_text = match obj.get("visible_text") {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(_) => return None,
    };
    let confidence = match obj.get("confidence") {
        None => 0.0,
        Some(v) => v.as_f64()?, // null/bool/string -> None -> fallback, like C# GetDouble throwing
    };
    Some((activity_label, application, content_type, topic, action, summary, visible_text, confidence))
}

async fn select_cancel<F: std::future::Future>(
    fut: F,
    cancel: &AtomicBool,
) -> Result<F::Output, Box<dyn std::error::Error>> {
    tokio::select! {
        r = fut => Ok(r),
        _ = wait_cancel(cancel) => Err("cancelled".into()),
    }
}

pub(crate) async fn wait_cancel(cancel: &AtomicBool) {
    while !cancel.load(Ordering::Relaxed) {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn test_settings(endpoint: &str) -> ActivityMonitorSettings {
        let mut s: ActivityMonitorSettings = serde_json::from_str("{}").unwrap();
        s.ollama_endpoint = endpoint.to_string();
        s.ollama_model = "test-model".to_string();
        s.queue_settings.processing_timeout_seconds = 30;
        s
    }

    /// Serve exactly one HTTP request; the task yields the raw request text.
    async fn one_shot(status: u16, response_body: &'static str) -> (String, tokio::task::JoinHandle<String>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let h = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut req: Vec<u8> = Vec::new();
            let mut buf = [0u8; 65536];
            loop {
                let n = sock.read(&mut buf).await.unwrap();
                if n == 0 {
                    break;
                }
                req.extend_from_slice(&buf[..n]);
                if let Some(pos) = req.windows(4).position(|w| w == b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&req[..pos]).to_lowercase();
                    let cl = head
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length:"))
                        .and_then(|v| v.trim().parse::<usize>().ok())
                        .unwrap_or(0);
                    if req.len() >= pos + 4 + cl {
                        break;
                    }
                }
            }
            let resp = format!(
                "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response_body}",
                response_body.len()
            );
            sock.write_all(resp.as_bytes()).await.unwrap();
            String::from_utf8_lossy(&req).to_string()
        });
        (format!("http://{addr}"), h)
    }

    fn request_body(raw: &str) -> &str {
        raw.split("\r\n\r\n").nth(1).unwrap_or("")
    }

    #[test]
    fn middle_frame_base64() {
        let b64 = |v: &[u8]| base64::engine::general_purpose::STANDARD.encode(v);
        assert!(frames_to_base64(&[]).is_empty());
        assert_eq!(frames_to_base64(&[vec![1], vec![2], vec![3]]), vec![b64(&[2])]);
        assert_eq!(frames_to_base64(&[vec![1], vec![2], vec![3], vec![4]]), vec![b64(&[3])]); // count/2 = 2
    }

    #[test]
    fn request_body_shape() {
        let body = chat_request_body("m", vec!["aGk=".to_string()]);
        assert_eq!(body["model"], "m");
        assert_eq!(body["stream"], false);
        assert_eq!(body["think"], false);
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], PROMPT);
        assert_eq!(body["messages"][0]["images"], json!(["aGk="]));
        assert_eq!(body["options"]["temperature"], 0.1);
        assert_eq!(body["options"]["top_p"], 0.9);
        assert_eq!(body["options"]["num_predict"], 1024);
        assert_eq!(body["format"]["type"], "object");
        assert_eq!(body["format"]["properties"]["content_type"]["enum"][0], "code");
        assert_eq!(body["format"]["required"].as_array().unwrap().len(), 8);
    }

    #[test]
    fn parse_structured_ok() {
        let r = parse_response(r#"{"activity_label":"Coding","application":"Comet","content_type":"code","topic":"rust","action":"coding","summary":"Porting.","visible_text":"t","confidence":0.92}"#);
        assert_eq!(r.activity_label, "Coding");
        assert_eq!(r.confidence, 0.92);
        assert_eq!(r.raw_response.as_deref(), Some(r#"{"activity_label":"Coding","application":"Comet","content_type":"code","topic":"rust","action":"coding","summary":"Porting.","visible_text":"t","confidence":0.92}"#));
    }

    #[test]
    fn parse_optional_defaults() {
        // visible_text/confidence absent -> defaults, not fallback
        let r = parse_response(r#"{"activity_label":"X","application":"","content_type":"","topic":"","action":"","summary":"s"}"#);
        assert_eq!(r.activity_label, "X");
        assert_eq!(r.visible_text, "");
        assert_eq!(r.confidence, 0.0);
    }

    #[test]
    fn parse_fallback_paths() {
        // non-JSON content -> fallback
        let r = parse_response("this is not json");
        assert_eq!(r.activity_label, "Activity");
        assert_eq!(r.confidence, 0.5);
        assert_eq!(r.summary, "this is not json");
        // missing required prop -> fallback
        let r = parse_response(r#"{"activity_label":"X"}"#);
        assert_eq!(r.activity_label, "Activity");
        // wrong type on required prop -> fallback
        let r = parse_response(r#"{"activity_label":5,"application":"","content_type":"","topic":"","action":"","summary":""}"#);
        assert_eq!(r.activity_label, "Activity");
        // >500 chars -> truncated with ellipsis
        let long = "x".repeat(600);
        let r = parse_response(&long);
        assert_eq!(r.summary.len(), 503);
        assert!(r.summary.ends_with("..."));
        // null confidence -> fallback (C# GetDouble throws on null)
        let r = parse_response(r#"{"activity_label":"X","application":"","content_type":"","topic":"","action":"","summary":"","confidence":null}"#);
        assert_eq!(r.activity_label, "Activity");
        // integer confidence is fine
        let r = parse_response(r#"{"activity_label":"X","application":"","content_type":"","topic":"","action":"","summary":"","confidence":1}"#);
        assert_eq!(r.confidence, 1.0);
    }

    #[tokio::test]
    async fn analyze_round_trip() {
        let inner = r#"{"activity_label":"Writing Rust code","application":"Comet","content_type":"code","topic":"migration","action":"coding","summary":"Porting capture.","visible_text":"plan - Comet","confidence":0.92}"#;
        let resp = json!({
            "model": "test-model", "created_at": "2026-11-16T10:00:00Z",
            "message": { "role": "assistant", "content": inner },
            "done": true, "done_reason": "stop", "total_duration": 123
        });
        let resp: &'static str = Box::leak(resp.to_string().into_boxed_str());
        let (ep, h) = one_shot(200, resp).await;
        let client = OllamaClient::new(&test_settings(&ep));
        let frames = vec![vec![1u8, 2], vec![3u8, 4], vec![5u8, 6]];
        let r = client.analyze_frames(&frames, &AtomicBool::new(false)).await.expect("result");
        assert_eq!(r.activity_label, "Writing Rust code");
        assert_eq!(r.application, "Comet");
        assert_eq!(r.content_type, "code");
        assert_eq!(r.confidence, 0.92);
        assert_eq!(r.raw_response.as_deref(), Some(inner));
        let req = h.await.unwrap();
        let line = req.lines().next().unwrap_or("");
        assert!(line.starts_with("POST /api/chat HTTP/1.1"), "request line: {line}");
        let sent: Value = serde_json::from_str(request_body(&req)).unwrap();
        let expected = chat_request_body(
            "test-model",
            vec![base64::engine::general_purpose::STANDARD.encode([3u8, 4])], // middle of 3 = index 1
        );
        assert_eq!(sent, expected, "request body mismatch");
    }

    #[tokio::test]
    async fn analyze_fallback_and_failures() {
        // malformed content -> fallback result, not None
        let resp: &'static str = Box::leak(
            json!({"message": {"role": "assistant", "content": "garbage not json"}, "done": true})
                .to_string()
                .into_boxed_str(),
        );
        let (ep, h) = one_shot(200, resp).await;
        let client = OllamaClient::new(&test_settings(&ep));
        let r = client.analyze_frames(&[vec![9u8]], &AtomicBool::new(false)).await.expect("fallback result");
        assert_eq!(r.activity_label, "Activity");
        assert_eq!(r.confidence, 0.5);
        h.await.unwrap();
        // HTTP 500 -> None
        let (ep, h) = one_shot(500, "boom").await;
        let client = OllamaClient::new(&test_settings(&ep));
        assert!(client.analyze_frames(&[vec![9u8]], &AtomicBool::new(false)).await.is_none());
        h.await.unwrap();
        // invalid chat JSON -> None
        let (ep, h) = one_shot(200, "not json at all").await;
        let client = OllamaClient::new(&test_settings(&ep));
        assert!(client.analyze_frames(&[vec![9u8]], &AtomicBool::new(false)).await.is_none());
        h.await.unwrap();
        // empty content -> None
        let resp: &'static str = Box::leak(json!({"message": {"content": ""}}).to_string().into_boxed_str());
        let (ep, h) = one_shot(200, resp).await;
        let client = OllamaClient::new(&test_settings(&ep));
        assert!(client.analyze_frames(&[vec![9u8]], &AtomicBool::new(false)).await.is_none());
        h.await.unwrap();
    }

    #[tokio::test]
    async fn health_checks() {
        let cancel = AtomicBool::new(false);
        let (ep, h) = one_shot(200, r#"{"models":[{"name":"test-model-q4","modified_at":"x","size":1}]}"#).await;
        let client = OllamaClient::new(&test_settings(&ep));
        assert!(client.check_health(&cancel).await);
        let req = h.await.unwrap();
        assert!(req.starts_with("GET /api/tags HTTP/1.1"));

        let (ep, h) = one_shot(200, r#"{"models":[{"name":"other"}]}"#).await;
        let client = OllamaClient::new(&test_settings(&ep));
        assert!(!client.check_health(&cancel).await);
        h.await.unwrap();

        let (ep, h) = one_shot(200, r#"{"models":null}"#).await;
        let client = OllamaClient::new(&test_settings(&ep));
        assert!(!client.check_health(&cancel).await);
        h.await.unwrap();

        let (ep, h) = one_shot(500, "err").await;
        let client = OllamaClient::new(&test_settings(&ep));
        assert!(!client.check_health(&cancel).await);
        h.await.unwrap();

        let (ep, h) = one_shot(200, "not json").await;
        let client = OllamaClient::new(&test_settings(&ep));
        assert!(!client.check_health(&cancel).await);
        h.await.unwrap();
    }
}
