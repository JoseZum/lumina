use crate::types::{SearchResult, SearchSource};
use std::collections::HashMap;

/// Merge two ranked result lists using Reciprocal Rank Fusion.
///
/// RRF formula: score(d) = Σ 1/(k + rank_i(d))
///
/// Documents appearing in both lists get boosted. Does not require
/// score calibration between lists.
pub fn rrf_merge(
    vector_results: Vec<SearchResult>,
    keyword_results: Vec<SearchResult>,
    k: u32,
) -> Vec<SearchResult> {
    let mut scores: HashMap<String, f32> = HashMap::new();
    let mut results_by_id: HashMap<String, SearchResult> = HashMap::new();

    // Score vector results
    for (rank, result) in vector_results.into_iter().enumerate() {
        let rrf_score = 1.0 / (k as f32 + (rank + 1) as f32);
        *scores.entry(result.chunk_id.clone()).or_insert(0.0) += rrf_score;
        results_by_id.entry(result.chunk_id.clone()).or_insert(result);
    }

    // Score keyword results
    for (rank, result) in keyword_results.into_iter().enumerate() {
        let rrf_score = 1.0 / (k as f32 + (rank + 1) as f32);
        *scores.entry(result.chunk_id.clone()).or_insert(0.0) += rrf_score;
        results_by_id.entry(result.chunk_id.clone()).or_insert(result);
    }

    // Build final list, sorted by RRF score descending
    let mut merged: Vec<SearchResult> = scores
        .into_iter()
        .filter_map(|(id, score)| {
            results_by_id.remove(&id).map(|mut r| {
                r.score = score;
                r.source = SearchSource::Fused;
                r
            })
        })
        .collect();

    merged.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SymbolKind;

    fn make_result(id: &str, score: f32, source: SearchSource) -> SearchResult {
        SearchResult {
            chunk_id: id.to_string(),
            file: "test.rs".to_string(),
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

        let merged = rrf_merge(vector, keyword, 60);
        assert_eq!(merged.len(), 4);
        // All items should be present
        let ids: Vec<&str> = merged.iter().map(|r| r.chunk_id.as_str()).collect();
        assert!(ids.contains(&"a"));
        assert!(ids.contains(&"b"));
        assert!(ids.contains(&"c"));
        assert!(ids.contains(&"d"));
    }

    #[test]
    fn test_rrf_overlap_boosts() {
        let vector = vec![
            make_result("a", 0.9, SearchSource::Vector),
            make_result("b", 0.8, SearchSource::Vector),
        ];
        let keyword = vec![
            make_result("a", 5.0, SearchSource::Keyword),
            make_result("c", 4.0, SearchSource::Keyword),
        ];

        let merged = rrf_merge(vector, keyword, 60);
        // "a" appears in both lists, should have highest RRF score
        assert_eq!(merged[0].chunk_id, "a");
        assert!(merged[0].score > merged[1].score);
    }

    #[test]
    fn test_rrf_empty_inputs() {
        let merged = rrf_merge(Vec::new(), Vec::new(), 60);
        assert!(merged.is_empty());

        let vector = vec![make_result("a", 0.9, SearchSource::Vector)];
        let merged = rrf_merge(vector, Vec::new(), 60);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].chunk_id, "a");
    }

    #[test]
    fn test_rrf_all_fused_source() {
        let vector = vec![make_result("a", 0.9, SearchSource::Vector)];
        let keyword = vec![make_result("b", 5.0, SearchSource::Keyword)];

        let merged = rrf_merge(vector, keyword, 60);
        for r in &merged {
            assert_eq!(r.source, SearchSource::Fused);
        }
    }
}
