# LUMINA - Implementation Master Plan

## What This Is

Lumina is an MCP server that gives Claude Code semantic code search capabilities.
Instead of Claude reading entire files to find what it needs, Lumina indexes
the codebase with AST-aware chunking + vector embeddings and serves only the
relevant code chunks through the MCP protocol.

## The Problem It Solves

Claude Code's native file reading is brute-force: it reads whole files, sometimes
multiple files, burning through context window tokens to find what it needs. For a
50K-line codebase, Claude might consume 20K+ tokens just navigating to the right
function. Lumina reduces that to ~800 tokens by doing the search before Claude sees
anything.

This is the exact same problem Cursor solved with their custom embedding pipeline +
Turbopuffer, and Sourcegraph Cody solved with Zoekt + SCIP. The difference: those
are closed-source, cloud-dependent systems tied to specific editors. Lumina is
local-first, open-source, and works with any MCP-compatible client.

## Architecture Overview

```
User's Codebase
      |
      v
[File Walker] --ignore .gitignore--> [Tree-Sitter Parser]
      |                                       |
      v                                       v
[SHA-256 Hasher] --skip unchanged-->  [AST Chunk Extractor]
                                              |
                                              v
                                    [Voyage code-3 API] --> embeddings
                                              |
                         +--------------------+--------------------+
                         |                                         |
                         v                                         v
                   [LanceDB Store]                         [Tantivy BM25 Index]
                   (vector search)                         (keyword search)
                         |                                         |
                         +--------------------+--------------------+
                                              |
                                              v
                                    [RRF Fusion + Reranker]
                                              |
                                              v
                                    [Token Budget Enforcer]
                                              |
                                              v
                                    [MCP Server (stdio)]
                                              |
                                              v
                                        Claude Code
```

## Stack Decisions

| Layer       | Technology         | Why This, Not That                                          |
|-------------|--------------------|-------------------------------------------------------------|
| Language    | Rust               | Single binary, no runtime deps, tree-sitter native support  |
| Parsing     | tree-sitter        | 20+ languages, incremental, used by GitHub/Neovim/Zed       |
| Embeddings  | Voyage code-3      | SOTA on code retrieval benchmarks, 1024 dims                |
| Vector DB   | LanceDB            | Local file, no server process, Arrow-native                 |
| Keyword     | tantivy            | Rust-native BM25, used by Quickwit/Meilisearch              |
| Reranker    | mxbai-rerank-large | Apache 2.0, 8K context, code+text                          |
| Transport   | stdio JSON-RPC     | MCP standard, zero network config                           |
| CLI         | clap               | Standard Rust CLI framework, derive macros                  |

## Documentation Index

Detailed implementation docs in `docs/`:

1. [Architecture & Design Philosophy](docs/01-ARCHITECTURE.md)
2. [Complete Project Structure](docs/02-PROJECT_STRUCTURE.md)
3. [Cargo.toml & Dependencies](docs/03-CARGO_TOML.md)
4. [Module Design (types, traits, functions)](docs/04-MODULE_DESIGN.md)
5. [MCP Protocol Deep Dive](docs/05-MCP_PROTOCOL.md)
6. [Implementation Order & Milestones](docs/06-IMPLEMENTATION_ORDER.md)
7. [Non-Obvious Design Decisions](docs/07-DESIGN_DECISIONS.md)
8. [Testing Strategy](docs/08-TESTING.md)

## P0 Scope (MVP)

What ships first:
- Tree-sitter chunker for Python + Rust
- Voyage code-3 embeddings via API
- LanceDB vector store (with brute-force fallback)
- Tantivy BM25 keyword index
- RRF fusion (no reranker yet)
- MCP server with 4 tools
- CLI: `lumina index` + `lumina query` + `lumina mcp`
- SHA-256 incremental indexing

What waits for P1:
- Cross-encoder reranking
- TypeScript/JavaScript/Go/Java grammars
- Watch mode / hot reload
- Symbol dependency graph
- Agent trace logging
