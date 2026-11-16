//! Phase 7 gate probe: exercise the Ollama client against the live server.
//! cargo run -p activity-monitor-core --example ollama_probe -- health
//! cargo run -p activity-monitor-core --example ollama_probe -- body <frame.jpg>
//! cargo run -p activity-monitor-core --example ollama_probe -- analyze <frame.jpg>

use std::sync::atomic::AtomicBool;

use activity_monitor_core::config::ActivityMonitorSettings;
use activity_monitor_core::ollama::{chat_request_body, OllamaClient};

const ENDPOINT: &str = "http://localhost:11434";

fn settings(model: &str, timeout_secs: i64) -> ActivityMonitorSettings {
    let mut s: ActivityMonitorSettings = serde_json::from_str("{}").unwrap();
    s.ollama_endpoint = ENDPOINT.to_string();
    s.ollama_model = model.to_string();
    s.queue_settings.processing_timeout_seconds = timeout_secs;
    s
}

/// Model name as reported by /api/tags (what the service would health-match on).
async fn reported_model() -> String {
    let tags: serde_json::Value = reqwest::get(format!("{ENDPOINT}/api/tags"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    tags["models"][0]["name"].as_str().unwrap().to_string()
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "health".into());
    let model = reported_model().await;
    eprintln!("reported model: {model}");
    match mode.as_str() {
        "health" => {
            let client = OllamaClient::new(&settings(&model, 30));
            let ok = client.check_health(&AtomicBool::new(false)).await;
            println!("HEALTH {ok} model={model}");
        }
        "body" => {
            let jpg = std::env::args().nth(2).expect("jpeg path");
            let bytes = std::fs::read(&jpg).unwrap();
            let images = vec![base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                &bytes,
            )];
            let body = chat_request_body(&model, images);
            let s = serde_json::to_string(&body).unwrap();
            if let Some(out) = std::env::args().nth(3) {
                std::fs::write(&out, &s).unwrap();
            }
            println!("body bytes: {}", s.len());
        }
        "analyze" => {
            let jpg = std::env::args().nth(2).expect("jpeg path");
            let bytes = std::fs::read(&jpg).unwrap();
            let client = OllamaClient::new(&settings(&model, 600));
            let frames = vec![bytes];
            match client.analyze_frames(&frames, &AtomicBool::new(false)).await {
                Some(r) => println!("{}", serde_json::to_string(&r).unwrap()),
                None => println!("null"),
            }
        }
        other => eprintln!("unknown mode {other}"),
    }
}
