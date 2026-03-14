use crate::mcp::protocol::{
    InitializeResult, JsonRpcRequest, JsonRpcResponse, ServerCapabilities, ServerInfo,
    ToolsCapability,
};
use crate::mcp::tools;
use crate::search::SearchEngine;
use serde_json::{json, Value};

/// Dispatch a JSON-RPC request and return a response (or None for notifications).
pub fn handle_request(
    request: &JsonRpcRequest,
    engine: &SearchEngine,
    token_budget: usize,
) -> Option<JsonRpcResponse> {
    match request.method.as_str() {
        // ── Lifecycle ──
        "initialize" => Some(handle_initialize(request.id.clone())),

        "initialized" => None, // Notification, no response

        "ping" => Some(JsonRpcResponse::success(request.id.clone(), json!({}))),

        // ── Tools ──
        "tools/list" => Some(handle_tools_list(request.id.clone())),

        "tools/call" => Some(handle_tools_call(
            request.id.clone(),
            &request.params,
            engine,
            token_budget,
        )),

        // ── Unknown ──
        _ => {
            // Per JSON-RPC: if it has an id, respond with method not found
            if request.id.is_some() {
                Some(JsonRpcResponse::error(
                    request.id.clone(),
                    -32601,
                    format!("Method not found: {}", request.method),
                ))
            } else {
                None // Unknown notification, ignore
            }
        }
    }
}

fn handle_initialize(id: Option<Value>) -> JsonRpcResponse {
    let result = InitializeResult {
        protocol_version: "2024-11-05".to_string(),
        capabilities: ServerCapabilities {
            tools: ToolsCapability {},
        },
        server_info: ServerInfo {
            name: "lumina".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
    };

    JsonRpcResponse::success(id, serde_json::to_value(result).unwrap())
}

fn handle_tools_list(id: Option<Value>) -> JsonRpcResponse {
    let definitions = tools::tool_definitions();
    JsonRpcResponse::success(id, json!({ "tools": definitions }))
}

fn handle_tools_call(
    id: Option<Value>,
    params: &Option<Value>,
    engine: &SearchEngine,
    token_budget: usize,
) -> JsonRpcResponse {
    let params = match params {
        Some(p) => p,
        None => {
            return JsonRpcResponse::error(id, -32602, "Missing params".to_string());
        }
    };

    let tool_name = match params.get("name").and_then(|v| v.as_str()) {
        Some(n) => n,
        None => {
            return JsonRpcResponse::error(id, -32602, "Missing tool name in params".to_string());
        }
    };

    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or(json!({}));

    let result = tools::handle_tool_call(engine, tool_name, &arguments, token_budget);

    JsonRpcResponse::success(id, serde_json::to_value(result).unwrap())
}
