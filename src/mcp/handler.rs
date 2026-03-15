use crate::config::LuminaConfig;
use crate::mcp::protocol::{
    InitializeResult, JsonRpcRequest, JsonRpcResponse, ServerCapabilities, ServerInfo,
    ToolsCapability,
};
use crate::mcp::tools;
use crate::search::SearchEngine;
use serde_json::{json, Value};

/// Result of handling a request. RebuildEngine signals the server loop
/// to recreate the SearchEngine (after indexing).
pub enum HandlerResult {
    /// Normal response (or None for notifications).
    Response(Option<JsonRpcResponse>),
    /// Send this response, then rebuild the SearchEngine.
    RebuildEngine(JsonRpcResponse),
}

/// Dispatch a JSON-RPC request.
pub fn handle_request(
    request: &JsonRpcRequest,
    engine: &SearchEngine,
    config: &LuminaConfig,
) -> HandlerResult {
    match request.method.as_str() {
        // ── Lifecycle ──
        "initialize" => HandlerResult::Response(Some(handle_initialize(request.id.clone()))),

        "initialized" => HandlerResult::Response(None),

        "ping" => HandlerResult::Response(Some(
            JsonRpcResponse::success(request.id.clone(), json!({})),
        )),

        // ── Tools ──
        "tools/list" => HandlerResult::Response(Some(handle_tools_list(request.id.clone()))),

        "tools/call" => handle_tools_call(
            request.id.clone(),
            &request.params,
            engine,
            config,
        ),

        // ── Unknown ──
        _ => {
            if request.id.is_some() {
                HandlerResult::Response(Some(JsonRpcResponse::error(
                    request.id.clone(),
                    -32601,
                    format!("Method not found: {}", request.method),
                )))
            } else {
                HandlerResult::Response(None)
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
    config: &LuminaConfig,
) -> HandlerResult {
    let params = match params {
        Some(p) => p,
        None => {
            return HandlerResult::Response(Some(
                JsonRpcResponse::error(id, -32602, "Missing params".to_string()),
            ));
        }
    };

    let tool_name = match params.get("name").and_then(|v| v.as_str()) {
        Some(n) => n,
        None => {
            return HandlerResult::Response(Some(
                JsonRpcResponse::error(id, -32602, "Missing tool name in params".to_string()),
            ));
        }
    };

    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or(json!({}));

    let (result, needs_rebuild) = tools::handle_tool_call(
        engine,
        config,
        tool_name,
        &arguments,
        config.response_token_budget,
    );

    let response = JsonRpcResponse::success(id, serde_json::to_value(result).unwrap());

    if needs_rebuild {
        HandlerResult::RebuildEngine(response)
    } else {
        HandlerResult::Response(Some(response))
    }
}
