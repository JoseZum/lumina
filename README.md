# Lumina
 
Lumina is a highly parallelized, high-throughput source code indexer and semantic search engine exposing a Model Context Protocol (MCP) interface (designed for tools like Claude Code).

## Core Functionalities

Currently, the following subsystems are implemented:

- **Chunker (`src/chunker/`)**: Parses source code files using `tree-sitter` and correctly splits code into logical chunks instead of naive text-level chunking. Supported languages include Rust, JS/TS, Python, C++, etc.
- **Embeddings (`src/embeddings/`)**: Handles vectorized representations of code chunks. Currently features an integration with Voyage API.
- **Store (`src/store/`)**: Persists chunks, vectors, and full-text search indexes. Utilizes `LanceDB` for fast, local vector search and `Tantivy` for full-text search.
- **Indexer (`src/indexer/`)**: A CPU-bound pipeline parallelized via `rayon` to rapidly walk files, hash contents (for deduplication), syntactically chunk code, and construct embeddings.
- **Search (`src/search/`)**: Provides hybrid search capabilities over the codebase (semantic vector search + full-text indexing).
- **MCP Server (`src/mcp/`)**: A fast, synchronous standard-IO server that allows external AI assistants and tools to trigger reads, indexing, and searches programmatically.

## Development Status
This repository is currently under heavy development. See the `docs/` folder for architectural decisions, implementation order, and protocol details.

## Setup & Run
Standard Rust development procedures apply:
```bash
cargo build --release
cargo test
```
