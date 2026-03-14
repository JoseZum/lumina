# Lumina

**Local-first semantic code search for Claude Code via MCP**

Lumina is a Rust MCP server that provides Claude Code with semantic code search capabilities using hybrid retrieval (vector embeddings + BM25 keyword search). Instead of reading files line-by-line, Claude can now search your codebase semantically — like Cursor, but local, fast, and under your control.

---

## What Problem Does This Solve?

Claude Code spends **80% of context window tokens on exploration** — reading files to understand your codebase. For large projects (10K+ files), this burns through tokens fast and slows down responses.

**Lumina fixes this by:**
- **Semantic search** — Claude asks "authentication middleware", Lumina returns relevant code chunks
- **Local & fast** — No cloud dependencies, indexes 100K files in <30s
- **Incremental** — Only re-indexes changed files (SHA-256 caching)
- **Symbol-aware** — Understands functions, classes, structs across 6 languages

---

## How It Works

```
┌─────────────────┐
│  Claude Code    │
│  (MCP client)   │
└────────┬────────┘
         │ JSON-RPC over stdio
         ▼
┌─────────────────┐      ┌──────────────────┐
│  Lumina MCP     │◄────►│  .lumina/        │
│  Server         │      │  ├─ index.lance/ │ (vector embeddings)
└────────┬────────┘      │  ├─ tantivy/     │ (BM25 keyword index)
         │               │  └─ hashes.bin   │ (SHA-256 cache)
         │               └──────────────────┘
         ▼
   Search Pipeline:
   1. Tree-sitter → Chunk code (AST-aware)
   2. Voyage API  → Embed chunks (1024-dim vectors)
   3. LanceDB     → Vector search (ANN)
   4. Tantivy     → Keyword search (BM25)
   5. RRF         → Fuse results (Reciprocal Rank Fusion)
   6. Return      → Top-k results to Claude
```

---

## Architecture Overview

### Core Modules

| Module | Files | Purpose |
|--------|-------|---------|
| **Types** | `types.rs`, `error.rs`, `config.rs` | Foundation types: `Chunk`, `SearchResult`, `LuminaError`, `LuminaConfig` |
| **Chunker** | `chunker/{mod,treesitter,languages}.rs` | Tree-sitter AST parsing → semantic chunks (functions, classes, methods) |
| **Embeddings** | `embeddings/{mod,voyage}.rs` | Voyage code-3 API client (1024-dim vectors), MockEmbedder for testing |
| **Storage** | `store/{mod,lance,tantivy_store}.rs` | LanceDB (vector store) + Tantivy (BM25 keyword store) |
| **Search** | `search/{mod,rrf,reranker}.rs` | Hybrid search pipeline: RRF fusion, reranking, result formatting |
| **Indexer** | `indexer/{mod,hasher}.rs` | Incremental indexing pipeline with SHA-256 caching |
| **MCP Server** | `mcp/{mod,protocol,handler,tools}.rs` | JSON-RPC 2.0 stdio server with 4 tools |
| **CLI** | `main.rs`, `lib.rs` | CLI entry point + factory functions |

### Data Flow: Indexing

```
1. Walk repo (ignore .gitignore, skip binaries, large files)
   ↓
2. Filter changed files (SHA-256 hash check)
   ↓
3. Parse with tree-sitter (parallel via rayon)
   ↓
4. Extract chunks (functions, classes, methods)
   ↓
5. Embed chunks (Voyage API, batched)
   ↓
6. Store in LanceDB (vectors) + Tantivy (keywords)
   ↓
7. Save hashes.bin for next incremental run
```

### Data Flow: Searching

```
1. Claude sends: semantic_search("authentication")
   ↓
2. Embed query → [0.234, 0.891, ..., 0.456] (1024-dim)
   ↓
3. Vector search (LanceDB) → top 30 candidates
   ↓
4. Keyword search (Tantivy BM25) → top 30 candidates
   ↓
5. RRF fusion → merge & rank by RRF score
   ↓
6. Format results (markdown, 2000 token budget)
   ↓
7. Return to Claude
```

---

## Installation

### npm Install (Recommended)

```bash
npm install -g lumina-search
```

This automatically:
- Detects your platform (Linux, macOS, Windows)
- Installs Rust if needed
- Builds the binary (~5 minutes first time)
- On Windows, builds inside WSL Ubuntu automatically

**Prerequisites:**
- Node.js >= 18
- [Voyage API Key](https://www.voyageai.com/)
- Windows only: WSL Ubuntu (`wsl --install -d Ubuntu`)

After install, the `lumina` command is available globally.

### From Source

```bash
git clone https://github.com/YOUR_USER/lumina.git
cd lumina
npm install
```

### Manual Install (no npm)

<details>
<summary>Click to expand</summary>

#### Windows (PowerShell)

```powershell
.\install.ps1
```

#### Linux/macOS or WSL

```bash
bash install.sh
exec bash
lumina --version
```

</details>

---

## Usage

### 1. Index a Repository

```bash
# In WSL (after installation)
export VOYAGE_API_KEY="pa-your-key-here"
lumina index --repo /mnt/c/path/to/your/project

# Or from Windows PowerShell
wsl -e bash -c 'export VOYAGE_API_KEY=pa-xxx && lumina index --repo /mnt/c/path/to/project'
```

**What happens:**
- Walks the repo (respects `.gitignore`)
- Parses supported files: `.py`, `.rs`, `.ts`, `.tsx`, `.js`, `.jsx`, `.go`, `.java`
- Chunks code using tree-sitter (AST-aware)
- Embeds chunks via Voyage API (batches of 128)
- Stores in `.lumina/` directory

**Incremental indexing:** On subsequent runs, only changed files are re-indexed (SHA-256 hash check).

**Force full re-index:**
```bash
lumina index --repo /path/to/repo --force
```

### 2. Query from CLI

```bash
# In WSL
export VOYAGE_API_KEY="pa-your-key"
lumina query "authentication middleware" -k 5 --repo /mnt/c/path/to/project

# From Windows
wsl -e bash -c 'export VOYAGE_API_KEY=pa-xxx && lumina query "authentication" --repo /mnt/c/path/to/project'
```

Returns markdown-formatted results with code snippets.

### 3. Use with Claude Code

#### a. Create `.claude.json` in your project root:

```json
{
  "mcpServers": {
    "lumina": {
      "command": "wsl",
      "args": ["-e", "lumina", "mcp", "--repo", "/mnt/c/path/to/your/project"],
      "env": {
        "VOYAGE_API_KEY": "pa-your-key-here"
      }
    }
  }
}
```

**Important:**
- Replace `/mnt/c/path/to/your/project` with the WSL path to your repo
- Replace `pa-your-key-here` with your actual Voyage API key
- First run `lumina index --repo /mnt/c/path/to/your/project` to create the index

#### b. Restart Claude Code

Claude Code will now show "lumina" in its available MCP tools.

#### c. Ask Claude to search

```
You: "Find the authentication middleware"
```

Claude will automatically use `semantic_search` instead of reading files!

### 4. Check Index Status

```bash
lumina status --repo /path/to/repo
```

Shows:
- Number of tracked files
- Number of indexed chunks
- Vector/keyword store sizes
- API key status

---

## MCP Tools

Lumina provides 4 tools to Claude Code:

### `semantic_search`

Natural language code search (hybrid vector + BM25).

**Input:**
- `query` (string): Natural language query (e.g., "JWT token validation")
- `k` (int, optional): Number of results (default: 5, max: 20)

**Output:** Markdown with code snippets, file paths, line numbers.

**Example:**
```json
{
  "name": "semantic_search",
  "arguments": {
    "query": "database connection pooling",
    "k": 5
  }
}
```

### `find_symbol`

Find symbols (functions, classes, structs) by name. Supports fuzzy matching.

**Input:**
- `name` (string): Symbol name (e.g., "UserService", "authenticate")
- `limit` (int, optional): Max results (default: 10)

**Output:** List of matching symbols with full code.

### `get_file_span`

Read a specific range of lines from a file.

**Input:**
- `file` (string): File path relative to repo root
- `start_line` (int): First line (1-indexed)
- `end_line` (int): Last line (1-indexed, inclusive)

**Output:** Code snippet with syntax highlighting.

### `list_indexed_files`

List all files in the index.

**Output:** List of indexed file paths.

---

## Configuration

Lumina uses `.lumina/config.toml` (optional). Defaults are shown below:

```toml
voyage_model = "voyage-code-3"
embedding_batch_size = 128

max_chunk_tokens = 500
min_chunk_tokens = 50
max_file_size = 1048576  # 1 MB

search_k_vector = 30
search_k_keyword = 30
rrf_k = 60
response_token_budget = 2000
```

**Environment variables override config:**
- `VOYAGE_API_KEY` — Required for embeddings

---

## Design Decisions

### Why Synchronous (Not Async)?

- **MCP server is synchronous** — stdio loop, one request at a time
- **Only LanceDB needs async** — wrapped with `tokio::runtime::block_on()`
- **Embeddings use `reqwest::blocking`** — simpler, no async spread
- Result: ~200 lines less code, easier to reason about

### Why Two Stores (LanceDB + Tantivy)?

**Vector search alone is insufficient** for code:
- Misses exact identifier matches ("UserService" query won't match "user_service")
- No BM25 term frequency signals

**Keyword search alone is insufficient:**
- Can't understand "authentication logic" → `verify_token()`
- No semantic similarity

**Solution:** Hybrid retrieval with RRF fusion (best of both worlds).

### Why Tree-Sitter (Not Regex)?

- **AST-aware chunking** — respects code structure (functions, classes)
- **Language-agnostic** — same pipeline for Python, Rust, TypeScript, etc.
- **Accurate boundaries** — no splitting mid-function

### Why SHA-256 Caching?

Two-level caching:
1. **File-level:** Skip parsing unchanged files
2. **Chunk-level:** Chunk ID = SHA-256 of text (content-addressable, dedup)

Result: Incremental indexing is **~50x faster** than full re-index.

---

## Performance

**Indexing speed:** ~3000 files/minute on 8-core CPU (tree-sitter parsing is CPU-bound)

**Search latency:**
- Vector search: ~10ms (LanceDB ANN)
- Keyword search: ~5ms (Tantivy BM25)
- RRF fusion: ~1ms
- **Total:** ~20ms (excluding embedding API call)

**Memory usage:** ~80-100MB peak during indexing (streaming pipeline)

---

## Supported Languages

| Language | Extensions | Tree-sitter Grammar |
|----------|-----------|---------------------|
| Python | `.py` | `tree-sitter-python` |
| Rust | `.rs` | `tree-sitter-rust` |
| TypeScript | `.ts`, `.tsx` | `tree-sitter-typescript` |
| JavaScript | `.js`, `.jsx` | `tree-sitter-javascript` |
| Go | `.go` | `tree-sitter-go` |
| Java | `.java` | `tree-sitter-java` |

**Adding more languages:** Add grammar to `Cargo.toml` + query to `chunker/languages.rs`.

---

## Troubleshooting

### `rustc ICE` errors during build

**Cause:** Rustc 1.94.0 has a bug in the dead code linter when emitting diagnostics for files on Windows filesystem.

**Workaround:** Project uses `#![allow(warnings)]` in `lib.rs` and `#![allow(dead_code)]` in `store/tantivy_store.rs`.

### `No VOYAGE_API_KEY set`

Lumina will fall back to `MockEmbedder` (random embeddings). Set `VOYAGE_API_KEY` environment variable.

### `Index not found`

Run `lumina index --repo .` first to create the index.

### MCP server not appearing in Claude Code

1. Check `.claude.json` syntax (valid JSON)
2. Restart Claude Code
3. Check Claude Code logs: `~/.claude/logs/`

---

## Roadmap (Future Work)

**Phase 2: Symbol Dependency Graph**
- Tree-sitter → call graph
- Answer "what breaks if I change this function?"

**Phase 3: Cross-repo shared index**
- Multiple repos → single index
- Answer "how do other projects use this API?"

**Phase 4: Agent trace dataset**
- Log Claude's searches
- Fine-tune embeddings on agent queries

---

## License

Apache 2.0

---

## Credits

Built with:
- [tree-sitter](https://tree-sitter.github.io/) — AST parsing
- [LanceDB](https://lancedb.com/) — Vector storage
- [Tantivy](https://github.com/quickwit-oss/tantivy) — Full-text search
- [Voyage AI](https://www.voyageai.com/) — Code embeddings
- [MCP](https://modelcontextprotocol.io/) — Model Context Protocol
