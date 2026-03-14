use std::path::Path;
use std::sync::Arc;

use lancedb::connect;
use lancedb::query::{ExecutableQuery, QueryBase};
use arrow_array::{
    FixedSizeListArray, Float32Array, RecordBatch, RecordBatchIterator,
    StringArray, UInt32Array,
};
use arrow_schema::{DataType, Field, Schema as ArrowSchema};

use crate::error::{LuminaError, Result};
use crate::types::{Chunk, SearchResult, SearchSource, SymbolKind};
use super::VectorStore;

const TABLE_NAME: &str = "chunks";
const EMBEDDING_DIM: i32 = 1024;

pub struct LanceStore {
    rt: tokio::runtime::Runtime,
    db: lancedb::Connection,
}

impl LanceStore {
    pub fn new(db_path: &Path) -> Result<Self> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| LuminaError::VectorStoreError(e.to_string()))?;

        let db_path_str = db_path.to_string_lossy().to_string();

        let db = rt.block_on(async {
            connect(&db_path_str)
                .execute()
                .await
                .map_err(|e| LuminaError::VectorStoreError(e.to_string()))
        })?;

        Ok(Self { rt, db })
    }

    fn arrow_schema() -> Arc<ArrowSchema> {
        Arc::new(ArrowSchema::new(vec![
            Field::new("chunk_id", DataType::Utf8, false),
            Field::new("file", DataType::Utf8, false),
            Field::new("symbol", DataType::Utf8, false),
            Field::new("kind", DataType::Utf8, false),
            Field::new("start_line", DataType::UInt32, false),
            Field::new("end_line", DataType::UInt32, false),
            Field::new("language", DataType::Utf8, false),
            Field::new("text", DataType::Utf8, false),
            Field::new(
                "vector",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    EMBEDDING_DIM,
                ),
                false,
            ),
        ]))
    }

    fn chunks_to_batch(chunks: &[Chunk]) -> Result<RecordBatch> {
        let schema = Self::arrow_schema();

        let chunk_ids: Vec<&str> = chunks.iter().map(|c| c.id.as_str()).collect();
        let files: Vec<&str> = chunks.iter().map(|c| c.file.as_str()).collect();
        let symbols: Vec<&str> = chunks.iter().map(|c| c.symbol.as_str()).collect();
        let kinds: Vec<&str> = chunks.iter().map(|c| c.kind.as_str()).collect();
        let start_lines: Vec<u32> = chunks.iter().map(|c| c.start_line).collect();
        let end_lines: Vec<u32> = chunks.iter().map(|c| c.end_line).collect();
        let languages: Vec<&str> = chunks.iter().map(|c| c.language.as_str()).collect();
        let texts: Vec<&str> = chunks.iter().map(|c| c.text.as_str()).collect();

        // Build the embedding vectors as a FixedSizeList
        let mut all_values = Vec::new();
        for chunk in chunks {
            let emb = chunk.embedding.as_ref().ok_or_else(|| {
                LuminaError::VectorStoreError("Chunk missing embedding".to_string())
            })?;
            if emb.len() != EMBEDDING_DIM as usize {
                return Err(LuminaError::VectorStoreError(format!(
                    "Embedding dimension mismatch: expected {}, got {}",
                    EMBEDDING_DIM,
                    emb.len()
                )));
            }
            all_values.extend_from_slice(emb);
        }

        let values_array = Float32Array::from(all_values);
        let list_field = Arc::new(Field::new("item", DataType::Float32, true));
        let vector_array = FixedSizeListArray::try_new(
            list_field.into(),
            EMBEDDING_DIM,
            Arc::new(values_array),
            None,
        ).map_err(|e| LuminaError::VectorStoreError(e.to_string()))?;

        RecordBatch::try_new(
            schema,
            vec![
                Arc::new(StringArray::from(chunk_ids)),
                Arc::new(StringArray::from(files)),
                Arc::new(StringArray::from(symbols)),
                Arc::new(StringArray::from(kinds)),
                Arc::new(UInt32Array::from(start_lines)),
                Arc::new(UInt32Array::from(end_lines)),
                Arc::new(StringArray::from(languages)),
                Arc::new(StringArray::from(texts)),
                Arc::new(vector_array),
            ],
        )
        .map_err(|e| LuminaError::VectorStoreError(e.to_string()))
    }

    fn batch_to_results(batch: &RecordBatch) -> Result<Vec<SearchResult>> {
        let chunk_ids = batch
            .column_by_name("chunk_id")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>())
            .ok_or_else(|| LuminaError::VectorStoreError("Missing chunk_id column".into()))?;
        let files = batch
            .column_by_name("file")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>())
            .ok_or_else(|| LuminaError::VectorStoreError("Missing file column".into()))?;
        let symbols = batch
            .column_by_name("symbol")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>())
            .ok_or_else(|| LuminaError::VectorStoreError("Missing symbol column".into()))?;
        let kinds = batch
            .column_by_name("kind")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>())
            .ok_or_else(|| LuminaError::VectorStoreError("Missing kind column".into()))?;
        let start_lines = batch
            .column_by_name("start_line")
            .and_then(|c| c.as_any().downcast_ref::<UInt32Array>())
            .ok_or_else(|| LuminaError::VectorStoreError("Missing start_line column".into()))?;
        let end_lines = batch
            .column_by_name("end_line")
            .and_then(|c| c.as_any().downcast_ref::<UInt32Array>())
            .ok_or_else(|| LuminaError::VectorStoreError("Missing end_line column".into()))?;
        let languages = batch
            .column_by_name("language")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>())
            .ok_or_else(|| LuminaError::VectorStoreError("Missing language column".into()))?;
        let texts = batch
            .column_by_name("text")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>())
            .ok_or_else(|| LuminaError::VectorStoreError("Missing text column".into()))?;
        let distances = batch
            .column_by_name("_distance")
            .and_then(|c| c.as_any().downcast_ref::<Float32Array>());

        let mut results = Vec::new();
        for i in 0..batch.num_rows() {
            let kind_str = kinds.value(i);
            let kind = match kind_str {
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

            // Convert distance to similarity score (lower distance = higher score)
            let score = distances
                .map(|d| 1.0 / (1.0 + d.value(i)))
                .unwrap_or(0.0);

            results.push(SearchResult {
                chunk_id: chunk_ids.value(i).to_string(),
                file: files.value(i).to_string(),
                symbol: symbols.value(i).to_string(),
                kind,
                start_line: start_lines.value(i),
                end_line: end_lines.value(i),
                language: languages.value(i).to_string(),
                text: texts.value(i).to_string(),
                score,
                source: SearchSource::Vector,
            });
        }

        Ok(results)
    }
}

impl VectorStore for LanceStore {
    fn upsert(&self, chunks: &[Chunk]) -> Result<()> {
        if chunks.is_empty() {
            return Ok(());
        }

        let batch = Self::chunks_to_batch(chunks)?;
        let schema = Self::arrow_schema();

        self.rt.block_on(async {
            let table_names = self
                .db
                .table_names()
                .execute()
                .await
                .map_err(|e| LuminaError::VectorStoreError(e.to_string()))?;

            if table_names.contains(&TABLE_NAME.to_string()) {
                let table = self
                    .db
                    .open_table(TABLE_NAME)
                    .execute()
                    .await
                    .map_err(|e| LuminaError::VectorStoreError(e.to_string()))?;

                let batches = RecordBatchIterator::new(
                    vec![Ok(batch)],
                    schema,
                );

                table
                    .add(Box::new(batches))
                    .execute()
                    .await
                    .map_err(|e| LuminaError::VectorStoreError(e.to_string()))?;
            } else {
                let batches = RecordBatchIterator::new(
                    vec![Ok(batch)],
                    schema,
                );

                self.db
                    .create_table(TABLE_NAME, Box::new(batches))
                    .execute()
                    .await
                    .map_err(|e| LuminaError::VectorStoreError(e.to_string()))?;
            }

            Ok(())
        })
    }

    fn search(&self, embedding: &[f32], limit: usize) -> Result<Vec<SearchResult>> {
        self.rt.block_on(async {
            let table_names = self
                .db
                .table_names()
                .execute()
                .await
                .map_err(|e| LuminaError::VectorStoreError(e.to_string()))?;

            if !table_names.contains(&TABLE_NAME.to_string()) {
                return Ok(Vec::new());
            }

            let table = self
                .db
                .open_table(TABLE_NAME)
                .execute()
                .await
                .map_err(|e| LuminaError::VectorStoreError(e.to_string()))?;

            let results = table
                .vector_search(embedding)
                .map_err(|e| LuminaError::VectorStoreError(e.to_string()))?
                .limit(limit)
                .execute()
                .await
                .map_err(|e| LuminaError::VectorStoreError(e.to_string()))?;

            use futures::TryStreamExt;
            let batches: Vec<RecordBatch> = results
                .try_collect()
                .await
                .map_err(|e| LuminaError::VectorStoreError(e.to_string()))?;

            let mut all_results = Vec::new();
            for batch in &batches {
                all_results.extend(Self::batch_to_results(batch)?);
            }

            Ok(all_results)
        })
    }

    fn delete_by_file(&self, file_path: &str) -> Result<()> {
        let predicate = format!("file = '{}'", file_path.replace('\'', "''"));

        self.rt.block_on(async {
            let table_names = self
                .db
                .table_names()
                .execute()
                .await
                .map_err(|e| LuminaError::VectorStoreError(e.to_string()))?;

            if !table_names.contains(&TABLE_NAME.to_string()) {
                return Ok(());
            }

            let table = self
                .db
                .open_table(TABLE_NAME)
                .execute()
                .await
                .map_err(|e| LuminaError::VectorStoreError(e.to_string()))?;

            table
                .delete(&predicate)
                .await
                .map_err(|e| LuminaError::VectorStoreError(e.to_string()))?;

            Ok(())
        })
    }

    fn count(&self) -> Result<usize> {
        self.rt.block_on(async {
            let table_names = self
                .db
                .table_names()
                .execute()
                .await
                .map_err(|e| LuminaError::VectorStoreError(e.to_string()))?;

            if !table_names.contains(&TABLE_NAME.to_string()) {
                return Ok(0);
            }

            let table = self
                .db
                .open_table(TABLE_NAME)
                .execute()
                .await
                .map_err(|e| LuminaError::VectorStoreError(e.to_string()))?;

            let count = table
                .count_rows(None)
                .await
                .map_err(|e| LuminaError::VectorStoreError(e.to_string()))?;

            Ok(count)
        })
    }
}
