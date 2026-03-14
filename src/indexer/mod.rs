pub mod hasher;

use crate::chunker::Chunker;
use crate::config::LuminaConfig;
use crate::embeddings::Embedder;
use crate::error::Result;
use crate::indexer::hasher::{make_relative, FileHasher};
use crate::store::{KeywordStore, VectorStore};
use crate::types::{Chunk, IndexStats};
use ignore::WalkBuilder;
use rayon::prelude::*;
use std::path::PathBuf;
use std::time::Instant;
use tracing::{debug, info, warn};

/// Directories to always skip, even if not in .gitignore.
const SKIP_DIRS: &[&str] = &[
    "node_modules", "target", "__pycache__", ".git", ".svn",
    ".hg", "vendor", "dist", "build", ".next", ".nuxt",
    "venv", ".venv", "env", ".tox", "coverage", ".lumina",
];

pub struct Indexer {
    chunker: Box<dyn Chunker>,
    embedder: Box<dyn Embedder>,
    vector_store: Box<dyn VectorStore>,
    keyword_store: Box<dyn KeywordStore>,
    hasher: FileHasher,
    config: LuminaConfig,
}

impl Indexer {
    pub fn new(
        chunker: Box<dyn Chunker>,
        embedder: Box<dyn Embedder>,
        vector_store: Box<dyn VectorStore>,
        keyword_store: Box<dyn KeywordStore>,
        config: LuminaConfig,
    ) -> Result<Self> {
        let hasher = FileHasher::new(config.hashes_path())?;
        Ok(Self {
            chunker,
            embedder,
            vector_store,
            keyword_store,
            hasher,
            config,
        })
    }

    /// Run the full indexing pipeline.
    pub fn index(&mut self) -> Result<IndexStats> {
        let start = Instant::now();
        let mut stats = IndexStats::default();

        // 1. Walk repo, collect supported files
        let files = self.walk_repo();
        stats.files_scanned = files.len();
        info!("Found {} files to consider", files.len());

        // 2. Filter unchanged files
        let changed_files = self.filter_changed_files(files, &mut stats);
        info!(
            "{} files changed, {} unchanged",
            changed_files.len(),
            stats.files_unchanged
        );

        if changed_files.is_empty() {
            stats.duration = start.elapsed();
            stats.chunks_total = self.vector_store.count().unwrap_or(0);
            info!("Nothing to index. {}", stats);
            return Ok(stats);
        }

        // 3. Delete old data for changed files
        for (rel_path, _) in &changed_files {
            let _ = self.vector_store.delete_by_file(rel_path);
            let _ = self.keyword_store.delete_by_file(rel_path);
        }

        // 4. Parse changed files with tree-sitter (parallel via rayon)
        let chunks = self.parse_files(&changed_files);
        info!("Parsed {} chunks from {} files", chunks.len(), changed_files.len());

        if chunks.is_empty() {
            stats.duration = start.elapsed();
            stats.chunks_total = self.vector_store.count().unwrap_or(0);
            return Ok(stats);
        }

        // 5. Embed chunks via Voyage API (sequential, batched)
        let mut chunks = chunks;
        self.embed_chunks(&mut chunks, &mut stats)?;

        // 6. Upsert to stores
        self.store_chunks(&chunks)?;

        // 7. Update hash cache for changed files
        for (rel_path, content) in &changed_files {
            self.hasher.update(rel_path, content.as_bytes());
        }
        self.hasher.save()?;

        stats.chunks_total = self.vector_store.count().unwrap_or(0);
        stats.duration = start.elapsed();

        info!("{}", stats);
        Ok(stats)
    }

    /// Walk the repository and collect all indexable file paths.
    fn walk_repo(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();

        let walker = WalkBuilder::new(&self.config.repo_root)
            .hidden(true) // respect hidden files
            .git_ignore(true) // respect .gitignore
            .git_global(true)
            .git_exclude(true)
            .build();

        for entry in walker.flatten() {
            let path = entry.path().to_path_buf();

            // Skip directories
            if path.is_dir() {
                continue;
            }

            // Skip files in skip directories
            if path.components().any(|c| {
                SKIP_DIRS.contains(&c.as_os_str().to_str().unwrap_or(""))
            }) {
                continue;
            }

            // Check extension is supported
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");
            if !self.chunker.supports(ext) {
                continue;
            }

            // Check file size
            if let Ok(meta) = std::fs::metadata(&path) {
                if meta.len() > self.config.max_file_size {
                    debug!("Skipping large file: {} ({} bytes)", path.display(), meta.len());
                    continue;
                }
            }

            files.push(path);
        }

        files
    }

    /// Read file content and check if it changed since last index.
    fn filter_changed_files(
        &self,
        files: Vec<PathBuf>,
        stats: &mut IndexStats,
    ) -> Vec<(String, String)> {
        let mut changed = Vec::new();

        for path in files {
            let content = match std::fs::read(&path) {
                Ok(bytes) => bytes,
                Err(e) => {
                    warn!("Failed to read {}: {}", path.display(), e);
                    stats.files_skipped += 1;
                    continue;
                }
            };

            // Skip binary files (null bytes in first 8KB)
            if is_binary(&content) {
                stats.files_skipped += 1;
                continue;
            }

            let content_str = match String::from_utf8(content) {
                Ok(s) => s,
                Err(_) => {
                    stats.files_skipped += 1;
                    continue;
                }
            };

            let rel_path = make_relative(&path, &self.config.repo_root);

            if self.hasher.has_changed(&rel_path, content_str.as_bytes()) {
                stats.files_changed += 1;
                changed.push((rel_path, content_str));
            } else {
                stats.files_unchanged += 1;
            }
        }

        changed
    }

    /// Parse and chunk files using rayon for parallelism.
    fn parse_files(&self, files: &[(String, String)]) -> Vec<Chunk> {
        files
            .par_iter()
            .flat_map(|(rel_path, content)| {
                let path = std::path::Path::new(rel_path);
                match self.chunker.chunk_file(path, content) {
                    Ok(chunks) => chunks,
                    Err(e) => {
                        warn!("Failed to chunk {}: {}", rel_path, e);
                        Vec::new()
                    }
                }
            })
            .collect()
    }

    /// Embed chunks that need new embeddings.
    fn embed_chunks(
        &self,
        chunks: &mut [Chunk],
        stats: &mut IndexStats,
    ) -> Result<()> {
        // Collect texts for embedding
        let texts: Vec<&str> = chunks.iter().map(|c| c.text.as_str()).collect();

        if texts.is_empty() {
            return Ok(());
        }

        info!("Embedding {} chunks...", texts.len());
        let embeddings = self.embedder.embed_texts(&texts)?;

        stats.chunks_embedded = embeddings.len();
        stats.embedding_api_calls = (texts.len() + self.config.embedding_batch_size - 1)
            / self.config.embedding_batch_size;

        // Assign embeddings to chunks
        for (chunk, embedding) in chunks.iter_mut().zip(embeddings.into_iter()) {
            chunk.embedding = Some(embedding);
        }

        Ok(())
    }

    /// Store chunks in both vector and keyword stores.
    fn store_chunks(&self, chunks: &[Chunk]) -> Result<()> {
        // Only chunks with embeddings go to vector store
        let with_embeddings: Vec<&Chunk> = chunks
            .iter()
            .filter(|c| c.embedding.is_some())
            .collect();

        if !with_embeddings.is_empty() {
            // Need owned chunks for the trait
            let owned: Vec<Chunk> = with_embeddings.into_iter().cloned().collect();
            self.vector_store.upsert(&owned)?;
        }

        // All chunks go to keyword store (no embedding needed)
        self.keyword_store.upsert(chunks)?;

        Ok(())
    }
}

/// Check if content appears to be binary (contains null bytes in first 8KB).
fn is_binary(content: &[u8]) -> bool {
    content.iter().take(8192).any(|&b| b == 0)
}
