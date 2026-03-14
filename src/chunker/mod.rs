pub mod languages;
pub mod treesitter;

pub use treesitter::TreeSitterChunker;

use crate::error::Result;
use crate::types::Chunk;
use std::path::Path;

/// Core trait for splitting source files into semantic chunks
pub trait Chunker: Send + Sync {
    /// Chunk a file given its path and string content
    fn chunk_file(&self, path: &Path, content: &str) -> Result<Vec<Chunk>>;

    /// Returns a list of supported file extensions (e.g., "rs", "py", "ts")
    fn supported_extensions(&self) -> Vec<&'static str>;

    /// Check if a file extension is supported
    fn is_supported(&self, extension: &str) -> bool {
        self.supported_extensions().contains(&extension)
    }
}
