use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;

use crate::config::MemoryConfig;
use crate::embeddings::{content_hash, EmbeddingProvider};
use crate::store::MemoryStore;
use crate::types::SearchResult;

pub struct HybridSearcher {
    store: Arc<MemoryStore>,
    embedder: Arc<dyn EmbeddingProvider>,
    vector_weight: f32,
    fts_weight: f32,
    temporal_decay_lambda: f64,
    mmr_lambda: f64,
}

impl HybridSearcher {
    pub fn new(
        store: Arc<MemoryStore>,
        embedder: Arc<dyn EmbeddingProvider>,
        config: &MemoryConfig,
    ) -> Self {
        Self {
            store,
            embedder,
            vector_weight: config.vector_weight,
            fts_weight: config.fts_weight,
            temporal_decay_lambda: config.temporal_decay_lambda,
            mmr_lambda: config.mmr_lambda,
        }
    }

    pub async fn search(
        &self,
        query: &str,
        limit: usize,
        _container_tag: Option<&str>,
    ) -> Result<Vec<SearchResult>> {
        let candidate_count = limit * 3;

        let hash = content_hash(query);
        let query_embedding = match self.store.get_cached_embedding(&hash)? {
            Some(cached) => cached,
            None => {
                let embeddings = self.embedder.embed(&[query.to_string()]).await?;
                let emb = embeddings.into_iter().next().unwrap_or_default();
                self.store
                    .cache_embedding(&hash, &emb, self.embedder.model_name())?;
                emb
            }
        };

        let vec_results = self
            .store
            .vector_search(&query_embedding, candidate_count)?;
        let fts_results = self.store.fts_search(query, candidate_count)?;

        let vec_scores: HashMap<String, f32> = vec_results.into_iter().collect();
        let fts_scores: HashMap<String, f64> = fts_results.into_iter().collect();

        let mut all_ids: Vec<String> = vec_scores
            .keys()
            .chain(fts_scores.keys())
            .cloned()
            .collect();
        all_ids.sort();
        all_ids.dedup();

        let (vec_min, vec_max) = min_max_f32(vec_scores.values().copied());
        let (fts_min, fts_max) = min_max_f64(fts_scores.values().copied());

        let now = chrono::Utc::now();
        let mut candidates: Vec<ScoredCandidate> = Vec::new();

        for id in &all_ids {
            let entry = match self.store.get(id)? {
                Some(e) => e,
                None => continue,
            };

            let norm_vec = vec_scores
                .get(id)
                .map(|&s| 1.0 - normalize_f32(s, vec_min, vec_max))
                .unwrap_or(0.0);

            let norm_fts = fts_scores
                .get(id)
                .map(|&s| normalize_f64(s.abs(), fts_min.abs(), fts_max.abs()))
                .unwrap_or(0.0);

            let combined =
                (self.vector_weight as f64 * norm_vec as f64) + (self.fts_weight as f64 * norm_fts);

            let age_hours = (now - entry.created_at).num_hours().max(0) as f64;
            let decay = (-self.temporal_decay_lambda * age_hours).exp();
            let final_score = combined * decay;

            let emb = self
                .store
                .get_cached_embedding(&content_hash(&entry.content))?;

            candidates.push(ScoredCandidate {
                result: SearchResult {
                    entry,
                    vector_score: vec_scores.get(id).copied(),
                    fts_score: fts_scores.get(id).copied(),
                    combined_score: combined,
                    final_score,
                },
                embedding: emb,
            });
        }

        candidates.sort_by(|a, b| {
            b.result
                .final_score
                .partial_cmp(&a.result.final_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let selected = self.apply_mmr(candidates, &query_embedding, limit);

        Ok(selected.into_iter().map(|c| c.result).collect())
    }

    fn apply_mmr(
        &self,
        candidates: Vec<ScoredCandidate>,
        _query_embedding: &[f32],
        limit: usize,
    ) -> Vec<ScoredCandidate> {
        if candidates.len() <= limit {
            return candidates;
        }

        let mut selected: Vec<ScoredCandidate> = Vec::with_capacity(limit);
        let mut remaining = candidates;

        if !remaining.is_empty() {
            selected.push(remaining.remove(0));
        }

        while selected.len() < limit && !remaining.is_empty() {
            let mut best_idx = 0;
            let mut best_mmr = f64::NEG_INFINITY;

            for (i, candidate) in remaining.iter().enumerate() {
                let relevance = candidate.result.final_score;

                let max_sim = selected
                    .iter()
                    .map(|s| match (&candidate.embedding, &s.embedding) {
                        (Some(a), Some(b)) => cosine_similarity(a, b) as f64,
                        _ => 0.0,
                    })
                    .fold(0.0_f64, f64::max);

                let mmr = self.mmr_lambda * relevance - (1.0 - self.mmr_lambda) * max_sim;

                if mmr > best_mmr {
                    best_mmr = mmr;
                    best_idx = i;
                }
            }

            selected.push(remaining.remove(best_idx));
        }

        selected
    }
}

struct ScoredCandidate {
    result: SearchResult,
    embedding: Option<Vec<f32>>,
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

fn normalize_f32(val: f32, min: f32, max: f32) -> f32 {
    if (max - min).abs() < f32::EPSILON {
        return 0.5;
    }
    (val - min) / (max - min)
}

fn normalize_f64(val: f64, min: f64, max: f64) -> f64 {
    if (max - min).abs() < f64::EPSILON {
        return 0.5;
    }
    (val - min) / (max - min)
}

fn min_max_f32(iter: impl Iterator<Item = f32>) -> (f32, f32) {
    let mut min = f32::MAX;
    let mut max = f32::MIN;
    let mut count = 0;
    for val in iter {
        if val < min {
            min = val;
        }
        if val > max {
            max = val;
        }
        count += 1;
    }
    if count == 0 {
        (0.0, 1.0)
    } else {
        (min, max)
    }
}

fn min_max_f64(iter: impl Iterator<Item = f64>) -> (f64, f64) {
    let mut min = f64::MAX;
    let mut max = f64::MIN;
    let mut count = 0;
    for val in iter {
        if val < min {
            min = val;
        }
        if val > max {
            max = val;
        }
        count += 1;
    }
    if count == 0 {
        (0.0, 1.0)
    } else {
        (min, max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embeddings::MockEmbeddingProvider;
    use crate::types::{MemoryEntry, MemorySource};

    fn setup() -> (Arc<MemoryStore>, Arc<dyn EmbeddingProvider>, MemoryConfig) {
        let store = Arc::new(MemoryStore::new_in_memory(64).unwrap());
        let embedder: Arc<dyn EmbeddingProvider> = Arc::new(MockEmbeddingProvider::new(64));
        let config = MemoryConfig {
            embedding_dimension: 64,
            ..Default::default()
        };
        (store, embedder, config)
    }

    #[tokio::test]
    async fn search_returns_results() {
        let (store, embedder, config) = setup();

        let texts = [
            "Rust programming language for systems",
            "Python scripting for data science",
            "Rust async runtime with tokio",
        ];

        for text in &texts {
            let entry = MemoryEntry::new(*text, MemorySource::Manual, "default");
            let emb = embedder.embed(&[text.to_string()]).await.unwrap();
            store.insert(&entry, &emb[0]).unwrap();
        }

        let searcher = HybridSearcher::new(store, embedder, &config);
        let results = searcher.search("rust programming", 5, None).await.unwrap();
        assert!(!results.is_empty());
    }

    #[tokio::test]
    async fn search_empty_store_returns_empty() {
        let (store, embedder, config) = setup();
        let searcher = HybridSearcher::new(store, embedder, &config);
        let results = searcher.search("anything", 5, None).await.unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn cosine_similarity_identical_vectors() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_similarity_orthogonal_vectors() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        assert!(cosine_similarity(&a, &b).abs() < 1e-6);
    }
}
