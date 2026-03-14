# 08 - Testing Strategy

## Testing Philosophy

Every module has unit tests. The MCP server has integration tests.
Tests run without API keys by default (mock embedder). Tests that need
real APIs are marked `#[ignore]` and run explicitly.

---

## Test Organization

```
tests/
├── common/
│   └── mod.rs              # Shared utilities: MockEmbedder, fixture helpers
├── fixtures/
│   ├── sample_rust/         # 3-file Rust project
│   ├── sample_python/       # 4-file Python project
│   └── sample_mixed/        # Multi-language project
├── test_chunker.rs          # Tree-sitter chunker tests
├── test_indexer.rs          # Full indexing pipeline tests
├── test_search.rs           # Search + RRF + formatting tests
└── test_mcp.rs              # MCP server protocol tests
```

---

## Shared Test Utilities (`tests/common/mod.rs`)

```rust
use lumina::embeddings::Embedder;
use lumina::error::Result;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Mock embedder that generates deterministic fake embeddings.
/// Uses SHA-256 of text to generate a reproducible 1024-dim vector.
/// These embeddings are NOT semantically meaningful — they're for
/// testing that the pipeline works, not that search is accurate.
pub struct MockEmbedder;

impl Embedder for MockEmbedder {
    fn embed_texts(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|t| fake_embedding(t)).collect())
    }

    fn embed_query(&self, query: &str) -> Result<Vec<f32>> {
        Ok(fake_embedding(query))
    }

    fn dimensions(&self) -> usize {
        1024
    }
}

/// Generate a deterministic 1024-dim vector from text.
/// Same text → same vector, always.
fn fake_embedding(text: &str) -> Vec<f32> {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    let hash = hasher.finalize();

    (0..1024).map(|i| {
        let byte = hash[i % 32] as f32;
        (byte / 255.0) * 2.0 - 1.0  // Normalize to [-1, 1]
    }).collect()
}

/// Get the path to a test fixture directory.
pub fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// Create a temporary directory with some test files.
/// Returns the tempdir (must be kept alive for the duration of the test).
pub fn create_test_repo() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();

    // Create a simple Rust file
    let src_dir = root.join("src");
    std::fs::create_dir_all(&src_dir).unwrap();

    std::fs::write(src_dir.join("main.rs"), r#"
fn main() {
    println!("Hello, world!");
    let result = add(2, 3);
    println!("2 + 3 = {}", result);
}

fn add(a: i32, b: i32) -> i32 {
    a + b
}

fn multiply(a: i32, b: i32) -> i32 {
    a * b
}
"#).unwrap();

    // Create a Python file
    std::fs::write(root.join("app.py"), r#"
class UserService:
    def __init__(self, db):
        self.db = db

    def create_user(self, name: str, email: str) -> dict:
        user = {"name": name, "email": email}
        self.db.insert(user)
        return user

    def find_user(self, email: str) -> dict:
        return self.db.find({"email": email})

def hash_password(password: str) -> str:
    import hashlib
    return hashlib.sha256(password.encode()).hexdigest()
"#).unwrap();

    // Create .gitignore
    std::fs::write(root.join(".gitignore"), "target/\n__pycache__/\n").unwrap();

    (tmp, root)
}
```

---

## Test Fixtures

### `tests/fixtures/sample_rust/`

```
src/main.rs:
```rust
use crate::user::UserService;

mod user;

fn main() {
    let service = UserService::new();
    let user = service.create("Alice", "alice@example.com");
    println!("Created user: {:?}", user);
}
```

```
src/user.rs:
```rust
#[derive(Debug, Clone)]
pub struct User {
    pub name: String,
    pub email: String,
}

pub struct UserService {
    users: Vec<User>,
}

impl UserService {
    pub fn new() -> Self {
        Self { users: Vec::new() }
    }

    pub fn create(&mut self, name: &str, email: &str) -> User {
        let user = User {
            name: name.to_string(),
            email: email.to_string(),
        };
        self.users.push(user.clone());
        user
    }

    pub fn find_by_email(&self, email: &str) -> Option<&User> {
        self.users.iter().find(|u| u.email == email)
    }

    pub fn list_all(&self) -> &[User] {
        &self.users
    }
}
```

### `tests/fixtures/sample_python/`

```
app/models.py:
```python
class User:
    def __init__(self, name: str, email: str):
        self.name = name
        self.email = email
        self.active = True

    def display_name(self) -> str:
        return f"{self.name} <{self.email}>"

    def deactivate(self):
        self.active = False


class Post:
    def __init__(self, title: str, content: str, author: User):
        self.title = title
        self.content = content
        self.author = author

    def summary(self) -> str:
        return f"{self.title} by {self.author.display_name()}"
```

```
app/auth.py:
```python
import hashlib
import secrets

SECRET_KEY = "change-me-in-production"

def hash_password(password: str) -> str:
    salt = secrets.token_hex(16)
    return hashlib.sha256(f"{salt}{password}".encode()).hexdigest()

def verify_password(password: str, hashed: str) -> bool:
    return hash_password(password) == hashed

def create_token(user_id: int) -> str:
    return secrets.token_urlsafe(32)

def verify_token(token: str) -> bool:
    return len(token) > 0
```

```
app/utils.py:
```python
from datetime import datetime

DATE_FORMAT = "%Y-%m-%d"
MAX_RETRIES = 3

def format_date(dt: datetime) -> str:
    return dt.strftime(DATE_FORMAT)

def retry(func, max_retries: int = MAX_RETRIES):
    for attempt in range(max_retries):
        try:
            return func()
        except Exception as e:
            if attempt == max_retries - 1:
                raise e
```

---

## Unit Tests by Module

### `test_chunker.rs`

```rust
use lumina::chunker::{Chunker, TreeSitterChunker};
use std::path::Path;

#[test]
fn test_chunk_python_functions() {
    let chunker = TreeSitterChunker::new(500, 50);
    let content = r#"
def hello():
    print("hello")

def world():
    print("world")
"#;

    let chunks = chunker.chunk_file(Path::new("test.py"), content).unwrap();

    // Should extract 2 functions
    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks[0].symbol, "hello");
    assert_eq!(chunks[1].symbol, "world");
    assert_eq!(chunks[0].language, "python");
}

#[test]
fn test_chunk_python_class() {
    let chunker = TreeSitterChunker::new(500, 50);
    let content = r#"
class Calculator:
    def add(self, a, b):
        return a + b

    def subtract(self, a, b):
        return a - b
"#;

    let chunks = chunker.chunk_file(Path::new("calc.py"), content).unwrap();

    // Class should be one chunk (within max_tokens)
    // OR split into methods if class is too big
    assert!(!chunks.is_empty());
    // At minimum, "Calculator" should appear as a symbol
    assert!(chunks.iter().any(|c| c.symbol.contains("Calculator")));
}

#[test]
fn test_chunk_rust_functions() {
    let chunker = TreeSitterChunker::new(500, 50);
    let content = r#"
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

pub fn multiply(a: i32, b: i32) -> i32 {
    a * b
}
"#;

    let chunks = chunker.chunk_file(Path::new("math.rs"), content).unwrap();

    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks[0].symbol, "add");
    assert_eq!(chunks[1].symbol, "multiply");
    assert_eq!(chunks[0].language, "rust");
}

#[test]
fn test_chunk_rust_struct_and_impl() {
    let chunker = TreeSitterChunker::new(500, 50);
    let content = r#"
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn distance(&self, other: &Point) -> f64 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
    }
}
"#;

    let chunks = chunker.chunk_file(Path::new("point.rs"), content).unwrap();

    // Should have at least struct + impl (or struct + individual methods)
    assert!(chunks.len() >= 2);
    assert!(chunks.iter().any(|c| c.symbol == "Point"));
}

#[test]
fn test_chunk_preserves_line_numbers() {
    let chunker = TreeSitterChunker::new(500, 50);
    let content = "# comment\n# another comment\ndef foo():\n    pass\n";

    let chunks = chunker.chunk_file(Path::new("test.py"), content).unwrap();

    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].start_line, 3); // 1-indexed, function starts at line 3
    assert_eq!(chunks[0].end_line, 4);
}

#[test]
fn test_chunk_deterministic_ids() {
    let chunker = TreeSitterChunker::new(500, 50);
    let content = "def foo():\n    pass\n";

    let chunks1 = chunker.chunk_file(Path::new("a.py"), content).unwrap();
    let chunks2 = chunker.chunk_file(Path::new("b.py"), content).unwrap();

    // Same content → same chunk ID (content-addressed)
    assert_eq!(chunks1[0].id, chunks2[0].id);
    // But different file paths
    assert_ne!(chunks1[0].file, chunks2[0].file);
}

#[test]
fn test_unsupported_extension() {
    let chunker = TreeSitterChunker::new(500, 50);
    let result = chunker.chunk_file(Path::new("data.csv"), "a,b,c\n1,2,3\n");

    assert!(result.is_err());
}

#[test]
fn test_syntax_error_doesnt_crash() {
    let chunker = TreeSitterChunker::new(500, 50);
    // Invalid Python syntax
    let content = "def broken(\n    this is not valid python\n";

    // Should not panic — may return empty vec or partial results
    let result = chunker.chunk_file(Path::new("broken.py"), content);
    assert!(result.is_ok()); // Must not crash
}

#[test]
fn test_empty_file() {
    let chunker = TreeSitterChunker::new(500, 50);
    let chunks = chunker.chunk_file(Path::new("empty.py"), "").unwrap();
    assert!(chunks.is_empty());
}

#[test]
fn test_merge_small_chunks() {
    let chunker = TreeSitterChunker::new(500, 50);
    // Three tiny functions that should be merged
    let content = r#"
def a(): pass
def b(): pass
def c(): pass
"#;
    let chunks = chunker.chunk_file(Path::new("tiny.py"), content).unwrap();

    // With min_tokens=50, these ~10-token functions should be merged
    // into fewer chunks
    assert!(chunks.len() <= 2); // Merged into 1 or 2 chunks
}
```

### `test_search.rs`

```rust
use lumina::search::rrf;
use lumina::types::{SearchResult, SearchSource, SymbolKind};

fn make_result(id: &str, score: f32, source: SearchSource) -> SearchResult {
    SearchResult {
        chunk_id: id.to_string(),
        file: format!("{}.rs", id),
        symbol: id.to_string(),
        kind: SymbolKind::Function,
        start_line: 1,
        end_line: 10,
        language: "rust".to_string(),
        text: format!("fn {}() {{}}", id),
        score,
        source,
    }
}

#[test]
fn test_rrf_disjoint_lists() {
    let vector = vec![
        make_result("a", 0.9, SearchSource::Vector),
        make_result("b", 0.8, SearchSource::Vector),
    ];
    let keyword = vec![
        make_result("c", 5.0, SearchSource::Keyword),
        make_result("d", 4.0, SearchSource::Keyword),
    ];

    let merged = rrf::rrf_merge(vector, keyword, 60);

    assert_eq!(merged.len(), 4);
    // All results should be present
    let ids: Vec<&str> = merged.iter().map(|r| r.chunk_id.as_str()).collect();
    assert!(ids.contains(&"a"));
    assert!(ids.contains(&"b"));
    assert!(ids.contains(&"c"));
    assert!(ids.contains(&"d"));
    // All sources should be Fused
    assert!(merged.iter().all(|r| r.source == SearchSource::Fused));
}

#[test]
fn test_rrf_overlap_boosts_score() {
    let vector = vec![
        make_result("a", 0.9, SearchSource::Vector),
        make_result("b", 0.8, SearchSource::Vector),
    ];
    let keyword = vec![
        make_result("a", 5.0, SearchSource::Keyword),  // "a" appears in both
        make_result("c", 4.0, SearchSource::Keyword),
    ];

    let merged = rrf::rrf_merge(vector, keyword, 60);

    // "a" should be ranked first because it appears in both lists
    assert_eq!(merged[0].chunk_id, "a");
    // "a" score should be higher than "b" or "c"
    assert!(merged[0].score > merged[1].score);
}

#[test]
fn test_rrf_empty_vector_list() {
    let vector = vec![];
    let keyword = vec![
        make_result("a", 5.0, SearchSource::Keyword),
    ];

    let merged = rrf::rrf_merge(vector, keyword, 60);
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].chunk_id, "a");
}

#[test]
fn test_rrf_empty_keyword_list() {
    let vector = vec![
        make_result("a", 0.9, SearchSource::Vector),
    ];
    let keyword = vec![];

    let merged = rrf::rrf_merge(vector, keyword, 60);
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].chunk_id, "a");
}

#[test]
fn test_rrf_both_empty() {
    let merged = rrf::rrf_merge(vec![], vec![], 60);
    assert!(merged.is_empty());
}

#[test]
fn test_rrf_k_constant_affects_ranking() {
    let vector = vec![
        make_result("a", 0.9, SearchSource::Vector),
        make_result("b", 0.8, SearchSource::Vector),
    ];
    let keyword = vec![
        make_result("c", 5.0, SearchSource::Keyword),
        make_result("d", 4.0, SearchSource::Keyword),
    ];

    let merged_k1 = rrf::rrf_merge(vector.clone(), keyword.clone(), 1);
    let merged_k60 = rrf::rrf_merge(vector, keyword, 60);

    // With k=1, rank 1 items get score 1/(1+1)=0.5, rank 2 gets 1/(1+2)=0.33
    // With k=60, rank 1 items get score 1/(60+1)≈0.016, rank 2 gets 1/(60+2)≈0.016
    // The difference between ranks is much smaller with higher k

    let diff_k1 = merged_k1[0].score - merged_k1[1].score;
    let diff_k60 = merged_k60[0].score - merged_k60[1].score;

    // Higher k → smaller score differences between ranks
    assert!(diff_k1 > diff_k60);
}
```

### `test_mcp.rs`

```rust
use assert_cmd::Command;
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Stdio};

/// Helper: start the MCP server as a subprocess
fn start_mcp_server(repo_path: &str) -> (Child, ChildStdin, BufReader<ChildStdout>) {
    let mut child = Command::cargo_bin("lumina")
        .unwrap()
        .args(["mcp", "--repo", repo_path])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    let stdin = child.stdin.take().unwrap();
    let stdout = BufReader::new(child.stdout.take().unwrap());

    (child, stdin, stdout)
}

/// Helper: send a JSON-RPC message and read the response
fn send_and_receive(stdin: &mut ChildStdin, stdout: &mut BufReader<ChildStdout>, msg: &str) -> Value {
    writeln!(stdin, "{}", msg).unwrap();
    stdin.flush().unwrap();

    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    serde_json::from_str(&line).unwrap()
}

/// Helper: send a notification (no response expected)
fn send_notification(stdin: &mut ChildStdin, msg: &str) {
    writeln!(stdin, "{}", msg).unwrap();
    stdin.flush().unwrap();
}

#[test]
fn test_mcp_initialize_handshake() {
    let fixture = env!("CARGO_MANIFEST_DIR").to_string() + "/tests/fixtures/sample_rust";
    let (mut child, mut stdin, mut stdout) = start_mcp_server(&fixture);

    // Step 1: Initialize
    let response = send_and_receive(
        &mut stdin,
        &mut stdout,
        r#"{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"0.1"}}}"#,
    );

    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 0);
    assert!(response["result"]["protocolVersion"].is_string());
    assert!(response["result"]["capabilities"]["tools"].is_object());
    assert_eq!(response["result"]["serverInfo"]["name"], "lumina");

    // Step 2: Send initialized notification (no response)
    send_notification(
        &mut stdin,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
    );

    // Step 3: List tools
    let response = send_and_receive(
        &mut stdin,
        &mut stdout,
        r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#,
    );

    let tools = response["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 4);

    // Verify tool names
    let tool_names: Vec<&str> = tools.iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(tool_names.contains(&"semantic_search"));
    assert!(tool_names.contains(&"find_symbol"));
    assert!(tool_names.contains(&"get_file_span"));
    assert!(tool_names.contains(&"list_indexed_files"));

    // Verify each tool has inputSchema
    for tool in tools {
        assert!(tool["inputSchema"].is_object(), "Tool {} missing inputSchema", tool["name"]);
    }

    child.kill().unwrap();
}

#[test]
fn test_mcp_unknown_method() {
    let fixture = env!("CARGO_MANIFEST_DIR").to_string() + "/tests/fixtures/sample_rust";
    let (mut child, mut stdin, mut stdout) = start_mcp_server(&fixture);

    // Initialize first
    send_and_receive(
        &mut stdin,
        &mut stdout,
        r#"{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"0.1"}}}"#,
    );

    // Send unknown method
    let response = send_and_receive(
        &mut stdin,
        &mut stdout,
        r#"{"jsonrpc":"2.0","id":99,"method":"nonexistent/method"}"#,
    );

    assert_eq!(response["id"], 99);
    assert!(response["error"].is_object());
    assert_eq!(response["error"]["code"], -32601); // METHOD_NOT_FOUND

    child.kill().unwrap();
}

#[test]
fn test_mcp_unknown_tool() {
    let fixture = env!("CARGO_MANIFEST_DIR").to_string() + "/tests/fixtures/sample_rust";
    let (mut child, mut stdin, mut stdout) = start_mcp_server(&fixture);

    // Initialize
    send_and_receive(
        &mut stdin,
        &mut stdout,
        r#"{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"0.1"}}}"#,
    );

    // Call unknown tool
    let response = send_and_receive(
        &mut stdin,
        &mut stdout,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"nonexistent_tool","arguments":{}}}"#,
    );

    // Should be a tool-level error (isError: true), not a protocol error
    assert!(response["result"]["isError"].as_bool().unwrap());

    child.kill().unwrap();
}

#[test]
fn test_mcp_id_preserved_as_number() {
    let fixture = env!("CARGO_MANIFEST_DIR").to_string() + "/tests/fixtures/sample_rust";
    let (mut child, mut stdin, mut stdout) = start_mcp_server(&fixture);

    let response = send_and_receive(
        &mut stdin,
        &mut stdout,
        r#"{"jsonrpc":"2.0","id":42,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"0.1"}}}"#,
    );

    // id must be 42 (number), not "42" (string)
    assert_eq!(response["id"], 42);
    assert!(response["id"].is_number());

    child.kill().unwrap();
}

#[test]
fn test_mcp_id_preserved_as_string() {
    let fixture = env!("CARGO_MANIFEST_DIR").to_string() + "/tests/fixtures/sample_rust";
    let (mut child, mut stdin, mut stdout) = start_mcp_server(&fixture);

    let response = send_and_receive(
        &mut stdin,
        &mut stdout,
        r#"{"jsonrpc":"2.0","id":"my-request-id","method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"0.1"}}}"#,
    );

    // id must be "my-request-id" (string), not a number
    assert_eq!(response["id"], "my-request-id");
    assert!(response["id"].is_string());

    child.kill().unwrap();
}

#[test]
fn test_mcp_notification_produces_no_response() {
    let fixture = env!("CARGO_MANIFEST_DIR").to_string() + "/tests/fixtures/sample_rust";
    let (mut child, mut stdin, mut stdout) = start_mcp_server(&fixture);

    // Initialize
    send_and_receive(
        &mut stdin,
        &mut stdout,
        r#"{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"0.1"}}}"#,
    );

    // Send notification
    send_notification(
        &mut stdin,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
    );

    // Send another request — if we get its response, it means
    // the notification didn't produce a spurious response
    let response = send_and_receive(
        &mut stdin,
        &mut stdout,
        r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#,
    );

    // The response should be for id:1 (tools/list), not for the notification
    assert_eq!(response["id"], 1);
    assert!(response["result"]["tools"].is_array());

    child.kill().unwrap();
}
```

### `test_indexer.rs`

```rust
mod common;

use common::{create_test_repo, MockEmbedder};
use lumina::chunker::TreeSitterChunker;
use lumina::config::LuminaConfig;
use lumina::indexer::Indexer;
use lumina::store::{LanceStore, TantivyStore};

#[test]
fn test_index_from_scratch() {
    let (tmp, repo_root) = create_test_repo();
    let config = LuminaConfig::load(repo_root.clone()).unwrap();

    // Create all components
    let chunker = Box::new(TreeSitterChunker::new(500, 50));
    let embedder = Box::new(MockEmbedder);
    let vector_store = Box::new(LanceStore::open(&config.lance_path()).unwrap());
    let keyword_store = Box::new(TantivyStore::open(&config.tantivy_path()).unwrap());

    let mut indexer = Indexer::new(chunker, embedder, vector_store, keyword_store, config).unwrap();
    let stats = indexer.index().unwrap();

    // Should have found and indexed files
    assert!(stats.files_scanned > 0);
    assert!(stats.files_changed > 0);
    assert!(stats.chunks_total > 0);
    assert!(stats.chunks_embedded > 0);
    assert_eq!(stats.chunks_cached, 0); // First run, nothing cached
}

#[test]
fn test_incremental_index_skips_unchanged() {
    let (tmp, repo_root) = create_test_repo();
    let config = LuminaConfig::load(repo_root.clone()).unwrap();

    let chunker = Box::new(TreeSitterChunker::new(500, 50));
    let embedder = Box::new(MockEmbedder);
    let vector_store = Box::new(LanceStore::open(&config.lance_path()).unwrap());
    let keyword_store = Box::new(TantivyStore::open(&config.tantivy_path()).unwrap());

    let mut indexer = Indexer::new(chunker, embedder, vector_store, keyword_store, config.clone()).unwrap();

    // First index
    let stats1 = indexer.index().unwrap();
    let first_embedded = stats1.chunks_embedded;

    // Re-create indexer (simulates new process)
    let chunker = Box::new(TreeSitterChunker::new(500, 50));
    let embedder = Box::new(MockEmbedder);
    let vector_store = Box::new(LanceStore::open(&config.lance_path()).unwrap());
    let keyword_store = Box::new(TantivyStore::open(&config.tantivy_path()).unwrap());
    let mut indexer = Indexer::new(chunker, embedder, vector_store, keyword_store, config).unwrap();

    // Second index — nothing changed
    let stats2 = indexer.index().unwrap();

    assert_eq!(stats2.files_changed, 0);
    assert_eq!(stats2.chunks_embedded, 0); // Nothing new to embed
    assert!(stats2.files_unchanged > 0);
}

#[test]
fn test_incremental_index_detects_changes() {
    let (tmp, repo_root) = create_test_repo();
    let config = LuminaConfig::load(repo_root.clone()).unwrap();

    // First index
    let chunker = Box::new(TreeSitterChunker::new(500, 50));
    let embedder = Box::new(MockEmbedder);
    let vector_store = Box::new(LanceStore::open(&config.lance_path()).unwrap());
    let keyword_store = Box::new(TantivyStore::open(&config.tantivy_path()).unwrap());
    let mut indexer = Indexer::new(chunker, embedder, vector_store, keyword_store, config.clone()).unwrap();
    indexer.index().unwrap();

    // Modify one file
    std::fs::write(repo_root.join("app.py"), "def new_function():\n    return 42\n").unwrap();

    // Re-index
    let chunker = Box::new(TreeSitterChunker::new(500, 50));
    let embedder = Box::new(MockEmbedder);
    let vector_store = Box::new(LanceStore::open(&config.lance_path()).unwrap());
    let keyword_store = Box::new(TantivyStore::open(&config.tantivy_path()).unwrap());
    let mut indexer = Indexer::new(chunker, embedder, vector_store, keyword_store, config).unwrap();
    let stats = indexer.index().unwrap();

    assert_eq!(stats.files_changed, 1); // Only app.py changed
    assert!(stats.chunks_embedded > 0); // New chunks were embedded
    assert!(stats.files_unchanged > 0); // Other files were skipped
}
```

---

## Running Tests

### All tests (no API keys needed)
```bash
cargo test
```

### With real Voyage API (integration tests)
```bash
VOYAGE_API_KEY=pa-your-key cargo test -- --ignored
```

### Single test module
```bash
cargo test --test test_chunker
cargo test --test test_mcp
cargo test --test test_search
```

### With logging output
```bash
RUST_LOG=lumina=debug cargo test -- --nocapture
```

---

## CI Configuration

```yaml
# .github/workflows/ci.yml
name: CI

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2

      - name: Check
        run: cargo check --all-targets

      - name: Test
        run: cargo test

      - name: Clippy
        run: cargo clippy -- -D warnings

      - name: Format check
        run: cargo fmt -- --check

  # Optional: test with real API on main branch only
  integration:
    runs-on: ubuntu-latest
    if: github.ref == 'refs/heads/main'
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - name: Integration tests
        env:
          VOYAGE_API_KEY: ${{ secrets.VOYAGE_API_KEY }}
        run: cargo test -- --ignored
```

---

## Test Coverage Goals

| Module | Target Coverage | Why |
|--------|-----------------|-----|
| `chunker/treesitter` | 90%+ | Core functionality, edge cases matter |
| `search/rrf` | 100% | Pure function, easy to test exhaustively |
| `mcp/protocol` | 100% | Serde correctness is critical |
| `mcp/handler` | 90%+ | Every method dispatch must be tested |
| `indexer/hasher` | 100% | Correctness critical for incremental indexing |
| `store/tantivy_store` | 80%+ | CRUD operations + search |
| `store/lance` | 70%+ | Depends on LanceDB SDK stability |
| `embeddings/voyage` | 50%+ | API client, mostly tested via integration tests |

Don't chase 100% coverage on everything. Test the critical paths and edge cases.
The MCP protocol tests are the most important — if the protocol is wrong,
nothing else matters.
