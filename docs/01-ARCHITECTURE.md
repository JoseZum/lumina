# 01 - Architecture & Design Philosophy

## Core Principle: Minimal Async, Maximum Throughput

The single most important architectural decision in Lumina is the concurrency model.

**The indexer is CPU-bound.** Tree-sitter parsing, SHA-256 hashing, and chunk extraction
are pure computation. The right tool for this is `rayon` (data parallelism), not `tokio`
(async IO). Using tokio for CPU-bound work would be over-engineering: async/await adds
complexity (lifetimes, pinning, Send bounds) without any throughput benefit when the
bottleneck is CPU, not IO.

**The only async dependency is LanceDB.** The LanceDB Rust SDK requires a tokio runtime.
We accommodate this with a minimal `tokio::runtime::Runtime` that we call `block_on()`
on when needed. This keeps async contained to one module instead of infecting the entire
codebase.

**The MCP server is synchronous.** It reads stdin line by line, processes each request,
writes a response to stdout. There's exactly one client (Claude Code) and exactly one
connection (stdio). There's nothing to be concurrent about. A simple `loop { read; process;
write; }` is the correct architecture.

```
Concurrency Map:

rayon threads      ┌─ tree-sitter parse
(CPU-bound work)   ├─ SHA-256 hash
                   ├─ chunk extraction
                   └─ content deduplication

main thread        ┌─ file walking (ignore::Walk iterator)
(sequential)       ├─ embedding API calls (reqwest::blocking, batched)
                   ├─ LanceDB writes (block_on)
                   ├─ tantivy writes (synchronous API)
                   └─ MCP server loop (stdin/stdout)
```

## Why This Architecture Beats Alternatives

### vs. Full async (tokio everywhere)

Full async would mean:
- Every function signature gets `async fn` or returns a `Future`
- Traits need `async_trait` macro (or Rust 1.75+ RPITIT)
- `rayon::par_iter()` doesn't compose with async — you'd need to spawn tasks
- Testing becomes harder (need `#[tokio::test]` everywhere)
- Error types need `Send + Sync + 'static` bounds

For what benefit? The MCP server handles one request at a time. The indexer
bottleneck is CPU (parsing), not IO. The only IO is Voyage API calls, which
have rate limits anyway — you can't speed them up with concurrency.

### vs. Pure sync (no tokio at all)

This would be ideal, but LanceDB's Rust SDK is async-only. If LanceDB adds
a sync API or we switch to a sync vector store, we can drop tokio entirely.
The architecture is designed to make this easy: tokio is used in exactly one
file (`store/lance.rs`) behind a sync trait.

### vs. Actor model (crossbeam channels, message passing)

Overkill for a tool that processes one query at a time and indexes repos in
batch. Actors make sense for servers handling concurrent connections (like an
LSP). Lumina's MCP server is single-client stdio. No actors needed.

## Data Flow

### Indexing Flow (batch, runs on `lumina index`)

```
1. WALK
   ignore::WalkBuilder::new(repo_root)
       .follow_links(false)       // No symlink loops
       .hidden(false)             // Skip hidden files
       .git_ignore(true)          // Respect .gitignore
       .build()
   → Iterator<DirEntry>

2. FILTER
   For each entry:
   - Is extension in supported list? (.py, .rs, .ts, .js, .go, .java)
   - Is file < 1MB? (skip generated/vendored files)
   - Is file text, not binary? (check for null bytes in first 8KB)
   - Is file in skip list? (node_modules, target, __pycache__, etc.)
   → Iterator<PathBuf>

3. HASH CHECK (per-file)
   SHA-256(file content) vs stored hash in .lumina/hashes.bin
   - If match: skip entirely (file unchanged)
   - If mismatch or new: proceed to parse
   → Vec<(PathBuf, String)>  // (path, content) of changed files

4. PARSE + CHUNK (parallel via rayon)
   rayon::par_iter over changed files:
   - tree-sitter parse → AST
   - Walk AST for semantic nodes (functions, classes, methods, structs)
   - Extract each node as a Chunk with metadata
   - Merge small nodes with siblings (< 50 tokens)
   - Split huge nodes into sub-nodes (> 500 tokens)
   - Compute SHA-256 of each chunk's text content
   → Vec<Chunk> (without embeddings yet)

5. DEDUPLICATE CHUNKS
   Check each chunk.id (content hash) against existing store:
   - If chunk exists with same hash: skip embedding (reuse existing)
   - If chunk is new: needs embedding
   → Vec<Chunk> (only new chunks that need embedding)

6. EMBED (sequential, batched)
   Group chunks into batches of 128 (Voyage's max batch size)
   For each batch:
   - POST to Voyage API with texts
   - Receive Vec<Vec<f32>> embeddings (1024 dims each)
   - Attach embeddings to chunks
   → Vec<Chunk> (with embeddings)

7. STORE
   For each file that changed:
   - Delete old chunks for that file from LanceDB + tantivy
   - Insert new chunks into LanceDB (with embeddings)
   - Insert new chunks into tantivy (text only, for BM25)
   → Updated indices

8. PERSIST HASHES
   Save updated HashMap<PathBuf, String> to .lumina/hashes.bin
   → Ready for next incremental index
```

### Query Flow (real-time, runs on MCP tool call)

```
1. RECEIVE QUERY
   Claude Code sends: tools/call semantic_search {query: "auth middleware", k: 5}

2. EMBED QUERY
   POST to Voyage API: embed_query("auth middleware")
   → Vec<f32> (1024 dims)

3. DUAL RETRIEVAL (can be parallel but sequential is fine for latency)
   a. Vector search on LanceDB:
      table.search(query_embedding).limit(30).execute()
      → Vec<(chunk_id, vector_score)>

   b. Keyword search on tantivy:
      QueryParser::parse("auth middleware")
      searcher.search(&query, &TopDocs::with_limit(30))
      → Vec<(chunk_id, bm25_score)>

4. RRF FUSION
   rrf_merge(vector_results, keyword_results, k=60)
   → Vec<SearchResult> ordered by RRF score

5. RERANK (optional, if API key configured)
   Send top 20 RRF results + query to reranker API
   → Vec<SearchResult> reordered by cross-encoder score

6. TOKEN BUDGET
   Take top results, format as markdown with code blocks
   Keep adding results until ~2000 token budget is reached
   Metadata (file, symbol, lines) always included
   Code body truncated if necessary

7. RESPOND
   Return MCP tool result with formatted text content
   → Claude Code receives relevant code chunks
```

## Module Dependency Graph

```
error.rs ◄────────────── every module
types.rs ◄────────────── every module except error

config.rs ◄──── main.rs, indexer, mcp

chunker/
  languages.rs ◄── treesitter.rs
  treesitter.rs ◄── indexer/mod.rs

embeddings/
  voyage.rs ◄── indexer/mod.rs, search/mod.rs

store/
  lance.rs ◄── indexer/mod.rs, search/mod.rs
  tantivy_store.rs ◄── indexer/mod.rs, search/mod.rs

search/
  rrf.rs ◄── search/mod.rs
  reranker.rs ◄── search/mod.rs
  mod.rs ◄── mcp/tools.rs

indexer/
  hasher.rs ◄── indexer/mod.rs
  mod.rs ◄── main.rs

mcp/
  protocol.rs ◄── mcp/handler.rs, mcp/mod.rs
  tools.rs ◄── mcp/handler.rs
  handler.rs ◄── mcp/mod.rs
  mod.rs ◄── main.rs

main.rs ── entry point, wires everything together
```

Key property: **no circular dependencies**. The dependency arrow always points
"downward" — mcp depends on search, search depends on store, store depends on types.
Nothing points back up. This makes the codebase easy to understand and test in isolation.

## What Makes This Different From Existing Solutions

### vs. srag (github.com/wrxck/srag)
srag uses SQLite + HNSW for vectors. Lumina uses LanceDB (purpose-built for vectors,
better ANN search at scale) + tantivy (purpose-built for BM25, better than SQLite FTS5).
srag's MCP implementation is a good reference but lacks incremental indexing.

### vs. claude-context (Zilliz)
Requires Milvus server running separately. Not local-first in the same way.
Lumina is a single binary with zero external dependencies (besides API keys).

### vs. code-sage
Similar approach but lacks RRF fusion and reranking. Vector-only retrieval
misses exact name matches that keyword search catches instantly.

### vs. Cursor's internal system
Cursor has custom embeddings trained on agent traces + Turbopuffer cloud storage.
Lumina uses off-the-shelf Voyage embeddings + local storage. Lower quality ceiling
but zero cloud dependency. The architecture allows swapping in custom embeddings later
when/if we collect enough usage data.
