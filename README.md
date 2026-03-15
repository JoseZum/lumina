# Lumina

**Give Claude superpowers to understand your codebase**

Lumina is a semantic code search engine that lets Claude search your code by **meaning**, not just keywords. Instead of reading files one-by-one (wasting 80% of context), Claude can instantly find exactly what it needs.

```
You:    "Find where authentication is handled"
Claude: Found 5 results:

  src/auth/middleware.rs:23     verify_jwt_token()        0.94
  src/auth/provider.rs:89      authenticate_user()        0.91
  src/routes/login.rs:12       handle_login()             0.87
```

**Works with:**
- Claude Code (via slash commands)
- Claude Desktop (via MCP server)
- Any MCP-compatible AI tool

---

## Installation

```bash
npm install -g lumina-search
```

**That's it.** Native binaries are downloaded automatically for your platform.

| Platform | Status |
|----------|--------|
| Windows x64 | Supported |
| macOS ARM64 (Apple Silicon) | Supported |
| macOS x64 (Intel) | Supported |
| Linux x64 | Supported |
| Linux ARM64 | Supported |

**Requirements:** Node.js >= 18

### How binary resolution works

Lumina uses a two-layer binary distribution (similar to esbuild):

1. **Platform npm packages** (best path) — `npm install` pulls `@lumina-search/darwin-arm64` (or your platform) as an optional dependency. The binary is inside the package. No network calls during postinstall.
2. **GitHub Releases** (fallback) — If the platform package isn't available, the postinstall script downloads the binary directly from the GitHub release.

If both fail, you can always build from source with `cargo build --release`.

---

## Quick Start

```bash
cd your-project
lumina init
```

**What `lumina init` does:**
1. Asks you to pick an embedding provider (Local/Voyage/OpenAI)
2. Indexes your codebase (~3000 files/minute)
3. Installs 4 Claude Code slash commands
4. Sets up the MCP server (`.mcp.json`)

Then restart Claude Code and start searching:

```
/lumina-search <query>     — Search your code
/lumina-index             — Re-index (incremental)
/lumina-status            — Check index health
/lumina-help              — Show all commands
```

**Example session:**

```
You:    /lumina-search JWT token validation

Claude: I found 3 relevant results:

        1. src/auth/jwt.rs:45 — verify_token() function (score: 0.92)
        2. src/middleware/auth.rs:12 — JwtMiddleware (score: 0.89)
        3. tests/auth_test.rs:67 — token validation tests (score: 0.85)

You:    Show me the verify_token function

Claude: [Uses get_file_span to read src/auth/jwt.rs:45-78]
        Here's the implementation... [shows code]

You:    I just fixed a bug in that function, re-index

Claude: [Uses index_repository tool]
        Indexed 1 changed file, 2 chunks updated.
```

---

## What's New in v0.3.0

### Retrieval Quality Fixes

- **Large chunks are now split, not dropped.** Functions and classes exceeding the token limit are split into overlapping sub-chunks by line boundary. Previously, they were silently discarded — making large functions invisible to search.
- **Chunk IDs include file path.** IDs are now `SHA-256(file + start_line + text)` instead of just `SHA-256(text)`. This eliminates ID collisions between identical code in different files, which previously corrupted the tantivy index and produced wrong RRF merges.
- **Directory search uses native store filters.** `search_in_directory` now pushes file prefix filters into LanceDB (SQL WHERE) and tantivy, instead of doing a global search and post-filtering. This dramatically improves recall when searching in small directories within large repos.
- **Optional Jina reranker.** Set `RERANKER_API_KEY` to enable cross-encoder reranking via Jina AI after RRF fusion. Improves result ordering beyond what RRF alone achieves.

### One-Command Install

- **Native binaries for all 5 platforms.** Windows, macOS (ARM64 + Intel), Linux (x64 + ARM64). No WSL, no compilation, no Rust toolchain needed.
- **Platform npm packages.** Follows the esbuild model (`@lumina-search/win32-x64`, etc.) so `npm install` doesn't depend on network calls during postinstall.
- **Smoke tests in CI.** Every binary is tested with `--version` and `--help` before release.

### Breaking Change

Chunk ID format changed. **Existing indexes are invalid after upgrading.** Run:

```bash
lumina index --repo . --force
```

---

## What Can Lumina Do?

### Semantic Code Search
Ask Claude to search your codebase in plain English:

```
/lumina-search authentication middleware
/lumina-search how does the payment flow work
/lumina-search database connection pooling
/lumina-search where are errors logged
```

Lumina understands **meaning**, not just keywords:
- Search "auth" → finds `verify_token()`, `JwtMiddleware`, `login_handler()`
- Search "database setup" → finds `initDb()`, `createPool()`, `migrations/`
- Search "error handling" → finds `try/catch` blocks, error classes, logging

### Search Within Directories
Narrow your search to specific folders:

```
Claude: "Search for API endpoints in the auth module"
→ Uses search_in_directory tool with native LanceDB filter
→ Only searches src/auth/ — no results wasted on other directories
```

### Find Symbols by Name
Find functions, classes, or structs instantly:

```
Claude: "Show me the UserService class"
→ Uses find_symbol tool
→ Returns full definition with code
```

### Read File Context
Get more context around search results:

```
Claude: "Show me lines 45-60 of auth.rs"
→ Uses get_file_span tool
→ Returns syntax-highlighted code
```

### Re-Index Without Leaving Claude
Update the search index from within your conversation:

```
You:    "I just added a new auth function, re-index the code"
Claude: [Uses index_repository tool]
        Indexing complete. 1 file changed, 3 new chunks embedded.
```

### Check Index Health
See what's indexed and how fresh it is:

```
Claude: "What's the status of the index?"
→ Uses get_index_status tool
→ Shows: 2,453 files tracked, 18,234 chunks, using Local provider
```

---

## Use Cases

### Onboarding New Developers
```
You:    "How does authentication work in this codebase?"
Claude: [Searches for auth patterns]
        Authentication uses JWT tokens. Here's the flow:
        1. Login endpoint (src/auth/login.rs:23)
        2. JWT middleware (src/middleware/auth.rs:45)
        3. Token verification (src/auth/jwt.rs:12)
```

### Bug Hunting
```
You:    "Find all places where we connect to the database"
Claude: [Searches for database connections]
        Found 8 locations:
        - src/db/pool.rs:23 (connection pool setup)
        - src/config.rs:89 (DB config loading)
        - tests/integration.rs:12 (test DB setup)
        ...
```

### Refactoring
```
You:    "Find all usages of the old UserModel class"
Claude: [Uses find_symbol("UserModel")]
        Found 12 references:
        - src/models/user.rs:45 (definition)
        - src/services/auth.rs:23 (import)
        - src/api/users.rs:67 (usage)
        ...
```

### Understanding Legacy Code
```
You:    "What does the payment processing system look like?"
Claude: [Searches in src/payments/]
        The payment system has 3 main components:
        1. PaymentProcessor (src/payments/processor.rs)
        2. Stripe integration (src/payments/stripe.rs)
        3. Payment webhooks (src/api/webhooks.rs)
```

---

## MCP Server (for Claude Desktop)

Lumina works as an MCP server, giving Claude **7 powerful tools**.

### Automatic Setup

Running `lumina init` creates `.mcp.json`:

```json
{
  "mcpServers": {
    "lumina": {
      "command": "lumina",
      "args": ["mcp", "--repo", "."]
    }
  }
}
```

Claude Code picks this up automatically. **No extra config needed.**

### Manual Setup (Claude Desktop)

Add to your `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "lumina": {
      "command": "lumina",
      "args": ["mcp", "--repo", "/absolute/path/to/your/project"]
    }
  }
}
```

**Config locations:**
- **macOS:** `~/Library/Application Support/Claude/claude_desktop_config.json`
- **Windows:** `%APPDATA%\Claude\claude_desktop_config.json`
- **Linux:** `~/.config/Claude/claude_desktop_config.json`

### MCP Tools

When connected, Claude gets these 7 tools automatically:

| Tool | What it does | Read-only? |
|------|-------------|-----------|
| **semantic_search** | Search code by meaning (hybrid vector + keyword + optional reranking) | Yes |
| **find_symbol** | Find functions/classes by name (fuzzy matching) | Yes |
| **search_in_directory** | Search within a specific folder (native store filters) | Yes |
| **get_file_span** | Read specific lines from a file | Yes |
| **list_indexed_files** | List all indexed files | Yes |
| **index_repository** | Re-index the codebase (incremental) | No |
| **get_index_status** | Check index health and stats | Yes |

**Claude automatically picks the right tool based on your request.** You don't need to know which tool to use — just ask naturally.

---

## CLI Reference

### `lumina init`

One command to set everything up.

```bash
lumina init                          # Interactive setup
lumina init --provider local         # Skip provider selection
lumina init --skip-index             # Only install Claude integration
lumina init --skip-commands          # Don't install slash commands
lumina init --skip-mcp               # Don't create .mcp.json
```

### `lumina index`

Index or re-index your codebase.

```bash
lumina index --repo .                      # Incremental index (only changed files)
lumina index --repo . --force              # Full re-index (delete cache)
lumina index --repo . --provider voyage    # Change embedding provider
```

**Incremental indexing is fast** — uses SHA-256 hashing to detect changed files. A 10,000-file codebase with 10 changed files re-indexes in ~5 seconds.

### `lumina query`

Search from the command line.

```bash
lumina query "authentication middleware" --repo .
lumina query "database connection" --repo . -k 10
```

### `lumina status`

Check index health.

```bash
lumina status --repo .
```

**Output:**
```
Index Status
  Data directory: /project/.lumina
  Tracked files: 2,453
  Vector chunks: 18,234
  Keyword chunks: 18,234
  Provider: Local (jina-embeddings-v2-base-code, 768 dims)
  API key: not needed
```

### `lumina mcp`

Start the MCP server. **You don't call this directly** — Claude Code/Desktop calls it via `.mcp.json`.

```bash
lumina mcp --repo .
```

---

## Embedding Providers

Lumina needs to convert your code into **vectors** (numbers that represent meaning). You have 3 options:

### Local (Recommended for Most Users)

**Cost:** Free
**Quality:** Good (768-dimensional embeddings)
**Model:** `jina-embeddings-v2-base-code` via [fastembed](https://github.com/Anush008/fastembed-rs)
**Privacy:** Everything stays on your machine

**When to use:**
- You want zero API costs
- You work on private/sensitive code
- You work offline
- You're just trying Lumina

```bash
lumina init --provider local
```

The model (~120MB) downloads automatically on first use. After that, all searches are instant and offline.

### Voyage AI (Best Quality)

**Cost:** $0.12 per 1M tokens (~$0.50 to index 10k files)
**Quality:** Best (1024-dimensional code-optimized embeddings)
**Model:** `voyage-code-3`

**When to use:**
- Production applications where quality matters
- You need the absolute best search results

```bash
export VOYAGE_API_KEY="pa-your-key"
lumina init --provider voyage
```

Get an API key at [voyageai.com](https://www.voyageai.com/).

### OpenAI (Widely Available)

**Cost:** $0.02 per 1M tokens (~$0.08 to index 10k files)
**Quality:** Good (1536-dimensional general-purpose embeddings)
**Model:** `text-embedding-3-small`

**When to use:**
- You already have OpenAI credits
- You want good quality at low cost

```bash
export OPENAI_API_KEY="sk-your-key"
lumina init --provider openai
```

Get an API key at [platform.openai.com](https://platform.openai.com/).

### Switching Providers

```bash
lumina index --repo . --provider voyage --force
```

The `--force` flag rebuilds the entire index with the new provider. You can't mix embedding providers — they use different vector dimensions.

---

## Optional: Jina Reranker

By default, Lumina uses RRF fusion to merge vector and keyword results. For better result ordering, you can enable cross-encoder reranking via Jina AI:

```bash
export RERANKER_API_KEY="jina_your-key"
```

When set, search results go through an additional reranking step after RRF fusion. The reranker scores each result against the query using a cross-encoder model, producing more accurate relevance ordering.

**Model:** `jina-reranker-v2-base-multilingual` (configurable via `reranker_model` in config)

Get an API key at [jina.ai](https://jina.ai/).

This is optional. Without it, Lumina uses a pass-through reranker that preserves RRF order.

---

## How Search Works (Under the Hood)

Lumina uses **hybrid retrieval with optional reranking** — the same techniques used by modern search engines.

### 1. Indexing

```
Your code files
      |
Tree-sitter (AST parsing)
      |
Extract semantic chunks (functions, classes, methods)
      |
  Split oversized chunks (line-boundary splitting with overlap)
      |
Generate embeddings (vectors)
      |
Store in dual index:
  - LanceDB (vector search)
  - Tantivy (keyword search)
```

**What's a "chunk"?**
A chunk is a meaningful piece of code — typically a function, class, or method. Lumina uses tree-sitter to parse your code structurally, not line-by-line. Large functions that exceed the token limit are automatically split into overlapping sub-chunks so nothing is lost.

**Chunk IDs** are `SHA-256(file_path + start_line + text)`, ensuring that identical code in different files gets distinct entries in the index.

### 2. Searching

```
Your query: "authentication middleware"
      |
Embed query -> [0.234, 0.891, ..., 0.456]
      |
+-------------------------+-------------------------+
|  Vector Search          |  Keyword Search         |
|  (semantic similarity)  |  (exact term matching)  |
|                         |                         |
|  Finds: verify_token()  |  Finds: JwtMiddleware   |
|  (no "auth" keyword!)   |  (exact match)          |
+-------------------------+-------------------------+
      |                         |
      +--------RRF Fusion-------+
                  |
         Merged & ranked results
                  |
      Optional: Jina Reranker (cross-encoder scoring)
                  |
          Top-k results to Claude
```

**Why hybrid?**
- **Vector search alone** misses exact identifiers. Searching "UserService" might not find `user_service`.
- **Keyword search alone** misses meaning. Searching "authentication" won't find `verify_token()`.
- **Hybrid = best of both.** Finds both semantic matches and exact identifiers.

**RRF (Reciprocal Rank Fusion)** merges results from both engines without requiring score calibration.

**Directory-scoped search** pushes file prefix filters directly into LanceDB (`WHERE file LIKE 'src/auth/%'`) and tantivy, avoiding the recall loss of global-search-then-post-filter.

---

## Supported Languages

Lumina uses [tree-sitter](https://tree-sitter.github.io/) for parsing. Currently supports:

| Language | Extensions | What Lumina indexes |
|----------|-----------|---------------------|
| **Python** | `.py` | Functions, classes, methods |
| **Rust** | `.rs` | Functions, structs, impls, traits |
| **TypeScript** | `.ts`, `.tsx` | Functions, classes, methods, interfaces |
| **JavaScript** | `.js`, `.jsx` | Functions, classes, methods |
| **Go** | `.go` | Functions, structs, methods, interfaces |
| **Java** | `.java` | Methods, classes, interfaces |

**Want more languages?** Lumina's architecture supports any tree-sitter grammar. File an issue on GitHub.

---

## Configuration

Lumina stores config in `.lumina/config.toml`:

```toml
# Embedding provider: "local" | "voyage" | "openai"
embedding_provider = "local"
embedding_model = "jinaai/jina-embeddings-v2-base-code"
embedding_dimensions = 768
embedding_batch_size = 128

# Reranker (optional — requires RERANKER_API_KEY env var)
reranker_model = "jina-reranker-v2-base-multilingual"

# Chunking (how code is split)
max_chunk_tokens = 500    # Chunks larger than this are split with overlap
min_chunk_tokens = 50     # Chunks smaller than this are skipped
max_file_size = 1048576   # 1 MB (skip larger files)

# Search (how results are ranked)
search_k_vector = 30      # Candidates from vector search
search_k_keyword = 30     # Candidates from keyword search
rrf_k = 60                # RRF fusion parameter

# MCP (how much code Claude gets)
response_token_budget = 2000  # ~1500 words of code per search
```

**What you might want to change:**

- `response_token_budget` — Increase to 3000-4000 if you want more code per search
- `max_chunk_tokens` — Decrease to 300 for smaller, more focused chunks
- `search_k_vector` / `search_k_keyword` — Increase to 50 each for larger projects

**To edit:**
```bash
nano .lumina/config.toml
```

Then re-index:
```bash
lumina index --repo .
```

---

## Troubleshooting

### "No index found"

**Fix:** Run `lumina init` or `lumina index --repo .` first.

### "Provider mismatch"

You switched embedding providers (e.g., Local → Voyage) after indexing.

**Fix:** Force a full re-index:
```bash
lumina index --repo . --force
```

### Slash commands not showing up in Claude Code

**Checklist:**
1. `.claude/skills/` directory exists in your project root?
2. Restarted Claude Code?
3. Type `/lumina` — do you see autocomplete suggestions?

**Fix:** Re-run `lumina init --skip-index` to reinstall slash commands.

### MCP server not connecting

**Checklist:**
1. `.mcp.json` exists in your project root?
2. `lumina` command works in terminal? (`lumina --version`)
3. Restarted Claude Code/Desktop?

**Debug:**
```bash
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' | lumina mcp --repo .
```

Should output a JSON-RPC response.

### Windows: "lumina not found"

```bash
# Reinstall — downloads a native Windows binary
npm install -g lumina-search

# Verify
lumina --version
```

If the binary still isn't found, check that npm's global `bin` directory is in your PATH:
```bash
npm config get prefix
# Add the resulting path + /bin (or just the path on Windows) to your PATH
```

### macOS / Linux: "lumina not found"

```bash
# Check npm global bin directory
npm config get prefix

# The binary should be at <prefix>/bin/lumina
ls $(npm config get prefix)/bin/lumina

# If missing, reinstall
npm install -g lumina-search
```

### Binary download fails during npm install

If the pre-built binary can't be downloaded (e.g., behind a corporate proxy), you can build from source:

```bash
# Install Rust: https://rustup.rs
cargo build --release
# Binary will be at target/release/lumina
```

### Search results are bad quality

1. **Enable the Jina reranker** (biggest improvement):
   ```bash
   export RERANKER_API_KEY="jina_your-key"
   ```

2. **Use Voyage AI embeddings** (better quality than Local):
   ```bash
   export VOYAGE_API_KEY="your-key"
   lumina index --repo . --provider voyage --force
   ```

3. **Increase search candidates** (more thorough search):
   Edit `.lumina/config.toml`:
   ```toml
   search_k_vector = 50
   search_k_keyword = 50
   ```

4. **Check what's indexed**:
   ```bash
   lumina status --repo .
   ```

### Indexing is slow

- Lumina respects `.gitignore` — make sure `node_modules/`, `target/`, etc. are ignored
- Indexing is CPU-bound (tree-sitter parsing). Expect ~3000 files/minute on an 8-core CPU.
- Embedding is network-bound (API calls). Local mode is fastest (no API).

### Upgrading from v0.2.x

v0.3.0 changed the chunk ID format. Existing indexes must be rebuilt:

```bash
npm install -g lumina-search
lumina index --repo . --force
```

---

## FAQ

**Q: Does Lumina send my code to the cloud?**
A: **Only if you use Voyage AI, OpenAI, or the Jina reranker.** With Local mode and no reranker (the default), everything stays on your machine. Zero API calls.

**Q: How big is the index?**
A: ~1-5 MB per 1000 files. A 10,000-file codebase creates a ~30-50 MB index.

**Q: Does it work offline?**
A: Yes, with Local mode and no reranker. The model downloads once (~120MB), then all searches are offline.

**Q: Does it work on Windows?**
A: Yes. Native Windows binaries are distributed — no WSL, no compilation required. Just `npm install -g lumina-search` and go.

**Q: Can I use it with GitHub Copilot / Cursor / other tools?**
A: Lumina uses the standard [MCP protocol](https://modelcontextprotocol.io/). Any MCP-compatible tool can connect to it. Currently tested with Claude Code and Claude Desktop.

**Q: What happens when I modify code?**
A: Run `/lumina-index` or `lumina index --repo .`. Lumina detects changed files via SHA-256 hashing and only re-indexes those.

**Q: Can I index multiple projects?**
A: Yes. Each project has its own `.lumina/` directory with an independent index. Run `lumina init` in each project root.

**Q: How is this different from GitHub Copilot or Cursor?**
A: Copilot and Cursor use proprietary search. Lumina is open source, runs locally, and gives you full control over the embedding provider, reranker, and index. Plus, it works with Claude.

**Q: Does it slow down Claude?**
A: No. Searches return in ~20-50ms. The MCP protocol is asynchronous — Claude gets results almost instantly.

**Q: What about large functions?**
A: Lumina automatically splits oversized chunks into overlapping sub-chunks (3-line overlap at boundaries). Nothing is silently dropped.

**Q: Can I contribute?**
A: Yes! Lumina is open source (Apache 2.0). File issues or PRs at [github.com/JoseZum/lumina](https://github.com/JoseZum/lumina).

---

## Performance

| Metric | Value |
|--------|-------|
| Indexing speed | ~3,000 files/minute (8-core CPU) |
| Search latency | ~20-50ms per query |
| Memory usage | ~80-100 MB during indexing |
| Index size | ~1-5 MB per 1,000 files |

**Tested on:**
- 10K-file React codebase: 3.5 minutes to index, 25ms search latency
- 5K-file Rust codebase: 1.8 minutes to index, 18ms search latency

---

## Architecture

```
lumina-search (npm)
  |
  +-- bin/lumina.js            JS launcher (resolves native binary)
  |
  +-- @lumina-search/win32-x64     Platform binary (Windows)
  +-- @lumina-search/darwin-arm64  Platform binary (macOS ARM64)
  +-- @lumina-search/darwin-x64    Platform binary (macOS Intel)
  +-- @lumina-search/linux-x64     Platform binary (Linux x64)
  +-- @lumina-search/linux-arm64   Platform binary (Linux ARM64)

lumina (Rust binary)
  |
  +-- chunker/        Tree-sitter AST parsing + chunk splitting
  +-- embeddings/     Voyage AI, OpenAI, or local fastembed
  +-- store/
  |     +-- lance.rs          LanceDB vector store (with native WHERE filters)
  |     +-- tantivy_store.rs  Tantivy keyword store (BM25)
  +-- search/
  |     +-- rrf.rs            Reciprocal Rank Fusion
  |     +-- reranker.rs       NoopReranker + JinaReranker
  +-- mcp/            MCP server (JSON-RPC over stdin/stdout)
  +-- indexer/        Incremental indexing with hash-based change detection
```

---

## License

Apache 2.0

---

## Credits

Built with:
- [tree-sitter](https://tree-sitter.github.io/) — AST parsing
- [LanceDB](https://lancedb.com/) — Vector storage
- [Tantivy](https://github.com/quickwit-oss/tantivy) — Keyword search (Rust's Lucene)
- [fastembed](https://github.com/Anush008/fastembed-rs) — Local embeddings (ONNX)
- [Voyage AI](https://www.voyageai.com/) — Best-in-class code embeddings
- [Jina AI](https://jina.ai/) — Cross-encoder reranking
- [Model Context Protocol](https://modelcontextprotocol.io/) — Claude integration
