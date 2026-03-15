use crate::config::LuminaConfig;
use crate::mcp::protocol::{ToolAnnotations, ToolDefinition, ToolResult};
use crate::search::SearchEngine;
use serde_json::{json, Value};

fn read_only() -> Option<ToolAnnotations> {
    Some(ToolAnnotations {
        read_only_hint: Some(true),
        destructive_hint: None,
    })
}

fn read_write() -> Option<ToolAnnotations> {
    Some(ToolAnnotations {
        read_only_hint: Some(false),
        destructive_hint: Some(false),
    })
}

/// Return all tool definitions for the MCP tools/list response.
pub fn tool_definitions() -> Vec<ToolDefinition> {
    vec![
        // ── Search Tools ──
        ToolDefinition {
            name: "semantic_search".to_string(),
            description: "Search the codebase using natural language. This is your primary search tool — use it to find code patterns, functions, and concepts. Combines vector similarity (understands meaning) with keyword matching (finds exact identifiers). Returns ranked code chunks with file paths and line numbers.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Natural language search query (e.g., 'authentication middleware', 'database connection pooling', 'error handling in API routes')"
                    },
                    "k": {
                        "type": "integer",
                        "description": "Number of results to return (default: 5, max: 20)",
                        "default": 5
                    }
                },
                "required": ["query"]
            }),
            annotations: read_only(),
        },
        ToolDefinition {
            name: "find_symbol".to_string(),
            description: "Find a symbol (function, class, struct, method) by name. Use when you know the exact or partial name of what you're looking for — faster and more precise than semantic_search for identifier lookups. Supports fuzzy matching.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Symbol name to search for (e.g., 'UserService', 'authenticate', 'handle_request')"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of results (default: 10)",
                        "default": 10
                    }
                },
                "required": ["name"]
            }),
            annotations: read_only(),
        },
        ToolDefinition {
            name: "search_in_directory".to_string(),
            description: "Semantic search scoped to a specific directory. Use when you want to search within a particular module, package, or folder (e.g., only search in src/auth/ or tests/).".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Natural language search query"
                    },
                    "directory": {
                        "type": "string",
                        "description": "Directory path relative to repo root (e.g., 'src/auth', 'tests', 'lib/models')"
                    },
                    "k": {
                        "type": "integer",
                        "description": "Number of results to return (default: 5, max: 20)",
                        "default": 5
                    }
                },
                "required": ["query", "directory"]
            }),
            annotations: read_only(),
        },

        // ── File Tools ──
        ToolDefinition {
            name: "get_file_span".to_string(),
            description: "Read a specific range of lines from a file. Use after semantic_search or find_symbol to get more context around a result. Returns syntax-highlighted code.".to_string(),
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
            annotations: read_only(),
        },
        ToolDefinition {
            name: "list_indexed_files".to_string(),
            description: "List all files in the index. Use to understand project structure and see what files are available for searching.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {},
            }),
            annotations: read_only(),
        },

        // ── Index Management ──
        ToolDefinition {
            name: "index_repository".to_string(),
            description: "Re-index the repository. Only changed files are re-processed (incremental). Use when the user has modified code and wants the search index updated. Returns indexing statistics.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "force": {
                        "type": "boolean",
                        "description": "Force full re-index, ignoring cache (default: false)",
                        "default": false
                    }
                }
            }),
            annotations: read_write(),
        },
        ToolDefinition {
            name: "get_index_status".to_string(),
            description: "Check the health and statistics of the search index. Shows: tracked files, vector/keyword chunk counts, embedding provider, model, and dimensions.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {},
            }),
            annotations: read_only(),
        },
    ]
}

/// Handle a tool call. Returns (result, needs_rebuild).
/// needs_rebuild is true only for index_repository.
pub fn handle_tool_call(
    engine: &SearchEngine,
    config: &LuminaConfig,
    tool_name: &str,
    arguments: &Value,
    token_budget: usize,
) -> (ToolResult, bool) {
    match tool_name {
        "semantic_search" => (handle_semantic_search(engine, arguments, token_budget), false),
        "find_symbol" => (handle_find_symbol(engine, arguments), false),
        "search_in_directory" => (handle_search_in_directory(engine, arguments, token_budget), false),
        "get_file_span" => (handle_get_file_span(engine, arguments), false),
        "list_indexed_files" => (handle_list_indexed_files(engine), false),
        "index_repository" => handle_index_repository(config, arguments),
        "get_index_status" => (handle_get_index_status(config), false),
        _ => (ToolResult::error(format!("Unknown tool: {}", tool_name)), false),
    }
}

// ── Search Tools ──

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

fn handle_search_in_directory(engine: &SearchEngine, args: &Value, budget: usize) -> ToolResult {
    let query = match args.get("query").and_then(|v| v.as_str()) {
        Some(q) => q,
        None => return ToolResult::error("Missing required parameter: query".into()),
    };

    let directory = match args.get("directory").and_then(|v| v.as_str()) {
        Some(d) => d.trim_end_matches('/'),
        None => return ToolResult::error("Missing required parameter: directory".into()),
    };

    let k = args
        .get("k")
        .and_then(|v| v.as_u64())
        .unwrap_or(5)
        .min(20) as usize;

    // Search with a larger k, then filter by directory prefix
    let search_k = (k * 5).min(50);

    match engine.semantic_search(query, search_k) {
        Ok(results) => {
            let filtered: Vec<_> = results
                .into_iter()
                .filter(|r| {
                    r.file.starts_with(directory)
                        || r.file.starts_with(&format!("{}/", directory))
                })
                .take(k)
                .collect();

            if filtered.is_empty() {
                return ToolResult::text(format!(
                    "No results found for '{}' in directory '{}'",
                    query, directory
                ));
            }

            let formatted = engine.format_results(query, &filtered, budget);
            ToolResult::text(formatted)
        }
        Err(e) => ToolResult::error(format!("Search failed: {}", e)),
    }
}

// ── File Tools ──

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

// ── Index Management ──

fn handle_index_repository(config: &LuminaConfig, args: &Value) -> (ToolResult, bool) {
    let force = args
        .get("force")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // Ensure data directory exists
    if let Err(e) = std::fs::create_dir_all(&config.data_dir) {
        return (ToolResult::error(format!("Failed to create data dir: {}", e)), false);
    }

    if force {
        // Delete index data for full re-index
        for path in [config.hashes_path(), config.provider_lock_path()] {
            if path.exists() {
                let _ = std::fs::remove_file(&path);
            }
        }
        for path in [config.lance_path(), config.tantivy_path()] {
            if path.exists() {
                let _ = std::fs::remove_dir_all(&path);
            }
        }
    }

    // Create indexer and run
    let mut indexer = match crate::create_indexer(config) {
        Ok(i) => i,
        Err(e) => return (ToolResult::error(format!("Failed to create indexer: {}", e)), false),
    };

    match indexer.index() {
        Ok(stats) => {
            // Save provider lock
            let lock_path = config.provider_lock_path();
            let _ = std::fs::write(&lock_path, format!("\"{}\"", config.embedding_provider));

            let output = format!(
                "## Indexing Complete\n\n\
                 - **Files scanned:** {}\n\
                 - **Files changed:** {}\n\
                 - **Files unchanged:** {}\n\
                 - **Chunks embedded:** {}\n\
                 - **Duration:** {:.1}s\n\n\
                 The search index has been updated. New searches will use the latest data.",
                stats.files_scanned,
                stats.files_changed,
                stats.files_unchanged,
                stats.chunks_embedded,
                stats.duration.as_secs_f64()
            );
            (ToolResult::text(output), true) // true = rebuild engine
        }
        Err(e) => (ToolResult::error(format!("Indexing failed: {}", e)), false),
    }
}

fn handle_get_index_status(config: &LuminaConfig) -> ToolResult {
    if !config.data_dir.exists() {
        return ToolResult::text(
            "## Index Status\n\n**No index found.** Run `index_repository` to create one.".into(),
        );
    }

    // Get vector count
    let vector_count = crate::store::lance::LanceStore::new(
        &config.lance_path(),
        config.embedding_dimensions,
    )
    .map(|s| {
        use crate::store::VectorStore;
        s.count().unwrap_or(0)
    })
    .unwrap_or(0);

    // Get keyword count
    let keyword_count = crate::store::tantivy_store::TantivyStore::new(&config.tantivy_path())
        .map(|s| {
            use crate::store::KeywordStore;
            s.count().unwrap_or(0)
        })
        .unwrap_or(0);

    // Get tracked file count
    let tracked_files = crate::indexer::hasher::FileHasher::new(config.hashes_path())
        .map(|h| h.tracked_count())
        .unwrap_or(0);

    // Check provider lock
    let lock_status = if config.provider_lock_path().exists() {
        match std::fs::read_to_string(config.provider_lock_path()) {
            Ok(s) => s.trim().trim_matches('"').to_string(),
            Err(_) => "unknown".to_string(),
        }
    } else {
        "not set".to_string()
    };

    let output = format!(
        "## Index Status\n\n\
         - **Tracked files:** {}\n\
         - **Vector chunks:** {}\n\
         - **Keyword chunks:** {}\n\
         - **Provider:** {} ({})\n\
         - **Model:** {}\n\
         - **Dimensions:** {}\n\
         - **Provider lock:** {}\n\
         - **Data directory:** {}",
        tracked_files,
        vector_count,
        keyword_count,
        config.embedding_provider.display_name(),
        config.embedding_provider,
        config.embedding_model,
        config.embedding_dimensions,
        lock_status,
        config.data_dir.display()
    );

    ToolResult::text(output)
}
