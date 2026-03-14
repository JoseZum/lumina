# 05 - MCP Protocol Deep Dive

This is the most failure-prone part of implementing an MCP server from scratch.
If the handshake isn't exactly right, Claude Code silently drops the server.
No error message, no log, nothing. It just doesn't show up as a tool.

This document specifies EVERY message in the protocol with exact JSON.

---

## Transport: stdio

```
┌──────────────┐     stdin (JSON-RPC)      ┌──────────────┐
│              │ ─────────────────────────► │              │
│  Claude Code │                            │    Lumina    │
│  (MCP Client)│ ◄───────────────────────── │  (MCP Server)│
│              │     stdout (JSON-RPC)      │              │
└──────────────┘                            └──────────────┘
                                               │
                                               │ stderr (logging)
                                               ▼
                                            /dev/null or log file
```

**Message framing**: Newline-delimited JSON (NDJSON).
- Each message is ONE complete JSON object on ONE line.
- Terminated by `\n` (newline character).
- NO Content-Length headers (that's LSP, not MCP).
- NO pretty-printing (no indentation, no multi-line JSON).
- NO empty lines between messages.
- NO BOM or other prefixes.

**Critical rules:**
1. `stdout` is EXCLUSIVELY for JSON-RPC messages. Not a single byte of non-JSON
   may be written to stdout. No `println!()`, no library debug output, nothing.
2. `stderr` is for logging. Configure `tracing-subscriber` to write to stderr.
3. Every message must be valid JSON. One malformed message kills the connection.
4. Flush stdout after every write. Buffered IO will cause Claude Code to hang.

---

## Protocol Version

As of the MCP specification current at time of writing, the protocol version is:

```
"2024-11-05"
```

The server MUST respond with the same protocol version the client sends. If the
client sends a newer version the server doesn't support, the server should respond
with the latest version it supports. Claude Code will negotiate down if possible.

**Important**: Check the MCP specification for the latest protocol version when
implementing. If Claude Code sends a version like "2025-03-26", you may need to
update. The safest approach: echo back whatever version the client sends, as long
as you support the core methods (initialize, tools/list, tools/call).

---

## Complete Message Sequence

### Phase 1: Initialization

The client initiates the connection. The server MUST NOT send anything before
receiving the `initialize` request.

#### Step 1: Client → Server (initialize request)

```json
{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{"roots":{"listChanged":true}},"clientInfo":{"name":"claude-code","version":"1.0.0"}}}
```

**Fields the server must handle:**
- `id`: Can be a number (0, 1, 2...) or a string ("init-1"). MUST be echoed
  back exactly in the response. Do NOT convert between types.
- `method`: Always `"initialize"` for this step.
- `params.protocolVersion`: The version the client wants to use.
- `params.capabilities`: What the client supports. For now, we can ignore this.
- `params.clientInfo`: Name and version of the client. Informational.

#### Step 2: Server → Client (initialize response)

```json
{"jsonrpc":"2.0","id":0,"result":{"protocolVersion":"2024-11-05","capabilities":{"tools":{}},"serverInfo":{"name":"lumina","version":"0.1.0"}}}
```

**Critical fields:**
- `id`: MUST match the request's `id` exactly. If request had `"id": 0`, response
  must have `"id": 0` (number, not string "0").
- `result.protocolVersion`: MUST match what the client sent (or a supported version).
- `result.capabilities.tools`: This object tells the client "I have tools." If this
  is missing, the client will NEVER call `tools/list`. The value `{}` means "I have
  tools, with default capabilities." You can also use `{"listChanged": false}` to
  indicate the tool list is static (doesn't change at runtime).
- `result.serverInfo.name`: Your server's name. Shows up in Claude Code's UI.
- `result.serverInfo.version`: Your server's version.

**What NOT to include:**
- Do NOT include `error` field. This is a success response.
- Do NOT include capabilities you don't support (resources, prompts, etc.)

#### Step 3: Client → Server (initialized notification)

```json
{"jsonrpc":"2.0","method":"notifications/initialized"}
```

**This is a NOTIFICATION, not a request:**
- It has NO `id` field.
- The server MUST NOT respond to it. If you send a response, the client may
  interpret it as a response to a future request and get confused.
- After receiving this, the server is ready for normal operation.

**Server action**: Set an internal `initialized` flag to `true`.

### Phase 2: Tool Discovery

After initialization, the client queries what tools are available.

#### Step 4: Client → Server (tools/list request)

```json
{"jsonrpc":"2.0","id":1,"method":"tools/list"}
```

**Note**: The `params` field may be absent, `null`, or `{}`. Handle all three:
```rust
// In deserialization
#[serde(default)]
pub params: Option<serde_json::Value>,
```

#### Step 5: Server → Client (tools/list response)

```json
{"jsonrpc":"2.0","id":1,"result":{"tools":[{"name":"semantic_search","description":"Search the codebase semantically. Use this when you need to find code related to a concept, understand how something is implemented, or locate relevant functions and classes. Returns ranked code chunks with file paths and line numbers.","inputSchema":{"type":"object","properties":{"query":{"type":"string","description":"Natural language description of what you're looking for. Examples: 'authentication middleware', 'database connection setup', 'error handling for API calls'"},"k":{"type":"integer","description":"Number of results to return (1-20, default: 5)","default":5,"minimum":1,"maximum":20}},"required":["query"]}},{"name":"find_symbol","description":"Find the definition of a specific symbol (function, class, struct, method, trait) by name. Use this when you know the exact name of what you're looking for. Returns the location and source code.","inputSchema":{"type":"object","properties":{"name":{"type":"string","description":"Exact or partial symbol name. Examples: 'UserService', 'create_user', 'AuthMiddleware'"}},"required":["name"]}},{"name":"get_file_span","description":"Read a specific range of lines from a source file. Use this to see more context around a search result or to read a specific section of code.","inputSchema":{"type":"object","properties":{"file":{"type":"string","description":"File path relative to the repository root. Example: 'src/auth/middleware.rs'"},"start_line":{"type":"integer","description":"Starting line number (1-indexed)","minimum":1},"end_line":{"type":"integer","description":"Ending line number (1-indexed, inclusive)","minimum":1}},"required":["file","start_line","end_line"]}},{"name":"list_indexed_files","description":"List all files in the search index with metadata (language, line count, chunk count). Use this to understand the codebase structure and what's available to search.","inputSchema":{"type":"object","properties":{}}}]}}
```

**Tool description guidelines** (these matter for auto-invocation):
- Start with what the tool does, then when to use it.
- Include "Use this when..." to guide Claude's tool selection.
- Keep descriptions under ~200 characters for token efficiency.
- Use concrete examples in parameter descriptions.
- The `inputSchema` follows JSON Schema draft 2020-12 format.

**Critical**: `inputSchema` must be valid JSON Schema. If it's malformed, Claude Code
may not render the tool correctly. Test your schemas with a JSON Schema validator.

### Phase 3: Tool Execution

When Claude decides to use a tool, the client sends a `tools/call` request.

#### Tool Call: semantic_search

**Request:**
```json
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"semantic_search","arguments":{"query":"authentication middleware","k":5}}}
```

**Success Response:**
```json
{"jsonrpc":"2.0","id":2,"result":{"content":[{"type":"text","text":"## Results for: \"authentication middleware\"\n\n### 1. src/middleware/auth.rs:15-42 — `authenticate` (function)\n```rust\npub async fn authenticate(req: &Request) -> Result<Claims> {\n    let token = req.header(\"Authorization\")\n        .ok_or(AuthError::MissingToken)?;\n    let claims = verify_jwt(token)?;\n    Ok(claims)\n}\n```\n\n### 2. src/handlers/login.rs:8-30 — `login_handler` (function)\n```rust\npub async fn login_handler(\n    Json(body): Json<LoginRequest>,\n) -> Result<Json<TokenResponse>> {\n    let user = find_user(&body.email).await?;\n    let token = create_jwt(&user)?;\n    Ok(Json(TokenResponse { token }))\n}\n```\n\n---\n*2 results from 2 files | ~420 tokens*"}],"isError":false}}
```

**Error Response (tool-level error):**
```json
{"jsonrpc":"2.0","id":2,"result":{"content":[{"type":"text","text":"Error: Index not found at '.lumina/'. Run `lumina index` first to build the search index."}],"isError":true}}
```

**Key distinction:**
- `result.isError: true` → the tool ran but encountered an error (missing index, file not found)
- `error` (instead of `result`) → protocol-level error (method not found, invalid params)

#### Tool Call: find_symbol

**Request:**
```json
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"find_symbol","arguments":{"name":"UserService"}}}
```

**Response:**
```json
{"jsonrpc":"2.0","id":3,"result":{"content":[{"type":"text","text":"## Symbol: UserService\n\n**Kind:** struct\n**File:** src/models/user.rs:12-45\n\n```rust\npub struct UserService {\n    db: DatabasePool,\n    cache: RedisClient,\n}\n\nimpl UserService {\n    pub fn new(db: DatabasePool, cache: RedisClient) -> Self { ... }\n    pub async fn create_user(&self, req: CreateUserRequest) -> Result<User> { ... }\n    pub async fn find_by_id(&self, id: Uuid) -> Result<Option<User>> { ... }\n    pub async fn update_email(&self, id: Uuid, email: String) -> Result<()> { ... }\n}\n```"}],"isError":false}}
```

#### Tool Call: get_file_span

**Request:**
```json
{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"get_file_span","arguments":{"file":"src/middleware/auth.rs","start_line":1,"end_line":50}}}
```

**Response:**
```json
{"jsonrpc":"2.0","id":4,"result":{"content":[{"type":"text","text":"## src/middleware/auth.rs (lines 1-50)\n\n```rust\nuse axum::{\n    extract::Request,\n    middleware::Next,\n    response::Response,\n};\nuse jsonwebtoken::{decode, DecodingKey, Validation};\n\n/// Authentication middleware.\n/// Validates JWT tokens and injects claims into request extensions.\npub async fn authenticate(\n    req: Request,\n    next: Next,\n) -> Result<Response, AuthError> {\n    let token = req.headers()\n        .get(\"Authorization\")\n        .and_then(|v| v.to_str().ok())\n        .and_then(|v| v.strip_prefix(\"Bearer \"))\n        .ok_or(AuthError::MissingToken)?;\n\n    let claims = decode::<Claims>(\n        token,\n        &DecodingKey::from_secret(SECRET.as_bytes()),\n        &Validation::default(),\n    )\n    .map_err(|_| AuthError::InvalidToken)?;\n\n    let mut req = req;\n    req.extensions_mut().insert(claims.claims);\n    Ok(next.run(req).await)\n}\n```"}],"isError":false}}
```

#### Tool Call: list_indexed_files

**Request:**
```json
{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"list_indexed_files","arguments":{}}}
```

**Response:**
```json
{"jsonrpc":"2.0","id":5,"result":{"content":[{"type":"text","text":"## Indexed Files (23 files, 156 chunks)\n\n| File | Language | Lines | Chunks |\n|------|----------|-------|--------|\n| src/main.rs | rust | 45 | 3 |\n| src/lib.rs | rust | 12 | 1 |\n| src/middleware/auth.rs | rust | 89 | 5 |\n| src/handlers/login.rs | rust | 67 | 4 |\n| src/handlers/users.rs | rust | 123 | 8 |\n| src/models/user.rs | rust | 156 | 10 |\n| ... | ... | ... | ... |\n\n*23 files | 4,567 total lines | 156 chunks indexed*"}],"isError":false}}
```

### Phase 4: Shutdown

The client closes stdin when it's done. The server should detect EOF and exit cleanly.

```rust
// In the main loop
for line in reader.lines() {
    let line = match line {
        Ok(line) => line,
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
        Err(e) => return Err(e.into()),
    };
    // ... process line
}
// Clean shutdown: flush stores, save state
```

No explicit shutdown message in the current MCP spec. EOF on stdin = done.

---

## Error Handling

### Protocol Errors (JSON-RPC level)

These indicate a problem with the message itself, not the tool execution.

**Parse error (malformed JSON):**
```json
{"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"Parse error: invalid JSON"}}
```

**Method not found:**
```json
{"jsonrpc":"2.0","id":6,"error":{"code":-32601,"message":"Method not found: tools/nonexistent"}}
```

**Invalid params:**
```json
{"jsonrpc":"2.0","id":7,"error":{"code":-32602,"message":"Invalid params: missing required field 'name'"}}
```

**Internal error:**
```json
{"jsonrpc":"2.0","id":8,"error":{"code":-32603,"message":"Internal error: database connection failed"}}
```

### Standard JSON-RPC Error Codes

```rust
pub const PARSE_ERROR: i32 = -32700;
pub const INVALID_REQUEST: i32 = -32600;
pub const METHOD_NOT_FOUND: i32 = -32601;
pub const INVALID_PARAMS: i32 = -32602;
pub const INTERNAL_ERROR: i32 = -32603;
```

### Tool Errors vs Protocol Errors

| Scenario | Error Type | Response Format |
|----------|-----------|-----------------|
| Unknown method "foo/bar" | Protocol | `{"error": {"code": -32601, ...}}` |
| Missing required param | Protocol | `{"error": {"code": -32602, ...}}` |
| Unknown tool name | Tool | `{"result": {"content": [...], "isError": true}}` |
| Index doesn't exist | Tool | `{"result": {"content": [...], "isError": true}}` |
| File not found | Tool | `{"result": {"content": [...], "isError": true}}` |
| Embedding API failure | Tool | `{"result": {"content": [...], "isError": true}}` |

Rule of thumb: If the method was correctly routed and the tool ran, use `isError`.
If the method couldn't be routed or params are invalid, use `error`.

---

## `src/mcp/protocol.rs` — Complete Type Definitions

```rust
use serde::{Deserialize, Serialize};
use serde_json::Value;

// ── Incoming Messages ──

/// A JSON-RPC 2.0 message from the MCP client.
/// Can be a Request (has id) or a Notification (no id).
#[derive(Debug, Deserialize)]
pub struct JsonRpcMessage {
    pub jsonrpc: String,

    /// Present for requests, absent for notifications.
    /// Can be a number or string — stored as Value to preserve type.
    #[serde(default)]
    pub id: Option<Value>,

    /// The RPC method name.
    /// Examples: "initialize", "tools/list", "tools/call",
    ///           "notifications/initialized"
    pub method: Option<String>,

    /// Method parameters. May be absent, null, or an object.
    #[serde(default)]
    pub params: Option<Value>,
}

impl JsonRpcMessage {
    /// Returns true if this is a notification (no id = no response expected).
    pub fn is_notification(&self) -> bool {
        self.id.is_none()
    }

    /// Get params as an object, defaulting to empty object.
    pub fn params_obj(&self) -> Value {
        self.params.clone().unwrap_or(Value::Object(Default::default()))
    }
}

// ── Outgoing Messages ──

/// A JSON-RPC 2.0 response to send back to the client.
#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Value,

    /// Present on success. Absent on error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,

    /// Present on error. Absent on success.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

impl JsonRpcResponse {
    /// Create a success response.
    pub fn success(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    /// Create an error response.
    pub fn error(id: Value, code: i32, message: String) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError { code, message }),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
}

// ── MCP-specific Types ──

/// The result of a tool execution.
#[derive(Debug, Serialize)]
pub struct ToolResult {
    pub content: Vec<ContentBlock>,
    #[serde(rename = "isError")]
    pub is_error: bool,
}

impl ToolResult {
    pub fn text(text: String) -> Self {
        Self {
            content: vec![ContentBlock::text(text)],
            is_error: false,
        }
    }

    pub fn error(message: String) -> Self {
        Self {
            content: vec![ContentBlock::text(message)],
            is_error: true,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ContentBlock {
    #[serde(rename = "type")]
    pub content_type: String,
    pub text: String,
}

impl ContentBlock {
    pub fn text(text: String) -> Self {
        Self {
            content_type: "text".to_string(),
            text,
        }
    }
}

/// Tool definition for the tools/list response.
#[derive(Debug, Serialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
}

// ── JSON-RPC Error Codes ──
pub const PARSE_ERROR: i32 = -32700;
pub const INVALID_REQUEST: i32 = -32600;
pub const METHOD_NOT_FOUND: i32 = -32601;
pub const INVALID_PARAMS: i32 = -32602;
pub const INTERNAL_ERROR: i32 = -32603;
```

---

## `src/mcp/mod.rs` — Server Main Loop

```rust
use crate::error::Result;
use crate::mcp::handler;
use crate::mcp::protocol::JsonRpcMessage;
use crate::search::SearchEngine;
use std::io::{self, BufRead, Write};
use tracing::{debug, error, info};

pub mod handler;
pub mod protocol;
pub mod tools;

pub struct McpServer {
    pub search_engine: SearchEngine,
    pub initialized: bool,
}

impl McpServer {
    pub fn new(search_engine: SearchEngine) -> Self {
        Self {
            search_engine,
            initialized: false,
        }
    }

    /// Run the MCP server main loop.
    ///
    /// Reads JSON-RPC messages from stdin, one per line.
    /// Writes responses to stdout.
    /// Logs to stderr via tracing.
    ///
    /// Exits when stdin reaches EOF (client closed connection).
    pub fn run(&mut self) -> Result<()> {
        let stdin = io::stdin();
        let stdout = io::stdout();
        let reader = io::BufReader::new(stdin.lock());
        let mut writer = io::BufWriter::new(stdout.lock());

        info!("Lumina MCP server starting");

        for line_result in reader.lines() {
            let line = match line_result {
                Ok(line) => line,
                Err(e) => {
                    // EOF or broken pipe = client disconnected
                    debug!("stdin closed: {}", e);
                    break;
                }
            };

            // Skip empty lines
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            // Parse JSON-RPC message
            let msg: JsonRpcMessage = match serde_json::from_str(line) {
                Ok(msg) => msg,
                Err(e) => {
                    error!("Failed to parse JSON-RPC message: {}", e);
                    let error_response = protocol::JsonRpcResponse::error(
                        serde_json::Value::Null,
                        protocol::PARSE_ERROR,
                        format!("Parse error: {}", e),
                    );
                    let json = serde_json::to_string(&error_response)?;
                    writeln!(writer, "{}", json)?;
                    writer.flush()?;
                    continue;
                }
            };

            debug!("Received: method={:?}, id={:?}", msg.method, msg.id);

            // Notifications (no id) — handle without response
            if msg.is_notification() {
                handler::handle_notification(self, &msg);
                continue;
            }

            // Requests (has id) — must respond
            let response = handler::handle_request(self, &msg);
            let json = serde_json::to_string(&response)?;

            debug!("Responding: {} bytes", json.len());

            writeln!(writer, "{}", json)?;
            writer.flush()?;  // CRITICAL: must flush after every message
        }

        info!("Lumina MCP server shutting down");
        Ok(())
    }
}
```

---

## `src/mcp/handler.rs` — Request Dispatcher

```rust
use crate::mcp::protocol::*;
use crate::mcp::tools;
use crate::mcp::McpServer;
use serde_json::{json, Value};
use tracing::warn;

/// Handle a JSON-RPC request (has id, expects response).
pub fn handle_request(server: &McpServer, msg: &JsonRpcMessage) -> JsonRpcResponse {
    let id = msg.id.clone().unwrap_or(Value::Null);
    let method = msg.method.as_deref().unwrap_or("");

    match method {
        "initialize" => handle_initialize(id, msg),
        "tools/list" => handle_tools_list(id),
        "tools/call" => handle_tools_call(server, id, msg),
        _ => {
            warn!("Unknown method: {}", method);
            JsonRpcResponse::error(id, METHOD_NOT_FOUND, format!("Method not found: {}", method))
        }
    }
}

/// Handle a JSON-RPC notification (no id, no response).
pub fn handle_notification(server: &mut McpServer, msg: &JsonRpcMessage) {
    let method = msg.method.as_deref().unwrap_or("");

    match method {
        "notifications/initialized" => {
            server.initialized = true;
            tracing::info!("MCP client initialized");
        }
        "notifications/cancelled" => {
            // Client cancelled a request. We don't support cancellation
            // in this simple implementation, but we shouldn't error.
            tracing::debug!("Received cancellation notification (ignored)");
        }
        _ => {
            // Per JSON-RPC spec, unknown notifications are silently ignored
            tracing::debug!("Unknown notification: {}", method);
        }
    }
}

fn handle_initialize(id: Value, _msg: &JsonRpcMessage) -> JsonRpcResponse {
    // Echo back the protocol version from the client.
    // In a more robust implementation, we'd negotiate versions.
    JsonRpcResponse::success(id, json!({
        "protocolVersion": "2024-11-05",
        "capabilities": {
            "tools": {}
        },
        "serverInfo": {
            "name": "lumina",
            "version": env!("CARGO_PKG_VERSION")
        }
    }))
}

fn handle_tools_list(id: Value) -> JsonRpcResponse {
    let tool_defs = tools::tool_definitions();
    let tools_json: Vec<Value> = tool_defs.into_iter()
        .map(|t| serde_json::to_value(t).unwrap())
        .collect();

    JsonRpcResponse::success(id, json!({ "tools": tools_json }))
}

fn handle_tools_call(server: &McpServer, id: Value, msg: &JsonRpcMessage) -> JsonRpcResponse {
    let params = msg.params_obj();

    let tool_name = match params.get("name").and_then(|v| v.as_str()) {
        Some(name) => name,
        None => {
            return JsonRpcResponse::error(
                id,
                INVALID_PARAMS,
                "Missing required parameter: name".to_string(),
            );
        }
    };

    let arguments = params.get("arguments")
        .cloned()
        .unwrap_or(Value::Object(Default::default()));

    let tool_result = tools::handle_tool_call(&server.search_engine, tool_name, arguments);
    let result_json = serde_json::to_value(tool_result).unwrap();

    JsonRpcResponse::success(id, result_json)
}
```

---

## `src/mcp/tools.rs` — Tool Definitions & Handlers

```rust
use crate::mcp::protocol::{ToolDefinition, ToolResult};
use crate::search::SearchEngine;
use serde_json::{json, Value};
use tracing::error;

/// Returns definitions for all 4 MCP tools.
pub fn tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "semantic_search".to_string(),
            description: "Search the codebase semantically. Use this when you need to find code related to a concept, understand how something is implemented, or locate relevant functions and classes. Returns ranked code chunks with file paths and line numbers.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Natural language description of what you're looking for"
                    },
                    "k": {
                        "type": "integer",
                        "description": "Number of results to return (1-20, default: 5)",
                        "default": 5,
                        "minimum": 1,
                        "maximum": 20
                    }
                },
                "required": ["query"]
            }),
        },
        ToolDefinition {
            name: "find_symbol".to_string(),
            description: "Find the definition of a specific symbol by name. Use when you know the exact name of a function, class, struct, or method.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Symbol name (e.g. 'UserService', 'create_user')"
                    }
                },
                "required": ["name"]
            }),
        },
        ToolDefinition {
            name: "get_file_span".to_string(),
            description: "Read specific lines from a file. Use after search to see more context around a result.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "file": {
                        "type": "string",
                        "description": "File path relative to repository root"
                    },
                    "start_line": {
                        "type": "integer",
                        "description": "Starting line (1-indexed)",
                        "minimum": 1
                    },
                    "end_line": {
                        "type": "integer",
                        "description": "Ending line (1-indexed, inclusive)",
                        "minimum": 1
                    }
                },
                "required": ["file", "start_line", "end_line"]
            }),
        },
        ToolDefinition {
            name: "list_indexed_files".to_string(),
            description: "List all files in the search index with metadata. Use to understand codebase structure.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
    ]
}

/// Dispatch a tool call to the appropriate handler.
pub fn handle_tool_call(engine: &SearchEngine, name: &str, args: Value) -> ToolResult {
    match name {
        "semantic_search" => handle_semantic_search(engine, args),
        "find_symbol" => handle_find_symbol(engine, args),
        "get_file_span" => handle_get_file_span(engine, args),
        "list_indexed_files" => handle_list_indexed_files(engine, args),
        _ => ToolResult::error(format!("Unknown tool: {}", name)),
    }
}

fn handle_semantic_search(engine: &SearchEngine, args: Value) -> ToolResult {
    let query = match args.get("query").and_then(|v| v.as_str()) {
        Some(q) => q,
        None => return ToolResult::error("Missing required parameter: query".to_string()),
    };

    let k = args.get("k")
        .and_then(|v| v.as_u64())
        .unwrap_or(5) as usize;

    let k = k.min(20).max(1);

    match engine.semantic_search(query, k) {
        Ok(results) => {
            let formatted = engine.format_results(query, &results, 2000);
            ToolResult::text(formatted)
        }
        Err(e) => {
            error!("semantic_search error: {}", e);
            ToolResult::error(format!("Search failed: {}", e))
        }
    }
}

fn handle_find_symbol(engine: &SearchEngine, args: Value) -> ToolResult {
    let name = match args.get("name").and_then(|v| v.as_str()) {
        Some(n) => n,
        None => return ToolResult::error("Missing required parameter: name".to_string()),
    };

    match engine.find_symbol(name) {
        Ok(symbols) if symbols.is_empty() => {
            ToolResult::text(format!("No symbol found matching: {}", name))
        }
        Ok(symbols) => {
            let mut output = format!("## Symbol: {}\n\n", name);
            for sym in &symbols {
                output.push_str(&format!(
                    "**{}** `{}` in {}:{}-{}\n```\n{}\n```\n\n",
                    sym.kind, sym.name, sym.file,
                    sym.start_line, sym.end_line, sym.signature
                ));
            }
            ToolResult::text(output)
        }
        Err(e) => ToolResult::error(format!("Symbol search failed: {}", e)),
    }
}

fn handle_get_file_span(engine: &SearchEngine, args: Value) -> ToolResult {
    let file = match args.get("file").and_then(|v| v.as_str()) {
        Some(f) => f,
        None => return ToolResult::error("Missing required parameter: file".to_string()),
    };

    let start = match args.get("start_line").and_then(|v| v.as_u64()) {
        Some(s) => s as u32,
        None => return ToolResult::error("Missing required parameter: start_line".to_string()),
    };

    let end = match args.get("end_line").and_then(|v| v.as_u64()) {
        Some(e) => e as u32,
        None => return ToolResult::error("Missing required parameter: end_line".to_string()),
    };

    match engine.get_file_span(file, start, end) {
        Ok(content) => {
            let lang = file.rsplit('.').next().unwrap_or("");
            let output = format!(
                "## {}:{}-{}\n\n```{}\n{}\n```",
                file, start, end, lang, content
            );
            ToolResult::text(output)
        }
        Err(e) => ToolResult::error(format!("Failed to read file: {}", e)),
    }
}

fn handle_list_indexed_files(engine: &SearchEngine, _args: Value) -> ToolResult {
    match engine.list_files() {
        Ok(files) if files.is_empty() => {
            ToolResult::text("No files indexed. Run `lumina index` first.".to_string())
        }
        Ok(files) => {
            let mut output = format!("## Indexed Files ({} files)\n\n", files.len());
            output.push_str("| File | Language | Lines | Chunks |\n");
            output.push_str("|------|----------|-------|--------|\n");

            for f in &files {
                output.push_str(&format!(
                    "| {} | {} | {} | {} |\n",
                    f.path, f.language, f.line_count, f.chunk_count
                ));
            }

            ToolResult::text(output)
        }
        Err(e) => ToolResult::error(format!("Failed to list files: {}", e)),
    }
}
```

---

## Testing the MCP Server Without Claude Code

### Method 1: Manual stdio test (bash)

```bash
# Start the server and send messages manually
echo '{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"0.1"}}}
{"jsonrpc":"2.0","method":"notifications/initialized"}
{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | lumina mcp --repo /path/to/repo
```

Expected output: 2 lines of JSON (one for initialize response, one for tools/list response).

### Method 2: Automated test with subprocess

See `tests/test_mcp.rs` in the Testing doc (08-TESTING.md).

### Method 3: MCP Inspector

The MCP project provides an inspector tool:
```bash
npx @modelcontextprotocol/inspector lumina mcp --repo .
```

This gives you a web UI to interact with the server, see messages, and test tools.

### Method 4: Claude Code with debug logging

```json
{
  "mcpServers": {
    "lumina": {
      "command": "lumina",
      "args": ["mcp", "--repo", "."],
      "env": {
        "VOYAGE_API_KEY": "pa-...",
        "RUST_LOG": "lumina=debug"
      }
    }
  }
}
```

Debug logs go to stderr, which Claude Code captures and can show in its debug output.

---

## Common Pitfalls (Ranked by Frequency)

1. **Writing non-JSON to stdout**: Any `println!()`, `dbg!()`, or library output to
   stdout kills the connection. Grep your code for these before testing.

2. **Responding to notifications**: `notifications/initialized` has no `id`. If you
   send a response, the client may pair it with the wrong request.

3. **Wrong `id` type**: Client sends `"id": 0` (number), you respond with `"id": "0"`
   (string). The client doesn't match them. Use `serde_json::Value` and clone exactly.

4. **Not flushing stdout**: `BufWriter` holds the response. The client hangs waiting.
   Always `writer.flush()` after `writeln!()`.

5. **Pretty-printing JSON**: `serde_json::to_string_pretty()` adds newlines inside the
   JSON. Since messages are newline-delimited, this makes the message span multiple lines.
   Always use `serde_json::to_string()`.

6. **Missing `capabilities.tools` in initialize response**: If you don't include
   `"tools": {}` in capabilities, the client never calls `tools/list`.

7. **Missing `isError` field in tool results**: Even on success, `isError` must be
   present and set to `false`.

8. **Not handling absent `params`**: Some messages have no `params` field at all
   (not even `null`). Use `#[serde(default)]` in deserialization.
