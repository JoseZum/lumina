# 06 - Implementation Order & Milestones

## Principle: Always Compiles, Always Runs

Every step in this order produces a project that compiles and has at least one
passing test. No "write it all then pray" steps.

---

## Milestone 0: Project Skeleton (30 minutes)

### Step 0.1: Initialize project
```bash
cargo init --name lumina
```

### Step 0.2: Write Cargo.toml
Copy from `docs/03-CARGO_TOML.md`. Run `cargo check` to verify all deps resolve.

**Validation**: `cargo check` succeeds.

### Step 0.3: Create module structure
Create all directories and empty `mod.rs` files:

```bash
mkdir -p src/{chunker,embeddings,store,search,indexer,mcp}
```

Create stub files with just `// TODO` in each. Wire up `lib.rs` with `pub mod` declarations.

**Validation**: `cargo check` succeeds with empty modules.

### Step 0.4: Add .gitignore
```
/target
/.lumina
```

### Step 0.5: First commit
```bash
git init && git add -A && git commit -m "Initial project skeleton"
```

---

## Milestone 1: Foundation Types (1 hour)

### Step 1.1: `src/error.rs`
Write the complete `LuminaError` enum with all variants and `Result<T>` alias.

**Test**: Write a test that creates each error variant and checks its Display output.

**Validation**: `cargo test` passes.

### Step 1.2: `src/types.rs`
Write all shared types: `Chunk`, `SymbolKind`, `SearchResult`, `SearchSource`,
`SymbolInfo`, `FileMetadata`, `IndexStats`.

**Test**: Write a test that creates a `Chunk`, serializes to JSON, deserializes back,
and verifies all fields match.

**Validation**: `cargo test` passes.

### Step 1.3: `src/config.rs`
Write `LuminaConfig` with defaults and TOML loading.

**Test**: Write a test that creates a temporary config.toml, loads it, and verifies
values override defaults correctly.

**Validation**: `cargo test` passes.

### Step 1.4: Commit
```
Foundation types: error, types, config
```

---

## Milestone 2: Tree-Sitter Chunker (3-4 hours)

**This is the highest-risk module.** Tree-sitter grammars can have ABI mismatches.
Start with ONE language (Python), get it working, then add more.

### Step 2.1: `src/chunker/mod.rs` — Trait definition
Write the `Chunker` trait with `chunk_file()` and `supported_extensions()`.

**Validation**: `cargo check` passes.

### Step 2.2: `src/chunker/languages.rs` — Python only
Write `LanguageConfig` struct and implement `get_config()` for Python only.
Include the tree-sitter query for Python functions and classes.

**Critical test**: Parse a simple Python function and verify the query captures it.

```rust
#[test]
fn test_python_grammar_loads() {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&tree_sitter_python::LANGUAGE.into()).unwrap();
    let tree = parser.parse("def hello():\n    pass\n", None).unwrap();
    assert!(!tree.root_node().has_error());
}
```

**If this test fails**: The tree-sitter ABI versions are incompatible. Try different
version combinations of `tree-sitter` and `tree-sitter-python`.

**Validation**: `cargo test` passes.

### Step 2.3: `src/chunker/treesitter.rs` — Core implementation
Implement `TreeSitterChunker` with:
1. Parse file with tree-sitter
2. Run chunk query
3. Extract captured nodes as Chunks
4. Compute SHA-256 IDs

Don't implement merge_small/split_large yet. Just the basic extraction.

**Tests**:
- `test_chunk_python_function`: Parse Python file with 2 functions → 2 chunks
- `test_chunk_python_class`: Parse Python file with a class → 1 chunk (the class)
- `test_chunk_includes_metadata`: Verify file path, symbol name, line numbers

**Validation**: `cargo test` passes. Manually inspect chunk output.

### Step 2.4: Create test fixtures
Create `tests/fixtures/sample_python/`:

```python
# tests/fixtures/sample_python/app/models.py
class User:
    def __init__(self, name: str, email: str):
        self.name = name
        self.email = email

    def display_name(self) -> str:
        return f"{self.name} <{self.email}>"

class Post:
    def __init__(self, title: str, author: User):
        self.title = title
        self.author = author
```

```python
# tests/fixtures/sample_python/app/auth.py
import hashlib
import secrets

def hash_password(password: str) -> str:
    salt = secrets.token_hex(16)
    return hashlib.sha256(f"{salt}{password}".encode()).hexdigest()

def verify_password(password: str, hashed: str) -> bool:
    return hash_password(password) == hashed

def create_token(user_id: int) -> str:
    return secrets.token_urlsafe(32)
```

### Step 2.5: Add Rust grammar support
Add `tree-sitter-rust` config to `languages.rs`. Test with:

```rust
#[test]
fn test_chunk_rust_function() {
    // Create a simple Rust file content
    let content = r#"
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

pub fn multiply(a: i32, b: i32) -> i32 {
    a * b
}
"#;
    // ... verify 2 chunks extracted
}
```

### Step 2.6: Add remaining languages (one at a time)
For each language: add config, write one test, verify it compiles.
Order: Python → Rust → TypeScript → JavaScript → Go → Java

If a grammar doesn't compile, skip it and file a TODO. Don't block on one language.

### Step 2.7: Implement merge_small and split_large
- `merge_small_chunks`: merge adjacent chunks under min_tokens
- `split_large_chunk`: split chunks over max_tokens at line boundaries

**Tests**: Create fixtures with tiny functions (2 lines) and huge classes (100+ lines).

### Step 2.8: Commit
```
Tree-sitter chunker with Python, Rust, TypeScript, JavaScript, Go, Java support
```

---

## Milestone 3: SHA-256 Hasher (30 minutes)

### Step 3.1: `src/indexer/hasher.rs`
Implement `FileHasher` with hash_content, is_changed, save/load.

**Tests**:
- `test_hash_deterministic`: Same content → same hash
- `test_hash_different`: Different content → different hash
- `test_cache_roundtrip`: save() then load() preserves all entries
- `test_is_changed_new_file`: New file (not in cache) → true
- `test_is_changed_same_content`: Same content → false
- `test_is_changed_different_content`: Changed content → true

**Validation**: `cargo test` passes.

### Step 3.2: Commit
```
SHA-256 incremental hashing for file/chunk deduplication
```

---

## Milestone 4: Storage Layer (3-4 hours)

### Step 4.1: Tantivy store
Implement `TantivyStore` with:
- `build_schema()`: define tantivy schema
- `upsert_chunks()`: add/update documents
- `keyword_search()`: BM25 query
- `symbol_search()`: exact match on symbol field
- `delete_by_file()`: remove docs by file path

**Tests** (using tempdir):
- `test_insert_and_keyword_search`: Insert 3 chunks, search for keyword, find match
- `test_bm25_relevance`: Insert 2 chunks, one more relevant, verify ordering
- `test_symbol_search`: Insert chunk with symbol "UserService", find by name
- `test_delete_by_file`: Insert chunks from 2 files, delete 1, verify only other remains

Tantivy is synchronous and well-documented. This should be straightforward.

**Validation**: `cargo test` passes.

### Step 4.2: LanceDB store — SPIKE FIRST
Before implementing the full store, write a standalone test that:
1. Creates a LanceDB connection
2. Creates a table with the chunk schema
3. Inserts one record with a fake embedding
4. Does a vector search
5. Reads back the result

```rust
#[tokio::test]
async fn spike_lancedb_basic() {
    let tmp = tempfile::tempdir().unwrap();
    let db = lancedb::connect(tmp.path().to_str().unwrap()).execute().await.unwrap();

    // Create schema, insert, search...
    // If this fails, we know LanceDB SDK has issues
    // before investing time in the full implementation
}
```

**If the spike fails**: Switch to Plan B (brute-force cosine similarity with bincode
persistence). See `docs/07-DESIGN_DECISIONS.md` for the fallback implementation.

### Step 4.3: LanceDB store — Full implementation
Implement `LanceStore` with:
- `open()`: connect to LanceDB, create or open table
- `upsert_chunks()`: convert chunks to Arrow RecordBatch, insert
- `vector_search()`: query by embedding vector
- `delete_by_file()`: filter and delete
- `list_files()`: scan for unique file paths

**Tests** (using tempdir):
- `test_insert_and_vector_search`: Insert chunks with random embeddings, search by vector
- `test_upsert_idempotent`: Insert same chunk twice, verify only 1 copy
- `test_delete_by_file`: Insert, delete, verify deleted

**Note**: LanceDB tests must use `#[tokio::test]` even though our trait is sync.
The `block_on()` wrapper handles the bridging.

### Step 4.4: Commit
```
Storage layer: tantivy BM25 + LanceDB vector store
```

---

## Milestone 5: Embedding Client (1 hour)

### Step 5.1: `src/embeddings/mod.rs` — Trait
Write `Embedder` trait.

### Step 5.2: `src/embeddings/voyage.rs` — Implementation
Implement `VoyageEmbedder` with API request/response types.

### Step 5.3: Mock embedder for CI
In `tests/common/mod.rs`:
```rust
pub struct MockEmbedder;
impl Embedder for MockEmbedder {
    fn embed_texts(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        // Deterministic fake embeddings based on text hash
    }
    fn embed_query(&self, query: &str) -> Result<Vec<f32>> { ... }
    fn dimensions(&self) -> usize { 1024 }
}
```

### Step 5.4: Integration test with real API (optional)
Mark with `#[ignore]` — only runs when VOYAGE_API_KEY is set.

```rust
#[test]
#[ignore = "requires VOYAGE_API_KEY"]
fn test_voyage_real_embedding() {
    let key = std::env::var("VOYAGE_API_KEY").unwrap();
    let embedder = VoyageEmbedder::new(key, "voyage-code-3".into(), 128);
    let result = embedder.embed_query("hello world").unwrap();
    assert_eq!(result.len(), 1024);
}
```

### Step 5.5: Commit
```
Voyage code-3 embedding client with mock for testing
```

---

## Milestone 6: Search Pipeline (2 hours)

### Step 6.1: `src/search/rrf.rs`
Implement `rrf_merge()` — pure function, no dependencies.

**Tests**:
- `test_rrf_disjoint_lists`: Two lists with no overlap → interleaved
- `test_rrf_overlap_boosts`: Items in both lists get higher score
- `test_rrf_empty_inputs`: One or both lists empty → handles gracefully
- `test_rrf_single_item`: Each list has 1 item → both in output

### Step 6.2: `src/search/reranker.rs`
Implement `NoopReranker` (pass-through) and `Reranker` trait.
`ApiReranker` can wait — NoopReranker is sufficient for P0.

### Step 6.3: `src/search/mod.rs`
Implement `SearchEngine` with:
- `semantic_search()`: embed → vector search → keyword search → RRF → rerank
- `find_symbol()`: delegate to keyword store
- `get_file_span()`: read file from disk
- `list_files()`: delegate to vector store
- `format_results()`: markdown formatter with token budget

**Tests** (with mock embedder and real tantivy + LanceDB on tempdir):
- `test_semantic_search_end_to_end`: Index fixture, search, get results
- `test_format_results_budget`: Verify output doesn't exceed token budget
- `test_find_symbol`: Index fixture with known symbol, find it

### Step 6.4: Commit
```
Search engine: RRF fusion, reranker interface, result formatting
```

---

## Milestone 7: Indexer Pipeline (2 hours)

### Step 7.1: `src/indexer/mod.rs`
Implement the full `Indexer` connecting chunker → embedder → stores.

**Tests** (with fixture repos and mock embedder):
- `test_index_from_scratch`: Index fixture → verify stats
- `test_incremental_index`: Index, modify 1 file, re-index → only 1 file reprocessed
- `test_skip_binary_files`: Add a binary file to fixture, verify it's skipped
- `test_skip_large_files`: Add a large file, verify skipped

### Step 7.2: Commit
```
Indexing pipeline with incremental support
```

---

## Milestone 8: MCP Server (2-3 hours)

### Step 8.1: `src/mcp/protocol.rs`
Write all JSON-RPC types. Pure serde structs.

**Tests**:
- `test_deserialize_initialize_request`
- `test_deserialize_tool_call`
- `test_serialize_success_response`
- `test_serialize_error_response`
- `test_serialize_tool_result`

### Step 8.2: `src/mcp/tools.rs`
Write tool definitions and handlers.

### Step 8.3: `src/mcp/handler.rs`
Write request dispatcher.

### Step 8.4: `src/mcp/mod.rs`
Write the main stdio loop.

### Step 8.5: MCP integration test
Write `tests/test_mcp.rs` that spawns the binary and tests the full handshake +
tool call sequence via piped stdin/stdout.

### Step 8.6: Commit
```
MCP server with stdio transport and 4 tools
```

---

## Milestone 9: CLI & Integration (1 hour)

### Step 9.1: `src/main.rs`
Wire everything together with clap subcommands.

### Step 9.2: `src/lib.rs`
Write factory functions (`create_indexer`, `create_search_engine`).

### Step 9.3: Manual end-to-end test
```bash
# Index a real repo
cargo run -- index --repo /path/to/some/project

# Query from CLI
cargo run -- query "authentication" --repo /path/to/some/project

# Start MCP server and test manually
echo '{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"0.1"}}}' | cargo run -- mcp --repo /path/to/some/project
```

### Step 9.4: Test with Claude Code
Add to `.mcp.json`:
```json
{
  "mcpServers": {
    "lumina": {
      "command": "cargo",
      "args": ["run", "--release", "--", "mcp", "--repo", "."],
      "env": { "VOYAGE_API_KEY": "pa-..." }
    }
  }
}
```

Verify that Claude Code:
1. Shows "lumina" tools in its available tools
2. Uses `semantic_search` when you ask about the codebase
3. Results are relevant and well-formatted

### Step 9.5: Final commit
```
CLI entry point and full integration
```

---

## Risk Mitigation Schedule

| Risk | When Detected | Mitigation | Time Cost |
|------|---------------|------------|-----------|
| tree-sitter ABI mismatch | Milestone 2, Step 2.2 | Pin compatible versions | 1 hour |
| LanceDB SDK too immature | Milestone 4, Step 4.2 | Switch to brute-force cosine | 2 hours |
| Voyage API rate limiting | Milestone 5, Step 5.4 | Add retry with backoff | 30 min |
| tree-sitter query wrong for language | Milestone 2, Step 2.6 | Add fallback to line-based chunking | 1 hour |
| MCP handshake wrong | Milestone 8, Step 8.5 | Use MCP inspector to debug | 1 hour |
| Token estimation too inaccurate | Milestone 6, Step 6.3 | Adjust heuristic constant | 15 min |
| arrow-array version conflict | Milestone 4, Step 4.2 | Match lancedb's arrow version | 30 min |

---

## Total Estimated Time

| Milestone | Time |
|-----------|------|
| M0: Skeleton | 30 min |
| M1: Types | 1 hour |
| M2: Chunker | 3-4 hours |
| M3: Hasher | 30 min |
| M4: Storage | 3-4 hours |
| M5: Embeddings | 1 hour |
| M6: Search | 2 hours |
| M7: Indexer | 2 hours |
| M8: MCP Server | 2-3 hours |
| M9: CLI | 1 hour |
| **Total** | **~16-20 hours** |

This assumes a senior Rust developer familiar with the ecosystem. If LanceDB or
tree-sitter cause unexpected issues, add 2-4 hours for debugging and fallbacks.
