use lumina::config::LuminaConfig;
use lumina::error::LuminaError;
use lumina::types::{Chunk, SymbolKind};
use std::path::PathBuf;

#[test]
fn test_error_display() {
    let err = LuminaError::IndexNotFound {
        path: PathBuf::from("/tmp/test"),
    };
    assert_eq!(
        err.to_string(),
        "Index not found at /tmp/test. Run `lumina index` first."
    );
}

#[test]
fn test_chunk_serialization() {
    let chunk = Chunk {
        id: "123".to_string(),
        file: "src/main.rs".to_string(),
        symbol: "main".to_string(),
        kind: SymbolKind::Function,
        start_line: 1,
        end_line: 10,
        language: "rust".to_string(),
        text: "fn main() {}".to_string(),
        embedding: None,
    };

    let json = serde_json::to_string(&chunk).unwrap();
    let deserialized: Chunk = serde_json::from_str(&json).unwrap();

    assert_eq!(chunk.id, deserialized.id);
    assert_eq!(chunk.file, deserialized.file);
    assert_eq!(chunk.kind, deserialized.kind);
    assert_eq!(chunk.start_line, deserialized.start_line);
}

#[test]
fn test_config_overrides() {
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join(".lumina");
    std::fs::create_dir(&data_dir).unwrap();

    let config_path = data_dir.join("config.toml");
    std::fs::write(&config_path, "max_chunk_tokens = 999\n").unwrap();

    let config = LuminaConfig::load(tmp.path().to_path_buf()).unwrap();
    
    assert_eq!(config.max_chunk_tokens, 999);
    assert_eq!(config.min_chunk_tokens, 50); // Default preserved
}
