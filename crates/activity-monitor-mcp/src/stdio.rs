//! stdio transport: one JSON-RPC message per line on stdin, responses on
//! stdout (newline-delimited, no embedded newlines). EOF ends the session.

use std::io::Write as _;
use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, BufReader};

use crate::rpc::{self, Rpc};

pub async fn serve(rpc: Arc<Rpc>) {
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    // Requests are handled strictly one at a time, so the blocking stdout
    // lock is fine and keeps writes line-atomic for the client.
    let mut out = std::io::stdout().lock();
    while let Ok(Some(line)) = lines.next_line().await {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let resp = match rpc::parse_message(line) {
            Ok(req) => rpc.handle(req).await,
            Err(e) => Some(e),
        };
        if let Some(resp) = resp {
            if let Ok(mut s) = serde_json::to_string(&resp) {
                s.push('\n');
                if out.write_all(s.as_bytes()).is_err() {
                    break; // client went away
                }
                let _ = out.flush().is_err();
            }
        }
    }
    tracing::info!("stdin closed, shutting down");
}
