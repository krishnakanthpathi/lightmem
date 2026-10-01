use crate::embeddings::{cosine_similarity, EmbeddingProvider};
use crate::models::{MemoryStatus, MemoryType, ScoredMemory};
use crate::storage::Storage;
use anyhow::Result;
use chrono::{DateTime, Utc};
use std::collections::HashMap;

pub struct HybridSearchEngine;

impl HybridSearchEngine {
    pub fn search(
        storage: &Storage,
        embedder: &dyn EmbeddingProvider,
        query: &str,
        category: Option<MemoryType>,
        status: Option<MemoryStatus>,
        as_of: Option<DateTime<Utc>>,
        limit: usize,
        min_similarity: Option<f32>,
    ) -> Result<Vec<ScoredMemory>> {
        let fetch_limit = (limit * 3).max(50);

        // 1. BM25 Search via FTS5
        let bm25_results = storage.search_bm25(query, category, status, as_of, fetch_limit)?;
        let mut bm25_ranks: HashMap<String, usize> = HashMap::new();
        for (rank_idx, (id, _)) in bm25_results.iter().enumerate() {
            bm25_ranks.insert(id.clone(), rank_idx + 1);
        }

        // 2. Vector Cosine Search
        let mut vector_ranks: HashMap<String, usize> = HashMap::new();
        let mut vector_scores: HashMap<String, f32> = HashMap::new();

        if let Ok(query_vector) = embedder.embed(query) {
            let candidates = storage.get_candidate_vectors(category, status, as_of)?;
            let mut scored_vectors: Vec<(String, f32)> = candidates
                .into_iter()
                .map(|(id, vec)| {
                    let sim = cosine_similarity(&query_vector, &vec);
                    (id, sim)
                })
                .collect();

            // Sort by cosine similarity descending
            scored_vectors.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

            for (rank_idx, (id, score)) in scored_vectors.into_iter().enumerate() {
                if let Some(min_sim) = min_similarity {
                    if score < min_sim && !bm25_ranks.contains_key(&id) {
                        continue;
                    }
                }
                vector_ranks.insert(id.clone(), rank_idx + 1);
                vector_scores.insert(id, score);
            }
        }

        // 3. Reciprocal Rank Fusion (RRF)
        // Formula: score = (1 / (60 + rank_bm25)) + (1 / (60 + rank_vec))
        const K: f32 = 60.0;
        let mut rrf_scores: HashMap<String, f32> = HashMap::new();

        for (id, rank) in &bm25_ranks {
            let rrf = 1.0 / (K + *rank as f32);
            *rrf_scores.entry(id.clone()).or_insert(0.0) += rrf;
        }

        for (id, rank) in &vector_ranks {
            let rrf = 1.0 / (K + *rank as f32);
            *rrf_scores.entry(id.clone()).or_insert(0.0) += rrf;
        }

        // Sort all unique candidate IDs by RRF score descending
        let mut ranked_candidates: Vec<(String, f32)> = rrf_scores.into_iter().collect();
        ranked_candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let top_ids: Vec<String> = ranked_candidates
            .into_iter()
            .take(limit)
            .map(|(id, _)| id)
            .collect();

        // 4. Hydrate full memory records
        let mut results = Vec::new();
        for id in top_ids {
            if let Some(mem) = storage.get_memory(&id)? {
                let bm25_rank = bm25_ranks.get(&id).copied();
                let vector_rank = vector_ranks.get(&id).copied();
                let score = vector_scores.get(&id).copied().unwrap_or(0.0);

                results.push(ScoredMemory {
                    memory: mem,
                    score,
                    bm25_rank,
                    vector_rank,
                });
            }
        }

        Ok(results)
    }
}
