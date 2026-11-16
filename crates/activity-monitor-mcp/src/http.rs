//! Streamable HTTP transport (spec 2026-07-28): POST-only endpoint on
//! 127.0.0.1, one JSON request per POST, JSON response (200) or 202 for
//! notifications. Stateless — no sessions, Connection: close per request.
//! Deliberately minimal HTTP/1.1: loopback only, so no TLS, no keep-alive.

use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::rpc::{self, Rpc, SUPPORTED_VERSIONS};

const MAX_HEAD: usize = 64 * 1024;
const MAX_BODY: usize = 4 * 1024 * 1024;

pub async fn serve(rpc: Arc<Rpc>, port: u16) {
    let listener = match TcpListener::bind(("127.0.0.1", port)).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!(port, error = %e, "Cannot bind 127.0.0.1");
            std::process::exit(1);
        }
    };
    tracing::info!(url = format!("http://127.0.0.1:{port}/mcp"), "Streamable HTTP listening (loopback only)");
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let rpc = rpc.clone();
                tokio::spawn(async move { handle_conn(stream, rpc).await });
            }
            Err(e) => tracing::warn!(error = %e, "accept failed"),
        }
    }
}

async fn handle_conn(mut stream: TcpStream, rpc: Arc<Rpc>) {
    let Some((request_line, headers)) = read_head(&mut stream).await else {
        return;
    };
    let get = |name: &str| headers.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str());

    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("");

    if method != "POST" {
        respond(&mut stream, "405 Method Not Allowed", None, b"MCP endpoint accepts POST only").await;
        return;
    }
    if path != "/mcp" && path != "/" {
        respond(&mut stream, "404 Not Found", None, b"MCP endpoint is /mcp").await;
        return;
    }
    // Streamable HTTP: client must accept both JSON and SSE.
    let accept = get("accept").unwrap_or("");
    if !accept.contains("application/json") || !accept.contains("text/event-stream") {
        respond(&mut stream, "406 Not Acceptable", None, b"Accept must include application/json and text/event-stream").await;
        return;
    }
    // Optional version header (2025-11-25 era); 2026-07-28 carries the version
    // per-request in _meta, which rpc::handle validates.
    if let Some(v) = get("mcp-protocol-version") {
        if !SUPPORTED_VERSIONS.contains(&v) {
            let body = rpc::err_resp(
                serde_json::Value::Null,
                rpc::UNSUPPORTED_VERSION,
                "Unsupported protocol version",
                Some(serde_json::json!({"supported": SUPPORTED_VERSIONS, "requested": v})),
            );
            respond(&mut stream, "400 Bad Request", Some("application/json"), serde_json::to_string(&body).unwrap().as_bytes()).await;
            return;
        }
    }
    let len = get("content-length")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);
    if len > MAX_BODY {
        respond(&mut stream, "413 Payload Too Large", None, b"Body too large").await;
        return;
    }
    let mut body = vec![0u8; len];
    if stream.read_exact(&mut body).await.is_err() {
        return;
    }
    let text = String::from_utf8_lossy(&body);
    let resp = match rpc::parse_message(&text) {
        Ok(req) => rpc.handle(req).await,
        Err(e) => Some(e),
    };
    match resp {
        Some(resp) => {
            let body = serde_json::to_string(&resp).unwrap();
            respond(&mut stream, "200 OK", Some("application/json"), body.as_bytes()).await;
        }
        None => respond(&mut stream, "202 Accepted", None, b"").await, // notification
    }
}

async fn read_head(stream: &mut TcpStream) -> Option<(String, Vec<(String, String)>)> {
    let mut buf: Vec<u8> = Vec::with_capacity(1024);
    let mut byte = [0u8; 1];
    loop {
        match stream.read(&mut byte).await {
            Ok(0) | Err(_) => return None,
            Ok(_) => {
                buf.push(byte[0]);
                if buf.ends_with(b"\r\n\r\n") {
                    break;
                }
                if buf.len() > MAX_HEAD {
                    return None;
                }
            }
        }
    }
    let text = String::from_utf8_lossy(&buf);
    let mut lines = text.split("\r\n");
    let request_line = lines.next()?.to_string();
    let headers = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
        .collect();
    Some((request_line, headers))
}

async fn respond(stream: &mut TcpStream, status: &str, content_type: Option<&str>, body: &[u8]) {
    let mut head = format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n", body.len());
    if let Some(ct) = content_type {
        head.push_str(&format!("Content-Type: {ct}\r\n"));
    }
    head.push_str("\r\n");
    let _ = stream.write_all(head.as_bytes()).await;
    let _ = stream.write_all(body).await;
    let _ = stream.shutdown().await;
}
