use crate::embeddings::{cosine_similarity, EmbeddingProvider};
use crate::models::{MemoryRecord, MemoryStatus, MemoryType, ScoredMemory};
use crate::storage::Storage;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use std::collections::{HashMap, HashSet};

pub struct HybridSearchEngine;

impl HybridSearchEngine {
    /// Extract non-stopword tokens from query for acronym/initials matching.
    pub fn extract_query_tokens(query: &str) -> Vec<String> {
        const STOPWORDS: &[&str] = &[
            "who", "what", "where", "when", "why", "how", "is", "are", "was", "were", "the", "a",
            "an", "in", "on", "at", "to", "for", "of", "with", "by", "from", "as", "and", "or",
            "my", "me", "do", "does", "did", "user", "name",
        ];
        query
            .split(|c: char| !c.is_alphanumeric())
            .map(|w| w.trim().to_lowercase())
            .filter(|w| w.len() >= 2 && !STOPWORDS.contains(&w.as_str()))
            .collect()
    }

    /// Compute an acronym / initials / exact-token boost for a memory against query tokens.
    /// Example: query token `"kk"` matches consecutive capitalized words `"Krishna Kanth"` (`K` + `K` = `kk`).
    pub fn compute_acronym_and_lexical_boost(query_tokens: &[String], mem: &MemoryRecord) -> f32 {
        if query_tokens.is_empty() {
            return 0.0;
        }

        let combined_orig = format!("{} {}", mem.title, mem.content);
        let orig_words: Vec<&str> = combined_orig
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .collect();

        let lower_words: Vec<String> = orig_words.iter().map(|w| w.to_lowercase()).collect();
        let tags_lower: Vec<String> = mem.tags.iter().map(|t| t.to_lowercase()).collect();

        let mut total_boost = 0.0f32;

        for q_tok in query_tokens {
            let q_chars: Vec<char> = q_tok.chars().collect();
            let n = q_chars.len();

            let mut matched_proper_initials = false;
            let mut matched_any_initials = false;

            // Check if any N consecutive words in title/content have initials matching `q_tok`
            if (2..=5).contains(&n) && orig_words.len() >= n {
                for window_idx in 0..=(orig_words.len() - n) {
                    let mut all_match = true;
                    let mut all_capitalized = true;
                    for k in 0..n {
                        let w_orig = orig_words[window_idx + k];
                        let first_orig = w_orig.chars().next().unwrap_or(' ');
                        if first_orig.to_ascii_lowercase() != q_chars[k] {
                            all_match = false;
                            break;
                        }
                        if !first_orig.is_ascii_uppercase() {
                            all_capitalized = false;
                        }
                    }
                    if all_match {
                        if all_capitalized {
                            matched_proper_initials = true;
                            break;
                        } else {
                            matched_any_initials = true;
                        }
                    }
                }
            }

            if matched_proper_initials {
                // Strong boost when query acronym matches Proper Noun initials (e.g. "kk" -> "Krishna Kanth")
                total_boost += 0.26;
            } else if matched_any_initials {
                total_boost += 0.12;
            } else if lower_words.iter().any(|w| w == q_tok)
                || tags_lower
                    .iter()
                    .any(|t| t == q_tok || t.starts_with(&format!("{}-", q_tok)))
            {
                // Exact token or tag prefix match (e.g. "kk" in "kk-linux")
                total_boost += 0.06;
            }
        }

        total_boost.min(0.35)
    }

    #[allow(clippy::too_many_arguments)]
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
        Self::search_with_identity(
            storage,
            embedder,
            query,
            category,
            status,
            as_of,
            limit,
            min_similarity,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn search_with_identity(
        storage: &Storage,
        embedder: &dyn EmbeddingProvider,
        query: &str,
        category: Option<MemoryType>,
        status: Option<MemoryStatus>,
        as_of: Option<DateTime<Utc>>,
        limit: usize,
        min_similarity: Option<f32>,
        identity: Option<&str>,
    ) -> Result<Vec<ScoredMemory>> {
        anyhow::ensure!(!query.trim().is_empty(), "Search query cannot be blank");
        if let Some(min) = min_similarity {
            anyhow::ensure!(
                min.is_finite() && (0.0..=1.0).contains(&min),
                "Minimum similarity must be between 0 and 1"
            );
        }
        if limit == 0 {
            return Ok(Vec::new());
        }
        let fetch_limit = limit.saturating_mul(6).max(80);
        let query_tokens = Self::extract_query_tokens(query);

        // 1. BM25 Search via FTS5
        let bm25_results = storage.search_bm25(query, category, status, as_of, fetch_limit)?;
        let mut bm25_ranks: HashMap<String, usize> = HashMap::new();
        for (rank_idx, (id, _)) in bm25_results.iter().enumerate() {
            bm25_ranks.insert(id.clone(), rank_idx + 1);
        }

        // 2. Vector Cosine Search
        let mut vector_ranks: HashMap<String, usize> = HashMap::new();
        let mut vector_scores: HashMap<String, f32> = HashMap::new();

        {
            let query_vector = embedder.embed(query).context("Query embedding failed")?;
            crate::embeddings::validate_vector(&query_vector)?;
            let candidates =
                storage.get_candidate_vectors_checked(category, status, as_of, identity)?;
            anyhow::ensure!(
                candidates
                    .iter()
                    .all(|(_, vector)| vector.len() == query_vector.len()
                        && vector.iter().all(|v| v.is_finite())),
                "Stored embedding dimensions/values are invalid; run lmem reindex"
            );
            let mut scored_vectors: Vec<(String, f32)> = candidates
                .into_iter()
                .map(|(id, vec)| {
                    let sim = cosine_similarity(&query_vector, &vec);
                    (id, sim)
                })
                .collect();

            // Select a bounded top-k pool without sorting the full vault.
            scored_vectors.retain(|(_, score)| *score > 0.0);
            let compare = |a: &(String, f32), b: &(String, f32)| {
                b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0))
            };
            if scored_vectors.len() > fetch_limit {
                scored_vectors.select_nth_unstable_by(fetch_limit, compare);
                scored_vectors.truncate(fetch_limit);
            }
            scored_vectors.sort_by(compare);

            for (rank_idx, (id, score)) in scored_vectors.into_iter().enumerate() {
                if let Some(min_sim) = min_similarity {
                    if score < min_sim {
                        continue;
                    }
                }
                vector_ranks.insert(id.clone(), rank_idx + 1);
                vector_scores.insert(id, score);
            }
        }

        // 3. Reciprocal Rank Fusion (RRF)
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

        // Sort candidate IDs by RRF score descending and inspect a wide pool for deduplication & acronym boosting
        let mut ranked_candidates: Vec<(String, f32)> = rrf_scores
            .iter()
            .map(|(id, score)| (id.clone(), *score))
            .collect();
        ranked_candidates.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

        let inspect_ids: Vec<String> = ranked_candidates
            .into_iter()
            .take(fetch_limit)
            .map(|(id, _)| id)
            .collect();

        // 4. Hydrate, deduplicate identical content, and apply Acronym/Initials boost
        let mut seen_contents: HashSet<(MemoryType, String)> = HashSet::new();
        let mut results = Vec::new();

        let mut memories = storage.get_memories_exact(&inspect_ids)?;
        for id in inspect_ids {
            if let Some(mem) = memories.remove(&id) {
                let key = (mem.category, crate::storage::duplicate_key(&mem.content));
                if !seen_contents.insert(key) {
                    continue;
                }
                if let Some(min) = min_similarity {
                    if vector_scores.get(&id).copied().unwrap_or(-1.0) < min {
                        continue;
                    }
                }
                let bm25_rank = bm25_ranks.get(&id).copied();
                let vector_rank = vector_ranks.get(&id).copied();
                let base_score = rrf_scores.get(&id).copied().unwrap_or(0.0) * (K + 1.0) / 2.0;
                let boost = Self::compute_acronym_and_lexical_boost(&query_tokens, &mem);
                let score = (0.85 * base_score + 0.15 * boost / 0.35).min(1.0);

                results.push(ScoredMemory {
                    memory: mem,
                    score,
                    bm25_rank,
                    vector_rank,
                });
            }
        }

        // Keep fused ranking in the final score; it is a ranking score, not a probability.
        results.sort_by(|a, b| {
            b.score
                .total_cmp(&a.score)
                .then_with(|| a.memory.id.cmp(&b.memory.id))
        });
        results.truncate(limit);

        Ok(results)
    }
}
