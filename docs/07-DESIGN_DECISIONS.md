# 07 - Non-Obvious Design Decisions

Every decision here was made deliberately. This document explains the WHY
behind choices that might seem arbitrary or wrong at first glance.

---

## 1. Repos with >100K Files Without OOM

### The Problem

A monorepo with 100K files at 5 chunks per file = 500K chunks.
Each chunk has ~500 tokens of text (~2KB) + 1024 floats of embedding (4KB).
Loading everything in memory: 500K × 6KB = ~3GB. Not acceptable.

### The Solution: Streaming Pipeline with Bounded Buffers

The indexer never holds all data in memory at once. It processes in stages:

```
Stage 1: Walk (iterator, O(1) memory)
    ↓
Stage 2: Filter + Hash check (1 file at a time)
    ↓
Stage 3: Parse (rayon batch of N files, ~50MB)
    ↓
Stage 4: Embed (batch of 128 chunks, ~1MB)
    ↓
Stage 5: Store (flush to disk, release memory)
    ↓
Stage 6: Repeat from Stage 2
```

The key is the **batch window**: we process files in groups of 1000,
parse them all (parallel via rayon), then embed the new chunks in batches
of 128, and flush to stores before processing the next group.

**Memory budget at each stage:**
- Walk iterator: ~0 (lazy, `ignore::Walk` yields one entry at a time)
- File reading: 1 file at a time, max 1MB (enforced by max_file_size)
- Parse buffer: ~1000 files × 3-5 chunks × 2KB text = ~15MB
- Embedding buffer: 128 chunks × 2KB text = 256KB
- Hash cache: 100K entries × ~100 bytes = ~10MB (kept in RAM for whole run)
- Tantivy writer heap: 50MB (configurable via `tantivy::IndexWriter::new()`)
- LanceDB: depends on internal implementation, but writes are batched

**Total peak memory: ~80-100MB** for a 100K file repo. Acceptable.

### What We Don't Do

- Don't load all file contents into a Vec<String> before processing.
- Don't collect all chunks before starting embedding.
- Don't keep embeddings in memory after storing them.
- Don't build an in-memory vector index (LanceDB handles this on disk).

---

## 2. Parallelism Strategy: rayon for CPU, Sequential for IO

### Why Not Tokio for Everything?

The indexing workload breaks down as:

| Operation | Type | Time per unit | Concurrency benefit |
|-----------|------|---------------|---------------------|
| tree-sitter parse | CPU-bound | ~1-5ms per file | **High** (linear with cores) |
| SHA-256 hash | CPU-bound | ~0.1ms per file | Moderate |
| File read (SSD) | IO-bound | ~0.05ms per file | Negligible (SSD is fast) |
| Voyage API call | Network IO | ~200ms per batch | Limited by rate limits |
| LanceDB write | Disk IO | ~10ms per batch | Limited by lock contention |
| Tantivy write | Disk IO | ~5ms per batch | Single-threaded by design |

Tree-sitter parsing dominates. For 10K files at 3ms each: 30 seconds single-threaded,
~4 seconds with 8 cores via rayon. This is the only operation worth parallelizing.

Voyage API calls are network IO, but rate-limited. Sending 3 concurrent batch
requests doesn't help if the API throttles you after 2. Sequential with batching
(128 texts per call) is simpler and more predictable.

Tantivy's `IndexWriter` is internally single-threaded for writes. Multiple threads
writing would just serialize on a mutex.

### rayon Usage Pattern

```rust
let chunks: Vec<Chunk> = changed_files
    .par_iter()  // rayon parallel iterator
    .flat_map(|(path, content)| {
        match self.chunker.chunk_file(path, content) {
            Ok(chunks) => chunks,
            Err(e) => {
                warn!("Failed to parse {}: {}", path, e);
                vec![]  // Skip files that fail to parse
            }
        }
    })
    .collect();
```

This is embarrassingly parallel. Each file is independent. No shared state.
No mutexes. rayon handles work-stealing across cores automatically.

### When Would We Need Tokio?

If we wanted to:
- Serve multiple MCP clients concurrently (SSE transport, not stdio)
- Run a file watcher that re-indexes in background while serving queries
- Make concurrent embedding API calls across multiple providers

None of these are P0 requirements.

---

## 3. LanceDB Schema for Efficient RRF

### Why Two Separate Stores?

LanceDB stores vectors + metadata. Tantivy stores text for BM25.
Why not just use LanceDB for everything?

**Option A: LanceDB only (full-text search on LanceDB)**
- LanceDB has a `full_text_search` feature (experimental in Rust SDK).
- Pro: One store, simpler architecture.
- Con: BM25 implementation may be less mature than tantivy's.
- Con: Can't use tantivy's advanced tokenizers and query language.
- Con: If LanceDB's FTS is broken, we lose keyword search entirely.

**Option B: Tantivy only (brute-force vector search)**
- Store embeddings as binary in tantivy, do cosine similarity manually.
- Pro: One store.
- Con: No ANN indexing. Linear scan for every query.
- Con: tantivy isn't designed for vector search.

**Option C: LanceDB + Tantivy (chosen)**
- Each store does what it's best at.
- Pro: Best quality — LanceDB's IVF-PQ for vectors, tantivy's BM25 for keywords.
- Pro: If LanceDB breaks, keyword search still works.
- Con: Two stores to maintain. Slightly more complex upsert/delete.

We chose Option C because hybrid search (vector + keyword) is the entire point
of the system. Using best-in-class tools for each retrieval method gives us
the highest quality results, which is the competitive differentiator.

### Data Duplication Between Stores

Both stores contain chunk metadata (id, file, symbol, lines). The text content
is stored in both (LanceDB for retrieval, tantivy for tokenized search). This
duplicates ~2KB per chunk. For 100K chunks: ~200MB of duplication. Acceptable
for the query quality benefit.

The embedding vectors (4KB per chunk) are ONLY in LanceDB. Tantivy doesn't
need them.

### RRF Doesn't Need Special Schema

RRF works on **ranks**, not scores. The algorithm:

1. Get top-30 from LanceDB vector search → assign ranks 1-30
2. Get top-30 from tantivy keyword search → assign ranks 1-30
3. For each document, compute: `score = Σ 1/(60 + rank_i)`
4. Sort by RRF score descending

This is a pure merge operation on two sorted lists. No special indexing needed.
No join tables. No pre-computed fusion scores. The RRF function is ~30 lines of
code and runs in microseconds.

The k=60 constant controls how much top results dominate. Higher k → more
democratic (lower results contribute more). k=60 is the standard value from
the original RRF paper (Cormack et al., 2009).

---

## 4. File Filtering: What Gets Indexed

### Binary Detection

We read the first 8192 bytes of each file and check for null bytes (`0x00`).
If any null byte is found, the file is likely binary and skipped.

Why 8192 bytes? UTF-8 text files should never contain null bytes. If there's
a null byte in the first 8KB, it's either binary or a file with unusual encoding.
8KB is enough to detect binary while being fast to read.

**Edge cases:**
- UTF-16 files: Contain null bytes. Will be skipped. This is acceptable because
  tree-sitter expects UTF-8 anyway.
- Files with embedded binary (e.g., Jupyter notebooks): Contain null bytes in
  output sections. These should be indexed via special handling in the future,
  but for P0, skipping is fine.

### Symlinks

`ignore::WalkBuilder::follow_links(false)` — we do NOT follow symlinks.

Reasons:
- Symlink loops cause infinite recursion.
- Symlinks often point to `node_modules`, `venv`, or other vendored directories.
- If a symlink points to a file that's also directly in the repo, we'd index it
  twice.

If a user has important code behind symlinks, they can set `follow_links: true`
in `.lumina/config.toml` (future feature).

### Large Files

Files > 1MB are skipped. Why?

- Generated files (protobuf, swagger, compiled assets) are often >1MB.
- Minified files (`bundle.min.js`) are >1MB.
- A 1MB file has ~25K lines. tree-sitter can parse it, but it generates many
  chunks that are unlikely to be uniquely useful.
- The Voyage API has a per-text limit of 16K tokens. A 1MB file would need
  to be split into many chunks anyway.

This threshold is configurable via `max_file_size` in config.

### Hardcoded Skip Patterns

Even if .gitignore is missing, we skip known vendored/generated directories:

```rust
const SKIP_DIRS: &[&str] = &[
    "node_modules", "target", "__pycache__", ".git",
    "vendor", "dist", "build", ".next", ".nuxt",
    "venv", ".venv", "coverage",
];
```

And file extensions:

```rust
const SKIP_EXTENSIONS: &[&str] = &[
    "min.js", "min.css", "map", "lock",
    "wasm", "pb.go", "pb.rs", "d.ts",
    "pyc", "pyo", "class", "o", "so", "dll",
    // ... media and archive formats
];
```

### Why Skip `.d.ts` Files?

TypeScript declaration files are:
- Often auto-generated from JavaScript libraries
- Very large (thousands of type definitions)
- Don't contain implementation logic
- Would consume embedding API quota without adding search value

If a user explicitly wants to index `.d.ts` files, they can override via config.

---

## 5. Chunk Sizing: 50-500 Tokens

### Why These Bounds?

**Minimum 50 tokens**: A chunk smaller than 50 tokens (roughly 2-3 lines of code)
is too small to be useful for search. A function signature alone without the body
doesn't provide enough context. Merging tiny chunks with siblings gives better
embedding quality.

**Maximum 500 tokens**: Embedding models (including Voyage code-3) have input limits,
but more importantly, larger chunks reduce retrieval precision. If a 2000-token chunk
matches a query, 75% of it is probably irrelevant. Smaller chunks = more precise results.

**Why not fixed-size chunks (like 256 tokens)?**

Fixed-size chunks cut through semantic boundaries:

```python
# Fixed-size chunking at 256 tokens might cut here:
class UserService:
    def create_user(self, name, email):
        user = User(name=name, email=email)
        self.db.save(user)
        return user
    # ← chunk boundary cuts the class in half
    def delete_user(self, user_id):
        self.db.delete(user_id)
```

AST-aware chunking keeps the whole function together:

```python
# Chunk 1: create_user (complete function)
# Chunk 2: delete_user (complete function)
```

The semantic completeness of each chunk improves both embedding quality
(the model sees a complete concept) and retrieval precision (the user gets
a complete function, not half of one).

### Merge Strategy

When merging small chunks:
1. Start with the first chunk
2. If the next sibling chunk is from the same file and both are < min_tokens:
   merge them into one chunk
3. The merged chunk gets the combined text and the first chunk's symbol name
4. Repeat until no more adjacent small chunks

Example: A Python file with three one-liner utility functions:
```python
def add(a, b): return a + b      # ~10 tokens → too small
def subtract(a, b): return a - b  # ~10 tokens → too small
def multiply(a, b): return a * b  # ~10 tokens → too small
```
These become one chunk: "add, subtract, multiply" with ~30 tokens.

### Split Strategy

When splitting large chunks (e.g., a 200-line class):
1. Look for child nodes in the AST (methods, inner functions)
2. Each child node becomes its own chunk
3. If a child node is still too large, split at line boundaries

The parent class declaration (without method bodies) can be kept as a
"signature chunk" that shows the class outline.

---

## 6. Token Budget Enforcer

### Why 2000 Tokens?

Claude Code's context window is precious. Every token in a tool response is
a token not available for reasoning. 2000 tokens is enough to show 5 relevant
code chunks with their metadata, which is typically sufficient for Claude to
understand the answer.

Cursor's Priompt system does something similar: prioritize information density
over completeness. It's better to show 5 highly relevant chunks than 20
medium-relevant chunks.

### Budget Allocation Strategy

```
Budget: 2000 tokens

Fixed costs:
  - Header ("## Search Results for: ...")    ~20 tokens
  - Footer ("*5 results from 3 files*")      ~15 tokens
  - Per-result metadata line                  ~30 tokens each

Variable costs:
  - Code block per result                     ~50-300 tokens each

With 5 results:
  Fixed: 20 + 15 + (5 × 30) = 185 tokens
  Available for code: 2000 - 185 = 1815 tokens
  Per result: ~363 tokens ≈ 25 lines of code
```

If a result's code exceeds its share:
1. First try: include the full function (might fit in budget)
2. If not: include just the signature + first 10 lines + "// ..."
3. If even that doesn't fit: include only metadata (file, symbol, lines)

Priority: later results are truncated before earlier ones. Result #1 always
gets full code. Result #5 might get truncated.

---

## 7. LanceDB Fallback Plan

If the LanceDB Rust SDK proves too immature for production use, here's the
complete fallback: brute-force cosine similarity with bincode persistence.

### Plan B: `BruteForceVectorStore`

```rust
use crate::store::VectorStore;
use crate::types::{Chunk, SearchResult, FileMetadata, SearchSource};
use std::path::{Path, PathBuf};

pub struct BruteForceVectorStore {
    chunks: Vec<StoredChunk>,
    store_path: PathBuf,
}

struct StoredChunk {
    id: String,
    file: String,
    symbol: String,
    kind: SymbolKind,
    start_line: u32,
    end_line: u32,
    language: String,
    text: String,
    embedding: Vec<f32>,  // Always present in stored chunks
}

impl BruteForceVectorStore {
    pub fn open(path: &Path) -> Result<Self> {
        let store_path = path.join("vectors.bin");
        let chunks = if store_path.exists() {
            let data = std::fs::read(&store_path)?;
            bincode::deserialize(&data)?
        } else {
            vec![]
        };
        Ok(Self { chunks, store_path })
    }

    fn save(&self) -> Result<()> {
        let data = bincode::serialize(&self.chunks)?;
        std::fs::write(&self.store_path, data)?;
        Ok(())
    }

    fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm_a == 0.0 || norm_b == 0.0 { 0.0 } else { dot / (norm_a * norm_b) }
    }
}

impl VectorStore for BruteForceVectorStore {
    fn vector_search(&self, embedding: &[f32], k: usize) -> Result<Vec<SearchResult>> {
        let mut scored: Vec<(usize, f32)> = self.chunks.iter()
            .enumerate()
            .map(|(i, chunk)| (i, Self::cosine_similarity(embedding, &chunk.embedding)))
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        Ok(scored.into_iter()
            .take(k)
            .map(|(i, score)| {
                let chunk = &self.chunks[i];
                SearchResult {
                    chunk_id: chunk.id.clone(),
                    file: chunk.file.clone(),
                    // ... rest of fields
                    score,
                    source: SearchSource::Vector,
                }
            })
            .collect())
    }

    // ... other trait methods
}
```

### Performance of Brute-Force

| Chunks | Vector Dims | Memory | Query Time |
|--------|-------------|--------|------------|
| 10K    | 1024        | 40MB   | ~5ms       |
| 50K    | 1024        | 200MB  | ~25ms      |
| 100K   | 1024        | 400MB  | ~50ms      |
| 200K   | 1024        | 800MB  | ~100ms     |
| 500K   | 1024        | 2GB    | ~250ms     |

For repos up to 100K files (500K chunks), brute-force is viable.
Query time of 50-100ms is fast enough for interactive use.

Beyond 200K chunks, consider adding HNSW indexing via `instant-distance`
or similar crate.

### When to Switch

The spike in Milestone 4 (Step 4.2) determines which path we take.
If LanceDB works: use it (better ANN, filtering, no memory issues).
If LanceDB fails: use brute-force (simpler, works, bounded perf).

The `VectorStore` trait ensures the rest of the codebase doesn't care
which implementation is behind it.

---

## 8. Error Recovery in the MCP Server

### What Happens When Things Fail During a Tool Call?

The MCP server must NEVER crash. Claude Code would lose the tool entirely
and the user would have to restart.

Error handling strategy:

```
[Embedding API fails]
  → Return tool error: "Embedding service unavailable. Try again in a few seconds."
  → Do NOT crash the server.

[LanceDB read fails]
  → Fall back to keyword-only search.
  → Include note: "Vector search unavailable, using keyword search only."

[Tantivy read fails]
  → Fall back to vector-only search.
  → Include note: "Keyword search unavailable, using semantic search only."

[Both stores fail]
  → Return tool error: "Search index unavailable. Run `lumina index` to rebuild."

[File not found for get_file_span]
  → Return tool error: "File not found: {path}"

[Tree-sitter parse fails during query]
  → This shouldn't happen (we're querying, not indexing)
  → If it does somehow: return internal error.

[JSON parse error on incoming message]
  → Send PARSE_ERROR response.
  → Continue the loop (don't exit).

[Stdout write fails]
  → The client disconnected. Exit the loop cleanly.
```

### Panic Handling

The MCP server should catch panics in tool handlers:

```rust
use std::panic;

let tool_result = panic::catch_unwind(|| {
    tools::handle_tool_call(&server.search_engine, tool_name, arguments)
}).unwrap_or_else(|_| {
    ToolResult::error("Internal error: tool handler panicked".to_string())
});
```

This prevents a bug in one tool from taking down the entire server.

---

## 9. Why No Framework for the MCP Server

### Options Considered

1. **`rmcp` crate**: Rust MCP SDK. Young, API may change.
2. **`mcp-sdk` crate**: Another Rust SDK. Same concerns.
3. **Hand-rolled JSON-RPC**: ~200 lines, full control.

### Why Hand-Rolled

The MCP protocol over stdio is trivially simple:
- Read a line of JSON from stdin
- Parse as JSON-RPC
- Dispatch to handler
- Serialize response as JSON
- Write to stdout

This is ~200 lines of code. A framework adds:
- A dependency that may break or become unmaintained
- Abstractions that hide the protocol details (bad for debugging)
- Potential stdout pollution (framework logging)
- Framework-specific patterns that complicate the code

The MCP protocol has exactly 3 methods we need to handle (`initialize`,
`tools/list`, `tools/call`) and 2 notifications (`notifications/initialized`,
`notifications/cancelled`). There's no complexity that warrants a framework.

### When to Reconsider

If MCP adds features we need (resources, prompts, sampling), or if the
protocol gets significantly more complex (SSE transport, auth), then a
framework might be worth the dependency. For P0 with stdio and tools only,
hand-rolled is the right call.

---

## 10. Embedding Model Choice: Voyage code-3

### Why Voyage code-3?

| Model | Dimensions | Code Retrieval Score | Latency | Cost |
|-------|-----------|---------------------|---------|------|
| Voyage code-3 | 1024 | SOTA (highest) | ~200ms/batch | $0.06/1M tokens |
| OpenAI text-embedding-3-large | 3072 | Good | ~150ms/batch | $0.13/1M tokens |
| OpenAI text-embedding-3-small | 1536 | Moderate | ~100ms/batch | $0.02/1M tokens |
| Cohere embed-v3.0 | 1024 | Good | ~200ms/batch | $0.10/1M tokens |
| Local (all-MiniLM-L6-v2) | 384 | Poor for code | ~5ms/batch | Free |

Voyage code-3 leads on code retrieval benchmarks (CodeSearchNet, CoSQA).
1024 dimensions is a good balance — 3× smaller than OpenAI large, better
quality for code.

### The "Local-First" Compromise

"Local-first" means your CODE stays local. The data at rest (chunks, embeddings,
indices) is all on your machine. The Voyage API only sees the text content of
chunks — no file paths, no project structure, no secrets (unless in the code).

For users who want fully offline operation, we can add a local embedding option
in P1 (e.g., via `ort` with an ONNX model). The `Embedder` trait makes this
a drop-in replacement.

### Batch Optimization

Voyage allows up to 128 texts per API call. We maximize this:
- Collect all new chunks that need embedding
- Group into batches of 128
- Send sequentially (rate limits make concurrency pointless)
- One batch of 128 texts takes ~200ms, same as one text

For 10K new chunks: 10,000 / 128 = 79 API calls × 200ms = ~16 seconds.
This is the indexing bottleneck, not tree-sitter parsing.

---

## 11. Why SHA-256 for Chunk IDs (Not UUIDs)

Chunk IDs are SHA-256 hashes of the chunk's text content. Not UUIDs, not
auto-incrementing integers.

**Benefits of content-addressable IDs:**
1. **Deduplication**: Same code in two files → same chunk ID → embed once.
2. **Incremental indexing**: If file changed but a function didn't, the chunk
   hash is the same → skip re-embedding.
3. **Cross-user cache**: Two users indexing the same repo get the same chunk IDs.
   A shared embedding cache is possible (future feature).
4. **Deterministic**: Tests produce the same IDs every run. No random UUIDs
   that change between test runs.

**Collision risk**: SHA-256 has 2^256 possible values. The probability of
collision with 1 billion chunks is approximately 1 in 10^57. Negligible.

---

## 12. Output Format: Why Markdown

Tool results are formatted as Markdown because:
1. Claude Code renders Markdown in its output.
2. Markdown code blocks preserve formatting and enable syntax highlighting.
3. Headers, tables, and horizontal rules structure the output naturally.
4. It's human-readable if debugging raw JSON-RPC messages.

The format prioritizes information density:
```markdown
### 1. src/auth/middleware.rs:15-42 — `authenticate` (function)
```rust
pub fn authenticate(...) -> ... {
    ...
}
```
```

This gives Claude: file path, line range, symbol name, symbol kind, and
the actual code. All in ~5 lines. Claude can immediately read the code
and understand the context without additional tool calls.
