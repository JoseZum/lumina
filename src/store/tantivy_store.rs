#![allow(dead_code)]

use std::path::Path;
use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, FuzzyTermQuery, Occur, QueryParser, RegexQuery};
use tantivy::schema::*;
use tantivy::{doc, Index, IndexWriter, ReloadPolicy};

use crate::error::{LuminaError, Result};
use crate::types::{Chunk, SearchResult, SearchSource, SymbolKind};
use super::KeywordStore;

pub struct TantivyStore {
    index: Index,
    schema: Schema,
    // Field handles
    f_chunk_id: Field,
    f_file: Field,
    f_symbol: Field,
    f_kind: Field,
    f_start_line: Field,
    f_end_line: Field,
    f_language: Field,
    f_text: Field,
}

impl TantivyStore {
    pub fn new(index_path: &Path) -> Result<Self> {
        let mut schema_builder = Schema::builder();

        let f_chunk_id = schema_builder.add_text_field("chunk_id", STRING | STORED);
        let f_file = schema_builder.add_text_field("file", STRING | STORED);
        let f_symbol = schema_builder.add_text_field("symbol", TEXT | STORED);
        let f_kind = schema_builder.add_text_field("kind", STRING | STORED);
        let f_start_line = schema_builder.add_u64_field("start_line", INDEXED | STORED);
        let f_end_line = schema_builder.add_u64_field("end_line", INDEXED | STORED);
        let f_language = schema_builder.add_text_field("language", STRING | STORED);
        let f_text = schema_builder.add_text_field("text", TEXT | STORED);

        let schema = schema_builder.build();

        let index = if index_path.exists() {
            Index::open_in_dir(index_path)
                .map_err(|e| LuminaError::KeywordStoreError(e.to_string()))?
        } else {
            std::fs::create_dir_all(index_path)?;
            Index::create_in_dir(index_path, schema.clone())
                .map_err(|e| LuminaError::KeywordStoreError(e.to_string()))?
        };

        Ok(Self {
            index,
            schema,
            f_chunk_id,
            f_file,
            f_symbol,
            f_kind,
            f_start_line,
            f_end_line,
            f_language,
            f_text,
        })
    }

    fn make_writer(&self) -> Result<IndexWriter> {
        self.index
            .writer(50_000_000) // 50MB heap
            .map_err(|e| LuminaError::KeywordStoreError(e.to_string()))
    }

    fn extract_result(&self, doc: &TantivyDocument, score: f32) -> SearchResult {
        let get_text = |field: Field| -> String {
            doc.get_first(field)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        };

        let get_u64 = |field: Field| -> u64 {
            doc.get_first(field)
                .and_then(|v| v.as_u64())
                .unwrap_or(0)
        };

        let kind_str = get_text(self.f_kind);
        let kind = match kind_str.as_str() {
            "function" => SymbolKind::Function,
            "method" => SymbolKind::Method,
            "class" => SymbolKind::Class,
            "struct" => SymbolKind::Struct,
            "enum" => SymbolKind::Enum,
            "trait" => SymbolKind::Trait,
            "impl" => SymbolKind::Impl,
            "interface" => SymbolKind::Interface,
            "module" => SymbolKind::Module,
            "constant" => SymbolKind::Constant,
            "type_alias" => SymbolKind::TypeAlias,
            _ => SymbolKind::TopLevel,
        };

        SearchResult {
            chunk_id: get_text(self.f_chunk_id),
            file: get_text(self.f_file),
            symbol: get_text(self.f_symbol),
            kind,
            start_line: get_u64(self.f_start_line) as u32,
            end_line: get_u64(self.f_end_line) as u32,
            language: get_text(self.f_language),
            text: get_text(self.f_text),
            score,
            source: SearchSource::Keyword,
        }
    }
}

impl KeywordStore for TantivyStore {
    fn upsert(&self, chunks: &[Chunk]) -> Result<()> {
        let mut writer = self.make_writer()?;

        for chunk in chunks {
            // Delete existing document with same chunk_id
            let term = tantivy::Term::from_field_text(self.f_chunk_id, &chunk.id);
            writer.delete_term(term);

            writer.add_document(doc!(
                self.f_chunk_id => chunk.id.clone(),
                self.f_file => chunk.file.clone(),
                self.f_symbol => chunk.symbol.clone(),
                self.f_kind => chunk.kind.as_str().to_string(),
                self.f_start_line => chunk.start_line as u64,
                self.f_end_line => chunk.end_line as u64,
                self.f_language => chunk.language.clone(),
                self.f_text => chunk.text.clone(),
            )).map_err(|e| LuminaError::KeywordStoreError(e.to_string()))?;
        }

        writer
            .commit()
            .map_err(|e| LuminaError::KeywordStoreError(e.to_string()))?;
        Ok(())
    }

    fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let reader = self
            .index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()
            .map_err(|e: tantivy::TantivyError| LuminaError::KeywordStoreError(e.to_string()))?;

        let searcher = reader.searcher();
        let query_parser = QueryParser::for_index(&self.index, vec![self.f_text, self.f_symbol]);
        let parsed_query = query_parser
            .parse_query(query)
            .map_err(|e| LuminaError::KeywordStoreError(e.to_string()))?;

        let top_docs = searcher
            .search(&parsed_query, &TopDocs::with_limit(limit))
            .map_err(|e| LuminaError::KeywordStoreError(e.to_string()))?;

        let mut results = Vec::new();
        for (score, doc_address) in top_docs {
            let doc: TantivyDocument = searcher
                .doc(doc_address)
                .map_err(|e| LuminaError::KeywordStoreError(e.to_string()))?;
            results.push(self.extract_result(&doc, score));
        }

        Ok(results)
    }

    fn search_filtered(&self, query: &str, limit: usize, file_prefix: &str) -> Result<Vec<SearchResult>> {
        let reader = self
            .index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()
            .map_err(|e: tantivy::TantivyError| LuminaError::KeywordStoreError(e.to_string()))?;

        let searcher = reader.searcher();
        let query_parser = QueryParser::for_index(&self.index, vec![self.f_text, self.f_symbol]);
        let text_query = query_parser
            .parse_query(query)
            .map_err(|e| LuminaError::KeywordStoreError(e.to_string()))?;

        // Escape regex metacharacters in file prefix, then match prefix or prefix/
        let escaped = file_prefix
            .replace('\\', "\\\\")
            .replace('.', "\\.")
            .replace('+', "\\+")
            .replace('*', "\\*")
            .replace('?', "\\?")
            .replace('(', "\\(")
            .replace(')', "\\)")
            .replace('[', "\\[")
            .replace(']', "\\]")
            .replace('{', "\\{")
            .replace('}', "\\}")
            .replace('^', "\\^")
            .replace('$', "\\$")
            .replace('|', "\\|");
        let pattern = format!("{}(/.*)?$", escaped);
        let file_query = RegexQuery::from_pattern(&pattern, self.f_file)
            .map_err(|e| LuminaError::KeywordStoreError(e.to_string()))?;

        let combined = BooleanQuery::new(vec![
            (Occur::Must, text_query),
            (Occur::Must, Box::new(file_query)),
        ]);

        let top_docs = searcher
            .search(&combined, &TopDocs::with_limit(limit))
            .map_err(|e| LuminaError::KeywordStoreError(e.to_string()))?;

        let mut results = Vec::new();
        for (score, doc_address) in top_docs {
            let doc: TantivyDocument = searcher
                .doc(doc_address)
                .map_err(|e| LuminaError::KeywordStoreError(e.to_string()))?;
            results.push(self.extract_result(&doc, score));
        }

        Ok(results)
    }

    fn search_symbol(&self, symbol_name: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let reader = self
            .index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()
            .map_err(|e: tantivy::TantivyError| LuminaError::KeywordStoreError(e.to_string()))?;

        let searcher = reader.searcher();
        let term = tantivy::Term::from_field_text(self.f_symbol, symbol_name);
        let query = FuzzyTermQuery::new(term, 1, true);

        let top_docs = searcher
            .search(&query, &TopDocs::with_limit(limit))
            .map_err(|e| LuminaError::KeywordStoreError(e.to_string()))?;

        let mut results = Vec::new();
        for (score, doc_address) in top_docs {
            let doc: TantivyDocument = searcher
                .doc(doc_address)
                .map_err(|e| LuminaError::KeywordStoreError(e.to_string()))?;
            results.push(self.extract_result(&doc, score));
        }

        Ok(results)
    }

    fn delete_by_file(&self, file_path: &str) -> Result<()> {
        let mut writer = self.make_writer()?;
        let term = tantivy::Term::from_field_text(self.f_file, file_path);
        writer.delete_term(term);
        writer
            .commit()
            .map_err(|e| LuminaError::KeywordStoreError(e.to_string()))?;
        Ok(())
    }

    fn list_files(&self) -> Result<Vec<String>> {
        let reader = self
            .index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()
            .map_err(|e: tantivy::TantivyError| LuminaError::KeywordStoreError(e.to_string()))?;

        let searcher = reader.searcher();
        let mut files = std::collections::HashSet::new();

        // Iterate all segments and all docs to collect unique file paths
        for segment_reader in searcher.segment_readers() {
            let store_reader = segment_reader
                .get_store_reader(1)
                .map_err(|e| LuminaError::KeywordStoreError(e.to_string()))?;
            
            for doc_id in 0..segment_reader.max_doc() {
                if segment_reader.is_deleted(doc_id) {
                    continue;
                }
                let doc: TantivyDocument = store_reader
                    .get(doc_id)
                    .map_err(|e| LuminaError::KeywordStoreError(e.to_string()))?;
                if let Some(val) = doc.get_first(self.f_file) {
                    if let Some(s) = val.as_str() {
                        files.insert(s.to_string());
                    }
                }
            }
        }

        Ok(files.into_iter().collect())
    }

    fn count(&self) -> Result<usize> {
        let reader = self
            .index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()
            .map_err(|e: tantivy::TantivyError| LuminaError::KeywordStoreError(e.to_string()))?;

        let searcher = reader.searcher();
        let mut total = 0u64;
        for segment in searcher.segment_readers() {
            total += segment.num_docs() as u64;
        }
        Ok(total as usize)
    }
}
