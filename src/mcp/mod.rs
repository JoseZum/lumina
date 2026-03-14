pub mod handler;
pub mod protocol;
pub mod tools;

use crate::mcp::protocol::JsonRpcRequest;
use crate::search::SearchEngine;
use std::io::{self, BufRead, Write};
use tracing::{debug, error, info};

/// Run the MCP server over stdio (NDJSON transport).
///
/// Protocol: newline-delimited JSON-RPC 2.0
/// - Read one line from stdin = one JSON-RPC request
/// - Write one line to stdout = one JSON-RPC response
/// - stderr is used for logging (never pollute stdout)
/// - Flush stdout after every write
pub fn run_server(engine: SearchEngine, token_budget: usize) -> io::Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let reader = stdin.lock();

    info!("Lumina MCP server starting...");

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                error!("Failed to read stdin: {}", e);
                break;
            }
        };

        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }

        debug!("← {}", line);

        // Parse JSON-RPC request
        let request: JsonRpcRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                error!("Failed to parse JSON-RPC: {}", e);
                let error_response = protocol::JsonRpcResponse::error(
                    None,
                    -32700,
                    format!("Parse error: {}", e),
                );
                let json = serde_json::to_string(&error_response).unwrap();
                writeln!(stdout, "{}", json)?;
                stdout.flush()?;
                continue;
            }
        };

        // Dispatch request
        if let Some(response) = handler::handle_request(&request, &engine, token_budget) {
            let json = serde_json::to_string(&response).unwrap();
            debug!("→ {}", json);
            writeln!(stdout, "{}", json)?;
            stdout.flush()?;
        }
        // If None, it was a notification — no response needed
    }

    info!("Lumina MCP server shutting down.");
    Ok(())
}
