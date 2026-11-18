//! MCP protocol core — spec revision 2026-07-28 (latest), dual-era with the
//! legacy 2025-11-25/2025-06-18 initialize handshake that current clients use.
//! Transport-agnostic: stdio and Streamable HTTP both call `Rpc::handle`.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use crate::tools::{self, ToolState};

pub const SUPPORTED_VERSIONS: [&str; 3] = ["2026-07-28", "2025-11-25", "2025-06-18"];
pub const SERVER_NAME: &str = "activity-monitor-mcp";
pub const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const INSTRUCTIONS: &str = "Read-only access to the local Activity Monitor database: timelines, summaries, and AI answers about your day. All tools are read-only; captured frames are never exposed.";

pub const PARSE_ERROR: i64 = -32700;
pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;
pub const INTERNAL_ERROR: i64 = -32603;
pub const UNSUPPORTED_VERSION: i64 = -32022;

#[derive(Debug)]
pub struct Request {
    pub id: Option<Value>,
    pub method: String,
    pub params: Value,
    pub meta: Value,
}

pub fn parse_message(raw: &str) -> Result<Request, Value> {
    let v: Value = serde_json::from_str(raw).map_err(|e| {
        err_resp(Value::Null, PARSE_ERROR, &format!("Parse error: {e}"), None)
    })?;
    let obj = v.as_object().cloned().ok_or_else(|| {
        err_resp(Value::Null, INVALID_REQUEST, "Invalid request: not an object", None)
    })?;
    if obj.get("jsonrpc").and_then(|j| j.as_str()) != Some("2.0") {
        return Err(err_resp(
            obj.get("id").cloned().unwrap_or(Value::Null),
            INVALID_REQUEST,
            "Invalid request: jsonrpc must be \"2.0\"",
            None,
        ));
    }
    let method = obj
        .get("method")
        .and_then(|m| m.as_str())
        .ok_or_else(|| {
            err_resp(
                obj.get("id").cloned().unwrap_or(Value::Null),
                INVALID_REQUEST,
                "Invalid request: missing method",
                None,
            )
        })?
        .to_string();
    Ok(Request {
        id: obj.get("id").cloned(),
        method,
        params: obj.get("params").cloned().unwrap_or(Value::Null),
        meta: obj.get("_meta").cloned().unwrap_or(Value::Null),
    })
}

pub fn ok_resp(id: Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

pub fn err_resp(id: Value, code: i64, message: &str, data: Option<Value>) -> Value {
    let mut error = json!({"code": code, "message": message});
    if let Some(d) = data {
        error["data"] = d;
    }
    json!({"jsonrpc": "2.0", "id": id, "error": error})
}

/// Protocol version requested by this message: modern per-request `_meta`
/// (top-level or inside params), or the legacy `initialize` handshake field.
fn requested_version(req: &Request) -> Option<String> {
    const KEY: &str = "io.modelcontextprotocol/protocolVersion";
    req.meta
        .get(KEY)
        .or_else(|| req.params.get("_meta").and_then(|m| m.get(KEY)))
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .or_else(|| {
            if req.method == "initialize" {
                req.params.get("protocolVersion").and_then(|v| v.as_str()).map(str::to_string)
            } else {
                None
            }
        })
}

fn discover_result() -> Value {
    json!({
        "resultType": "complete",
        "supportedVersions": SUPPORTED_VERSIONS,
        "capabilities": { "tools": { "listChanged": false } },
        "instructions": INSTRUCTIONS,
        "_meta": { "io.modelcontextprotocol/serverInfo": { "name": SERVER_NAME, "version": SERVER_VERSION } }
    })
}

fn legacy_initialize_result(req: &Request) -> Value {
    // Echo the client's version when we support it; otherwise fall back to
    // the newest legacy revision we implement.
    let requested = requested_version(req);
    let version = requested
        .filter(|v| SUPPORTED_VERSIONS.contains(&v.as_str()))
        .unwrap_or_else(|| "2025-11-25".to_string());
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": { "name": SERVER_NAME, "version": SERVER_VERSION },
        "instructions": INSTRUCTIONS
    })
}

pub struct Rpc {
    pub state: Arc<ToolState>,
    pub cancel: Arc<AtomicBool>,
    cancelled_ids: Mutex<HashSet<String>>,
}

impl Rpc {
    pub fn new(state: Arc<ToolState>) -> Self {
        Rpc {
            state,
            cancel: Arc::new(AtomicBool::new(false)),
            cancelled_ids: Mutex::new(HashSet::new()),
        }
    }

    /// Handle one JSON-RPC message. Returns the response to send, or None for
    /// notifications.
    pub async fn handle(&self, req: Request) -> Option<Value> {
        if req.method == "notifications/cancelled" {
            if let Some(id) = req.params.get("requestId") {
                self.cancelled_ids.lock().unwrap().insert(id.to_string());
            }
            return None;
        }
        if req.method.starts_with("notifications/") {
            return None; // initialized and friends: no response
        }
        let id = match &req.id {
            Some(id) => id.clone(),
            None => return None, // a request without id is a notification
        };
        if let Some(v) = requested_version(&req) {
            if !SUPPORTED_VERSIONS.contains(&v.as_str()) {
                return Some(err_resp(
                    id,
                    UNSUPPORTED_VERSION,
                    "Unsupported protocol version",
                    Some(json!({"supported": SUPPORTED_VERSIONS, "requested": v})),
                ));
            }
        }
        match req.method.as_str() {
            "server/discover" => Some(ok_resp(id, discover_result())),
            "initialize" => Some(ok_resp(id, legacy_initialize_result(&req))),
            "ping" => Some(ok_resp(id, json!({}))),
            "tools/list" => Some(ok_resp(
                id,
                json!({ "resultType": "complete", "tools": tools::tool_defs() }),
            )),
            "tools/call" => Some(self.handle_tool_call(id, req.params).await),
            other => Some(err_resp(id, METHOD_NOT_FOUND, &format!("Method not found: {other}"), None)),
        }
    }

    async fn handle_tool_call(&self, id: Value, params: Value) -> Value {
        let name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
        let args = params.get("arguments").cloned().unwrap_or(json!({}));
        if !tools::tool_defs()
            .as_array()
            .map(|t| t.iter().any(|d| d["name"] == name))
            .unwrap_or(false)
        {
            return err_resp(id, INVALID_PARAMS, &format!("Unknown tool: {name}"), None);
        }
        if self.cancelled_ids.lock().unwrap().contains(&id.to_string()) {
            self.cancel.store(true, Ordering::Relaxed);
        }
        match tools::call(&self.state, name, &args, &self.cancel).await {
            Ok(structured) => {
                let text = serde_json::to_string_pretty(&structured).unwrap_or_default();
                ok_resp(
                    id,
                    json!({
                        "resultType": "complete",
                        "content": [{ "type": "text", "text": text }],
                        "structuredContent": structured,
                        "isError": false
                    }),
                )
            }
            Err(msg) => {
                tracing::warn!(tool = name, error = %msg, "Tool call failed");
                ok_resp(
                    id,
                    json!({
                        "resultType": "complete",
                        "content": [{ "type": "text", "text": msg }],
                        "isError": true
                    }),
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::ToolState;
    use activity_monitor_core::db::ActivityDatabase;
    use activity_monitor_core::timefmt;

    fn seed_db() -> (TempGuard, ActivityDatabase) {
        let dir = std::env::temp_dir().join(format!("mcp-test-{}", uuid_lite()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = ActivityDatabase::open(&dir.join("ActivityData.db")).unwrap();
        db.initialize().unwrap();
        let ts = timefmt::today_start_utc_o();
        db.conn()
            .execute(
                "INSERT INTO ActivityEvents (Timestamp, EventType, ProcessName, WindowTitle, IsIdle) VALUES (?1, 'focus_change', 'code.exe', 'main.rs - VS Code', 0)",
                rusqlite::params![ts],
            )
            .unwrap();
        db.conn()
            .execute(
                "INSERT INTO InferenceResults (RequestId, ProcessedAt, ActivityLabel, Summary, Confidence) VALUES ('r1', ?1, 'Coding in VS Code', 'Editing main.rs', 0.9)",
                rusqlite::params![ts],
            )
            .unwrap();
        (TempGuard(dir), db)
    }

    struct TempGuard(std::path::PathBuf);
    impl Drop for TempGuard {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn uuid_lite() -> String {
        // Windows clock resolution is ~15 ms, so nanos alone collide when
        // tests run in parallel; add a process-local counter.
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        format!(
            "{}-{n}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        )
    }

    fn rpc() -> (Rpc, TempGuard) {
        let (guard, db) = seed_db();
        (
            Rpc::new(Arc::new(ToolState {
                db: std::sync::Mutex::new(Some(db)),
                ollama: None,
            })),
            guard,
        )
    }

    fn req(id: i64, method: &str, params: Value) -> Request {
        Request {
            id: Some(json!(id)),
            method: method.into(),
            params,
            meta: json!({}),
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn discover_modern() {
        let (r, _g) = rpc();
        let resp = r
            .handle(Request {
                meta: json!({ "io.modelcontextprotocol/protocolVersion": "2026-07-28" }),
                ..req(1, "server/discover", json!({}))
            })
            .await
            .unwrap();
        assert_eq!(resp["result"]["supportedVersions"][0], "2026-07-28");
        assert_eq!(resp["result"]["resultType"], "complete");
        assert_eq!(resp["result"]["_meta"]["io.modelcontextprotocol/serverInfo"]["name"], SERVER_NAME);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn legacy_initialize_handshake() {
        let (r, _g) = rpc();
        let resp = r
            .handle(req(1, "initialize", json!({"protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name":"t","version":"0"}})))
            .await
            .unwrap();
        assert_eq!(resp["result"]["protocolVersion"], "2025-11-25");
        assert!(resp["result"]["capabilities"]["tools"].is_object());
        assert_eq!(resp["result"]["serverInfo"]["name"], SERVER_NAME);
        // initialized notification -> no response
        assert!(r.handle(req(0, "notifications/initialized", json!({}))).await.is_none());
        // ping
        assert_eq!(r.handle(req(2, "ping", json!({}))).await.unwrap()["result"], json!({}));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn tools_list_and_call() {
        let (r, _g) = rpc();
        let list = r.handle(req(1, "tools/list", json!({}))).await.unwrap();
        let tools = list["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 7);
        assert!(tools.iter().all(|t| t["inputSchema"]["type"] == "object" && t["annotations"]["readOnlyHint"] == true));
        let today = timefmt::today_local_date().to_string();
        let call = r
            .handle(req(2, "tools/call", json!({"name": "todays_timeline", "arguments": {"date": today}})))
            .await
            .unwrap();
        assert_eq!(call["result"]["isError"], false);
        let events = call["result"]["structuredContent"]["events"].as_array().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["processName"], "code.exe");
        // text block mirrors the structured content
        assert!(call["result"]["content"][0]["text"].as_str().unwrap().contains("code.exe"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn search_activity_hits_and_misses() {
        let (r, _g) = rpc();
        let hit = r
            .handle(req(
                1,
                "tools/call",
                json!({"name": "search_activity", "arguments": {"query": "main.rs"}}),
            ))
            .await
            .unwrap();
        assert_eq!(hit["result"]["isError"], false);
        let sc = &hit["result"]["structuredContent"];
        assert_eq!(sc["count"], 1);
        assert_eq!(sc["matches"][0]["activityLabel"], "Coding in VS Code");
        let miss = r
            .handle(req(
                2,
                "tools/call",
                json!({"name": "search_activity", "arguments": {"query": "nosuchkeyword"}}),
            ))
            .await
            .unwrap();
        assert_eq!(miss["result"]["isError"], false);
        assert_eq!(miss["result"]["structuredContent"]["count"], 0);
        // missing query -> tool error result (not a protocol error)
        let bad = r
            .handle(req(3, "tools/call", json!({"name": "search_activity", "arguments": {}})))
            .await
            .unwrap();
        assert_eq!(bad["result"]["isError"], true);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn errors_and_version_gate() {
        let (r, _g) = rpc();
        let bad = r
            .handle(Request {
                meta: json!({ "io.modelcontextprotocol/protocolVersion": "1900-01-01" }),
                ..req(1, "tools/list", json!({}))
            })
            .await
            .unwrap();
        assert_eq!(bad["error"]["code"], UNSUPPORTED_VERSION);
        assert_eq!(bad["error"]["data"]["supported"][0], "2026-07-28");
        let unknown = r.handle(req(2, "nope/method", json!({}))).await.unwrap();
        assert_eq!(unknown["error"]["code"], METHOD_NOT_FOUND);
        let badtool = r.handle(req(3, "tools/call", json!({"name": "nope", "arguments": {}}))).await.unwrap();
        assert_eq!(badtool["error"]["code"], INVALID_PARAMS);
        // notification with unknown method -> silence
        assert!(r.handle(Request { id: None, method: "notifications/whatever".into(), params: json!({}), meta: json!({}) }).await.is_none());
        // cancelled notification -> silence
        assert!(r.handle(Request { id: None, method: "notifications/cancelled".into(), params: json!({"requestId": 9}), meta: json!({}) }).await.is_none());
    }

    #[test]
    fn parse_errors() {
        assert!(parse_message("not json").is_err());
        let e = parse_message("{\"jsonrpc\":\"1.0\",\"id\":1,\"method\":\"x\"}").unwrap_err();
        assert_eq!(e["error"]["code"], INVALID_REQUEST);
    }
}
