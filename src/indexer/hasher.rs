use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::Result;

/// Manages SHA-256 hashes of file contents for incremental indexing.
/// Tracks which files have changed since the last indexing run.
pub struct FileHasher {
    /// Map of file path (relative to repo root) → SHA-256 hex hash
    hashes: HashMap<String, String>,
    /// Path to the serialized hash cache file
    cache_path: PathBuf,
}

impl FileHasher {
    /// Create a new FileHasher, loading any existing cache from disk.
    pub fn new(cache_path: PathBuf) -> Result<Self> {
        let hashes = if cache_path.exists() {
            let data = fs::read(&cache_path)?;
            bincode::deserialize(&data).unwrap_or_default()
        } else {
            HashMap::new()
        };

        Ok(Self { hashes, cache_path })
    }

    /// Hash the contents of a file. Returns the SHA-256 hex digest.
    pub fn hash_content(content: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content);
        hex::encode(hasher.finalize())
    }

    /// Check if a file has changed since the last index.
    /// Returns `true` if the file is new or its content hash differs.
    pub fn has_changed(&self, relative_path: &str, content: &[u8]) -> bool {
        let new_hash = Self::hash_content(content);
        match self.hashes.get(relative_path) {
            Some(old_hash) => old_hash != &new_hash,
            None => true, // New file
        }
    }

    /// Update the stored hash for a file.
    pub fn update(&mut self, relative_path: &str, content: &[u8]) {
        let hash = Self::hash_content(content);
        self.hashes.insert(relative_path.to_string(), hash);
    }

    /// Remove the stored hash for a file (e.g., when file is deleted).
    pub fn remove(&mut self, relative_path: &str) {
        self.hashes.remove(relative_path);
    }

    /// Persist the hash cache to disk.
    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.cache_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let data = bincode::serialize(&self.hashes)
            .map_err(|e| crate::error::LuminaError::ConfigError(e.to_string()))?;
        fs::write(&self.cache_path, data)?;
        Ok(())
    }

    /// Get the stored hash for a file, if any.
    pub fn get_hash(&self, relative_path: &str) -> Option<&str> {
        self.hashes.get(relative_path).map(|s| s.as_str())
    }

    /// Get the number of tracked files.
    pub fn tracked_count(&self) -> usize {
        self.hashes.len()
    }

    /// Get all tracked file paths.
    pub fn tracked_files(&self) -> Vec<&str> {
        self.hashes.keys().map(|s| s.as_str()).collect()
    }

    /// Clear all tracked hashes.
    pub fn clear(&mut self) {
        self.hashes.clear();
    }
}

/// Make a relative path string from an absolute path and a repo root.
pub fn make_relative(path: &Path, repo_root: &Path) -> String {
    path.strip_prefix(repo_root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace("\\", "/")
}
