use lumina::chunker::{Chunker, TreeSitterChunker};
use lumina::types::SymbolKind;
use std::path::Path;

#[test]
fn test_rust_chunking() {
    let chunker = TreeSitterChunker::new(500, 10);
    let code = r#"
pub fn hello_world() {
    println!("Hello, world!");
}

pub struct Config {
    pub name: String,
    pub value: i32,
}

impl Config {
    pub fn new(name: &str, value: i32) -> Self {
        Self {
            name: name.to_string(),
            value,
        }
    }
}
"#;
    let chunks = chunker.chunk_file(Path::new("test.rs"), code).unwrap();
    assert!(!chunks.is_empty(), "Should produce at least one chunk");

    // Check that all chunks have the correct language
    for chunk in &chunks {
        assert_eq!(chunk.language, "rust");
    }
}

#[test]
fn test_python_chunking() {
    let chunker = TreeSitterChunker::new(500, 10);
    let code = r#"
class UserService:
    def __init__(self, db):
        self.db = db

    def get_user(self, user_id):
        return self.db.find(user_id)

def authenticate(token):
    if not token:
        return False
    return verify_jwt(token)
"#;
    let chunks = chunker.chunk_file(Path::new("test.py"), code).unwrap();
    assert!(!chunks.is_empty(), "Should produce at least one chunk");

    for chunk in &chunks {
        assert_eq!(chunk.language, "python");
    }
}

#[test]
fn test_unsupported_extension() {
    let chunker = TreeSitterChunker::new(500, 10);
    let result = chunker.chunk_file(Path::new("test.txt"), "hello");
    assert!(result.is_err(), "Should fail for unsupported extension");
}

#[test]
fn test_chunk_id_deterministic() {
    let chunker = TreeSitterChunker::new(500, 10);
    let code = r#"
pub fn hello() {
    println!("hello");
}
"#;
    let chunks1 = chunker.chunk_file(Path::new("a.rs"), code).unwrap();
    let chunks2 = chunker.chunk_file(Path::new("b.rs"), code).unwrap();

    // Same code content should produce same chunk ID regardless of file path
    if !chunks1.is_empty() && !chunks2.is_empty() {
        assert_eq!(chunks1[0].id, chunks2[0].id);
    }
}

#[test]
fn test_supported_extensions() {
    let chunker = TreeSitterChunker::new(500, 10);
    let exts = chunker.supported_extensions();
    assert!(exts.contains(&"rs"));
    assert!(exts.contains(&"py"));
    assert!(exts.contains(&"ts"));
    assert!(exts.contains(&"js"));
    assert!(exts.contains(&"go"));
    assert!(exts.contains(&"java"));
    assert!(!exts.contains(&"txt"));
}
