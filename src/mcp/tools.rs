use crate::mcp::protocol::{ToolDefinition, ToolResult};
use crate::search::SearchEngine;
use serde_json::{json, Value};

/// Return all tool definitions for the MCP tools/list response.
pub fn tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "semantic_search".to_string(),
            description: "Search the codebase using natural language. Returns relevant code chunks ranked by semantic similarity and keyword match.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Natural language search query (e.g., 'authentication middleware', 'database connection pooling')"
                    },
                    "k": {
                        "type": "integer",
                        "description": "Number of results to return (default: 5, max: 20)",
                        "default": 5
                    }
                },
                "required": ["query"]
            }),
        },
        ToolDefinition {
            name: "find_symbol".to_string(),
            description: "Find a symbol (function, class, struct, etc.) by name. Supports fuzzy matching.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Symbol name to search for (e.g., 'UserService', 'authenticate')"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of results (default: 10)",
                        "default": 10
                    }
                },
                "required": ["name"]
            }),
        },
        ToolDefinition {
            name: "get_file_span".to_string(),
            description: "Read a specific range of lines from a file. Use after semantic_search to get more context around a result.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "file": {
                        "type": "string",
                        "description": "File path relative to repository root (e.g., 'src/auth/middleware.rs')"
                    },
                    "start_line": {
                        "type": "integer",
                        "description": "First line to read (1-indexed)"
                    },
                    "end_line": {
                        "type": "integer",
                        "description": "Last line to read (1-indexed, inclusive)"
                    }
                },
                "required": ["file", "start_line", "end_line"]
            }),
        },
        ToolDefinition {
            name: "list_indexed_files".to_string(),
            description: "List all files that have been indexed. Useful to understand project structure.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {},
            }),
        },
    ]
}

/// Handle a tool call and return the result.
pub fn handle_tool_call(
    engine: &SearchEngine,
    tool_name: &str,
    arguments: &Value,
    token_budget: usize,
) -> ToolResult {
    match tool_name {
        "semantic_search" => handle_semantic_search(engine, arguments, token_budget),
        "find_symbol" => handle_find_symbol(engine, arguments),
        "get_file_span" => handle_get_file_span(engine, arguments),
        "list_indexed_files" => handle_list_indexed_files(engine),
        _ => ToolResult::error(format!("Unknown tool: {}", tool_name)),
    }
}

fn handle_semantic_search(engine: &SearchEngine, args: &Value, budget: usize) -> ToolResult {
    let query = match args.get("query").and_then(|v| v.as_str()) {
        Some(q) => q,
        None => return ToolResult::error("Missing required parameter: query".into()),
    };

    let k = args
        .get("k")
        .and_then(|v| v.as_u64())
        .unwrap_or(5)
        .min(20) as usize;

    match engine.semantic_search(query, k) {
        Ok(results) => {
            let formatted = engine.format_results(query, &results, budget);
            ToolResult::text(formatted)
        }
        Err(e) => ToolResult::error(format!("Search failed: {}", e)),
    }
}

fn handle_find_symbol(engine: &SearchEngine, args: &Value) -> ToolResult {
    let name = match args.get("name").and_then(|v| v.as_str()) {
        Some(n) => n,
        None => return ToolResult::error("Missing required parameter: name".into()),
    };

    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(10) as usize;

    match engine.find_symbol(name, limit) {
        Ok(results) => {
            if results.is_empty() {
                return ToolResult::text(format!("No symbols found matching '{}'", name));
            }

            let mut output = format!("## Symbols matching: \"{}\"\n\n", name);
            for (i, r) in results.iter().enumerate() {
                output.push_str(&format!(
                    "### {}. `{}` ({}) — {}:{}-{}\n```{}\n{}\n```\n\n",
                    i + 1,
                    r.symbol,
                    r.kind,
                    r.file,
                    r.start_line,
                    r.end_line,
                    r.language,
                    r.text
                ));
            }
            ToolResult::text(output)
        }
        Err(e) => ToolResult::error(format!("Symbol search failed: {}", e)),
    }
}

fn handle_get_file_span(engine: &SearchEngine, args: &Value) -> ToolResult {
    let file = match args.get("file").and_then(|v| v.as_str()) {
        Some(f) => f,
        None => return ToolResult::error("Missing required parameter: file".into()),
    };

    let start_line = match args.get("start_line").and_then(|v| v.as_u64()) {
        Some(l) => l as u32,
        None => return ToolResult::error("Missing required parameter: start_line".into()),
    };

    let end_line = match args.get("end_line").and_then(|v| v.as_u64()) {
        Some(l) => l as u32,
        None => return ToolResult::error("Missing required parameter: end_line".into()),
    };

    match engine.get_file_span(file, start_line, end_line) {
        Ok(content) => {
            let lang = std::path::Path::new(file)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");
            let output = format!(
                "## {}:{}-{}\n```{}\n{}\n```",
                file, start_line, end_line, lang, content
            );
            ToolResult::text(output)
        }
        Err(e) => ToolResult::error(format!("Failed to read file: {}", e)),
    }
}

fn handle_list_indexed_files(engine: &SearchEngine) -> ToolResult {
    match engine.list_files() {
        Ok(files) => {
            if files.is_empty() {
                return ToolResult::text("No files indexed yet. Run `lumina index` first.".into());
            }

            let mut output = format!("## Indexed Files ({} total)\n\n", files.len());
            for file in &files {
                output.push_str(&format!("- {}\n", file));
            }
            ToolResult::text(output)
        }
        Err(e) => ToolResult::error(format!("Failed to list files: {}", e)),
    }
}
