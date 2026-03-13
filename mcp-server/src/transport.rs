use std::io::{self, BufRead, Write};

use anyhow::Result;
use serde_json::Value;

use crate::handler::McpHandler;
use crate::protocol::{JsonRpcRequest, JsonRpcResponse};

/// Run the MCP server over stdin/stdout (JSON-RPC over stdio).
///
/// Each line on stdin is a JSON-RPC request. Each response is written as a
/// single line to stdout. All diagnostic output goes to stderr.
pub async fn run_stdio(handler: McpHandler) -> Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let reader = stdin.lock();

    eprintln!("savhub-mcp: server starting");

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                eprintln!("savhub-mcp: stdin read error: {e}");
                break;
            }
        };

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let request: JsonRpcRequest = match serde_json::from_str(trimmed) {
            Ok(req) => req,
            Err(e) => {
                eprintln!("savhub-mcp: parse error: {e}");
                let response = JsonRpcResponse::error(None, -32700, "Parse error");
                write_response(&stdout, &response)?;
                continue;
            }
        };

        // Notifications (no id) don't require a response
        let is_notification = request.id.is_none();

        eprintln!("savhub-mcp: <- {}", request.method);

        let response = handler.handle_request(&request).await;

        if let Some(response) = response {
            if !is_notification {
                write_response(&stdout, &response)?;
            }
        }
    }

    eprintln!("savhub-mcp: server stopping");
    Ok(())
}

fn write_response(stdout: &io::Stdout, response: &JsonRpcResponse) -> Result<()> {
    let json = serde_json::to_string(response)?;
    let mut out = stdout.lock();
    writeln!(out, "{json}")?;
    out.flush()?;
    Ok(())
}

/// Send a JSON-RPC notification (no id, no response expected).
#[allow(dead_code)]
pub fn send_notification(method: &str, params: Value) -> Result<()> {
    let notification = serde_json::json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params
    });
    let json = serde_json::to_string(&notification)?;
    let stdout = io::stdout();
    let mut out = stdout.lock();
    writeln!(out, "{json}")?;
    out.flush()?;
    Ok(())
}
