use super::{Chunker, languages::LanguageConfig};
use crate::error::{LuminaError, Result};
use crate::types::{Chunk, SymbolKind};
use sha2::{Digest, Sha256};
use std::path::Path;
use streaming_iterator::StreamingIterator;
use tree_sitter::{Parser, Query, QueryCursor};

pub struct TreeSitterChunker {
    max_tokens: usize,
    min_tokens: usize,
}

impl TreeSitterChunker {
    pub fn new(max_tokens: usize, min_tokens: usize) -> Self {
        Self {
            max_tokens,
            min_tokens,
        }
    }

    fn calculate_id(file: &str, start_line: u32, text: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(file.as_bytes());
        hasher.update(b":");
        hasher.update(start_line.to_string().as_bytes());
        hasher.update(b":");
        hasher.update(text.as_bytes());
        hex::encode(hasher.finalize())
    }

    fn clean_text(text: &str) -> String {
        text.lines()
            .map(|l| l.trim_end()) // Remove trailing whitespace but preserve indentation
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string()
    }

    /// Split an oversized chunk into sub-chunks by line boundaries with overlap.
    fn split_chunk(
        &self,
        file: &str,
        symbol: &str,
        kind: SymbolKind,
        text: &str,
        start_line: u32,
        language: &str,
    ) -> Vec<Chunk> {
        let lines: Vec<&str> = text.lines().collect();
        let target_chars = self.max_tokens * 4; // ~max_tokens tokens
        let overlap_lines = 3;
        let mut chunks = Vec::new();
        let mut part = 1;
        let mut i = 0;

        while i < lines.len() {
            let mut end = i;
            let mut char_count = 0;
            while end < lines.len() && char_count < target_chars {
                char_count += lines[end].len() + 1;
                end += 1;
            }

            let chunk_text = lines[i..end].join("\n");
            let chunk_start = start_line + i as u32;
            let chunk_end = start_line + (end - 1) as u32;
            let part_symbol = if lines.len() > end - i {
                format!("{} (part {})", symbol, part)
            } else {
                symbol.to_string()
            };

            chunks.push(Chunk {
                id: Self::calculate_id(file, chunk_start, &chunk_text),
                file: file.to_string(),
                symbol: part_symbol,
                kind,
                start_line: chunk_start,
                end_line: chunk_end,
                language: language.to_string(),
                text: chunk_text,
                embedding: None,
            });

            i = if end >= lines.len() { end } else { end.saturating_sub(overlap_lines) };
            part += 1;
        }
        chunks
    }
}

impl Chunker for TreeSitterChunker {
    fn supported_extensions(&self) -> Vec<&'static str> {
        vec!["rs", "py", "js", "jsx", "ts", "tsx", "go", "java"]
    }

    fn chunk_file(&self, path: &Path, content: &str) -> Result<Vec<Chunk>> {
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let config = LanguageConfig::get(extension).ok_or_else(|| {
            LuminaError::UnsupportedLanguage {
                extension: extension.to_string(),
            }
        })?;

        let mut parser = Parser::new();
        parser
            .set_language(&config.language)
            .map_err(|e| LuminaError::ParseError {
                file: path.to_string_lossy().to_string(),
                reason: e.to_string(),
            })?;

        let tree = parser
            .parse(content, None)
            .ok_or_else(|| LuminaError::ParseError {
                file: path.to_string_lossy().to_string(),
                reason: "Tree-sitter failed to parse file".to_string(),
            })?;

        let query = Query::new(&config.language, config.query).map_err(|e| {
            LuminaError::QueryError {
                language: config.name.to_string(),
                reason: e.to_string(),
            }
        })?;

        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, tree.root_node(), content.as_bytes());

        let mut chunks = Vec::new();
        let file_path = path.to_string_lossy().replace("\\", "/");

        // Simple extraction: one chunk per captured node
        while let Some(m) = matches.next() {
            // Find the @name capture and the block capture
            let mut name_text = String::new();
            let mut block_node = None;
            let mut kind = SymbolKind::TopLevel;

            for index in 0..m.captures.len() {
                let capture = m.captures[index];
                let capture_name: &str = &query.capture_names()[capture.index as usize];

                if capture_name == "name" {
                    name_text = capture
                        .node
                        .utf8_text(content.as_bytes())
                        .unwrap_or("")
                        .to_string();
                } else {
                    // It's the block itself (@function, @class, etc.)
                    block_node = Some(capture.node);
                    kind = match capture_name {
                        "function" => SymbolKind::Function,
                        "method" => SymbolKind::Method,
                        "class" => SymbolKind::Class,
                        "struct" => SymbolKind::Struct,
                        "enum" => SymbolKind::Enum,
                        "trait" => SymbolKind::Trait,
                        "impl" => SymbolKind::Impl,
                        "interface" => SymbolKind::Interface,
                        "type_alias" => SymbolKind::TypeAlias,
                        _ => SymbolKind::TopLevel,
                    };
                }
            }

            if let Some(node) = block_node {
                let text = node.utf8_text(content.as_bytes()).unwrap_or("");
                let clean_text = Self::clean_text(text);

                // Very basic token count estimate (characters / 4)
                let estimated_tokens = (clean_text.len() + 3) / 4;

                let start_line = (node.start_position().row + 1) as u32;

                if estimated_tokens >= self.min_tokens && estimated_tokens <= self.max_tokens {
                    chunks.push(Chunk {
                        id: Self::calculate_id(&file_path, start_line, &clean_text),
                        file: file_path.clone(),
                        symbol: name_text,
                        kind,
                        start_line,
                        end_line: (node.end_position().row + 1) as u32,
                        language: config.name.to_string(),
                        text: clean_text,
                        embedding: None,
                    });
                } else if estimated_tokens > self.max_tokens {
                    chunks.extend(self.split_chunk(
                        &file_path, &name_text, kind, &clean_text,
                        start_line, config.name,
                    ));
                }
                // Chunks below min_tokens are skipped (too small to be useful)
            }
        }

        // Add top-level fallback if no chunks were found but file has content
        if chunks.is_empty() && !content.trim().is_empty() {
            let clean_text = Self::clean_text(content);
            let estimated_tokens = (clean_text.len() + 3) / 4;
            
            if estimated_tokens <= self.max_tokens {
                chunks.push(Chunk {
                    id: Self::calculate_id(&file_path, 1, &clean_text),
                    file: file_path,
                    symbol: String::new(),
                    kind: SymbolKind::TopLevel,
                    start_line: 1,
                    end_line: (content.lines().count() as u32).max(1),
                    language: config.name.to_string(),
                    text: clean_text,
                    embedding: None,
                });
            } else {
                chunks.extend(self.split_chunk(
                    &file_path, "", SymbolKind::TopLevel, &clean_text,
                    1, config.name,
                ));
            }
        }

        Ok(chunks)
    }
}
