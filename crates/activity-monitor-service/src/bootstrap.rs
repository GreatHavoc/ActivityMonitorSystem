//! Day-zero bootstrap, run before the monitoring loop starts (blocking):
//! make sure Ollama is installed, running, and has the configured model.
//! - server reachable + model present  -> nothing to do
//! - server reachable, model missing    -> `ollama pull <model>`
//! - server unreachable                 -> start `ollama serve`; if no ollama.exe
//!   is found, download the official release zip (curl + tar, both ship with
//!   Windows 10+) into %LOCALAPPDATA%\Programs\Ollama, then serve + pull
//! The database is created by the caller (ActivityDatabase::open + initialize).
//! Any failure logs an actionable error and the service keeps monitoring
//! without inference rather than exiting.
//! Env override: ACTIVITY_MONITOR_OLLAMA_EXE points at a specific ollama.exe.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use activity_monitor_core::config::ActivityMonitorSettings;
use activity_monitor_core::ollama::OllamaClient;

const OLLAMA_ZIP_URL: &str =
    "https://github.com/ollama/ollama/releases/latest/download/ollama-windows-amd64.zip";
const SERVE_POLL: Duration = Duration::from_secs(3);
const SERVE_POLLS: u32 = 30; // ~90 s

pub async fn ensure_ollama(settings: &ActivityMonitorSettings, cancel: &AtomicBool) {
    let client = OllamaClient::new(settings);
    if client.check_health(cancel).await {
        tracing::info!(model = %settings.ollama_model, "Ollama ready");
        return;
    }
    let exe = match find_ollama() {
        Some(e) => e,
        None => match install_ollama() {
            Some(e) => e,
            None => {
                tracing::error!(
                    "Ollama is not installed and the download failed; monitoring continues without inference. \
                     Manual fix: winget install Ollama.Ollama, then: ollama pull {}",
                    settings.ollama_model
                );
                return;
            }
        },
    };
    if !client.server_up(cancel).await {
        start_serve(&exe);
        for _ in 0..SERVE_POLLS {
            if cancel.load(Ordering::Relaxed) {
                return;
            }
            tokio::time::sleep(SERVE_POLL).await;
            if client.server_up(cancel).await {
                break;
            }
        }
    }
    if !client.server_up(cancel).await {
        tracing::error!("Ollama server did not come up; monitoring continues without inference");
        return;
    }
    pull_model(&exe, &settings.ollama_model);
    if client.check_health(cancel).await {
        tracing::info!(model = %settings.ollama_model, "Ollama model ready");
    } else {
        tracing::error!(model = %settings.ollama_model, "Pull finished but model not listed; check `ollama list`");
    }
}

fn install_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(|d| PathBuf::from(d).join(r"Programs\Ollama"))
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files\Ollama"))
}

fn find_ollama() -> Option<PathBuf> {
    if let Some(e) = std::env::var_os("ACTIVITY_MONITOR_OLLAMA_EXE") {
        let p = PathBuf::from(e);
        if p.is_file() {
            return Some(p);
        }
    }
    let mut candidates: Vec<PathBuf> = vec![install_dir().join("ollama.exe")];
    if let Some(path) = std::env::var_os("PATH") {
        candidates.extend(std::env::split_paths(&path).map(|p| p.join("ollama.exe")));
    }
    if let Some(d) = std::env::var_os("LOCALAPPDATA") {
        candidates.push(PathBuf::from(d).join(r"Programs\Ollama\bin\ollama.exe"));
    }
    candidates.into_iter().find(|c| c.is_file()).or_else(|| find_recursive(&install_dir(), 3))
}

fn find_recursive(dir: &Path, depth: u32) -> Option<PathBuf> {
    if depth == 0 || !dir.is_dir() {
        return None;
    }
    let entries = std::fs::read_dir(dir).ok()?;
    for e in entries.flatten() {
        let p = e.path();
        if p.is_file() && p.file_name().map(|n| n == "ollama.exe").unwrap_or(false) {
            return Some(p);
        }
        if p.is_dir() {
            if let Some(found) = find_recursive(&p, depth - 1) {
                return Some(found);
            }
        }
    }
    None
}

fn install_ollama() -> Option<PathBuf> {
    let tmp = std::env::temp_dir().join("ollama-install");
    std::fs::create_dir_all(&tmp).ok()?;
    let zip = tmp.join("ollama-windows-amd64.zip");
    tracing::info!(url = OLLAMA_ZIP_URL, "Downloading Ollama (first run only)");
    let ok = Command::new("curl.exe")
        .args(["-fL", "--retry", "3", "-o"])
        .arg(&zip)
        .arg(OLLAMA_ZIP_URL)
        .stdin(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !ok || !zip.is_file() {
        tracing::error!("Ollama download failed");
        return None;
    }
    let dest = install_dir();
    std::fs::create_dir_all(&dest).ok()?;
    // bsdtar ships with Windows 10+ and reads zip
    let ok = Command::new("tar.exe")
        .args(["-xf"])
        .arg(&zip)
        .arg("-C")
        .arg(&dest)
        .stdin(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    let _ = std::fs::remove_file(&zip);
    if !ok {
        tracing::error!("Ollama archive extraction failed");
        return None;
    }
    let found = find_recursive(&dest, 4);
    if found.is_some() {
        tracing::info!(dir = %dest.display(), "Ollama installed");
    }
    found
}

fn start_serve(exe: &Path) {
    let dir = exe.parent().unwrap_or_else(|| Path::new("."));
    match Command::new(exe)
        .arg("serve")
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(_) => tracing::info!(exe = %exe.display(), "Started `ollama serve`"),
        Err(e) => tracing::error!(error = %e, "Could not start `ollama serve`"),
    }
}

fn pull_model(exe: &Path, model: &str) {
    tracing::info!(model, "Pulling model (first run downloads ~2 GB)");
    let dir = exe.parent().unwrap_or_else(|| Path::new("."));
    match Command::new(exe).arg("pull").arg(model).current_dir(dir).output() {
        Ok(o) if o.status.success() => {
            tracing::info!(model, "Model pull finished")
        }
        Ok(o) => tracing::error!(
            model,
            status = %o.status,
            stderr = %String::from_utf8_lossy(&o.stderr).chars().rev().take(400).collect::<String>().chars().rev().collect::<String>(),
            "Model pull failed"
        ),
        Err(e) => tracing::error!(model, error = %e, "Could not run `ollama pull`"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::AtomicBool;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// Serves one GET /api/tags with the given models array.
    async fn tags_server(models: Vec<String>) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let h = tokio::spawn(async move {
            while let Ok((mut sock, _)) = listener.accept().await {
                let mut buf = [0u8; 65536];
                let _ = sock.read(&mut buf).await;
                let models: Vec<serde_json::Value> = models
                    .iter()
                    .map(|m| json!({"name": m, "size": 1i64}))
                    .collect();
                let body = json!({"models": models}).to_string();
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = sock.write_all(resp.as_bytes()).await;
            }
        });
        (format!("http://{addr}"), h)
    }

    fn settings(endpoint: &str) -> ActivityMonitorSettings {
        let mut s: ActivityMonitorSettings = serde_json::from_str("{}").unwrap();
        s.ollama_endpoint = endpoint.into();
        s.ollama_model = "qwen3-vl:2b".into();
        s
    }

    #[tokio::test(flavor = "current_thread")]
    async fn ready_path_returns_immediately() {
        let (ep, server) = tags_server(vec!["qwen3-vl:2b".to_string()]).await;
        let cancel = AtomicBool::new(false);
        ensure_ollama(&settings(&ep), &cancel).await; // model present -> no pull
        server.abort();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn missing_model_triggers_pull_and_does_not_hang() {
        let (ep, server) = tags_server(vec!["some-other-model".to_string()]).await;
        // point at cmd.exe: `cmd.exe pull ...` exits immediately with an error,
        // which is exactly the failure path we want to exercise
        unsafe { std::env::set_var("ACTIVITY_MONITOR_OLLAMA_EXE", r"C:\Windows\System32\cmd.exe") };
        let cancel = AtomicBool::new(false);
        ensure_ollama(&settings(&ep), &cancel).await; // server up, model missing -> pull attempt
        unsafe { std::env::remove_var("ACTIVITY_MONITOR_OLLAMA_EXE") };
        server.abort();
    }
}
